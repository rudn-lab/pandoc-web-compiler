use api::{
    verification::{check_hash_difficulty, ProofOfWorkAttempt, ProofOfWorkChallenge},
    VerificationMethod,
};
use axum::{
    extract::{Path, State},
    Json,
};
use itsdangerous::Signer;
use rand::Rng;

use crate::{result::AppError, AppState};

// Salt is OK to be hardcoded: https://itsdangerous.palletsprojects.com/en/2.2.x/concepts/#the-salt
const POW_SALT: &str = "pandoc-proof-of-work";

pub async fn get_challenge() -> String {
    let secret_key = std::env::var("SECRET_KEY").unwrap();
    let mut rng = rand::thread_rng();
    let nonce: [u8; 32] = rng.gen();
    let challenge = ProofOfWorkChallenge {
        nonce: nonce.into(),
        difficulty: 10,
        algorithm: api::verification::ProofOfWorkAlgorithm::Sha256,
    };

    let json_text = serde_json::to_string(&challenge).unwrap();
    let itsdangerous_signer = itsdangerous::default_builder(secret_key)
        .with_salt(POW_SALT)
        .build();

    let output = itsdangerous_signer.sign(json_text);

    output
}

pub async fn verify_challenge(
    State(AppState { db, .. }): State<AppState>,
    Path(token): Path<String>,
    Json(ProofOfWorkAttempt {
        challenge_string,
        nonce_postfix,
    }): Json<ProofOfWorkAttempt>,
) -> Result<String, AppError> {
    // We're only expecting valid attempts, so any errors can be returned as AppErrors.
    // The official client will retry in that case.

    // Extract and verify the challenge
    let secret_key = std::env::var("SECRET_KEY").unwrap();
    let itsdangerous_signer = itsdangerous::default_builder(secret_key).build();
    let challenge = itsdangerous_signer.unsign(&challenge_string);
    let challenge = match challenge {
        Ok(c) => c,
        Err(why) => Err(anyhow::anyhow!(
            "Failed to decode challenge string: {}",
            why
        ))?,
    };

    let challenge_struct: ProofOfWorkChallenge = serde_json::from_str(&challenge)
        .map_err(|why| anyhow::anyhow!("failed to parse challenge: {why}; this is a bug!"))?;

    // Verify the challenge
    match challenge_struct.algorithm {
        api::verification::ProofOfWorkAlgorithm::Sha256 => {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&challenge_struct.nonce);
            hasher.update(&nonce_postfix);
            let result = hasher.finalize();
            let result = &result[..];

            let target_difficulty = challenge_struct.difficulty;
            let solved_difficulty = check_hash_difficulty(result);
            if target_difficulty > solved_difficulty {
                return Err(anyhow::anyhow!("Proof of work not satisfied: required difficulty: {target_difficulty}, solved difficulty: {solved_difficulty}"))?;
            }
        }
    }

    // Check if the account exists
    let account = sqlx::query!("SELECT * FROM accounts WHERE token=?", token)
        .fetch_optional(&db)
        .await?;
    let account = match account {
        Some(a) => a,
        None => return Err(anyhow::anyhow!("Invalid token"))?,
    };

    // If the account is already verified, no need to verify it again
    if account.verification_method.is_some() {
        return Err(anyhow::anyhow!(
            "Account already verified by method {:?}",
            account.verification_method
        ))?;
    }

    sqlx::query!(
        "UPDATE accounts SET verification_method=? WHERE token=?",
        VerificationMethod::ProofOfWork as i64,
        token
    )
    .execute(&db)
    .await?;

    Ok("success".to_string())
}
