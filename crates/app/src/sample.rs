//! Sample ROs, shown until Xtime is connected (marked as samples on screen). They use the Xtime crate's own types,
//! so the real dashboard drops in without changes. Made-up vehicles and RO numbers; no real customer data.

use serde_json::json;
use xtime::dashboard::{AssignedRo, DashColumns};

fn ro(ro: &str, tag: &str, year: &str, make: &str, model: &str, vin: &str, mode: f64) -> AssignedRo {
    AssignedRo {
        ro: ro.into(),
        tag: tag.into(),
        year: year.into(),
        make: make.into(),
        model: model.into(),
        vin: vin.into(),
        mode: Some(mode),
        waiting_parts: false,
        waiting_customer: false,
        waiting_warranty: false,
        dash: DashColumns::default(),
    }
}

pub fn my_ros() -> Vec<AssignedRo> {
    let mut list = vec![
        ro("100101", "S1", "2026", "Ford", "Bronco", "SAMPLE0VIN0000001", 6.0),
        ro("100102", "S2", "2024", "Ford", "F-150", "SAMPLE0VIN0000002", 3.0),
        ro("100103", "S3", "2023", "Ford", "Explorer", "SAMPLE0VIN0000003", 4.0),
        ro("100104", "S4", "2025", "Ford", "Maverick", "SAMPLE0VIN0000004", 6.0),
        ro("100105", "S5", "2022", "Ford", "Escape", "SAMPLE0VIN0000005", 7.0),
    ];
    list[1].waiting_parts = true;
    list[2].waiting_customer = true;
    list[0].dash.customer = Some(json!("Sample"));
    list[0].dash.advisor_name = "Sample Advisor".into();
    list[0].dash.waiter = true;
    list
}