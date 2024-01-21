use std::{
    fs::File,
    io::{ErrorKind, Write},
    path::PathBuf,
};

use anyhow::{bail, Result};
use chrono::{DateTime, Duration, Utc};
use reqwest::Client;
use secrecy::{Secret, SecretString};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use tokio::task::spawn_blocking;
use tracing::{debug, info};

use crate::Environment;
use crate::{perform_request, serialize_optional_secret, serialize_secret};

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
    scope: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ClientCreds {
    id: String,
    secret: SecretString,
}

pub(crate) struct Authenticator {
    client: Client,
    env: Environment,
    token_path: PathBuf,
    credentials: ClientCreds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthData {
    #[serde(serialize_with = "serialize_secret")]
    access_token: SecretString,
    // TODO: Remove option
    #[serde(default)]
    expires_at: Option<DateTime<Utc>>,
    token_type: String,
    #[serde(serialize_with = "serialize_secret")]
    refresh_token: SecretString,
    scope: Option<String>,
    redirect_uri: String,
}

impl Authenticator {
    pub(crate) fn new(
        client: Client,
        env: Environment,
        token_path: PathBuf,
        credentials: &ClientCreds,
    ) -> Authenticator {
        Self {
            client,
            env,
            token_path,
            credentials: credentials.clone(),
        }
    }

    pub fn client_id(&self) -> &str {
        &self.credentials.id
    }

    pub(crate) async fn authenticate(
        &self,
        access_code: Secret<String>,
        redirect_uri: &str,
    ) -> Result<()> {
        let fetched_at = Utc::now();
        let token_response = self.fetch_access_token(&access_code, redirect_uri).await?;

        info!(?token_response, "Response");
        let state = AuthData::from_response(token_response, fetched_at, redirect_uri.to_owned());

        self.write_auth_data(&state).await?;

        Ok(())
    }

    pub(crate) async fn access_token(&self) -> Result<SecretString> {
        let token_path = self.token_path.to_owned();
        let mut data: AuthData = spawn_blocking(move || match File::open(&token_path) {
            Ok(f) => Ok(serde_json::from_reader(f)?),
            Err(e) if e.kind() == ErrorKind::NotFound => {
                bail!("No cached authentication token: {:?}", token_path)
            }
            Err(e) => Err(e.into()),
        })
        .await??;

        // TODO: Check expiry
        let at = Utc::now();
        if data.is_expired(at) {
            debug!("Access token expired, refreshing");
            data = self.refresh_access_token(&data, at).await?;
            self.write_auth_data(&data).await?;
        }

        Ok(data.access_token)
    }

    async fn fetch_access_token(
        &self,
        access_code: &Secret<String>,
        redirect_uri: &str,
    ) -> Result<FetchAccessTokenResponse> {
        let url = self
            .env
            .auth_url_builder()
            .path_and_query("/connect/token")
            .build()?;
        let fetch_access_token_request = FetchAccessTokenRequest {
            grant_type: GrantType::AuthorizationCode,
            client_id: self.credentials.id.clone(),
            client_secret: self.credentials.secret.clone(),
            redirect_uri: redirect_uri.to_owned(),
            code: Some(access_code.clone()),
            refresh_token: None,
        };
        let token_response = perform_request(
            self.client
                .post(&url.to_string())
                .form(&fetch_access_token_request),
        )
        .await?;
        Ok(token_response)
    }
    async fn refresh_access_token(&self, data: &AuthData, at: DateTime<Utc>) -> Result<AuthData> {
        let url = self
            .env
            .auth_url_builder()
            .path_and_query("/connect/token")
            .build()?;
        let fetch_access_token_request = FetchAccessTokenRequest {
            grant_type: GrantType::RefreshToken,
            client_id: self.credentials.id.to_owned(),
            client_secret: self.credentials.secret.clone(),
            redirect_uri: data.redirect_uri.clone(),
            code: None,
            refresh_token: Some(data.refresh_token.clone()),
        };

        let token_response = perform_request(
            self.client
                .post(&url.to_string())
                .form(&fetch_access_token_request),
        )
        .await?;

        Ok(AuthData::from_response(
            token_response,
            at,
            data.redirect_uri.clone(),
        ))
    }

    async fn write_auth_data(&self, state: &AuthData) -> Result<()> {
        let state = state.clone();
        let token_path = self.token_path.to_owned();
        spawn_blocking(move || {
            let mut tmpf = NamedTempFile::new_in(".")?;
            serde_json::to_writer_pretty(&mut tmpf, &state)?;
            tmpf.as_file_mut().flush()?;
            tmpf.persist(&token_path)?;
            debug!(?token_path, "Stored auth data");
            Ok(())
        })
        .await?
    }
}

impl AuthData {
    fn from_response(
        response: FetchAccessTokenResponse,
        fetched_at: DateTime<Utc>,
        redirect_uri: String,
    ) -> Self {
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
            redirect_uri,
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
