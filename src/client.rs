use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use hyper::{http::uri, Uri};
use reqwest::Client;
use rust_decimal::Decimal;
use secrecy::{ExposeSecret, Secret};
use serde::{Deserialize, Serialize};

use crate::{authentication::Authenticator, perform_request};

#[derive(Debug, Serialize, Deserialize)]
pub struct Response<T> {
    #[serde(rename = "results")]
    pub results: Vec<T>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserInfoResult {
    #[serde(rename = "full_name")]
    pub full_name: String,
    #[serde(rename = "update_timestamp")]
    pub update_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountsResult {
    #[serde(rename = "update_timestamp")]
    pub update_timestamp: String,
    #[serde(rename = "account_id")]
    pub account_id: String,
    #[serde(rename = "account_type")]
    pub account_type: String,
    #[serde(rename = "display_name")]
    pub display_name: String,
    pub currency: String,
    #[serde(rename = "account_number")]
    pub account_number: AccountNumber,
    pub provider: AccountsProvider,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountNumber {
    pub iban: String,
    pub number: String,
    #[serde(rename = "sort_code")]
    pub sort_code: String,
    #[serde(rename = "swift_bic")]
    pub swift_bic: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountsProvider {
    #[serde(rename = "provider_id")]
    pub provider_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardsResult {
    #[serde(rename = "account_id")]
    pub account_id: String,
    #[serde(rename = "card_network")]
    pub card_network: String,
    #[serde(rename = "card_type")]
    pub card_type: String,
    pub currency: String,
    #[serde(rename = "display_name")]
    pub display_name: String,
    #[serde(rename = "partial_card_number")]
    pub partial_card_number: String,
    #[serde(rename = "name_on_card")]
    pub name_on_card: String,
    #[serde(rename = "valid_from")]
    pub valid_from: Option<String>,
    #[serde(rename = "valid_to")]
    pub valid_to: Option<String>,
    #[serde(rename = "update_timestamp")]
    pub update_timestamp: DateTime<Utc>,
    pub provider: CardsProvider,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardsProvider {
    #[serde(rename = "provider_id")]
    pub provider_id: String,
    #[serde(rename = "logo_uri")]
    pub logo_uri: Option<String>,
    #[serde(rename = "display_name")]
    pub display_name: Option<String>,
}

type BalanceResponse = Response<BalanceResult>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalanceResult {
    pub currency: String,
    pub available: Decimal,
    pub current: Decimal,
    pub overdraft: Option<Decimal>,
    #[serde(rename = "update_timestamp")]
    pub update_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionsResult {
    // Is this _always_ present?
    #[serde(rename = "transaction_id")]
    pub transaction_id: Option<String>,
    #[serde(rename = "normalised_provider_transaction_id")]
    pub normalised_provider_transaction_id: String,
    #[serde(rename = "provider_transaction_id")]
    pub provider_transaction_id: String,
    pub timestamp: DateTime<Utc>,
    pub description: String,
    pub amount: Decimal,
    pub currency: String,
    #[serde(rename = "transaction_type")]
    pub transaction_type: String,
    #[serde(rename = "transaction_category")]
    pub transaction_category: String,
    #[serde(rename = "transaction_classification")]
    pub transaction_classification: Vec<String>,
    #[serde(rename = "merchant_name")]
    pub merchant_name: Option<String>,
    #[serde(rename = "running_balance")]
    pub running_balance: Option<TransactionsRunningBalance>,
    pub meta: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionsRunningBalance {
    pub amount: Decimal,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StandingOrderResult {
    #[serde(flatten)]
    inner: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectDebitResult {
    #[serde(flatten)]
    inner: serde_json::Value,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub enum Environment {
    Sandbox,
    Live,
}

pub struct TlClient {
    client: Client,
    env: Environment,
    auth: Authenticator,
}

const SANDBOX_API_HOST: &str = "api.truelayer-sandbox.com";
const SANDBOX_AUTH_HOST: &str = "auth.truelayer-sandbox.com";
const LIVE_API_HOST: &str = "api.truelayer.com";
const LIVE_AUTH_HOST: &str = "auth.truelayer.com";
pub(crate) const REDIRECT_URI: &str = "https://console.truelayer.com/redirect-page";

impl TlClient {
    pub fn new(
        client: reqwest::Client,
        env: Environment,
        token_path: &Path,
        client_id: String,
        client_secret: Secret<String>,
    ) -> Self {
        let token_path = token_path.to_owned();
        let auth = Authenticator::new(
            client.clone(),
            env.clone(),
            token_path,
            client_id,
            client_secret,
        );
        Self { client, env, auth }
    }

    pub async fn authenticate(&self, access_code: Secret<String>) -> Result<()> {
        self.auth.authenticate(access_code).await?;

        Ok(())
    }

    pub async fn fetch_info(&self) -> Result<Response<UserInfoResult>> {
        let url = self
            .env
            .api_url_builder()
            .path_and_query("/data/v1/info")
            .build()?;
        let access_token = self.auth.access_token().await?;
        let info_response = perform_request(
            self.client
                .get(&url.to_string())
                .bearer_auth(access_token.expose_secret()),
        )
        .await?;
        Ok(info_response)
    }

    pub async fn fetch_accounts(&self) -> Result<Response<AccountsResult>> {
        let url = self
            .env
            .api_url_builder()
            .path_and_query("/data/v1/accounts")
            .build()?;
        let access_token = self.auth.access_token().await?;
        let info_response = perform_request(
            self.client
                .get(&url.to_string())
                .bearer_auth(access_token.expose_secret()),
        )
        .await?;
        Ok(info_response)
    }

    pub async fn account_balance(&self, account_id: &str) -> Result<BalanceResponse> {
        let url = self
            .env
            .api_url_builder()
            .path_and_query(format!(
                "/data/v1/accounts/{account}/balance",
                account = urlencoding::encode(account_id)
            ))
            .build()?;
        let access_token = self.auth.access_token().await?;
        let response = perform_request(
            self.client
                .get(&url.to_string())
                .bearer_auth(access_token.expose_secret()),
        )
        .await?;
        Ok(response)
    }

    pub async fn account_pending(&self, account_id: &str) -> Result<Response<TransactionsResult>> {
        let url = self
            .env
            .api_url_builder()
            .path_and_query(format!(
                "/data/v1/accounts/{account}/transactions/pending",
                account = urlencoding::encode(account_id)
            ))
            .build()?;
        let access_token = self.auth.access_token().await?;
        let response = perform_request(
            self.client
                .get(&url.to_string())
                .bearer_auth(access_token.expose_secret()),
        )
        .await?;
        Ok(response)
    }

    pub async fn account_standing_orders(
        &self,
        account_id: &str,
    ) -> Result<Response<StandingOrderResult>> {
        let url = self
            .env
            .api_url_builder()
            .path_and_query(format!(
                "/data/v1/accounts/{account}/standing_orders",
                account = urlencoding::encode(account_id)
            ))
            .build()?;
        let access_token = self.auth.access_token().await?;
        let response = perform_request(
            self.client
                .get(&url.to_string())
                .bearer_auth(access_token.expose_secret()),
        )
        .await?;
        Ok(response)
    }

    pub async fn account_direct_debits(
        &self,
        account_id: &str,
    ) -> Result<Response<DirectDebitResult>> {
        let url = self
            .env
            .api_url_builder()
            .path_and_query(format!(
                "/data/v1/accounts/{account}/direct_debits",
                account = urlencoding::encode(account_id)
            ))
            .build()?;
        let access_token = self.auth.access_token().await?;
        let response = perform_request(
            self.client
                .get(&url.to_string())
                .bearer_auth(access_token.expose_secret()),
        )
        .await?;
        Ok(response)
    }

    pub async fn account_transactions(
        &self,
        account_id: &str,
        from_date: NaiveDate,
        to_date: NaiveDate,
    ) -> Result<Response<TransactionsResult>> {
        let url = self
            .env
            .api_url_builder()
            .path_and_query(format!(
                "/data/v1/accounts/{account}/transactions",
                account = urlencoding::encode(account_id)
            ))
            .build()?;
        let access_token = self.auth.access_token().await?;
        let response = perform_request(
            self.client
                .get(&url.to_string())
                .query(&[("from", &from_date), ("to", &to_date)])
                .bearer_auth(access_token.expose_secret()),
        )
        .await?;
        Ok(response)
    }

    pub async fn fetch_cards(&self) -> Result<Response<CardsResult>> {
        let url = self
            .env
            .api_url_builder()
            .path_and_query("/data/v1/cards")
            .build()?;
        let access_token = self.auth.access_token().await?;
        let response = perform_request(
            self.client
                .get(&url.to_string())
                .bearer_auth(access_token.expose_secret()),
        )
        .await?;
        Ok(response)
    }

    pub async fn card_balance(&self, card_id: &str) -> Result<BalanceResponse> {
        let url = self
            .env
            .api_url_builder()
            .path_and_query(format!(
                "/data/v1/cards/{account}/balance",
                account = urlencoding::encode(card_id)
            ))
            .build()?;
        let access_token = self.auth.access_token().await?;
        let response = perform_request(
            self.client
                .get(&url.to_string())
                .bearer_auth(access_token.expose_secret()),
        )
        .await?;
        Ok(response)
    }

    pub async fn card_pending(&self, account_id: &str) -> Result<Response<TransactionsResult>> {
        let url = self
            .env
            .api_url_builder()
            .path_and_query(format!(
                "/data/v1/cards/{account}/transactions/pending",
                account = urlencoding::encode(account_id)
            ))
            .build()?;
        let access_token = self.auth.access_token().await?;
        let response = perform_request(
            self.client
                .get(&url.to_string())
                .bearer_auth(access_token.expose_secret()),
        )
        .await?;
        Ok(response)
    }

    pub async fn card_transactions(
        &self,
        card_id: &str,
        from_date: NaiveDate,
        to_date: NaiveDate,
    ) -> Result<Response<TransactionsResult>> {
        let url = self
            .env
            .api_url_builder()
            .path_and_query(format!(
                "/data/v1/cards/{account}/transactions",
                account = urlencoding::encode(card_id)
            ))
            .build()?;
        let access_token = self.auth.access_token().await?;
        let response = perform_request(
            self.client
                .get(&url.to_string())
                .query(&[("from", &from_date), ("to", &to_date)])
                .bearer_auth(access_token.expose_secret()),
        )
        .await?;
        Ok(response)
    }
}

impl Environment {
    fn api_url_builder(&self) -> uri::Builder {
        let host = match self {
            Environment::Sandbox => SANDBOX_API_HOST,
            Environment::Live => LIVE_API_HOST,
        };
        Uri::builder().scheme("https").authority(host)
    }

    pub(crate) fn auth_url_builder(&self) -> uri::Builder {
        let host = match self {
            Environment::Sandbox => SANDBOX_AUTH_HOST,
            Environment::Live => LIVE_AUTH_HOST,
        };
        Uri::builder().scheme("https").authority(host)
    }
}
