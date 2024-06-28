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

use crate::Prover;
use aide::{
    axum::{routing::post_with, ApiRouter, IntoApiResponse},
    transform::TransformOperation,
};
use anyhow::bail;
use axum::response::IntoResponse;
use axum::{extract::State, http::StatusCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use snarkos_node_router_core::error::ServerError;
use snarkos_node_router_core::{extractor::Json, try_api};
use snarkvm::ledger::puzzle::{PartialSolution, Solution};
use snarkvm::ledger::store::ConsensusStorage;
use snarkvm::prelude::{Address, Network};
use std::convert::Infallible;

pub fn init_routes<N: Network, C: ConsensusStorage<N>>(prover: Prover<N, C>) -> ApiRouter {
    ApiRouter::new().api_route("/submit_solution", post_with(submit_handler::<N, C>, submit_docs)).with_state(prover)
}

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
//
// unsafe fn encode_field<T>(t: &T) -> String {
//     let slice = std::slice::from_raw_parts(t as *const T as *const u8, std::mem::size_of::<T>());
//     STANDARD.encode(slice)
// }
// unsafe fn decode_field<T>(s: &[u8]) -> eyre::Result<T> {
//     let slice = STANDARD.decode(s)?;
//     let ptr = slice.as_ptr() as *const T;
//     Ok(std::ptr::read(ptr))
// }
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SubmitSolutionResponse {
    pub msg: String,
}

async fn submit_handler<N: Network, C: ConsensusStorage<N>>(
    prover: State<Prover<N, C>>,
    Json(payload): Json<SubmitSolutionRequest>,
) -> impl IntoApiResponse {
    let solution = try_api!(payload.get_solution().map_err(|e| {
        warn!("Invalid solution: {:?}", payload);
        ServerError::<Infallible>::InvalidRequest(e.to_string())
    }));
    prover.broadcast_solution(solution).await;
    // TODO: enqueue solution to database and message queue
    let response = SubmitSolutionResponse { msg: "submitted".into() };
    (StatusCode::OK, Json(response)).into_response()
}

fn submit_docs(op: TransformOperation) -> TransformOperation {
    op.description("Submit Method").response::<200, Json<SubmitSolutionResponse>>()
}
