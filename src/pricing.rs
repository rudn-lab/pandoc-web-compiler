use api::PricingInfo;

pub fn get_current_pricing() -> PricingInfo {
    PricingInfo {
        cpu_time_factor: 100.0,
        process_fork_cost: 1000.0,
        wall_time_factor: 5.0,
        upload_mb_factor: 50.0,
        upload_file_factor: 0.5,
        overdraft_seconds_allowed: 60.0,
        error_order_cost: 100.0,
    }
}
