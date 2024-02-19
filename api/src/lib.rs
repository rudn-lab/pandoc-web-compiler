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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PricingInfo {
    pub wall_time_factor: f64,
    pub cpu_time_factor: f64,
    pub upload_mb_factor: f64,
    pub upload_file_factor: f64,
    pub process_fork_cost: f64,
    pub overdraft_seconds_allowed: f64,
    pub error_order_cost: f64,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum OrderInfoResult {
    /// Either the order does not exist or you can't access it.
    NotAccessible,

    /// The order is currently being executed. Interact with it using the websocket connection.
    Running,

    /// The order is now completed.
    Completed(OrderInfoFull),
}

/// This record is stored in the database.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OrderInfo {
    pub balance_before: f64,
    pub order_cost: f64,
    pub pricing_applied: PricingInfo,
    pub termination: JobTerminationStatus,
}

/// This is returned in the API for requests about orders that are already done.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OrderInfoFull {
    pub record: OrderInfo,
    pub is_on_disk: bool,
    pub created_at_unix_time: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Default, Debug, PartialEq)]
pub struct OrderExecutionMetrics {
    pub cpu_seconds: f64,
    pub wall_seconds: f64,
    pub processes_forked: usize,
    pub uploaded_mb: f64,
    pub uploaded_files: usize,
    pub time_until_overdraft_stop: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OrderExecutionMetricsCosts {
    pub cpu_time: f64,
    pub wall_time: f64,
    pub processes: f64,
    pub upload_mb: f64,
    pub upload_files: f64,
}

impl OrderExecutionMetrics {
    pub fn calculate_costs(&self, pricing: &PricingInfo) -> OrderExecutionMetricsCosts {
        OrderExecutionMetricsCosts {
            cpu_time: self.cpu_seconds * pricing.cpu_time_factor,
            wall_time: self.wall_seconds * pricing.wall_time_factor,
            processes: self.processes_forked as f64 * pricing.process_fork_cost,
            upload_mb: self.uploaded_mb * pricing.upload_mb_factor,
            upload_files: self.uploaded_files as f64 * pricing.upload_file_factor,
        }
    }
}

impl OrderExecutionMetricsCosts {
    pub fn grand_total(&self) -> f64 {
        self.cpu_time + self.wall_time + self.processes + self.upload_files + self.upload_mb
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum JobTerminationStatus {
    /// Something unexpected happened, and the job terminated itself.
    AbnormalTermination(String),

    /// Something so unexpected happened, that the job terminated uncleanly, and the manager reaped it.
    VeryAbnormalTermination(String),

    /// The process has exited after consuming a particular amount of resources.
    ProcessExit {
        exit_code: i32,
        cause: TerminationCause,
        metrics: OrderExecutionMetrics,
        costs: OrderExecutionMetricsCosts,
    },
}

/// Why did the process exit?
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum TerminationCause {
    /// It terminated by itself
    NaturalTermination,

    /// Killed because of user request
    UserKill,

    /// Killed because of running out of money
    BalanceKill,
}

/// This represents the status of a running job.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum JobStatus {
    /// The job is preparing to start executing the makefile.
    Preparing,

    /// The job is currently executing the makefile, with the following metrics.
    Executing(OrderExecutionMetrics),

    /// The job is now terminated
    Terminated(JobTerminationStatus),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LiveStatus {
    pub status: JobStatus,
    pub pricing: PricingInfo,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OrderFileList(pub Vec<OrderFile>);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OrderFile {
    pub path: String,
    pub size_bytes: u64,
}
