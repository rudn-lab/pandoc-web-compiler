use api::{UserInfo, UserInfoResult};
use axum::{
    extract::{Path, State},
    Json,
};

use crate::{result::AppError, AppState};

pub async fn get_user(
    State(AppState { db }): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<UserInfoResult>, AppError> {
    let data = match sqlx::query!("SELECT * FROM accounts WHERE token=?", token)
        .fetch_optional(&db)
        .await?
    {
        Some(v) => UserInfoResult::Ok(UserInfo {
            name: v.user_name,
            balance: v.balance,
        }),
        None => UserInfoResult::NoSuchToken,
    };

    Ok(Json(data))
}
