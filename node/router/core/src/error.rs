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

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use schemars::JsonSchema;
use serde_json::json;
use std::{
    convert::Infallible,
    fmt::{Debug, Display},
};
use thiserror::Error;

pub trait AppError: Debug + Display + Send + Sync + 'static {
    fn to_status_code(&self) -> StatusCode;
}
impl AppError for Infallible {
    fn to_status_code(&self) -> StatusCode {
        unreachable!()
    }
}
#[derive(Debug, Error, JsonSchema)]
pub enum ServerError<E = Infallible> {
    #[error("Internal error: {0}")]
    InternalError(String),
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("{0}")]
    AppError(E),
}
impl<E: AppError> ServerError<E> {
    pub fn to_status_code(&self) -> StatusCode {
        match self {
            ServerError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ServerError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            ServerError::AppError(e) => e.to_status_code(),
        }
    }
}
impl<E: AppError> From<E> for ServerError<E> {
    fn from(e: E) -> Self {
        Self::AppError(e)
    }
}

impl<E: AppError> IntoResponse for ServerError<E> {
    fn into_response(self) -> Response {
        let message = format!("{}", self);
        let status = self.to_status_code();
        (
            status,
            Json(json!({
                "error": message,
            })),
        )
            .into_response()
    }
}
#[macro_export]
macro_rules! try_api {
    (
        $($t:tt)*
    ) => {{
        let func = || -> Result<_, _> {
            $($t)*
        };

        match func() {
            Ok(v) => v,
            Err(e) => {
                let error: $crate::error::ServerError<_> = e.into();
                return axum::response::IntoResponse::into_response(error);
            }
        }

    }};
}
