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
    pub cpu_time_factor: f64,
    pub upload_mb_factor: f64,
    pub upload_file_factor: f64,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum OrderInfoResult {
    /// Either the order does not exist or you can't access it.
    NotAccessible,

    /// The order is currently being executed. Interact with it using the websocket connection.
    Running,

    /// The order is now completed.
    Completed(OrderInfo),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct OrderInfo {
    balance_before: f64,
    order_cost: f64,
    pricing_applied: PricingInfo,
}
