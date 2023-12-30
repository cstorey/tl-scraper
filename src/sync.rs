use std::{cmp::min, io::Write, ops::RangeInclusive, path::Path, sync::Arc};

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use tempfile::NamedTempFile;
use tokio::task::block_in_place;
use tracing::{debug, info, instrument, Instrument, Span};

use crate::{
    client::{AccountsResult, CardsResult},
    JobHandle, TlClient,
};

#[instrument(skip_all, fields(?period))]
pub async fn sync_accounts(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
    period: RangeInclusive<NaiveDate>,
    jobs: JobHandle,
) -> Result<(), anyhow::Error> {
    let accounts = accounts(tl.clone(), target_dir.clone()).await?;
    for account_item in accounts {
        account(&jobs, &tl, &target_dir, account_item, period.clone())
            .instrument(Span::current())
            .await?;
    }
    Ok(())
}

#[instrument(skip_all, fields(account_id=%account.account_id))]
async fn account(
    jobs: &JobHandle,
    tl: &Arc<TlClient>,
    target_dir: &Arc<Path>,
    account: AccountsResult,
    period: RangeInclusive<NaiveDate>,
) -> Result<(), anyhow::Error> {
    jobs.spawn(
        account_balance(tl.clone(), target_dir.clone(), account.clone())
            .instrument(Span::current()),
    )?;
    jobs.spawn(
        account_pending(tl.clone(), target_dir.clone(), account.clone())
            .instrument(Span::current()),
    )?;
    for month in months(period) {
        jobs.spawn(
            account_tx(tl.clone(), target_dir.clone(), account.clone(), month)
                .instrument(Span::current()),
        )?;
    }

    if false {
        // Only available when you've _recently_ authenticated.
        jobs.spawn(
            account_standing_orders(tl.clone(), target_dir.clone(), account.clone())
                .instrument(Span::current()),
        )?;
        jobs.spawn(
            account_direct_debits(tl.clone(), target_dir.clone(), account.clone())
                .instrument(Span::current()),
        )?;
    }
    Ok(())
}

#[instrument(skip_all)]
pub async fn sync_cards(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
    period: RangeInclusive<NaiveDate>,
    jobs: JobHandle,
) -> Result<(), anyhow::Error> {
    let cards = cards(tl.clone(), target_dir.clone()).await?;
    for card_result in cards {
        card(&jobs, &tl, &target_dir, card_result, period.clone())
            .instrument(Span::current())
            .await?;
    }
    Ok(())
}

#[instrument(skip_all, fields(account_id=%card.account_id))]
async fn card(
    jobs: &JobHandle,
    tl: &Arc<TlClient>,
    target_dir: &Arc<Path>,
    card: CardsResult,
    period: RangeInclusive<NaiveDate>,
) -> Result<(), anyhow::Error> {
    jobs.spawn(
        card_balance(tl.clone(), target_dir.clone(), card.account_id.clone())
            .instrument(Span::current()),
    )?;
    jobs.spawn(
        card_pending(tl.clone(), target_dir.clone(), card.account_id.clone())
            .instrument(Span::current()),
    )?;
    for month in months(period) {
        jobs.spawn(
            card_tx(
                tl.clone(),
                target_dir.clone(),
                card.account_id.clone(),
                month,
            )
            .instrument(Span::current()),
        )?
    }
    Ok(())
}

#[instrument(skip_all)]
pub async fn sync_info(tl: Arc<TlClient>, target_dir: Arc<Path>) -> Result<()> {
    let user_info = tl.fetch_info().await?;
    write_jsons_atomically(&target_dir.join("user-info.jsons"), &user_info.results).await?;
    Ok(())
}

#[instrument(skip_all)]
async fn accounts(tl: Arc<TlClient>, target_dir: Arc<Path>) -> Result<Vec<AccountsResult>> {
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

#[instrument(skip_all)]
async fn account_balance(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
    account: AccountsResult,
) -> Result<()> {
    info!("Fetch balance");
    let bal = tl.account_balance(&account.account_id).await?;
    let path = &target_dir
        .join("accounts")
        .join(account_dir_name(&account))
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

#[instrument(skip_all)]
async fn account_pending(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
    account: AccountsResult,
) -> Result<()> {
    info!("Fetch pending transactions");
    let bal = tl.account_pending(&account.account_id).await?;
    let path = &target_dir
        .join("accounts")
        .join(account_dir_name(&account))
        .join("pending.jsons");
    write_jsons_atomically(path, &bal.results).await?;
    Ok(())
}

#[instrument(skip_all)]
async fn account_standing_orders(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
    account: AccountsResult,
) -> Result<()> {
    info!("Fetch standing orders");
    let bal = tl.account_standing_orders(&account.account_id).await?;
    let path = &target_dir
        .join("accounts")
        .join(account_dir_name(&account))
        .join("standing-orders.jsons");
    write_jsons_atomically(path, &bal.results).await?;
    Ok(())
}

#[instrument(skip_all)]
async fn account_direct_debits(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
    account: AccountsResult,
) -> Result<()> {
    info!("Fetch direct debits");
    let bal = tl.account_direct_debits(&account.account_id).await?;
    let path = &target_dir
        .join("accounts")
        .join(account_dir_name(&account))
        .join("standing-orders.jsons");
    write_jsons_atomically(path, &bal.results).await?;
    Ok(())
}

#[instrument(skip_all, fields(?month))]
async fn account_tx(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
    account: AccountsResult,
    month: RangeInclusive<NaiveDate>,
) -> Result<()> {
    // TODO: split into per-month jobs.
    let mut txes = tl
        .account_transactions(&account.account_id, *month.start(), *month.end())
        .await?;

    if txes.results.is_empty() {
        info!("No results for month found");
        return Ok(());
    }

    txes.results.reverse();
    write_jsons_atomically(
        &target_dir
            .join("accounts")
            .join(&account_dir_name(&account))
            .join(month.start().format("%Y-%m.jsons").to_string()),
        &txes.results,
    )
    .await?;
    Ok(())
}

#[instrument(skip_all)]
async fn cards(tl: Arc<TlClient>, target_dir: Arc<Path>) -> Result<Vec<CardsResult>> {
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

#[instrument(skip_all)]
async fn card_balance(tl: Arc<TlClient>, target_dir: Arc<Path>, account_id: String) -> Result<()> {
    info!("Fetch balance");
    let bal = tl.card_balance(&account_id).await?;
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

#[instrument(skip_all)]
async fn card_pending(tl: Arc<TlClient>, target_dir: Arc<Path>, account_id: String) -> Result<()> {
    info!("Fetch pending transactions");
    let bal = tl.card_pending(&account_id).await?;
    let path = &target_dir
        .join("cards")
        .join(account_id)
        .join("pending.jsons");
    write_jsons_atomically(path, &bal.results).await?;
    Ok(())
}

#[instrument(skip_all, fields(?month))]
async fn card_tx(
    tl: Arc<TlClient>,
    target_dir: Arc<Path>,
    account_id: String,
    month: RangeInclusive<NaiveDate>,
) -> Result<()> {
    let mut txes = tl
        .card_transactions(&account_id, *month.start(), *month.end())
        .await?;

    if txes.results.is_empty() {
        info!(?month, "No results for month found");
        return Ok(());
    }

    txes.results.reverse();

    write_jsons_atomically(
        &target_dir
            .join("cards")
            .join(&account_id)
            .join(month.start().format("%Y-%m.jsons").to_string()),
        &txes.results,
    )
    .await?;
    Ok(())
}

fn months(period: RangeInclusive<NaiveDate>) -> impl Iterator<Item = RangeInclusive<NaiveDate>> {
    let month_start_date = period.start().with_day(1).expect("day one");

    let month_starts = month_start_date.iter_days().filter(|d| d.day() == 1);
    let month_ends = month_starts.clone().skip(1).map({
        let period = period.clone();
        move |d| min(d.pred_opt().unwrap(), *period.end())
    });
    month_starts
        .take_while(move |d| d <= period.end())
        .zip(month_ends)
        .map(|(a, b)| a..=b)
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
