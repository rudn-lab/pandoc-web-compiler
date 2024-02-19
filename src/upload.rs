use std::fmt::Debug;

use api::{OrderInfoResult, UserInfo, UserInfoResult};
use axum::{
    extract::{ws::WebSocket, Multipart, Path, State, WebSocketUpgrade},
    response::IntoResponse,
    Json,
};
use sqlx::SqlitePool;
use tokio::{io::AsyncWriteExt, sync::mpsc};
use tracing::Instrument;

use crate::{manager::ManagerRequest, result::AppError, worker::RunningJobHandle, AppState};

pub async fn upload_order(
    State(AppState {
        db,
        manager_connection,
    }): State<AppState>,
    Path(token): Path<String>,
    mut files: Multipart,
) -> Result<String, AppError> {
    let data = match sqlx::query!("SELECT * FROM accounts WHERE token=?", token)
        .fetch_optional(&db)
        .await?
    {
        Some(v) => v,
        None => Err(anyhow::anyhow!("No such token found"))?,
    };

    let span = tracing::debug_span!("order_upload");
    async move {
        tracing::debug!("Received order from {data:?}");

        let order_id = ManagerRequest::allocate_order(&manager_connection, data.id).await;
        tracing::debug!("The order was allocated ID {order_id}");

        tracing::debug!("Starting to copy files into work directory...");
        // Now we need to copy the files into the work directory.
        let mut idx = 0;
        let mut size = 0;
        while let Some(field) = files.next_field().await? {
            idx += 1;
            let name = field
                .name()
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("lost+found-{idx}.bin"));
            tracing::debug!("File: {name:?}");
            let data = field.bytes().await?;
            tracing::debug!("Data: {} bytes", data.len());
            size += data.len();
            let path = safe_path::scoped_join(format!("/compile/{order_id}"), name)?;

            // If the directory doesn't exist, we need to create it.
            let file_parent = path.parent().ok_or_else(|| {
                anyhow::anyhow!("Error while creating parent dir for path {path:?}")
            })?;
            if !file_parent.exists() {
                tokio::fs::create_dir_all(file_parent).await?;
            }

            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open(path)
                .await?;

            file.write_all(&data).await?;
        }
        tracing::debug!("Files copied to work directory!");

        ManagerRequest::uploaded_files(
            &manager_connection,
            order_id,
            idx,
            size as f64 / 1024.0 / 1024.0,
        )
        .await;

        Ok(format!("{order_id}"))
    }
    .instrument(span)
    .await
}

pub async fn get_order_status(
    State(AppState {
        db,
        manager_connection,
    }): State<AppState>,
    Path((token, order_id)): Path<(String, i64)>,
) -> Result<Json<OrderInfoResult>, AppError> {
    let data = match sqlx::query!("SELECT orders.* FROM accounts INNER JOIN orders ON orders.user_id=accounts.id WHERE token=? AND orders.id=?", token, order_id)
        .fetch_optional(&db)
        .await?
    {
        Some(v) => v,
        None => return Ok(Json(OrderInfoResult::NotAccessible)),
    };

    // TODO: actually implement order manipulation
    Ok(Json(OrderInfoResult::Running))
}

pub async fn get_live_order_status(
    State(AppState {
        db,
        manager_connection,
    }): State<AppState>,
    Path((token, order_id)): Path<(String, i64)>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, AppError> {
    let _data = match sqlx::query!("SELECT orders.* FROM accounts INNER JOIN orders ON orders.user_id=accounts.id WHERE token=? AND orders.id=?", token, order_id)
        .fetch_optional(&db)
        .await?
    {
        Some(v) => v,
        None => Err(anyhow::anyhow!("the order does not exist or is inaccessible"))?,
    };

    let status = ManagerRequest::query_live_status(&manager_connection, order_id).await;
    match status {
        Some(handle) => Ok(ws.on_upgrade(move |ws| {
            handle_live_order_status(handle, order_id, db, manager_connection, ws)
        })),
        None => Ok(ws.on_upgrade(move |ws| timer(ws))),
    }
}

async fn timer(mut ws: WebSocket) {
    let mut idx = 0;
    // TODO: actually implement order manipulation
    loop {
        ws.send(axum::extract::ws::Message::Text(format!(
            "still alive~ {idx}",
        )))
        .await
        .unwrap();
        idx += 1;
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn handle_live_order_status(
    mut handle: RunningJobHandle,
    order_id: i64,
    db: SqlitePool,
    manager_connection: mpsc::Sender<ManagerRequest>,
    mut ws: WebSocket,
) {
    let update_interval = std::time::Duration::from_millis(500);
    let mut last_update_at = std::time::SystemTime::UNIX_EPOCH;

    loop {
        tokio::select! {
            _ =  handle.status.changed() => {
                if last_update_at.elapsed().unwrap() > update_interval {
                    ws.send(axum::extract::ws::Message::Text(format!(
                        "{:?}",
                        handle.status.borrow_and_update()
                    )))
                    .await
                    .unwrap();
                    last_update_at = std::time::SystemTime::now();
                }
            }
            _ =  handle.job_termination.recv() => {
                    ws.send(axum::extract::ws::Message::Text(format!(
                        "DONE",
                    )))
                    .await
                    .unwrap();
                    ws.close().await.unwrap();
                    return;
            }
        }
    }
}
