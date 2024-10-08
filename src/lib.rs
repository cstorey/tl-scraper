use again::RetryPolicy;
use anyhow::Result;
use reqwest::RequestBuilder;
use secrecy::{ExposeSecret, Secret, Zeroize};
use serde::{de::DeserializeOwned, Serialize, Serializer};
use tracing::{debug, error};

mod auth;
mod client;
mod config;
mod join_pool;
mod sync;

pub use auth::authenticate;
pub use client::{ClientCreds, Environment, TlClient};
pub use config::{MainConfig, ProviderConfig, ScraperConfig};
pub use join_pool::{JobHandle, JobPool};
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

async fn perform_request<R: DeserializeOwned, B: Fn() -> RequestBuilder>(
    retry_policy: &RetryPolicy,
    build: B,
) -> Result<R> {
    async fn inner<R: DeserializeOwned, B: Fn() -> RequestBuilder>(build: B) -> Result<R> {
        let res = build().send().await?;
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

    retry_policy.retry(|| inner(&build)).await
}
