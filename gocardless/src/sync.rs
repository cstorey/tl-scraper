use std::path::PathBuf;

use chrono::NaiveDate;
use clap::Parser;
use color_eyre::{eyre::eyre, Result};
use serde::Serialize;
use tokio::io::AsyncWriteExt;
use tracing::{debug, instrument};
use uuid::Uuid;

use crate::{
    accounts::{Account, Balances},
    auth::load_token,
    client::BankDataClient,
    connect::Requisition,
    transactions::{Transactions, TransactionsQuery},
};

#[derive(Debug, Parser)]
pub struct Cmd {
    #[clap(short = 't', long = "token", help = "Token file")]
    token: PathBuf,
    #[clap(short = 'o', long = "output", help = "Output path")]
    output: PathBuf,
    #[clap(short = 'r', long = "requisition-id", help = "Requisition ID")]
    requisition_id: Uuid,
    #[clap(short = 's', long = "start-date", help = "Start Date")]
    start_date: NaiveDate,
    #[clap(short = 'e', long = "end-date", help = "End Date")]
    end_date: NaiveDate,
}
impl Cmd {
    #[instrument("sync", skip_all, fields(
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

        self.write_file("account-details.json", &details).await?;

        let details = client
            .get::<Balances>(&format!("/api/v2/accounts/{}/balances/", account_id,))
            .await?;

        self.write_file("balances.json", &details).await?;

        let transactions = client
            .get::<Transactions>(&format!(
                "/api/v2/accounts/{}/transactions/?{}",
                account_id,
                serde_urlencoded::to_string(TransactionsQuery {
                    date_from: self.start_date,
                    date_to: self.end_date,
                })?
            ))
            .await?;

        self.write_file("transactions.json", &transactions).await?;

        Ok(())
    }

    #[instrument(skip_all, fields(?path))]
    async fn write_file(
        &self,
        path: &str,
        data: impl Serialize,
    ) -> Result<(), color_eyre::eyre::Error> {
        let path = self.output.join(path);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut of = tokio::fs::File::create(&path).await?;
        let buf = serde_json::to_string_pretty(&data)?;
        of.write_all(buf.as_bytes()).await?;
        of.flush().await?;

        debug!(size=%buf.len(), ?path, "Wrote data to file");

        Ok(())
    }
}
