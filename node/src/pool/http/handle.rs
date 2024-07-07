use crate::model::{ProverErased, PuzzleResponse, SolutionMessage};
use aide::axum::IntoApiResponse;
use aide::transform::TransformOperation;
use aide::NoApi;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use snarkos_node_router_core::error::ServerError;
use snarkos_node_router_core::extractor::ip::SecureClientIp;
use snarkos_node_router_core::extractor::Json;
use snarkvm::ledger::puzzle::Solution;
use snarkvm::prelude::Network;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SubmitSolutionRequest {
    pub address: String,
    pub solution: SolutionMessage,
}

impl SubmitSolutionRequest {
    pub fn get_solution<N: Network>(&self) -> Result<Solution<N>, anyhow::Error> {
        let solution = Solution::<N>::try_from(self.solution.clone())?;
        Ok(solution)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SubmitSolutionResponse {
    pub msg: String,
}

pub async fn submit_solution_handler(
    prover: State<Arc<dyn ProverErased>>,
    ip_addr: NoApi<SecureClientIp>,
    Json(payload): Json<SubmitSolutionRequest>,
) -> impl IntoApiResponse {
    info!("Received solution from: {}", ip_addr.0 .0);
    let ip_addr = SocketAddr::new(ip_addr.0 .0, 0);
    if let Err(err) = prover.submit_solution(ip_addr, payload).await {
        warn!("Failed to submit solution: {:?}", err);
        return ServerError::<Infallible>::InvalidRequest(err.to_string()).into_response();
    }
    let response = SubmitSolutionResponse { msg: "submitted".into() };
    (StatusCode::OK, Json(response)).into_response()
}

pub fn submit_docs(op: TransformOperation) -> TransformOperation {
    op.description("Submit Method").response::<200, Json<SubmitSolutionResponse>>()
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct PoolAddressResponse {
    pub pool_address: String,
}

pub async fn pool_address_handler(prover: State<Arc<dyn ProverErased>>) -> impl IntoApiResponse {
    let pool_address = prover.pool_address();
    let response = PoolAddressResponse { pool_address };
    (StatusCode::OK, Json(response)).into_response()
}

pub fn pool_address_docs(op: TransformOperation) -> TransformOperation {
    op.description("Pool Address Method").response::<200, Json<PoolAddressResponse>>()
}

pub async fn puzzle_handler(prover: State<Arc<dyn ProverErased>>) -> impl IntoApiResponse {
    let puzzle = match prover.puzzle() {
        Ok(puzzle) => puzzle,
        Err(err) => {
            warn!("Failed to get puzzle: {:?}", err);
            return ServerError::<Infallible>::InternalError(format!("Failed to get puzzle: {:?}", err))
                .into_response();
        }
    };
    (StatusCode::OK, Json(puzzle)).into_response()
}
pub fn puzzle_docs(op: TransformOperation) -> TransformOperation {
    op.description("Puzzle Method").response::<200, Json<PuzzleResponse>>()
}
