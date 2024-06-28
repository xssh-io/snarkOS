use crate::prover::http::error::ProverError;
use crate::{NodeInterface, Prover};
use aide::axum::IntoApiResponse;
use aide::transform::TransformOperation;
use anyhow::bail;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use snarkos_node_router_core::error::ServerError;
use snarkos_node_router_core::extractor::Json;
use snarkvm::ledger::puzzle::{PartialSolution, Solution};
use snarkvm::ledger::store::ConsensusStorage;
use snarkvm::prelude::{Address, Network};
use std::convert::Infallible;
use std::ops::Deref;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
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

/// A helper struct around a puzzle solution.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct SolutionMessage {
    /// The partial solution.
    pub partial_solution: PartialSolutionMessage,
    /// The solution target.
    pub target: u64,
}

impl<N: Network> TryFrom<SolutionMessage> for Solution<N> {
    type Error = anyhow::Error;

    fn try_from(value: SolutionMessage) -> Result<Self, Self::Error> {
        let partial = value.partial_solution.try_into()?;
        let this = Solution::new(partial, value.target);
        Ok(this)
    }
}

impl<N: Network> From<Solution<N>> for SolutionMessage {
    fn from(value: Solution<N>) -> Self {
        Self { partial_solution: value.partial_solution().clone().into(), target: value.target() }
    }
}

/// The partial solution for the puzzle from a prover.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct PartialSolutionMessage {
    /// The solution ID.
    pub solution_id: String,
    /// The epoch hash.
    pub epoch_hash: String,
    /// The address of the prover.
    pub address: String,
    /// The counter for the solution.
    pub counter: u64,
}

impl<N: Network> TryFrom<PartialSolutionMessage> for PartialSolution<N> {
    type Error = anyhow::Error;

    fn try_from(value: PartialSolutionMessage) -> Result<Self, Self::Error> {
        let epoch_hash = match value.epoch_hash.parse::<N::BlockHash>() {
            Ok(ok) => ok,
            Err(_) => bail!("Invalid epoch hash: {}", value.epoch_hash),
        };
        let address = match value.address.parse::<Address<N>>() {
            Ok(ok) => ok,
            Err(_) => bail!("Invalid address: {}", value.address),
        };
        let this = Self::new(epoch_hash, address, value.counter)?;
        Ok(this)
    }
}

impl<N: Network> From<PartialSolution<N>> for PartialSolutionMessage {
    fn from(value: PartialSolution<N>) -> Self {
        Self {
            solution_id: value.id().to_string(),
            epoch_hash: value.epoch_hash().to_string(),
            address: value.address().to_string(),
            counter: value.counter(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SubmitSolutionResponse {
    pub msg: String,
}

pub async fn submit_solution_handler<N: Network, C: ConsensusStorage<N>>(
    prover: State<Prover<N, C>>,
    Json(payload): Json<SubmitSolutionRequest>,
) -> impl IntoApiResponse {
    let solution = match payload.get_solution() {
        Ok(ok) => ok,
        Err(e) => {
            warn!("Invalid solution: {:?}", payload);
            return ServerError::<Infallible>::InvalidRequest(e.to_string()).into_response();
        }
    };
    if solution.address() != prover.address() {
        return ServerError::AppError(ProverError::InvalidPoolAddress(payload.solution.partial_solution.address))
            .into_response();
    }

    prover.broadcast_solution(solution).await;
    // TODO: enqueue solution to database and message queue
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

pub async fn pool_address_handler<N: Network, C: ConsensusStorage<N>>(
    prover: State<Prover<N, C>>,
) -> impl IntoApiResponse {
    let prover: &Prover<N, C> = prover.deref();
    let pool_address = prover.address().to_string();
    let response = PoolAddressResponse { pool_address };
    (StatusCode::OK, Json(response)).into_response()
}

pub fn pool_address_docs(op: TransformOperation) -> TransformOperation {
    op.description("Pool Address Method").response::<200, Json<PoolAddressResponse>>()
}
