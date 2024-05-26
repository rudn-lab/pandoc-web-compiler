use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum ProofOfWorkAlgorithm {
    Sha256,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ProofOfWorkChallenge {
    pub nonce: Vec<u8>,
    pub difficulty: u64,
    pub algorithm: ProofOfWorkAlgorithm,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ProofOfWorkAttempt {
    pub challenge_string: String,
    pub nonce_postfix: Vec<u8>,
}

/// Compute the difficulty of the PoW hash
pub fn check_hash_difficulty(hash: &[u8]) -> u64 {
    // Count the number of bits in front, which are equal to zero
    let mut count: u64 = 0;
    let mut iter = hash.iter();
    while let Some(b) = iter.next() {
        // If the current byte is zero, increase the count by 8
        if *b == 0 {
            count += 8;
        } else {
            // If the current byte is not zero, count the number of bits in front that are equal to zero,
            // then break out of the loop
            count += b.leading_zeros() as u64;
            break;
        }
    }
    count
}
