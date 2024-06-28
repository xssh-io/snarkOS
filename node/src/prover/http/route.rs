use aide::axum::{
    routing::{get_with, post_with},
    ApiRouter,
};
use snarkvm::ledger::store::ConsensusStorage;
use snarkvm::prelude::Network;

use crate::{handle, Prover};

pub fn init_routes<N: Network, C: ConsensusStorage<N>>(prover: Prover<N, C>) -> ApiRouter {
    ApiRouter::new()
        .api_route("/submit_solution", post_with(handle::submit_solution_handler::<N, C>, handle::submit_docs))
        .api_route("/pool_address", get_with(handle::pool_address_handler, handle::pool_address_docs))
        .with_state(prover)
}
