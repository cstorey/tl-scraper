use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, Utc};
use clap::Parser;
use color_eyre::{eyre::eyre, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument};
use uuid::Uuid;

use crate::{auth::load_token, client::BankDataClient, connect::Requisition};

#[derive(Debug, Parser)]
pub struct ListCmd {
    #[clap(short = 't', long = "token", help = "Token file")]
    token: PathBuf,
    #[clap(short = 'r', long = "requisition-id", help = "Requisition ID")]
    requisition_id: Uuid,
}

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

impl ListCmd {
    #[instrument("list-accounts", skip_all, fields(
        token = ?self.token,
        requisition_id = %self.requisition_id,
    ))]
    pub(crate) async fn run(&self) -> Result<()> {
        let token = load_token(&self.token).await?;

        let client = BankDataClient::new(token);

        let requisition = client
            .get::<Requisition>(&format!("/api/v2/requisitions/{}/", self.requisition_id))
            .await?;

        debug!(?requisition, "Got requisition",);

        if !requisition.is_linked() {
            return Err(eyre!("Requisition not linked"));
        }

        for acc in requisition.accounts.iter().cloned() {
            self.list_account(&client, acc).await?;
        }
        Ok(())
    }

    #[instrument(skip_all,fields(%account_id))]
    async fn list_account(&self, client: &BankDataClient, account_id: Uuid) -> Result<()> {
        let details = client
            .get::<Account>(&format!("/api/v2/accounts/{}/", account_id))
            .await?;

        info!(%account_id, "Account details: {}", serde_json::to_string_pretty(&details)?);
        let details = client
            .get::<Balances>(&format!("/api/v2/accounts/{}/balances/", account_id,))
            .await?;

        info!(%account_id, "Account details: {}", serde_json::to_string_pretty(&details)?);

        Ok(())
    }
}
