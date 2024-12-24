use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub(crate) struct TransactionsQuery {
    pub(crate) date_from: NaiveDate,
    pub(crate) date_to: NaiveDate,
}

#[derive(Debug, Serialize, Deserialize)]

pub(crate) struct Transactions {
    pub(crate) transactions: TransactionsInner,
    #[serde(flatten)]
    pub(crate) other: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]

pub(crate) struct TransactionsInner {
    pub(crate) booked: Vec<Transaction>,
    pub(crate) pending: Vec<Transaction>,
    #[serde(flatten)]
    pub(crate) other: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Transaction {
    #[serde(flatten)]
    pub(crate) other: serde_json::Value,
}
