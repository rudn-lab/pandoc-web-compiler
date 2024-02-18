use std::collections::HashMap;
use axum::extract::Multipart;
use serde::{Serialize, Deserialize};
use sqlx::SqlitePool;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use crate::worker;

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
    },

    /// Sent by a worker thread in order to announce its existence and register its comms.
    BeginWork {
        order_id: i64,
        status: tokio::sync::watch::Receiver<worker::WorkStatus>,
        cancel: tokio_util::sync::CancellationToken,
        exit: oneshot::Receiver<()>
    },

    /// Sent by a worker thread to announce that it's finished with its work and written everything out to disk.
    FinishWork {
        order_id: i64,
    },
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
    pub async fn uploaded_files(sender: &mpsc::Sender<ManagerRequest>, order_id: i64) {
        sender.send(Self::UploadFiles { order_id }).await.expect("Manager thread closed");
    }
}


pub async fn run_manager(
    mut recv: mpsc::Receiver<ManagerRequest>,
    send: mpsc::Sender<ManagerRequest>,
    db: SqlitePool,
    cancel: CancellationToken,
) -> ! {
    let mut running_orders = HashMap::new();
    loop {
        tokio::select! {
            _ = cancel.cancelled() => panic!("Cancellation token caused manager thread to stop"),
            msg = recv.recv() => {
                if let Some(msg) = msg 
                    {if let Err(why) = handle_msg(msg, &db, &mut running_orders, &send).await
                        {eprintln!("Error in manager: {why}")}
                    }
                else {panic!("All senders to manager thread have closed");}
                },
        }
    }
}

async fn handle_msg(msg: ManagerRequest, db: &SqlitePool, running_orders: &mut HashMap<i64, (watch::Receiver<worker::WorkStatus>, CancellationToken, oneshot::Receiver<()>)>, sender: &mpsc::Sender<ManagerRequest>) -> anyhow::Result<()> {
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
        ManagerRequest::UploadFiles { order_id } => {
            tracing::debug!("Spawning a new task to work on order {order_id}");
            tokio::task::spawn(worker::run_order_work(order_id, db.clone(), sender.clone()));
        },

        ManagerRequest::BeginWork { order_id, status, cancel, exit } => {
            running_orders.insert(order_id, (status, cancel, exit));
        },
        ManagerRequest::FinishWork { order_id } => {running_orders.remove(&order_id);},
    }

    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
pub enum OrderStatusInfo {
    AbnormalTermination,
}