use std::{cmp::min, io::Write, path::Path};

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use tempfile::NamedTempFile;
use tokio::task::block_in_place;
use tracing::{debug, info, instrument};

use crate::{
    client::{AccountsResult, CardsResult},
    TlClient,
};

pub async fn run_sync(
    tl: TlClient,
    from_date: NaiveDate,
    to_date: NaiveDate,
    target_dir: &Path,
) -> Result<()> {
    scrape_info(&tl, target_dir).await?;

    let accounts = scrape_accounts(&tl, target_dir).await?;

    for account in accounts {
        scrape_account_balance(&tl, target_dir, &account).await?;
        scrape_account_tx(&tl, target_dir, &account, from_date, to_date).await?;
    }

    let cards = scrape_cards(&tl, target_dir).await?;

    for card in cards {
        scrape_card_balance(&tl, target_dir, &card.account_id).await?;
        scrape_card_tx(&tl, target_dir, &card.account_id, from_date, to_date).await?;
    }

    Ok(())
}

#[instrument(skip(tl, target_dir))]
async fn scrape_info(tl: &TlClient, target_dir: &Path) -> Result<()> {
    let user_info = tl.fetch_info().await?;
    write_atomically(&target_dir.join("user-info.json"), &user_info).await?;
    Ok(())
}

#[instrument(skip(tl, target_dir))]
async fn scrape_accounts(tl: &TlClient, target_dir: &Path) -> Result<Vec<AccountsResult>> {
    let accounts = tl.fetch_accounts().await?;
    write_atomically(&target_dir.join("accounts.json"), &accounts).await?;
    Ok(accounts.results)
}

#[instrument(skip(tl, target_dir, account), fields(account_id=%account.account_id))]
async fn scrape_account_balance(
    tl: &TlClient,
    target_dir: &Path,
    account: &AccountsResult,
) -> Result<()> {
    info!("Fetch balance");
    let bal = tl.account_balance(&account.account_id).await?;
    let path = &target_dir
        .join("accounts")
        .join(&format!(
            "{} {}",
            account.account_number.sort_code, account.account_number.number
        ))
        .join("balance.json");
    write_atomically(path, &bal).await?;
    Ok(())
}

#[instrument(skip(tl, target_dir, account, from_date, to_date), fields(account_id=%account.account_id))]
async fn scrape_account_tx(
    tl: &TlClient,
    target_dir: &Path,
    account: &AccountsResult,
    from_date: NaiveDate,
    to_date: NaiveDate,
) -> Result<()> {
    info!(?from_date, ?to_date, "Fetch transactions");
    for (start_of_month, end_of_month) in months(from_date, to_date) {
        debug!(?start_of_month, ?end_of_month, "Scrape month");
        let mut txes = tl
            .account_transactions(&account.account_id, start_of_month, end_of_month)
            .await?;

        if txes.results.is_empty() {
            info!(?start_of_month, "No results for month found");
            continue;
        }

        txes.results
            .sort_by_key(|r| (r.timestamp, r.normalised_provider_transaction_id.clone()));
        write_atomically(
            &target_dir
                .join("accounts")
                .join(&format!(
                    "{} {}",
                    account.account_number.sort_code, account.account_number.number
                ))
                .join(start_of_month.format("%Y-%m.json").to_string()),
            &txes,
        )
        .await?;
    }
    Ok(())
}

#[instrument(skip(tl, target_dir))]
async fn scrape_cards(tl: &TlClient, target_dir: &Path) -> Result<Vec<CardsResult>> {
    let cards = tl.fetch_cards().await?;
    write_atomically(&target_dir.join("cards.json"), &cards).await?;
    Ok(cards.results)
}

#[instrument(skip(tl, target_dir))]
async fn scrape_card_balance(tl: &TlClient, target_dir: &Path, account_id: &str) -> Result<()> {
    info!("Fetch balance");
    let bal = tl.card_balance(account_id).await?;
    write_atomically(
        &target_dir
            .join("cards")
            .join(&account_id)
            .join("balance.json"),
        &bal,
    )
    .await?;
    Ok(())
}

#[instrument(skip(tl, target_dir))]
async fn scrape_card_tx(
    tl: &TlClient,
    target_dir: &Path,
    account_id: &str,
    from_date: NaiveDate,
    to_date: NaiveDate,
) -> Result<()> {
    info!("Fetch transactions");
    for (start_of_month, end_of_month) in months(from_date, to_date) {
        debug!("Scrape month");
        let mut txes = tl
            .card_transactions(account_id, start_of_month, end_of_month)
            .await?;

        if txes.results.is_empty() {
            info!(?start_of_month, "No results for month found");
            continue;
        }

        txes.results
            .sort_by_key(|r| (r.timestamp, r.normalised_provider_transaction_id.clone()));

        write_atomically(
            &target_dir
                .join("cards")
                .join(account_id)
                .join(start_of_month.format("%Y-%m.json").to_string()),
            &txes,
        )
        .await?;
    }
    Ok(())
}

fn months(
    from_date: NaiveDate,
    to_date: NaiveDate,
) -> impl Iterator<Item = (NaiveDate, NaiveDate)> {
    let month_start_date = from_date.with_day(1).expect("day one");

    let month_starts = month_start_date.iter_days().filter(|d| d.day() == 1);
    let month_ends = month_starts
        .clone()
        .skip(1)
        .map(move |d| min(d.pred(), to_date));
    month_starts
        .take_while(move |d| d <= &to_date)
        .zip(month_ends)
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
