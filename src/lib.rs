use std::path::Path;

use anyhow::Result;
use authentication::Authenticator;
use chrono::{DateTime, Utc};
use hyper::Uri;
use reqwest::{Client, RequestBuilder, Response};
use rust_decimal::Decimal;
use secrecy::{ExposeSecret, Secret, Zeroize};
use serde::{Deserialize, Serialize, Serializer};
use tracing::{debug, error};

mod authentication;

#[derive(Debug, Serialize, Deserialize)]
pub struct UserInfoResponse {
    #[serde(rename = "results")]
    results: Vec<UserInfoResult>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserInfoResult {
    #[serde(rename = "full_name")]
    pub full_name: String,
    #[serde(rename = "update_timestamp")]
    pub update_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountsResponse {
    pub results: Vec<AccountsResult>,
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
pub struct CardsResponse {
    pub results: Vec<CardsResult>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalanceResponse {
    pub results: Vec<BalanceResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalanceResult {
    pub currency: String,
    pub available: Decimal,
    pub current: Decimal,
    pub overdraft: Option<Decimal>,
    #[serde(rename = "update_timestamp")]
    pub update_timestamp: DateTime<Utc>,
}

pub struct TlClient {
    client: Client,
    auth: Authenticator,
}

const SANDBOX_API_HOST: &str = "api.truelayer-sandbox.com";
const SANDBOX_AUTH_HOST: &str = "auth.truelayer-sandbox.com";
const REDIRECT_URI: &str = "https://console.truelayer.com/redirect-page";

impl TlClient {
    pub fn new(
        client: reqwest::Client,
        token_path: &Path,
        client_id: String,
        client_secret: Secret<String>,
    ) -> Self {
        let token_path = token_path.to_owned();
        let auth = Authenticator::new(client.clone(), token_path, client_id, client_secret);
        Self { client, auth }
    }

    pub async fn authenticate(&self, access_code: Secret<String>) -> Result<()> {
        self.auth.authenticate(access_code).await?;

        Ok(())
    }

    pub async fn fetch_info(&self) -> Result<UserInfoResponse> {
        let url = Uri::builder()
            .scheme("https")
            .authority(SANDBOX_API_HOST)
            .path_and_query("/data/v1/info")
            .build()?;
        let access_token = self.auth.access_token().await?;
        let info_response = perform_request(
            self.client
                .get(&url.to_string())
                .bearer_auth(access_token.expose_secret()),
        )
        .await?
        .json::<UserInfoResponse>()
        .await?;
        Ok(info_response)
    }

    pub async fn fetch_accounts(&self) -> Result<AccountsResponse> {
        let url = Uri::builder()
            .scheme("https")
            .authority(SANDBOX_API_HOST)
            .path_and_query("/data/v1/accounts")
            .build()?;
        let access_token = self.auth.access_token().await?;
        let info_response = perform_request(
            self.client
                .get(&url.to_string())
                .bearer_auth(access_token.expose_secret()),
        )
        .await?
        .json()
        .await?;
        Ok(info_response)
    }

    pub async fn account_balance(&self, account_id: &str) -> Result<BalanceResponse> {
        let url = Uri::builder()
            .scheme("https")
            .authority(SANDBOX_API_HOST)
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
        .await?
        .json()
        .await?;
        Ok(response)
    }

    pub async fn fetch_cards(&self) -> Result<CardsResponse> {
        let url = Uri::builder()
            .scheme("https")
            .authority(SANDBOX_API_HOST)
            .path_and_query("/data/v1/cards")
            .build()?;
        let access_token = self.auth.access_token().await?;
        let response = perform_request(
            self.client
                .get(&url.to_string())
                .bearer_auth(access_token.expose_secret()),
        )
        .await?
        .json()
        .await?;
        Ok(response)
    }
}

fn serialize_secret<T: Zeroize + Serialize, S: Serializer>(
    secret: &Secret<T>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    secret.expose_secret().serialize(serializer)
}
fn serialize_optional_secret<T: Zeroize + Serialize, S: Serializer>(
    secret: &Option<Secret<T>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    secret
        .as_ref()
        .map(|s| s.expose_secret())
        .serialize(serializer)
}

async fn perform_request(req: RequestBuilder) -> Result<Response> {
    let res = req.send().await?;

    if let Err(error) = res.error_for_status_ref() {
        error!(%error, status=?res.status(), "Failed response");
        if let Ok(body) = res.text().await {
            debug!(%error, ?body, "Response body");
        }
        Err(error.into())
    } else {
        Ok(res)
    }
}
