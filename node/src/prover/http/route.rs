use std::sync::Arc;

use aide::axum::{
    routing::{get_with, post_with},
    ApiRouter,
};

use crate::handle;
use crate::model::ProverErased;

pub fn init_routes(prover: Arc<dyn ProverErased>) -> ApiRouter {
    ApiRouter::new()
        .api_route("/submit_solution", post_with(handle::submit_solution_handler, handle::submit_docs))
        .api_route("/pool_address", get_with(handle::pool_address_handler, handle::pool_address_docs))
        .with_state(prover)
}
