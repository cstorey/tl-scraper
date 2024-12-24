use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub(crate) struct TransactionsQuery {
    pub(crate) date_from: NaiveDate,
    pub(crate) date_to: NaiveDate,
}

#[derive(Debug, Serialize, Deserialize, Default)]

pub(crate) struct Transactions {
    pub(crate) transactions: TransactionsInner,
    #[serde(flatten)]
    pub(crate) other: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Default)]

pub(crate) struct TransactionsInner {
    pub(crate) booked: Vec<Transaction>,
    pub(crate) pending: Vec<Transaction>,
    #[serde(flatten)]
    pub(crate) other: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Transaction {
    #[serde(
        rename = "bookingDate",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) booking_date: Option<NaiveDate>,
    #[serde(
        rename = "bookingDateTime",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) booking_date_time: Option<DateTime<Utc>>,
    #[serde(rename = "valueDate", default, skip_serializing_if = "Option::is_none")]
    pub(crate) value_date: Option<NaiveDate>,
    #[serde(flatten)]
    pub(crate) other: serde_json::Value,
}
