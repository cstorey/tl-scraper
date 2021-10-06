use std::{cmp::min, io::Write, path::Path};

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use tempfile::NamedTempFile;
use tokio::task::block_in_place;
use tracing::{debug, info};

use crate::{client::AccountsResult, TlClient};

pub async fn run_sync(
    tl: TlClient,
    from_date: NaiveDate,
    to_date: NaiveDate,
    target_dir: &Path,
) -> Result<()> {
    scrape_info(&tl, target_dir).await?;

    let accounts = scrape_accounts(&tl, target_dir).await?;

    for account in accounts {
        scrape_account(&tl, target_dir, &account.account_id).await?;
        scrape_account_tx(&tl, target_dir, &account.account_id, from_date, to_date).await?;
    }

    Ok(())
}

async fn scrape_info(tl: &TlClient, target_dir: &Path) -> Result<()> {
    let user_info = tl.fetch_info().await?;
    write_atomically(&target_dir.join("user-info.json"), &user_info).await?;
    Ok(())
}

async fn scrape_accounts(tl: &TlClient, target_dir: &Path) -> Result<Vec<AccountsResult>> {
    let accounts = tl.fetch_accounts().await?;
    write_atomically(&target_dir.join("accounts.json"), &accounts).await?;
    Ok(accounts.results)
}

async fn scrape_account(tl: &TlClient, target_dir: &Path, account_id: &str) -> Result<()> {
    info!(%account_id, "Fetch balance");
    let bal = tl.account_balance(account_id).await?;
    write_atomically(
        &target_dir
            .join("accounts")
            .join(&account_id)
            .join("balance.json"),
        &bal,
    )
    .await?;
    Ok(())
}

async fn scrape_account_tx(
    tl: &TlClient,
    target_dir: &Path,
    account_id: &str,
    from_date: NaiveDate,
    to_date: NaiveDate,
) -> Result<()> {
    info!(%account_id, ?from_date, ?to_date, "Fetch transactions");
    let month_start_date = from_date.with_day(1).expect("day one");

    let month_starts = month_start_date.iter_days().filter(|d| d.day() == 1);
    for (start_of_month, end_of_month) in month_starts
        .clone()
        .filter(|d| d <= &to_date)
        .zip(month_starts.skip(1).map(|d| min(d.pred(), to_date)))
    {
        debug!(%account_id, ?start_of_month, ?end_of_month, "Scrape month");
        let txes = tl
            .account_transactions(account_id, start_of_month, end_of_month)
            .await?;
        write_atomically(
            &target_dir
                .join("accounts")
                .join(account_id)
                .join(start_of_month.format("%Y-%m.json").to_string()),
            &txes,
        )
        .await?;
    }
    Ok(())
}

async fn write_atomically<T: Serialize>(path: &Path, data: &T) -> Result<()> {
    block_in_place(|| {
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(dir)?;
        let mut tmpf = NamedTempFile::new_in(dir)?;
        serde_json::to_writer_pretty(&mut tmpf, &data)?;
        tmpf.as_file_mut().flush()?;
        tmpf.persist(path)?;
        debug!(?path, "Stored data");
        Ok(())
    })
}
