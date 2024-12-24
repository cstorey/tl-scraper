use std::fmt;

use axum::http::{uri::Scheme, Uri};
use color_eyre::{
    eyre::{eyre, Context},
    Result,
};
use reqwest::{header::CONTENT_TYPE, Client};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::{debug, trace, warn};

use crate::auth::Token;

const BANK_DATA_HOST: &str = "bankaccountdata.gocardless.com";

#[derive(Clone)]
pub(crate) struct UnauthenticatedBankDataClient {
    http: Client,
}
#[derive(Clone)]
pub(crate) struct BankDataClient {
    http: Client,
    token: Token,
}
#[derive(Debug, Deserialize)]
struct ErrorResponse {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    detail: String,
    #[serde(default)]
    status_code: u16,
    #[serde(flatten)]
    other: serde_json::Value,
}

pub(crate) trait RequestErrors: Sized {
    async fn parse_error(self) -> Result<Self>;
}

impl UnauthenticatedBankDataClient {
    fn new() -> Self {
        let http = Client::new();
        UnauthenticatedBankDataClient { http }
    }

    pub(crate) async fn post<Response: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> Result<Response> {
        // "https://bankaccountdata.gocardless.com/api/v2/token/new/"
        let url = Uri::builder()
            .scheme(Scheme::HTTPS)
            .authority(BANK_DATA_HOST)
            .path_and_query(path)
            .build()
            .wrap_err("Build base URI")?
            .to_string();

        debug!(%url, "POST");

        let resp = self
            .http
            .post(url)
            .json(body)
            .send()
            .await?
            .parse_error()
            .await?;

        log_rate_limits(&resp)?;

        let data = resp.json().await?;

        Ok(data)
    }
}

impl BankDataClient {
    pub(crate) fn new(token: Token) -> Self {
        let http = Client::new();
        Self { http, token }
    }

    pub(crate) fn unauthenticated() -> UnauthenticatedBankDataClient {
        UnauthenticatedBankDataClient::new()
    }

    pub(crate) async fn get<Response: DeserializeOwned>(&self, path: &str) -> Result<Response> {
        // "https://bankaccountdata.gocardless.com/api/v2/token/new/"
        let url = Uri::builder()
            .scheme(Scheme::HTTPS)
            .authority(BANK_DATA_HOST)
            .path_and_query(path)
            .build()
            .wrap_err("Build base URI")?
            .to_string();

        debug!(%url, "GET");
        let resp = self
            .http
            .get(url)
            .bearer_auth(&self.token.access)
            .send()
            .await?
            .parse_error()
            .await?;

        log_rate_limits(&resp)?;

        let data = resp.json().await?;

        Ok(data)
    }

    pub(crate) async fn post<Response: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> Result<Response> {
        // "https://bankaccountdata.gocardless.com/api/v2/token/new/"
        let url = Uri::builder()
            .scheme(Scheme::HTTPS)
            .authority(BANK_DATA_HOST)
            .path_and_query(path)
            .build()
            .wrap_err("Build base URI")?
            .to_string();

        debug!(%url, "POST");
        let resp = self
            .http
            .post(url)
            .json(body)
            .bearer_auth(&self.token.access)
            .send()
            .await?
            .parse_error()
            .await?;

        log_rate_limits(&resp)?;

        let data = resp.json().await?;

        Ok(data)
    }
}

fn log_rate_limits(resp: &reqwest::Response) -> Result<()> {
    const HTTP_X_RATELIMIT_LIMIT: &str = "HTTP_X_RATELIMIT_LIMIT";
    let Some(limit) = resp.headers().get(HTTP_X_RATELIMIT_LIMIT) else {
        warn!(header=%HTTP_X_RATELIMIT_LIMIT, "rate limit header missing");
        return Ok(());
    };

    const HTTP_X_RATELIMIT_REMAINING: &str = "HTTP_X_RATELIMIT_REMAINING";
    let Some(remaining) = resp.headers().get(HTTP_X_RATELIMIT_REMAINING) else {
        warn!(header=%HTTP_X_RATELIMIT_REMAINING, "rate limit header missing");
        return Ok(());
    };

    const HTTP_X_RATELIMIT_RESET: &str = "HTTP_X_RATELIMIT_RESET";
    let Some(reset) = resp.headers().get(HTTP_X_RATELIMIT_RESET) else {
        warn!(header=%HTTP_X_RATELIMIT_RESET, "rate limit header missing");
        return Ok(());
    };

    let limit = limit
        .to_str()
        .context(HTTP_X_RATELIMIT_LIMIT)?
        .parse::<u64>()
        .context(HTTP_X_RATELIMIT_LIMIT)?;
    let remaining = remaining
        .to_str()
        .context(HTTP_X_RATELIMIT_REMAINING)?
        .parse::<u64>()
        .context(HTTP_X_RATELIMIT_LIMIT)?;
    let reset = reset
        .to_str()
        .context(HTTP_X_RATELIMIT_RESET)?
        .parse::<u64>()
        .context(HTTP_X_RATELIMIT_LIMIT)?;

    debug!(%limit, %remaining, %reset, "Rate limit status");

    Ok(())
}

impl RequestErrors for reqwest::Response {
    async fn parse_error(self) -> Result<Self> {
        let resp = self;
        if resp.status().is_client_error() {
            match resp.headers().get(CONTENT_TYPE) {
                Some(content_type) if content_type.as_bytes() == b"application/json" => {
                    let err = resp.json::<ErrorResponse>().await?;
                    return Err(err.into());
                }
                Some(content_type) => {
                    warn!(?content_type, "unknown content type");
                    let status = resp.status();
                    let content = resp.text().await?;
                    debug!(?content, "Response body");
                    return Err(eyre!(
                        "Unrecognised response; code: {status}; content: {content:?}"
                    ));
                }
                None => {}
            }
        }

        trace!(status=?resp.status(), headers=?resp.headers());
        Ok(resp.error_for_status()?)
    }
}

impl std::error::Error for ErrorResponse {
    fn description(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ErrorResponse {
            summary,
            detail,
            status_code,
            other,
        } = self;
        write!(
            f,
            "Summary: {summary:?}; details: {detail:?}, status_code: {status_code:?}, other: {}",
            serde_json::to_string(other)
                .unwrap_or_else(|err| format!("Error rendering other: {err}")),
        )
    }
}
