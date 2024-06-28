use axum::http::StatusCode;
use schemars::JsonSchema;
use snarkos_node_router_core::error::AppError;
use thiserror::Error;

#[derive(Debug, JsonSchema, Error)]
pub enum ProverError {
    #[error("Invalid pool address: {0}")]
    InvalidPoolAddress(String),
}

impl AppError for ProverError {
    fn to_status_code(&self) -> StatusCode {
        StatusCode::BAD_REQUEST
    }
}
