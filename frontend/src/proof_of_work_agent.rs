use api::verification::check_hash_difficulty;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use wasm_bindgen::prelude::*;
use yew_agent::reactor::{reactor, ReactorScope};

#[derive(Clone, Debug, Serialize, Deserialize, Hash)]
pub enum HashMethod {
    Sha256,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PowReactorCommand {
    Input(PowReactorInput),
    Stop,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PowReactorInput {
    pub difficulty: u64,
    pub nonce: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PowReactorOutput {
    Progress {
        attempts_done: usize,
        max_difficulty_found: u64,
        hashes_per_second: f64,
    },
    FoundSolution {
        nonce: Vec<u8>,
    },
}

// #[wasm_bindgen(module = "node_modules/hash-wasm/dist/index.umd.js")]
// //#[wasm_bindgen]
// extern "C" {
//     async fn sha256(data: &[u8]) -> JsValue;
// }

fn increment_byte_slice(data: &mut [u8]) {
    for i in (0..data.len()).rev() {
        if data[i] == 255 {
            data[i] = 0;
        } else {
            data[i] += 1;
            break;
        }
    }
}

#[reactor(Sha256PowReactor)]
pub async fn sha256_proof_of_work_agent(
    mut scope: ReactorScope<PowReactorCommand, PowReactorOutput>,
) {
    loop {
        // Wait for reactor input
        let mut input = None;
        while let Some(cmd) = scope.next().await {
            match cmd {
                PowReactorCommand::Input(data) => {
                    input = Some(data);
                    break;
                }
                _ => {}
            }
        }
        let input = if let Some(i) = input {
            i
        } else {
            return;
        };

        let mut hasher_original = sha2::Sha256::new();
        hasher_original.update(input.nonce);

        let mut attempts_done: usize = 0;

        let mut my_attempt = vec![0u8; std::mem::size_of_val(&0usize)];

        let mut max_difficulty_found = 0;
        let mut last_time = web_time::Instant::now();
        'solution_loop: loop {
            futures::select! {
                m = scope.next() => match m {
                    Some(PowReactorCommand::Stop) => break,
                    _ => {}
                },
                _ = yew::platform::time::sleep(std::time::Duration::from_millis(0)).fuse() => {
                }
            }
            let loop_attempts = 1000;
            for _ in 0..loop_attempts {
                attempts_done += 1;
                increment_byte_slice(&mut my_attempt);

                let mut hasher = hasher_original.clone();
                hasher.update(&my_attempt);
                let hash = hasher.finalize();
                let hash = &hash[..];

                let difficulty = check_hash_difficulty(&hash);
                if difficulty >= input.difficulty {
                    scope
                        .send(PowReactorOutput::FoundSolution { nonce: my_attempt })
                        .await
                        .expect("failed to send answer");
                    break 'solution_loop;
                }
                if difficulty > max_difficulty_found {
                    max_difficulty_found = difficulty;
                }
            }

            let hashes_per_second = loop_attempts as f64 / last_time.elapsed().as_secs_f64();
            scope
                .send(PowReactorOutput::Progress {
                    attempts_done,
                    max_difficulty_found,
                    hashes_per_second,
                })
                .await
                .expect("failed to send progress");

            last_time = web_time::Instant::now();
        }
    }
}
