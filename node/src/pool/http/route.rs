use std::sync::Arc;

use crate::model::ProverErased;
use crate::ws::WsConfig;
use crate::{handle, ws};
use aide::axum::{
    routing::{get_with, post_with},
    ApiRouter,
};

pub fn init_routes(prover: Arc<dyn ProverErased>, ws_config: WsConfig) -> ApiRouter {
    ApiRouter::new()
        .api_route("/solution", post_with(handle::submit_solution_handler, handle::submit_docs))
        .api_route("/pool_address", get_with(handle::pool_address_handler, handle::pool_address_docs))
        .api_route("/puzzle", get_with(handle::puzzle_handler, handle::puzzle_docs))
        .api_route("/ws", get_with(ws::ws_handler, ws::ws_handler_docs).with_state(ws_config.clone()))
        .with_state(prover)
}
