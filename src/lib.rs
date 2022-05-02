use anyhow::Result;
use reqwest::RequestBuilder;
use secrecy::{ExposeSecret, Secret, Zeroize};
use serde::{de::DeserializeOwned, Serialize, Serializer};
use tracing::{debug, error};

mod authentication;
mod client;
mod sync;

pub use client::{Environment, TlClient};
pub use sync::{sync_accounts, sync_cards, sync_info};

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

async fn perform_request<R: DeserializeOwned>(req: RequestBuilder) -> Result<R> {
    let res = req.send().await?;

    if let Err(error) = res.error_for_status_ref() {
        error!(%error, status=?res.status(), "Failed response");
        if let Ok(body) = res.text().await {
            debug!(%error, ?body, "Response body");
        }
        Err(error.into())
    } else {
        let result = res.json().await?;
        Ok(result)
    }
}
