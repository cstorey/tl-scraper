use std::{cmp::min, io::Write, path::Path, sync::Arc};

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

pub async fn sync_accounts(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
    from_date: NaiveDate,
    to_date: NaiveDate,
) -> Result<(), anyhow::Error> {
    let accounts = scrape_accounts(tl.clone(), target_dir.clone()).await?;
    for account in accounts {
        scrape_account_balance(tl.clone(), target_dir.clone(), &account).await?;
        scrape_account_pending(tl.clone(), target_dir.clone(), &account).await?;
        scrape_account_tx(tl.clone(), target_dir.clone(), &account, from_date, to_date).await?;

        if false {
            // Only available when you've _recently_ authenticated.
            scrape_account_standing_orders(tl.clone(), target_dir.clone(), &account).await?;
            scrape_account_direct_debits(tl.clone(), target_dir.clone(), &account).await?;
        }
    }
    Ok(())
}

pub async fn sync_cards(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
    from_date: NaiveDate,
    to_date: NaiveDate,
) -> Result<(), anyhow::Error> {
    let cards = scrape_cards(tl.clone(), target_dir.clone()).await?;
    for card in cards {
        scrape_card_balance(tl.clone(), target_dir.clone(), &card.account_id).await?;
        scrape_card_pending(tl.clone(), target_dir.clone(), &card.account_id).await?;
        scrape_card_tx(
            tl.clone(),
            target_dir.clone(),
            &card.account_id,
            from_date,
            to_date,
        )
        .await?;
    }
    Ok(())
}

#[instrument(skip(tl, target_dir))]
pub async fn sync_info(tl: Arc<TlClient>, target_dir: Arc<Path>) -> Result<()> {
    let user_info = tl.fetch_info().await?;
    write_jsons_atomically(&target_dir.join("user-info.jsons"), &user_info.results).await?;
    Ok(())
}

#[instrument(skip(tl, target_dir))]
async fn scrape_accounts(tl: Arc<TlClient>, target_dir: Arc<Path>) -> Result<Vec<AccountsResult>> {
    let accounts = tl.fetch_accounts().await?;
    for account in accounts.results.iter() {
        let path = target_dir
            .join("accounts")
            .join(account_dir_name(account))
            .join("account.jsons");
        write_jsons_atomically(&path, &[account]).await?;
    }
    Ok(accounts.results)
}

#[instrument(skip(tl, target_dir, account), fields(account_id=%account.account_id))]
async fn scrape_account_balance(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
    account: &AccountsResult,
) -> Result<()> {
    info!("Fetch balance");
    let bal = tl.account_balance(&account.account_id).await?;
    let path = &target_dir
        .join("accounts")
        .join(account_dir_name(account))
        .join("balance.jsons");
    write_jsons_atomically(path, &bal.results).await?;
    Ok(())
}

fn account_dir_name(account: &AccountsResult) -> String {
    let account_path = if let (Some(sort_code), Some(number)) = (
        account.account_number.sort_code.as_ref(),
        account.account_number.number.as_ref(),
    ) {
        format!("{} {}", sort_code, number)
    } else {
        account.account_id.clone()
    };
    account_path
}

#[instrument(skip(tl, target_dir, account), fields(account_id=%account.account_id))]
async fn scrape_account_pending(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
    account: &AccountsResult,
) -> Result<()> {
    info!("Fetch pending transactions");
    let bal = tl.account_pending(&account.account_id).await?;
    let path = &target_dir
        .join("accounts")
        .join(account_dir_name(account))
        .join("pending.jsons");
    write_jsons_atomically(path, &bal.results).await?;
    Ok(())
}

#[instrument(skip(tl, target_dir, account), fields(account_id=%account.account_id))]
async fn scrape_account_standing_orders(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
    account: &AccountsResult,
) -> Result<()> {
    info!("Fetch standing orders");
    let bal = tl.account_standing_orders(&account.account_id).await?;
    let path = &target_dir
        .join("accounts")
        .join(account_dir_name(account))
        .join("standing-orders.jsons");
    write_jsons_atomically(path, &bal.results).await?;
    Ok(())
}

#[instrument(skip(tl, target_dir, account), fields(account_id=%account.account_id))]
async fn scrape_account_direct_debits(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
    account: &AccountsResult,
) -> Result<()> {
    info!("Fetch direct debits");
    let bal = tl.account_direct_debits(&account.account_id).await?;
    let path = &target_dir
        .join("accounts")
        .join(account_dir_name(account))
        .join("standing-orders.jsons");
    write_jsons_atomically(path, &bal.results).await?;
    Ok(())
}

#[instrument(skip(tl, target_dir, account, from_date, to_date), fields(account_id=%account.account_id))]
async fn scrape_account_tx(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
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

        txes.results.reverse();
        write_jsons_atomically(
            &target_dir
                .join("accounts")
                .join(&account_dir_name(account))
                .join(start_of_month.format("%Y-%m.jsons").to_string()),
            &txes.results,
        )
        .await?;
    }
    Ok(())
}

#[instrument(skip(tl, target_dir))]
async fn scrape_cards(tl: Arc<TlClient>, target_dir: Arc<Path>) -> Result<Vec<CardsResult>> {
    let cards = tl.fetch_cards().await?;
    write_jsons_atomically(&target_dir.join("cards.jsons"), &cards.results).await?;
    for card in cards.results.iter() {
        let path = target_dir
            .join("cards")
            .join(&card.account_id)
            .join("account.jsons");
        write_jsons_atomically(&path, &[card]).await?;
    }

    Ok(cards.results)
}

#[instrument(skip(tl, target_dir))]
async fn scrape_card_balance(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
    account_id: &str,
) -> Result<()> {
    info!("Fetch balance");
    let bal = tl.card_balance(account_id).await?;
    write_jsons_atomically(
        &target_dir
            .join("cards")
            .join(account_id)
            .join("balance.jsons"),
        &bal.results,
    )
    .await?;
    Ok(())
}

#[instrument(skip(tl, target_dir, account_id), fields(account_id=%account_id))]
async fn scrape_card_pending(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
    account_id: &str,
) -> Result<()> {
    info!("Fetch pending transactions");
    let bal = tl.card_pending(account_id).await?;
    let path = &target_dir
        .join("cards")
        .join(account_id)
        .join("pending.jsons");
    write_jsons_atomically(path, &bal.results).await?;
    Ok(())
}

#[instrument(skip(tl, target_dir))]
async fn scrape_card_tx(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
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

        txes.results.reverse();

        write_jsons_atomically(
            &target_dir
                .join("cards")
                .join(account_id)
                .join(start_of_month.format("%Y-%m.jsons").to_string()),
            &txes.results,
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

async fn write_jsons_atomically<T: Serialize>(path: &Path, data: &[T]) -> Result<()> {
    block_in_place(|| {
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(dir)?;
        let mut tmpf = NamedTempFile::new_in(dir)?;
        let mut buf = Vec::new();
        for item in data {
            serde_json::to_writer(&mut buf, &item)?;
            assert!(!buf.contains(&b'\n'));
            tmpf.write_all(&buf)?;
            tmpf.write_all(b"\n")?;
            buf.clear();
        }
        tmpf.as_file_mut().flush()?;
        tmpf.persist(path)?;
        debug!(?path, "Stored data");
        Ok(())
    })
}
