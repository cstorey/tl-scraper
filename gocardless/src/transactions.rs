use std::path::PathBuf;

use chrono::NaiveDate;
use clap::Parser;
use color_eyre::{eyre::eyre, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument};
use uuid::Uuid;

use crate::{auth::load_token, client::BankDataClient, connect::Requisition};

#[derive(Debug, Parser)]
pub struct Cmd {
    #[clap(short = 't', long = "token", help = "Token file")]
    token: PathBuf,
    #[clap(short = 'r', long = "requisition-id", help = "Requisition ID")]
    requisition_id: Uuid,
    #[clap(short = 's', long = "start-date", help = "Start Date")]
    start_date: NaiveDate,
    #[clap(short = 'e', long = "end-date", help = "End Date")]
    end_date: NaiveDate,
}

#[derive(Debug, Serialize)]
struct TransactionsQuery {
    date_from: NaiveDate,
    date_to: NaiveDate,
}

#[derive(Debug, Serialize, Deserialize)]

struct Transactions {
    pub(crate) transactions: TransactionsInner,
    #[serde(flatten)]
    pub(crate) other: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]

struct TransactionsInner {
    pub(crate) booked: Vec<Transaction>,
    pub(crate) pending: Vec<Transaction>,
    #[serde(flatten)]
    pub(crate) other: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct Transaction {
    #[serde(flatten)]
    pub(crate) other: serde_json::Value,
}

impl Cmd {
    #[instrument("list-transactions", skip_all, fields(
        token = ?self.token,
        requisition_id = %self.requisition_id,
        start_date = %self.start_date,
        end_date = %self.end_date,
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
            self.list_account(&client, acc, self.start_date, self.end_date)
                .await?;
        }
        Ok(())
    }

    #[instrument(skip_all,fields(%account_id, %start_date, %end_date))]
    async fn list_account(
        &self,
        client: &BankDataClient,
        account_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<()> {
        let transactions = client
            .get::<Transactions>(&format!(
                "/api/v2/accounts/{}/transactions/?{}",
                account_id,
                serde_urlencoded::to_string(TransactionsQuery {
                    date_from: start_date,
                    date_to: end_date
                })?
            ))
            .await?;

        info!(%account_id, "Account details: {}", serde_json::to_string_pretty(&transactions)?);

        Ok(())
    }
}
