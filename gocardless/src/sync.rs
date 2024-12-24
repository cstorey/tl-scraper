use std::{cmp, path::PathBuf};

use chrono::{Datelike, Days, Local, Months, NaiveDate};
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
    #[clap(
        short = 'd',
        long = "historical-days",
        help = "Number of days we can access",
        default_value_t = 90
    )]
    history_days: u64,
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

        let end_date = Local::now().date_naive();
        let mut start_date = end_date - Days::new(self.history_days);
        if start_date.day() > 1 {
            start_date = start_date + Months::new(1);
            start_date = start_date - Days::new(start_date.day0().into());
        }
        debug!(%start_date, %end_date, "Scanning date range");

        for acc in requisition.accounts.iter().cloned() {
            self.list_account(&client, acc, start_date, end_date)
                .await?;
        }
        Ok(())
    }

    #[instrument(skip_all,fields(%account_id))]
    async fn list_account(
        &self,
        client: &BankDataClient,
        account_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<()> {
        let details = client
            .get::<Account>(&format!("/api/v2/accounts/{}/", account_id))
            .await?;

        self.write_file("account-details.json", &details).await?;

        let details = client
            .get::<Balances>(&format!("/api/v2/accounts/{}/balances/", account_id,))
            .await?;

        self.write_file("balances.json", &details).await?;

        let ranges = (0..)
            .map(|n| start_date + Months::new(n))
            .take_while(|d| d <= &end_date)
            .map(|month_start| {
                let month_end = (month_start + Months::new(1))
                    .pred_opt()
                    .expect("previous day");
                (month_start, cmp::min(month_end, end_date))
            });

        for (start, end) in ranges {
            debug!(%start, %end, "scanning month");
            let transactions = client
                .get::<Transactions>(&format!(
                    "/api/v2/accounts/{}/transactions/?{}",
                    account_id,
                    serde_urlencoded::to_string(&TransactionsQuery {
                        date_from: start,
                        date_to: end,
                    })?
                ))
                .await?;

            let path = start.format("%Y-%m.json").to_string();
            self.write_file(&path, &transactions).await?;
        }

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
