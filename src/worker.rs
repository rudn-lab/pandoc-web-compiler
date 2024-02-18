use std::time::Duration;

use axum::extract::Multipart;
use sqlx::SqlitePool;
use tokio::{
    io::AsyncWriteExt,
    sync::{mpsc, oneshot, watch},
};
use tokio_util::sync::CancellationToken;

use crate::manager::ManagerRequest;

/// This represents the status of a running job.
#[derive(Debug)]
pub enum WorkStatus {
    /// The job is copying files into the work dir.
    CopyingFiles,
}

#[tracing::instrument(level = "info")]
pub async fn run_order_work(
    order_id: i64,
    db: SqlitePool,
    sender: mpsc::Sender<ManagerRequest>,
) -> anyhow::Result<()> {
    let (status_send, status_recv) = watch::channel(WorkStatus::CopyingFiles);
    let cancel = CancellationToken::new();

    // We'll send a value here if we finish successfully, and drop it on the way out of the function otherwise.
    let (exit_send, exit_recv) = oneshot::channel();

    // The first thing to do is to announce ourselves.
    sender
        .send(ManagerRequest::BeginWork {
            order_id,
            status: status_recv,
            cancel: cancel.clone(),
            exit: exit_recv,
        })
        .await?;

    tokio::time::sleep(Duration::from_secs(30)).await;

    sender
        .send(ManagerRequest::FinishWork { order_id })
        .await
        .unwrap();
    exit_send.send(()).unwrap();
    Ok(())
}
