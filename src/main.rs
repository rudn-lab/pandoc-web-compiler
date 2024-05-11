mod admin;
mod manager;
mod pricing;
mod profile;
mod result;
mod upload;
mod worker;

use api::{OrderInfo, PricingInfo};
use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Json, Router,
};
use manager::{run_manager, ManagerRequest};
use pricing::get_current_pricing;
use tokio::sync::mpsc;

#[derive(Clone)]
struct AppState {
    db: sqlx::SqlitePool,
    manager_connection: mpsc::Sender<ManagerRequest>,
}

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv(); // Try loading values, ignoring missing files.

    tracing_subscriber::fmt::init();

    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL should point at a sqlite db");
    if let Some(prefix) = url.strip_prefix("sqlite://") {
        std::fs::File::options()
            .create(true)
            .write(true)
            .open(prefix)
            .expect("Could not create db file");
    }

    let db = sqlx::SqlitePool::connect(&url)
        .await
        .expect("Could not connect to the database");
    sqlx::migrate!()
        .run(&db)
        .await
        .expect("Failed to apply migrations");

    // Mark all orders that were running before with an abnormal termination.
    let abnormal = serde_json::to_string(&OrderInfo {
        balance_before: 0.0,
        order_cost: 0.0,
        pricing_applied: get_current_pricing(),
        termination: api::JobTerminationStatus::VeryAbnormalTermination(format!(
            "Job was marked as running across application restart"
        )),
    })
    .unwrap();
    sqlx::query!(
        "UPDATE orders SET status_json=?, is_running=0 WHERE is_running=1",
        abnormal
    )
    .execute(&db)
    .await
    .unwrap();

    let (manager_connection, manager_rx) = mpsc::channel(100);
    let cancel = tokio_util::sync::CancellationToken::new();

    tokio::task::spawn(run_manager(
        manager_rx,
        manager_connection.clone(),
        db.clone(),
        cancel.clone(),
    ));

    // build our application with a single route
    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/pricing", get(get_quote))
        .route("/user-info/:token", get(profile::get_user))
        .route("/orders/:token/new", post(upload::upload_order))
        .route("/orders/:token/:id", get(upload::get_order_status))
        .route("/orders/:token/:id/files", get(upload::get_order_file_list))
        .route(
            "/orders/:token/:id/files/download/:name",
            get(upload::fetch_file),
        )
        .route("/orders/:token/:id/ws", get(upload::get_live_order_status))
        .route(
            "/orders/:token/:id/stream/:stream",
            get(upload::get_live_order_stream),
        )
        .route("/admin/make-user", post(admin::make_user))
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024)) // 100MB
        .with_state(AppState {
            db,
            manager_connection,
        });

    tokio::task::spawn({
        let cancel = cancel.clone();
        async move {
            let _ = tokio::signal::ctrl_c().await;
            cancel.cancel();
        }
    });

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(cancel.cancelled_owned())
        .await
        .unwrap();
}

async fn get_quote() -> Json<PricingInfo> {
    Json(get_current_pricing())
}
