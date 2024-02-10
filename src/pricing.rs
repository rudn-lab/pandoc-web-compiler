use api::{PricingInfo, StoragePlan};

pub fn get_current_pricing() -> PricingInfo {
    PricingInfo {
        user_time_factor: 100.0,
        sys_time_factor: 25.0,
        wall_time_factor: 5.0,
        upload_mb_factor: 200.0,
        upload_file_factor: 1.0,
        storage_plans: vec![
            StoragePlan {
                price_per_mb: 0.0,
                retention_seconds: 5 * 60,
            },
            StoragePlan {
                price_per_mb: 200.0,
                retention_seconds: 60 * 60,
            },
        ],
    }
}
