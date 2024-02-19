use std::fmt::Debug;

use api::{LiveStatus, OrderInfoResult};
use axum::{
    extract::{ws::WebSocket, Multipart, Path, State, WebSocketUpgrade},
    response::IntoResponse,
    Json,
};
use sqlx::SqlitePool;
use tokio::{io::AsyncWriteExt, sync::mpsc};
use tracing::Instrument;

use crate::{
    manager::ManagerRequest, pricing::get_current_pricing, result::AppError,
    worker::RunningJobHandle, AppState,
};

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

    match ManagerRequest::query_live_status(&manager_connection, order_id).await {
        Some(_handle) => Ok(Json(OrderInfoResult::Running)),
        None => {
            // It's not running: only use data from database.

            Ok(Json(OrderInfoResult::Completed {
                info: serde_json::from_str(&data.status_json.ok_or(anyhow::anyhow!(
                    "Database row didn't have data for a completed job"
                ))?)?,
                is_on_disk: data.is_on_disk,
            }))
        }
    }
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
        None => Ok(ws.on_upgrade(move |ws| close_immediately(ws))),
    }
}

async fn close_immediately(mut ws: WebSocket) {
    ws.send(axum::extract::ws::Message::Close(Some(
        axum::extract::ws::CloseFrame {
            code: 1008,
            reason: "Job has already terminated at time of connection".into(),
        },
    )))
    .await
    .unwrap();
    ws.close().await.unwrap();
    return;
}

async fn handle_live_order_status(
    mut handle: RunningJobHandle,
    order_id: i64,
    db: SqlitePool,
    manager_connection: mpsc::Sender<ManagerRequest>,
    mut ws: WebSocket,
) {
    let update_interval = std::time::Duration::from_millis(200);
    let mut last_update_at = std::time::SystemTime::UNIX_EPOCH;

    loop {
        tokio::select! {
            _ =  handle.status.changed() => {
                if last_update_at.elapsed().unwrap() > update_interval {
                    let data = serde_json::to_string(
                        &LiveStatus{
                            status: (&*handle.status.borrow_and_update()).clone(),
                            pricing: get_current_pricing(),
                        }
                    ).unwrap();
                    tracing::debug!("Sending WS msg: {data:?}");
                    ws.send(axum::extract::ws::Message::Text(data))
                    .await
                    .unwrap();
                    last_update_at = std::time::SystemTime::now();
                }
            }
            Err(_) =  handle.job_termination.recv() => {
                    ws.send(axum::extract::ws::Message::Close(Some(
                        axum::extract::ws::CloseFrame {
                            code: 1000,
                            reason: "Job has terminated".into(),
                        },
                    )))
                    .await
                    .unwrap();
                    ws.close().await.unwrap();
                    return;
            }

            Some(msg) = ws.recv() => {
                if let Ok(msg) = msg {
                    match msg {
                        axum::extract::ws::Message::Text(_data) => {
                            // The user has requested a stop: otherwise no message is sent.
                            handle.stop.cancel();
                        },
                        _ => {}
                    }
                }
            }
        }
    }
}
