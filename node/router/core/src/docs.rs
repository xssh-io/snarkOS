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

use std::sync::Arc;

use aide::axum::routing::get;
use aide::{
    axum::{ApiRouter, IntoApiResponse},
    openapi::OpenApi,
    scalar::Scalar,
};
use axum::{response::IntoResponse, Extension};

use crate::extractor::Json;

pub fn docs_routes(base: &str, title: &str) -> ApiRouter {
    let docs = format!("{}/openapi.json", base);
    let router: ApiRouter = ApiRouter::new()
        .api_route("/", Scalar::new(docs).with_title(title).axum_route())
        .api_route("/openapi.json", get(serve_docs));

    router
}

async fn serve_docs(Extension(api): Extension<Arc<OpenApi>>) -> impl IntoApiResponse {
    Json(api).into_response()
}
