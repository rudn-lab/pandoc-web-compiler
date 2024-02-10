use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub enum UserInfoResult {
    Ok(UserInfo),
    NoSuchToken,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UserInfo {
    pub name: String,
    pub balance: f64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PricingInfo {
    pub wall_time_factor: f64,
    pub user_time_factor: f64,
    pub sys_time_factor: f64,
    pub upload_mb_factor: f64,
    pub upload_file_factor: f64,
    pub storage_plans: Vec<StoragePlan>,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct StoragePlan {
    pub retention_seconds: u32,
    pub price_per_mb: f64,
}
