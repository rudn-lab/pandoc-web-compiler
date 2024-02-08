use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue},
    Json,
};

use crate::{result::AppError, AppState};

pub async fn make_user(
    State(AppState { db }): State<AppState>,
    headers: HeaderMap,
    Json(name): Json<String>,
) -> Result<Json<String>, AppError> {
    let h = headers.get("X-AuthToken");
    if h != Some(&HeaderValue::from_str(&std::env::var("SECRET_KEY").unwrap()).unwrap()) {
        Err(anyhow::anyhow!("Secret key invalid, got: {h:?}"))?
    }

    use rand::distributions::DistString;
    let token = rand::distributions::Alphanumeric.sample_string(&mut rand::thread_rng(), 32);

    sqlx::query!(
        "INSERT INTO accounts (user_name, token, balance) VALUES (?,?,?)",
        name,
        token,
        0.0
    )
    .execute(&db)
    .await?;

    Ok(Json(token))
}
