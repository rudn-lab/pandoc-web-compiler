use api::{UserInfo, UserInfoResult};
use axum::extract::{Multipart, Path, State};

use crate::{result::AppError, AppState};

pub async fn upload_order(
    State(AppState { db }): State<AppState>,
    Path(token): Path<String>,
    mut files: Multipart,
) -> Result<String, AppError> {
    let _data = match sqlx::query!("SELECT * FROM accounts WHERE token=?", token)
        .fetch_optional(&db)
        .await?
    {
        Some(v) => UserInfoResult::Ok(UserInfo {
            name: v.user_name,
            balance: v.balance,
        }),
        None => Err(anyhow::anyhow!("No such token found"))?,
    };

    let mut what_uploaded = String::from("Uploaded these:\n");
    while let Some(field) = files.next_field().await.unwrap() {
        let name = field.name().unwrap().to_string();
        let data = field.bytes().await.unwrap();

        what_uploaded.push_str(&format!("{name}: {} bytes\n", data.len()));
    }

    Ok(what_uploaded)
}
