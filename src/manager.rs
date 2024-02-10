use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use sqlx::SqlitePool;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

pub enum ManagerRequest {
    /// Request a new order to be allocated.
    /// This creates a database record corresponding to this order,
    /// creates the folder for it,
    /// and returns the order ID to the given channel.
    AllocateOrder {
        user_id: i64,
        recv: oneshot::Sender<i64>,
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
}

pub async fn run_manager(
    mut recv: mpsc::Receiver<ManagerRequest>,
    db: SqlitePool,
    cancel: CancellationToken,
) {
    let mut running_orders: HashMap<i64, ()> = HashMap::new();

    tokio::select! {
        _ = cancel.cancelled() => return,
        msg = recv.recv() => {if let Some(msg) = msg {if let Err(why) = handle_msg(msg, &db, &mut running_orders).await {eprintln!("Error in manager: {why}")}} else {return;}},
    }
}

async fn handle_msg(msg: ManagerRequest, db: &SqlitePool, running_orders: &mut HashMap<i64, ()>) -> anyhow::Result<()> {
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
        }
    }

    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
pub enum OrderStatusInfo {
    AbnormalTermination,
}