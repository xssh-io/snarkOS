use std::sync::Arc;

use crate::model::ProverErased;
use crate::ws::WsConfig;
use crate::{handle, ws};
use aide::axum::{
    routing::{get_with, post_with},
    ApiRouter,
};
use snarkvm::prelude::Network;

pub fn init_routes<N: Network>(prover: Arc<dyn ProverErased<N>>, ws_config: WsConfig) -> ApiRouter {
    ApiRouter::new()
        .api_route("/solution", post_with(handle::submit_solution_handler, handle::submit_docs))
        .api_route("/pool_address", get_with(handle::pool_address_handler, handle::pool_address_docs))
        .api_route("/puzzle", get_with(handle::puzzle_handler, handle::puzzle_docs))
        .api_route("/ws", get_with(ws::ws_handler, ws::ws_handler_docs).with_state(ws_config.clone()))
        .with_state(prover)
}
