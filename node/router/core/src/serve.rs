use crate::docs::docs_routes;
use crate::extractor::ip::AxumClientIpSourceConfig;
use aide::axum::ApiRouter;
use aide::openapi::{Info, OpenApi};
use axum::extract::Request;
use axum::{Extension, ServiceExt};
use eyre::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower::{Layer, ServiceBuilder};
use tower_http::cors::CorsLayer;
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::trace::TraceLayer;
use tracing::*;
use url::Url;

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServeAxumConfig {
    #[serde(default = "String::new")]
    pub title: String,
    #[serde_as(as = "serde_with::DisplayFromStr")]
    pub url: Url,
    #[serde(default)]
    pub ip_source: AxumClientIpSourceConfig,
}
impl ServeAxumConfig {
    pub fn empty() -> Self {
        Self {
            title: String::new(),
            url: Url::parse("http://localhost:8080").unwrap(),
            ip_source: AxumClientIpSourceConfig::Insecure,
        }
    }
}
#[derive(Debug, Clone)]
pub struct ServeAxum {
    title: String,
    url: Url,
    ip: AxumClientIpSourceConfig,
}
impl ServeAxum {
    pub fn new(config: ServeAxumConfig) -> Self {
        aide::gen::extract_schemas(true);
        aide::gen::on_error(|error| warn!("Error generating openapi.json: {}", error));

        Self { title: config.title, url: config.url, ip: config.ip_source }
    }
    pub async fn serve(self, routes: ApiRouter) -> Result<()> {
        let addr = self.url.authority();
        ensure!(self.url.port().is_some(), "Port is not specified in {}", self.url);

        let base_path = self.url.path();
        let cors = CorsLayer::permissive();
        let mut api = OpenApi { info: Info { title: self.title.clone(), ..Info::default() }, ..OpenApi::default() };
        let docs_url = self.url.join("docs")?;
        let docs_path = docs_url.path();

        let mut router = ApiRouter::new()
            .nest(base_path, routes)
            .nest(docs_path, docs_routes(&docs_path, &self.title))
            .finish_api(&mut api)
            .layer(Extension(Arc::new(api)))
            .layer(cors)
            .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

        if let AxumClientIpSourceConfig::Secure(config) = self.ip {
            router = router.layer(config.into_extension());
        }

        // NormalizePathLayer must be applied before the Router
        let router = NormalizePathLayer::trim_trailing_slash().layer(router);
        let listener = TcpListener::bind(&addr).await.with_context(|| format!("failed to bind to {}", addr))?;

        info!("Starting {} server at {}", self.title, self.url);
        info!("OpenAPI docs are accessible at {}", docs_url);
        axum::serve(listener, ServiceExt::<Request>::into_make_service_with_connect_info::<SocketAddr>(router)).await?;
        Ok(())
    }
}
