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

use aide::{
    axum::{
        routing::{post, post_with},
        ApiRouter, IntoApiResponse,
    },
    transform::TransformOperation,
};
use axum::{extract::State, http::StatusCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use snarkos_node_router_core::{extractor::Json, try_api};

pub fn init_routes() -> ApiRouter {
    ApiRouter::new().api_route("/submit", post(submit_handler))
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]

struct SolutionRequest {
    solution: String,
    address: String,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct TestResponse {
    msg: String,
}
async fn submit_handler(Json(mut payload): Json<SolutionRequest>) -> impl IntoApiResponse {
    let response = TestResponse { msg: "submit".into() };
    (StatusCode::OK, Json(response))
}

fn submit_transform(op: TransformOperation) -> TransformOperation {
    op.description("Submit Method").response::<200, Json<TestResponse>>()
}
