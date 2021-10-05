use std::path::Path;

use anyhow::Result;
use authentication::Authenticator;
use hyper::Uri;
use reqwest::{Client, RequestBuilder, Response};
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
    pub update_timestamp: String,
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
