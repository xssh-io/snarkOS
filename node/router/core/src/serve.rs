// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkOS library.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at:
// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::docs::docs_routes;
use aide::axum::ApiRouter;
use aide::openapi::{Info, OpenApi};
use axum::extract::Request;
use axum::{Extension, ServiceExt};
use axum_client_ip::SecureClientIpSource;
use eyre::{ensure, Result};
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
}
#[derive(Debug, Clone)]
pub struct ServeAxum {
    title: String,
    url: Url,
}
impl ServeAxum {
    pub fn new(config: ServeAxumConfig) -> Self {
        aide::gen::extract_schemas(true);
        aide::gen::on_error(|error| warn!("Error generating openapi.json: {}", error));

        Self { title: config.title, url: config.url }
    }
    pub async fn serve(self, routes: ApiRouter) -> Result<()> {
        let addr = self.url.authority();
        ensure!(self.url.port().is_some(), "Port is not specified in {}", self.url);

        let base_path = self.url.path();
        let cors = CorsLayer::permissive();
        let mut api = OpenApi { info: Info { title: self.title.clone(), ..Info::default() }, ..OpenApi::default() };
        let docs_url = self.url.join("docs")?;
        let docs_path = docs_url.path();

        let router = ApiRouter::new()
            .nest(base_path, routes)
            .nest(docs_path, docs_routes(&docs_path, &self.title))
            .finish_api(&mut api)
            .layer(Extension(Arc::new(api)))
            .layer(cors)
            .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
            .layer(SecureClientIpSource::ConnectInfo.into_extension());

        // NormalizePathLayer must be applied before the Router
        let router = NormalizePathLayer::trim_trailing_slash().layer(router);
        let listens = TcpListener::bind(&addr).await?;

        info!("Starting {} server at {}", self.title, self.url);
        info!("OpenAPI docs are accessible at {}", docs_url);
        axum::serve(listens, ServiceExt::<Request>::into_make_service_with_connect_info::<SocketAddr>(router)).await?;
        Ok(())
    }
}
