use crate::{NodeInterface, Pool};
use anyhow::{bail, Error};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use snarkvm::ledger::puzzle::{PartialSolution, Solution};
use snarkvm::ledger::store::ConsensusStorage;
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
#[async_trait]
pub trait ProverErased: Send + Sync {
    async fn submit_solution(
        &self,
        peer_ip: SocketAddr,
        address: String,
        solution: SolutionMessage,
    ) -> Result<(), Error>;
    fn pool_address(&self) -> String;
}
#[async_trait]
impl<N: Network, C: ConsensusStorage<N>> ProverErased for Pool<N, C> {
    async fn submit_solution(
        &self,
        peer_ip: SocketAddr,
        address: String,
        solution: SolutionMessage,
    ) -> Result<(), Error> {
        let solution = match solution.try_into() {
            Ok(ok) => ok,
            Err(e) => bail!("Invalid solution: {}", e),
        };
        if address != self.address().to_string() {
            bail!("Invalid pool address: {}", address);
        }
        self.confirm_and_broadcast_solution(peer_ip, solution).await
    }

    fn pool_address(&self) -> String {
        let address = self.address();
        address.to_string()
    }
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub enum WsMessage {
    NewEpoch { epoch: u64 },
}
