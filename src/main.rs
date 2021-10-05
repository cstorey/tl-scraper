use std::{fs::File, io::Write, path::Path};

use anyhow::Result;
use hyper::Uri;
use reqwest::{RequestBuilder, Response};
use secrecy::{ExposeSecret, Secret, SecretString, Zeroize};
use serde::{Deserialize, Serialize, Serializer};
use structopt::StructOpt;
use tempfile::NamedTempFile;
use tracing::{debug, error, info};

#[derive(Debug, StructOpt)]
struct Options {
    #[structopt(short = "i", long = "client-id")]
    client_id: String,
    #[structopt(short = "s", long = "client-secret")]
    client_secret: SecretString,
    #[structopt(short = "t", long = "access-code")]
    access_code: SecretString,
}

#[derive(Debug, Serialize, Deserialize)]
enum GrantType {
    #[serde(rename = "authorization_code")]
    AuthorizationCode,
}

#[derive(Debug, Serialize)]
struct FetchAccessTokenRequest {
    grant_type: GrantType,
    client_id: String,
    #[serde(serialize_with = "serialize_secret_string")]
    client_secret: SecretString,
    redirect_uri: String,
    #[serde(serialize_with = "serialize_secret_string")]
    code: SecretString,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct FetchAccessTokenResponse {
    #[serde(serialize_with = "serialize_secret_string")]
    access_token: SecretString,
    expires_in: i64,
    token_type: String,
    #[serde(serialize_with = "serialize_secret_string")]
    refresh_token: SecretString,
    scope: String,
}

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

const SANDBOX_API_HOST: &str = "api.truelayer-sandbox.com";
const SANDBOX_AUTH_HOST: &str = "auth.truelayer-sandbox.com";
const REDIRECT_URI: &str = "https://console.truelayer.com/redirect-page";

const TOKEN_FILE: &str = "token.json";

#[tokio::main]
async fn main() -> Result<()> {
    let opts = Options::from_args();

    tracing_log::LogTracer::init()?;
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_ansi(false)
            .with_timer(tracing_subscriber::fmt::time::ChronoUtc::rfc3339())
            .with_thread_names(true)
            .with_thread_ids(true)
            .finish(),
    )?;

    let client = reqwest::Client::new();

    let token_path = Path::new(TOKEN_FILE);
    let token_response = if token_path.exists() {
        let data: FetchAccessTokenResponse = serde_json::from_reader(&File::open(token_path)?)?;
        data
    } else {
        let url = Uri::builder()
            .scheme("https")
            .authority(SANDBOX_AUTH_HOST)
            .path_and_query("/connect/token")
            .build()?;

        let fetch_access_token_request = FetchAccessTokenRequest {
            grant_type: GrantType::AuthorizationCode,
            client_id: opts.client_id,
            client_secret: opts.client_secret,
            redirect_uri: REDIRECT_URI.into(),
            code: opts.access_code,
        };

        let token_response = perform_request(
            client
                .post(&url.to_string())
                .form(&fetch_access_token_request),
        )
        .await?
        .json::<FetchAccessTokenResponse>()
        .await?;

        info!(?token_response, "Response");
        let mut tmpf = NamedTempFile::new_in(".")?;
        serde_json::to_writer_pretty(&mut tmpf, &token_response)?;
        tmpf.as_file_mut().flush()?;
        tmpf.persist(token_path)?;
        token_response
    };

    let url = Uri::builder()
        .scheme("https")
        .authority(SANDBOX_API_HOST)
        .path_and_query("/data/v1/info")
        .build()?;

    let info_response = perform_request(
        client
            .get(&url.to_string())
            .bearer_auth(token_response.access_token.expose_secret()),
    )
    .await?
    .json::<UserInfoResponse>()
    .await?;

    info!(json=?info_response, "Response");

    Ok(())
}

fn serialize_secret_string<T: Zeroize + Serialize, S: Serializer>(
    secret: &Secret<T>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    secret.expose_secret().serialize(serializer)
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
