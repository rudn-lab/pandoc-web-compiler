use axum::{routing::get, Router};

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv(); // Try loading values, ignoring missing files.

    let db = sqlx::SqlitePool::connect(
        &std::env::var("DATABASE_URL").expect("DATABASE_URL should point at a sqlite db"),
    )
    .await
    .expect("Could not connect to the database");
    sqlx::migrate!()
        .run(&db)
        .await
        .expect("Failed to apply migrations");

    // build our application with a single route
    let app = Router::new().route("/", get(|| async { "Hello, World!" }));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
