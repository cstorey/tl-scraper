use std::{
    io,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Duration, Utc};
use clap::{Args, Parser};
use color_eyre::Result;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument, warn};

use crate::client::BankDataClient;

#[derive(Debug, Parser)]
pub struct Cmd {
    #[clap(flatten)]
    auth: AuthArgs,
}

#[derive(Debug, Clone, Args)]
pub struct AuthArgs {
    #[clap(short = 's', long = "secrets", help = "Secrets file")]
    secrets: PathBuf,
    #[clap(short = 't', long = "token", help = "Token file")]
    token: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Secrets {
    secret_id: String,
    secret_key: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TokenRefreshReq {
    refresh: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TokenRefreshResp {
    access: String,
    access_expires: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TokenPair {
    access: String,
    access_expires: i64,
    refresh: String,
    refresh_expires: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Token {
    pub(crate) access: String,
    access_expires: DateTime<Utc>,
    refresh: String,
    refresh_expires: DateTime<Utc>,
}

impl Cmd {
    #[instrument("auth", skip_all)]
    pub(crate) async fn run(&self) -> Result<()> {
        let _ = self.auth.load_token().await?;

        Ok(())
    }
}

impl Token {
    pub(crate) fn from_token_pair(authed_at: DateTime<Utc>, gctoken: &TokenPair) -> Token {
        Token {
            access: gctoken.access.clone(),
            access_expires: authed_at + Duration::seconds(gctoken.access_expires),
            refresh: gctoken.refresh.clone(),
            refresh_expires: authed_at + Duration::seconds(gctoken.refresh_expires),
        }
    }

    fn refreshed(&self, authed_at: DateTime<Utc>, refresh: &TokenRefreshResp) -> Token {
        Token {
            access: refresh.access.clone(),
            access_expires: authed_at + Duration::seconds(refresh.access_expires),
            refresh: self.refresh.clone(),
            refresh_expires: self.refresh_expires,
        }
    }
}

#[instrument(skip_all, fields(?path))]
async fn store_token(path: &Path, tok: &Token) -> Result<()> {
    let buf = serde_json::to_vec(&tok)?;
    tokio::fs::write(&path, buf).await?;
    debug!(?path, "Stored token");
    Ok(())
}

#[instrument(skip_all, fields(?path))]
async fn load_token(path: &Path) -> Result<Option<Token>> {
    let buf = match tokio::fs::read(path).await {
        Ok(buf) => buf,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            warn!("Token file not found");
            return Ok(None);
        }
        Err(err) => return Err(err.into()),
    };

    let mut token = serde_json::from_slice::<Token>(&buf)?;

    let now = Utc::now();

    if token.access_expires <= now {
        debug!(expired_at=?token.access_expires, "Access token expired, refreshing");
        token = refresh_token(&token).await?;

        store_token(path, &token).await?;
    }

    debug!(?path, "Loaded token");

    Ok(Some(token))
}

#[instrument(skip_all)]
async fn refresh_token(token: &Token) -> Result<Token> {
    let client = BankDataClient::unauthenticated();

    let authed_at = Utc::now();

    let refresh = client
        .post::<TokenRefreshResp>(
            "/api/v2/token/refresh/",
            &TokenRefreshReq {
                refresh: token.refresh.clone(),
            },
        )
        .await?;

    let tok = token.refreshed(authed_at, &refresh);

    Ok(tok)
}

#[instrument(skip_all, fields(?path))]
async fn load_secrets(path: &Path) -> Result<Secrets> {
    let buf = tokio::fs::read(&path).await?;
    let secrets = serde_json::from_slice(&buf)?;

    debug!(?path, "Loaded secrets");

    Ok(secrets)
}

impl AuthArgs {
    pub(crate) async fn load_token(&self) -> Result<Token> {
        let authed_at = Utc::now();

        if let Some(token) = load_token(&self.token).await? {
            if token.refresh_expires >= authed_at {
                return Ok(token);
            } else {
                debug!("Refresh token expired; refreshing");
            }
        }

        let secrets = load_secrets(&self.secrets).await?;

        info!("Authing");

        let client = BankDataClient::unauthenticated();

        let tokens = client
            .post::<TokenPair>("/api/v2/token/new/", &secrets)
            .await?;

        let tok = Token::from_token_pair(authed_at, &tokens);

        store_token(&self.token, &tok).await?;

        Ok(tok)
    }
}
