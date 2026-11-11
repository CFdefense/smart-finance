//! Account-related HTTP response models.

use chrono::NaiveDate;
use serde::Serialize;
use utoipa::ToSchema;

/// One recorded daily value of a user's linked-account net worth.
///
/// We store the total as well as the number of accounts
/// included or excluded from that calculation.
#[derive(Debug, Serialize, ToSchema)]
pub struct NetWorthSnapshot {
    /// Calendar date when the snapshot was recorded.
    pub snapshot_date: NaiveDate,

    /// Linked-account assets minus liabilities, in USD.
    pub amount: f64,

    /// Number of supported accounts used in this snapshot.
    pub included_accounts: i32,

    /// Accounts skipped due to unsupported type, currency,
    /// or unavailable balance.
    pub excluded_accounts: i32,

    /// True when this is a synthetic sandbox history point.
    ///
    /// The dashboard uses this to clearly identify
    /// demonstration history rather than genuine records.
    pub is_demo: bool,
}

/// Response containing the authenticated user's history.
#[derive(Debug, Serialize, ToSchema)]
pub struct NetWorthHistoryResponse {
    /// Historical values in chronological order.
    pub snapshots: Vec<NetWorthSnapshot>,
}
