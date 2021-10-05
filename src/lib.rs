use std::{
    fs::File,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use anyhow::{bail, Result};
use chrono::{DateTime, Duration, Utc};
use hyper::Uri;
use reqwest::{Client, RequestBuilder, Response};
use secrecy::{ExposeSecret, Secret, SecretString, Zeroize};
use serde::{Deserialize, Serialize, Serializer};
use tempfile::NamedTempFile;
use tracing::{debug, error, info};

#[derive(Debug, Serialize, Deserialize)]
enum GrantType {
    #[serde(rename = "authorization_code")]
    AuthorizationCode,
    #[serde(rename = "refresh_token")]
    RefreshToken,
}

#[derive(Debug, Serialize)]
struct FetchAccessTokenRequest {
    grant_type: GrantType,
    client_id: String,
    #[serde(serialize_with = "serialize_secret")]
    client_secret: SecretString,
    redirect_uri: String,
    #[serde(
        serialize_with = "serialize_optional_secret",
        skip_serializing_if = "Option::is_none"
    )]
    code: Option<SecretString>,
    #[serde(
        serialize_with = "serialize_optional_secret",
        skip_serializing_if = "Option::is_none"
    )]
    refresh_token: Option<SecretString>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct FetchAccessTokenResponse {
    #[serde(serialize_with = "serialize_secret")]
    access_token: SecretString,
    expires_in: i64,
    token_type: String,
    #[serde(serialize_with = "serialize_secret")]
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

pub struct TlClient {
    client: Client,
    auth: Authenticator,
}

struct Authenticator {
    client: Client,
    token_path: PathBuf,
    client_id: String,
    client_secret: SecretString,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthData {
    #[serde(serialize_with = "serialize_secret")]
    access_token: SecretString,
    // TODO: Remove option
    #[serde(default)]
    expires_at: Option<DateTime<Utc>>,
    token_type: String,
    #[serde(serialize_with = "serialize_secret")]
    refresh_token: SecretString,
    scope: String,
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
        let auth = Authenticator {
            client: client.clone(),
            token_path,
            client_id,
            client_secret,
        };
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

impl Authenticator {
    pub async fn authenticate(&self, access_code: Secret<String>) -> Result<()> {
        let fetched_at = Utc::now();
        let token_response = self.fetch_access_token(&access_code).await?;

        info!(?token_response, "Response");
        let state = AuthData::from_response(token_response, fetched_at);

        self.write_auth_data(&state).await?;

        Ok(())
    }

    async fn access_token(&self) -> Result<SecretString> {
        let mut data: AuthData = match File::open(&self.token_path) {
            Ok(f) => serde_json::from_reader(f)?,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                bail!("No cached authentication token: {:?}", self.token_path)
            }
            Err(e) => return Err(e.into()),
        };

        // TODO: Check expiry
        let at = Utc::now();
        if data.is_expired(at) {
            debug!("Access token expired, refreshing");
            let resp = self.refresh_access_token(&data).await?;
            data = AuthData::from_response(resp, at);
            self.write_auth_data(&data).await?;
        }

        Ok(data.access_token)
    }

    async fn fetch_access_token(
        &self,
        access_code: &Secret<String>,
    ) -> Result<FetchAccessTokenResponse> {
        let url = Uri::builder()
            .scheme("https")
            .authority(SANDBOX_AUTH_HOST)
            .path_and_query("/connect/token")
            .build()?;
        let fetch_access_token_request = FetchAccessTokenRequest {
            grant_type: GrantType::AuthorizationCode,
            client_id: self.client_id.to_owned(),
            client_secret: self.client_secret.clone(),
            redirect_uri: REDIRECT_URI.into(),
            code: Some(access_code.clone()),
            refresh_token: None,
        };
        let token_response = perform_request(
            self.client
                .post(&url.to_string())
                .form(&fetch_access_token_request),
        )
        .await?
        .json::<FetchAccessTokenResponse>()
        .await?;
        Ok(token_response)
    }
    async fn refresh_access_token(&self, data: &AuthData) -> Result<FetchAccessTokenResponse> {
        let url = Uri::builder()
            .scheme("https")
            .authority(SANDBOX_AUTH_HOST)
            .path_and_query("/connect/token")
            .build()?;
        let fetch_access_token_request = FetchAccessTokenRequest {
            grant_type: GrantType::RefreshToken,
            client_id: self.client_id.to_owned(),
            client_secret: self.client_secret.clone(),
            redirect_uri: REDIRECT_URI.into(),
            code: None,
            refresh_token: Some(data.refresh_token.clone()),
        };

        let token_response = perform_request(
            self.client
                .post(&url.to_string())
                .form(&fetch_access_token_request),
        )
        .await?
        .json::<FetchAccessTokenResponse>()
        .await?;
        Ok(token_response)
    }

    async fn write_auth_data(&self, state: &AuthData) -> Result<()> {
        let mut tmpf = NamedTempFile::new_in(".")?;
        serde_json::to_writer_pretty(&mut tmpf, &state)?;
        tmpf.as_file_mut().flush()?;
        tmpf.persist(&self.token_path)?;
        debug!(token_path=?self.token_path, "Stored auth data");
        Ok(())
    }
}

impl AuthData {
    fn from_response(response: FetchAccessTokenResponse, fetched_at: DateTime<Utc>) -> Self {
        let FetchAccessTokenResponse {
            access_token,
            token_type,
            scope,
            refresh_token,
            expires_in,
        } = response;

        Self {
            access_token,
            token_type,
            scope,
            expires_at: Some(fetched_at + Duration::seconds(expires_in)),
            refresh_token,
        }
    }

    fn is_expired(&self, at: DateTime<Utc>) -> bool {
        if let Some(expiry) = self.expires_at {
            expiry <= at
        } else {
            true
        }
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
