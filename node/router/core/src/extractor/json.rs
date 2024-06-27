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

use crate::error::ServerError;
use aide::operation::OperationIo;
use axum::extract::FromRequest;
use axum::response::IntoResponse;
use axum_jsonschema::JsonSchemaRejection;
use serde::Serialize;

/// This Json extractor is different from axum::extract::Json because the error will be reported
/// in JSON format.
#[derive(FromRequest, OperationIo)]
#[from_request(via(axum_jsonschema::Json), rejection(ServerError))]
#[aide(input_with = "axum_jsonschema::Json<T>", output_with = "axum_jsonschema::Json<T>", json_schema)]
pub struct Json<T>(pub T);

impl<T> IntoResponse for Json<T>
where
    T: Serialize,
{
    fn into_response(self) -> axum::response::Response {
        axum::Json(self.0).into_response()
    }
}
impl<E> From<JsonSchemaRejection> for ServerError<E> {
    fn from(rejection: JsonSchemaRejection) -> Self {
        match rejection {
            JsonSchemaRejection::Json(j) => Self::InvalidRequest(j.to_string()),
            JsonSchemaRejection::Serde(x) => Self::InvalidRequest(x.to_string()),
            JsonSchemaRejection::Schema(s) => Self::InvalidRequest(format!("Invalid schema: {:?}", s)),
        }
    }
}
