//! Response shapes that exist only so the OpenAPI document can describe them. They are never
//! constructed: the handlers pass a computed `serde_json::Value` through. Hence `dead_code`.

#![allow(dead_code)]

use serde::Serialize;

// The shapes below document what `connectors::clients::compute_*` builds with
// `json!`; they are schema only (the handlers pass the computed value through).
#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct FinanceAccount {
    name: String,
    balance: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct SpendByCategory {
    category: String,
    amount: f64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct SpendByDay {
    day: String,
    amount: f64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct FinanceTransaction {
    description: String,
    amount: String,
    created_at: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct FinanceSummary {
    balance: f64,
    accounts: Vec<FinanceAccount>,
    spend_by_category: Vec<SpendByCategory>,
    spend_by_day: Vec<SpendByDay>,
    recent_transactions: Vec<FinanceTransaction>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct WeekSummary {
    transaction_count: i64,
    spent: f64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct TagCount {
    tag: String,
    count: i64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct RecentRecording {
    title: String,
    duration_minutes: f64,
    recorded_at: Option<String>,
    tags: Vec<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct PocketaiSummary {
    recordings_count: i64,
    total_duration_minutes: f64,
    tag_breakdown: Vec<TagCount>,
    recent_recordings: Vec<RecentRecording>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct PocketaiWeek {
    recordings_count: i64,
}

/// Each side is null when that connector is not connected or its call failed.
#[derive(Serialize, utoipa::ToSchema)]
pub(super) struct SnapshotOut {
    up_bank: Option<WeekSummary>,
    pocketai: Option<PocketaiWeek>,
}
