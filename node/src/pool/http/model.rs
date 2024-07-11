use crate::handle::SubmitSolutionRequest;
use anyhow::{bail, Error, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use snarkvm::ledger::puzzle::{PartialSolution, Solution};
use snarkvm::prelude::{Address, Network};
use std::net::SocketAddr;

/// A helper struct around a puzzle solution.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct SolutionMessage {
    /// The partial solution.
    pub partial_solution: PartialSolutionMessage,
    /// The solution target.
    pub target: u64,
}

impl<N: Network> TryFrom<SolutionMessage> for Solution<N> {
    type Error = Error;

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
    type Error = Error;

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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PuzzleResponse {
    pub epoch_hash: String,
    pub coinbase_target: u64,
    pub difficulty: u64,
    pub block_round: u64,
}
#[async_trait]
pub trait ProverErased: Send + Sync {
    async fn submit_solution(&self, peer_ip: SocketAddr, request: SubmitSolutionRequest) -> Result<bool, Error>;
    fn pool_address(&self) -> String;
    fn puzzle(&self) -> Result<PuzzleResponse>;
}
