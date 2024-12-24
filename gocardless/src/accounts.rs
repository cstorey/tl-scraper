use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Account {
    pub(crate) id: Uuid,
    pub(crate) created: DateTime<Utc>,
    pub(crate) last_accessed: Option<DateTime<Utc>>,
    pub(crate) iban: String,
    pub(crate) status: String,
    pub(crate) institution_id: String,
    pub(crate) owner_name: String,
    #[serde(flatten)]
    pub(crate) other: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Balances {
    pub(crate) balances: Vec<Balance>,
    #[serde(flatten)]
    pub(crate) other: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Balance {
    #[serde(rename = "balanceAmount")]
    pub(crate) balance_amount: BalanceAmount,
    #[serde(rename = "balanceType")]
    pub(crate) balance_type: String,

    #[serde(rename = "creditLimitIncluded", default)]
    pub(crate) credit_limit_included: Option<bool>,
    #[serde(rename = "lastChangeDateTime", default)]
    pub(crate) last_change: Option<DateTime<Utc>>,
    #[serde(rename = "referenceDate")]
    pub(crate) reference_date: NaiveDate,

    #[serde(flatten)]
    pub(crate) other: serde_json::Value,
}
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct BalanceAmount {
    pub(crate) amount: Decimal,
    pub(crate) currency: String,
    #[serde(flatten)]
    pub(crate) other: serde_json::Value,
}
