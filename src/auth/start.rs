use std::{borrow::Cow, collections::HashMap, sync::Arc};

use anyhow::{anyhow, Context, Result};
use askama::Template;
use axum::{
    extract::State,
    http::uri,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use hyper::Uri;
use tracing::info;

use crate::{auth::WebResult, Environment, TlClient};

use super::WebError;

#[derive(Clone)]
pub(crate) struct Start {
    client: Arc<TlClient>,
    base_url: Uri,
}

#[derive(Template)]
#[template(path = "auth_start.html")]
struct StartTemplate {
    url: hyper::Uri,
}

#[derive(Debug)]
struct AskamaTemplate<T>(T);

pub(crate) fn routes(client: Arc<TlClient>, base_url: Uri) -> Router {
    Router::new()
        .route("/", get(Start::index))
        .with_state(Start { client, base_url })
}

// #[debug_handler]
impl Start {
    async fn index(State(state): State<Start>) -> WebResult<impl IntoResponse> {
        Ok(state.handle_index()?)
    }

    fn handle_index(&self) -> Result<impl IntoResponse> {
        let host = match self.client.env() {
            Environment::Sandbox => "auth.truelayer-sandbox.com",
            Environment::Live => "auth.truelayer.com",
        };

        let providers = match self.client.env() {
            Environment::Sandbox => "uk-cs-mock uk-ob-all uk-oauth-all",
            Environment::Live => "uk-ob-all uk-oauth-all",
        };
        let redirect_url = Uri::builder()
            .scheme(
                self.base_url
                    .scheme()
                    .cloned()
                    .ok_or(anyhow!("Base URL missing scheme: {}", self.base_url))?,
            )
            .authority(
                self.base_url
                    .authority()
                    .cloned()
                    .ok_or(anyhow!("Base URL missing authority: {}", self.base_url))?,
            )
            .path_and_query("/start-redirect")
            .build()
            .context("Build redirect URI")?;

        info!(%redirect_url);

        let query = HashMap::<&str, Cow<'_, str>>::from([
        ("response_type", "code".into()),
        ("client_id", self.client.client_id().into()),
        ("redirect_uri", redirect_url.to_string().into() ),
        (
            "scope",
            "info accounts balance cards transactions direct_debits standing_orders offline_access".into(),
        ),
        ("providers", providers.into(),),
    ]);
        let qs = serde_urlencoded::to_string(query).context("encode query")?;
        let u = uri::Builder::new()
            .scheme("https")
            .authority(host)
            .path_and_query(format!("/?{}", qs))
            .build()
            .map_err(anyhow::Error::from)?;
        let template = StartTemplate { url: u };
        Ok(AskamaTemplate(template))
    }
}

impl<T: Template> IntoResponse for AskamaTemplate<T> {
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(html) => Html(html).into_response(),
            Err(err) => WebError::from(anyhow::Error::from(err)).into_response(),
        }
    }
}
