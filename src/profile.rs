use api::{
    ChangePasswordRequest, ChangePasswordResponse, LoginRequest, RedeemPromocodeResponse, UserInfo,
    UserInfoResult,
};
use axum::{
    extract::{Path, State},
    Json,
};
use base64::Engine;
use password_hash::PasswordHashString;

use crate::{result::AppError, AppState};

pub async fn get_user(
    State(AppState { db, .. }): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<UserInfoResult>, AppError> {
    let data = match sqlx::query!("SELECT * FROM accounts WHERE token=?", token)
        .fetch_optional(&db)
        .await?
    {
        Some(v) => UserInfoResult::Ok(UserInfo {
            name: v.user_name,
            balance: v.balance,
            verification: v.verification_method.into(),
        }),
        None => UserInfoResult::NoSuchToken,
    };

    Ok(Json(data))
}

#[axum_macros::debug_handler]
pub async fn login(
    State(AppState { db, .. }): State<AppState>,
    Json(LoginRequest { handle, password }): Json<LoginRequest>,
) -> Result<Json<Option<String>>, AppError> {
    // First fetch the login corresponding to the handle.
    let login = match sqlx::query!("SELECT * FROM logins WHERE handle=?", handle)
        .fetch_optional(&db)
        .await?
    {
        Some(row) => row,
        None => return Ok(Json(None)),
    };

    // Load the stored hashed password
    let true_hash = PasswordHashString::new(&login.password_hash)
        .expect("Database stored password hash string invalid");

    /// This is separated into a function because `&dyn PasswordVerifier` is not [`Sync`];
    /// because of this, such objects may not exist across await points.
    /// This is a fully-sync function so it avoids that issue.
    fn check_password(true_hash: PasswordHashString, tested_password: String) -> bool {
        let hashers: &[&dyn password_hash::PasswordVerifier] = &[&argon2::Argon2::default()];
        true_hash
            .password_hash()
            .verify_password(hashers, tested_password)
            .is_ok()
    }

    if !check_password(true_hash, password) {
        return Ok(Json(None));
    }

    // At this time, we know that the password is correct
    // Rotate the token for the associated account, then return it.
    use rand::distributions::DistString;
    let token = rand::distributions::Alphanumeric.sample_string(&mut rand::thread_rng(), 32);

    sqlx::query!(
        "UPDATE accounts SET token=? WHERE id=?",
        token,
        login.account_id
    )
    .execute(&db)
    .await?;

    Ok(Json(Some(token)))
}

pub async fn change_password(
    State(AppState { db, .. }): State<AppState>,
    Path(token): Path<String>,
    Json(ChangePasswordRequest { new_password }): Json<ChangePasswordRequest>,
) -> Result<Json<ChangePasswordResponse>, AppError> {
    // First find the account corresponding to the token
    let data = match sqlx::query!("SELECT * FROM accounts WHERE token=?", token)
        .fetch_optional(&db)
        .await?
    {
        Some(row) => row,
        None => return Ok(Json(ChangePasswordResponse::InvalidToken)),
    };

    // Now make a new password hash
    let salt = uuid::Uuid::new_v4();
    let salt = salt.as_bytes();
    let salt = base64::prelude::BASE64_STANDARD_NO_PAD.encode(&salt);
    let hash = password_hash::PasswordHash::generate(
        argon2::Argon2::default(),
        new_password.as_bytes(),
        password_hash::Salt::from_b64(&salt).expect("Failed to parse generated salt"),
    )
    .expect("Failed to hash new user password");
    let hash_str = hash.to_string();

    // Store it into the database with the user's data

    sqlx::query!(
        "UPDATE logins SET password_hash=? WHERE account_id=?",
        hash_str,
        data.id
    )
    .execute(&db)
    .await?;

    // Rotate the account token
    use rand::distributions::DistString;
    let token = rand::distributions::Alphanumeric.sample_string(&mut rand::thread_rng(), 32);

    sqlx::query!("UPDATE accounts SET token=? WHERE id=?", token, data.id)
        .execute(&db)
        .await?;

    Ok(Json(ChangePasswordResponse::Ok { new_token: token }))
}

pub async fn redeem_promocode(
    State(AppState { db, .. }): State<AppState>,
    Path((token, code)): Path<(String, String)>,
) -> Result<Json<RedeemPromocodeResponse>, AppError> {
    // First find the account corresponding to the token
    let account = match sqlx::query!("SELECT * FROM accounts WHERE token=?", token)
        .fetch_optional(&db)
        .await?
    {
        Some(row) => row,
        None => return Err(anyhow::anyhow!("User token is invalid"))?,
    };

    // Then find the promocode
    let promocode = match sqlx::query!("SELECT * FROM promocodes WHERE code=?", code)
        .fetch_optional(&db)
        .await?
    {
        Some(row) => row,
        None => return Ok(Json(RedeemPromocodeResponse::NotFound)),
    };

    // Check if it's been claimed.
    if let (Some(by_id), Some(when)) = (promocode.claimed_by, promocode.claimed_at_unix_time) {
        return Ok(Json(RedeemPromocodeResponse::AlreadyRedeemed {
            when_unix_time: when as u64,
            by_me: by_id == account.id,
        }));
    }

    // Transactionally alter the balance, and also mark the promocode as claimed.
    let mut tx = db.begin().await?;

    let user_balance_after = sqlx::query!(
        "UPDATE accounts SET balance=balance+? WHERE id=? RETURNING balance",
        promocode.money_value,
        account.id
    )
    .fetch_one(&mut *tx)
    .await?
    .balance;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    sqlx::query!(
        "UPDATE promocodes SET claimed_by=?, claimed_at_unix_time=? WHERE id=?",
        account.id,
        now,
        promocode.id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Json(RedeemPromocodeResponse::Ok {
        promocode_value: promocode.money_value as f64,
        user_balance_after,
    }))
}
