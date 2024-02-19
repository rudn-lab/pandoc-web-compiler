use std::collections::HashMap;
use api::JobTerminationStatus;
use serde::{Serialize, Deserialize};
use sqlx::SqlitePool;
use tokio::{sync::{broadcast::error::TryRecvError, mpsc, oneshot}, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use crate::worker::{self, RunningJobHandle};

#[derive(Debug)]
pub enum ManagerRequest {
    /// Request a new order to be allocated.
    /// This creates a database record corresponding to this order,
    /// creates the folder for it,
    /// and returns the order ID to the given channel.
    AllocateOrder {
        user_id: i64,
        recv: oneshot::Sender<i64>,
    },

    /// The files have been fully received, begin the order processing.
    UploadFiles {
        order_id: i64,
        file_count: usize,
        file_size_mb: f64,
    },

    /// Sent by a worker thread in order to announce its existence and register its comms.
    BeginWork {
        order_id: i64,
        handle: RunningJobHandle,
    },

    /// Sent by a worker thread to announce that it's finished with its work and written everything out to disk.
    FinishWork {
        order_id: i64,
    },

    /// A client is asking for an order's live status.
    /// Returns None if the job is not running anymore.
    QueryLiveStatus {
        order_id: i64,
        recv: oneshot::Sender<Option<RunningJobHandle>>
    },

    /// An internal message sent occasionally asking the manager to garbage-collect dead jobs.
    PruneDeadJobs,
}

impl ManagerRequest {
    pub async fn allocate_order(sender: &mpsc::Sender<ManagerRequest>, user_id: i64) -> i64 {
        let (send, recv) = oneshot::channel();
        sender
            .send(Self::AllocateOrder {
                user_id,
                recv: send,
            })
            .await
            .expect("Manager thread closed");
        recv.await
            .expect("Manager didn't respond to allocate_order")
    }
    pub async fn uploaded_files(sender: &mpsc::Sender<ManagerRequest>, order_id: i64, file_count: usize, file_size_mb: f64) {
        sender.send(Self::UploadFiles { order_id, file_count, file_size_mb }).await.expect("Manager thread closed");
    }

    pub async fn query_live_status(sender: &mpsc::Sender<ManagerRequest>, order_id: i64) -> Option<RunningJobHandle> {
        let (send, recv) = oneshot::channel();
        sender
            .send(Self::QueryLiveStatus {
                order_id,
                recv: send,
            })
            .await
            .expect("Manager thread closed");
        recv.await
            .expect("Manager didn't respond to query_live_status")
    }

}


pub async fn run_manager(
    mut recv: mpsc::Receiver<ManagerRequest>,
    send: mpsc::Sender<ManagerRequest>,
    db: SqlitePool,
    cancel: CancellationToken,
) -> ! {
    let mut running_job_handles = HashMap::new();
    let mut running_join_handles = HashMap::new();
    
    let send_out = send.clone();
    tokio::spawn(async move {
        loop {
            send_out.send(ManagerRequest::PruneDeadJobs).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    });

    loop {
        tokio::select! {
            _ = cancel.cancelled() => panic!("Cancellation token caused manager thread to stop"),
            msg = recv.recv() => {
                if let Some(msg) = msg 
                    {if let Err(why) = handle_msg(msg, &db, &mut running_job_handles, &mut running_join_handles, &send).await
                        {tracing::error!("Error in manager: {why}")}
                    }
                else {panic!("All senders to manager thread have closed");}
                },
        }
    }
}

async fn handle_msg(msg: ManagerRequest, db: &SqlitePool, running_handles: &mut HashMap<i64, RunningJobHandle>, join_handles: &mut HashMap<i64, JoinHandle<anyhow::Result<()>>>,  sender: &mpsc::Sender<ManagerRequest>) -> anyhow::Result<()> {
    tracing::debug!("Received message: {msg:?}");
    match msg {
        ManagerRequest::AllocateOrder { user_id, recv } => {
            // The ID is generated randomly -- over the life of the application it's not expected to collide.
            let id= (rand::random::<u64>() / 4) as i64;
            let now = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)).unwrap().as_secs() as i64;

            // Create a directory for the order.
            std::fs::create_dir_all(format!("/compile/{id}"))?;

            // Create a database row for the order.
            sqlx::query!(
                "INSERT INTO orders (id, user_id, created_at_unix_time, is_on_disk, is_running) VALUES (?, ?, ?, 1, 1)",
                id, 
                user_id,
                now,
            ).execute(db).await?;

            if let Err(id) = recv.send(id) {
                // Failed to make the order, deallocate the ID.
                sqlx::query!(
                    "DELETE FROM orders WHERE id=?",
                    id, 
                ).execute(db).await?;
                std::fs::remove_dir_all(format!("/compile/{id}"))?;
            }
        },
        ManagerRequest::UploadFiles { order_id, file_count, file_size_mb } => {
            tracing::debug!("Spawning a new task to work on order {order_id}");
            join_handles.insert(order_id, tokio::task::spawn(worker::run_order_work(order_id, db.clone(), sender.clone(), (file_count, file_size_mb))));
        },

        ManagerRequest::BeginWork { order_id, handle } => {
            running_handles.insert(order_id, handle);
        },
        ManagerRequest::FinishWork { order_id } => {running_handles.remove(&order_id); join_handles.remove(&order_id);},

        ManagerRequest::QueryLiveStatus { order_id, recv } => {
            // Check if there's an item in running_orders
            if let Some(handle) = running_handles.get_mut(&order_id) {
                // If the exit handle has sent, or is inaccessible, then the job has exited.
                match handle.job_termination.try_recv() {
                    Ok(_) | Err(TryRecvError::Closed) => {
                        // The job has exited: schedule it to be removed
                        let _ = recv.send(None);
                        let sender_out = sender.clone();
                        tokio::spawn(async move {sender_out.send(ManagerRequest::PruneDeadJobs).await});
                    },
                    _ => {
                        // The job is still running: give the caller a handle to it
                        let _ = recv.send(Some(handle.clone()));
                    },
                }
            } else {
                // The job has already been fully cleaned out: go look in the database for it.
                let _ = recv.send(None);
            }
        },

        ManagerRequest::PruneDeadJobs => {
            // Loop through the job join handles.
            // If any have finished, check their status to know what to write.
            let mut to_take = vec![];
            for (id, handle) in join_handles.iter() {
                if handle.is_finished() {
                    to_take.push(*id);
                }
            }

            for id in to_take {
                let handle = join_handles.remove(&id).unwrap();
                running_handles.remove(&id);
                let status = match handle.await {
                    Err(panic) => Some(JobTerminationStatus::VeryAbnormalTermination(format!("Task panic: {panic}"))),
                    Ok(Err(result_err)) => Some(JobTerminationStatus::VeryAbnormalTermination(format!("Task returned Err: {result_err}"))),
                    Ok(Ok(())) => None 
                };

                if let Some(status) = status {
                    let status_json = serde_json::to_string(&status).unwrap();
                    sqlx::query!("UPDATE orders SET is_running=0, status_json=? WHERE id=?", status_json, id).execute(db).await?;
                }
            }
        }


    }

    Ok(())
}

