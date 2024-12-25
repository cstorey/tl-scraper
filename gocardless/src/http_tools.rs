use std::fmt;

use color_eyre::{eyre::eyre, Result};
use reqwest::header::CONTENT_TYPE;
use serde::Deserialize;
use tracing::{debug, warn};

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    summary: String,
    detail: String,
    status_code: u16,
}

pub(crate) trait RequestErrors: Sized {
    async fn parse_error(self) -> Result<Self>;
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
        } = self;
        write!(
            f,
            "Summary: {summary:?}; details: {detail:?}, status_code: {status_code:?}",
        )
    }
}
