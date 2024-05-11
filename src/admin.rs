use api::ChangePasswordRequest;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, HeaderValue},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{result::AppError, AppState};

#[derive(Serialize, Deserialize)]
pub struct RegistrationRequest {
    name: String,
    handle: String,
    password: String,
}

#[derive(Serialize, Deserialize)]
pub struct ResetPasswordRequest {
    handle: String,
    password: String,
}

pub async fn make_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(RegistrationRequest {
        name,
        handle,
        password,
    }): Json<RegistrationRequest>,
) -> Result<Json<String>, AppError> {
    let db = &state.db;
    let h = headers.get("X-AuthToken");
    if h != Some(&HeaderValue::from_str(&std::env::var("SECRET_KEY").unwrap()).unwrap()) {
        Err(anyhow::anyhow!("Secret key invalid, got: {h:?}"))?
    }

    use rand::distributions::DistString;
    let token = rand::distributions::Alphanumeric.sample_string(&mut rand::thread_rng(), 32);

    let new_account = sqlx::query!(
        "INSERT INTO accounts (user_name, token, balance) VALUES (?,?,?) RETURNING *",
        name,
        token,
        0.0
    )
    .fetch_one(db)
    .await?;

    sqlx::query!(
        "INSERT INTO logins (handle, password_hash, account_id) VALUES (?,?,?)",
        handle,
        "",
        new_account.id
    )
    .execute(db)
    .await?;

    let Json(resp) = crate::profile::change_password(
        State(state),
        Path(token),
        Json(ChangePasswordRequest {
            new_password: password,
        }),
    )
    .await?;
    let token = match resp {
        api::ChangePasswordResponse::Ok { new_token } => new_token,
        api::ChangePasswordResponse::InvalidToken => unreachable!(),
    };

    Ok(Json(token))
}

pub async fn reset_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(ResetPasswordRequest { handle, password }): Json<ResetPasswordRequest>,
) -> Result<Json<String>, AppError> {
    let db = &state.db;
    let h = headers.get("X-AuthToken");
    if h != Some(&HeaderValue::from_str(&std::env::var("SECRET_KEY").unwrap()).unwrap()) {
        Err(anyhow::anyhow!("Secret key invalid, got: {h:?}"))?
    }

    let data = match sqlx::query!(
        "SELECT * FROM accounts INNER JOIN logins ON logins.account_id=accounts.id WHERE handle=?",
        handle
    )
    .fetch_optional(db)
    .await?
    {
        Some(row) => row,
        None => return Err(anyhow::anyhow!("No such handle found"))?,
    };

    let old_token = data.token;

    let Json(resp) = crate::profile::change_password(
        State(state),
        Path(old_token),
        Json(ChangePasswordRequest {
            new_password: password,
        }),
    )
    .await?;
    let token = match resp {
        api::ChangePasswordResponse::Ok { new_token } => new_token,
        api::ChangePasswordResponse::InvalidToken => unreachable!(),
    };

    Ok(Json(token))
}
