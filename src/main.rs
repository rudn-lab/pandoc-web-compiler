mod admin;
mod pricing;
mod profile;
mod result;
mod upload;

use api::PricingInfo;
use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Json, Router,
};
use pricing::get_current_pricing;

#[derive(Clone)]
struct AppState {
    db: sqlx::SqlitePool,
}

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv(); // Try loading values, ignoring missing files.

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

    // build our application with a single route
    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/pricing", get(get_quote))
        .route("/user-info/:token", get(profile::get_user))
        .route("/orders/new/:token", post(upload::upload_order))
        .route("/admin/make-user", post(admin::make_user))
        .layer(DefaultBodyLimit::max(25 * 1024 * 1024)) // 25MB
        .with_state(AppState { db });

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_quote() -> Json<PricingInfo> {
    Json(get_current_pricing())
}
