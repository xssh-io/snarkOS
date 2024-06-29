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

mod http;
mod router;
use crate::tcp::{
    protocols::{Disconnect, Handshake, OnConnect, Reading, Writing},
    P2P,
};
use crate::traits::NodeInterface;
pub use http::*;
use snarkos_account::Account;
use snarkos_node_bft::ledger_service::ProverLedgerService;
use snarkos_node_router::{
    messages::{Message, NodeType, UnconfirmedSolution},
    Heartbeat, Inbound, Outbound, Router, Routing,
};
use snarkos_node_sync::{BlockSync, BlockSyncMode};
use snarkvm::{
    ledger::narwhal::Data,
    prelude::{
        block::{Block, Header},
        puzzle::{Puzzle, Solution},
        store::ConsensusStorage,
        Network,
    },
    synthesizer::VM,
};

use self::http::handle::SubmitSolutionRequest;
use crate::handle::PoolAddressResponse;
use crate::route::init_routes;
use crate::ws::WsConfig;
use aleo_std::StorageMode;
use anyhow::{Context, Result};
use colored::Colorize;
use core::{marker::PhantomData, time::Duration};
use parking_lot::{Mutex, RwLock};
use rand::{rngs::OsRng, CryptoRng, Rng};
use snarkos_node_bft::helpers::fmt_id;
use snarkos_node_router_core::serve::{ServeAxum, ServeAxumConfig};
use snarkvm::prelude::Address;
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
};
use tokio::task::JoinHandle;

/// A prover is a light node, capable of producing proofs for consensus.
#[derive(Clone)]
pub struct Prover<N: Network, C: ConsensusStorage<N>> {
    /// The router of the node.
    router: Router<N>,
    /// The sync module.
    sync: Arc<BlockSync<N>>,
    /// The genesis block.
    genesis: Block<N>,
    /// The puzzle.
    puzzle: Puzzle<N>,
    /// The latest epoch hash.
    latest_epoch_hash: Arc<RwLock<Option<N::BlockHash>>>,
    /// The latest block header.
    latest_block_header: Arc<RwLock<Option<Header<N>>>>,
    /// The number of puzzle instances.
    puzzle_instances: Arc<AtomicU8>,
    /// The maximum number of puzzle instances.
    max_puzzle_instances: u8,
    /// The spawned handles.
    handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
    /// The shutdown signal.
    shutdown: Arc<AtomicBool>,
    /// Node Type
    node_type: NodeType,
    /// pool_address
    pool_address: Option<Address<N>>,
    pool_base_url: Option<String>,
    http_client: reqwest::Client,
    ws_config: WsConfig,
    /// PhantomData.
    _phantom: PhantomData<C>,
}

impl<N: Network, C: ConsensusStorage<N>> Prover<N, C> {
    /// Initializes a new prover node.
    pub async fn new(
        node_ip: SocketAddr,
        account: Account<N>,
        trusted_peers: &[SocketAddr],
        genesis: Block<N>,
        storage_mode: StorageMode,
        shutdown: Arc<AtomicBool>,
        node_type: NodeType,
        pool_base_url: Option<String>,
    ) -> Result<Self> {
        // Initialize the signal handler.
        let signal_node = Self::handle_signals(shutdown.clone());

        // Initialize the ledger service.
        let ledger_service = Arc::new(ProverLedgerService::new());
        // Initialize the sync module.
        let sync = BlockSync::new(BlockSyncMode::Router, ledger_service.clone());
        // Determine if the prover should allow external peers.
        let allow_external_peers = true;

        // Initialize the node router.
        let router = Router::new(
            node_ip,
            NodeType::Prover,
            account,
            trusted_peers,
            Self::MAXIMUM_NUMBER_OF_PEERS as u16,
            allow_external_peers,
            matches!(storage_mode, StorageMode::Development(_)),
        )
        .await?;
        // Compute the maximum number of puzzle instances.
        let max_puzzle_instances = num_cpus::get().saturating_sub(2).clamp(1, 6);
        let client = reqwest::Client::new();
        let pool_address;
        if let Some(base_url) = pool_base_url.as_ref() {
            let response = client.get(format!("{}/pool_address", base_url)).send().await?;
            let resp: PoolAddressResponse = response.json().await?;
            pool_address =
                Some(resp.pool_address.parse().with_context(|| format!("Invalid address: {}", resp.pool_address))?);
        } else {
            pool_address = None;
        }
        // Initialize the node.
        let node = Self {
            router,
            sync: Arc::new(sync),
            genesis,
            puzzle: VM::<N, C>::new_puzzle()?,
            latest_epoch_hash: Default::default(),
            latest_block_header: Default::default(),
            puzzle_instances: Default::default(),
            max_puzzle_instances: u8::try_from(max_puzzle_instances)?,
            handles: Default::default(),
            shutdown,
            node_type,
            pool_address,
            pool_base_url,
            http_client: reqwest::Client::new(),
            ws_config: WsConfig::new(),
            _phantom: Default::default(),
        };
        // Initialize the routing.
        node.initialize_routing().await;
        if let NodeType::ProverPool = node.node_type {
            node.enable_http_request().await;
        }

        // Initialize the puzzle.
        node.initialize_puzzle().await;
        // Initialize the notification message loop.
        node.handles.lock().push(crate::start_notification_message_loop());
        // Pass the node to the signal handler.
        let _ = signal_node.set(node.clone());
        // Return the node.
        Ok(node)
    }
    async fn enable_http_request(&self) {
        let config = ServeAxumConfig {
            title: "Aleo Prover Pool".to_string(),
            url: self
                .pool_base_url
                .as_deref()
                .expect("expected to have pool-base-url")
                .parse()
                .expect("failed to parse url"),
        };
        let this = self.clone();
        let ws_config = self.ws_config.clone();
        tokio::spawn(async move {
            if let Err(err) = ServeAxum::new(config).serve(init_routes(Arc::new(this), ws_config)).await {
                error!("Failed to serve HTTP: {:?}", err);
            }
        });
    }
}

#[async_trait]
impl<N: Network, C: ConsensusStorage<N>> NodeInterface<N> for Prover<N, C> {
    /// Shuts down the node.
    async fn shut_down(&self) {
        info!("Shutting down...");

        // Shut down the puzzle.
        debug!("Shutting down the puzzle...");
        self.shutdown.store(true, Ordering::Relaxed);

        // Abort the tasks.
        debug!("Shutting down the prover...");
        self.handles.lock().iter().for_each(|handle| handle.abort());

        // Shut down the router.
        self.router.shut_down().await;

        info!("Node has shut down.");
    }
}

impl<N: Network, C: ConsensusStorage<N>> Prover<N, C> {
    /// Initialize a new instance of the puzzle.
    async fn initialize_puzzle(&self) {
        match self.node_type {
            NodeType::ProverPool => {
                self.enable_http_request().await;
                let prover = self.clone();
                // TODO: still need to dispatch tasks in side prover.puzzle_loop()
                self.handles.lock().push(tokio::spawn(async move {
                    prover.puzzle_loop().await;
                }));
                return;
            }
            NodeType::Prover | NodeType::ProverPoolWorker => {
                for _ in 0..self.max_puzzle_instances {
                    let prover = self.clone();
                    self.handles.lock().push(tokio::spawn(async move {
                        prover.puzzle_loop().await;
                    }));
                }
            }
            _ => unreachable!(),
        };
    }

    /// Executes an instance of the puzzle.
    async fn puzzle_loop(&self) {
        loop {
            // If the node is not connected to any peers, then skip this iteration.
            if self.router.number_of_connected_peers() == 0 {
                debug!("Skipping an iteration of the puzzle (no connected peers)");
                tokio::time::sleep(Duration::from_secs(N::ANCHOR_TIME as u64)).await;
                continue;
            }

            // If the number of instances of the puzzle exceeds the maximum, then skip this iteration.
            if self.num_puzzle_instances() > self.max_puzzle_instances {
                // Sleep for a brief period of time.
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }

            // Read the latest epoch hash.
            let latest_epoch_hash = *self.latest_epoch_hash.read();
            // Read the latest state.
            let latest_state = self
                .latest_block_header
                .read()
                .as_ref()
                .map(|header| (header.coinbase_target(), header.proof_target()));

            // If the latest epoch hash and latest state exists, then proceed to generate a solution.
            if let (Some(epoch_hash), Some((coinbase_target, proof_target))) = (latest_epoch_hash, latest_state) {
                // Execute the puzzle.
                let prover = self.clone();
                let result = tokio::task::spawn_blocking(move || {
                    prover.puzzle_iteration(epoch_hash, coinbase_target, proof_target, &mut OsRng)
                })
                .await;

                // If the prover found a solution, then broadcast it.
                if let Ok(Some((solution_target, solution))) = result {
                    info!("Found a Solution '{}' (Proof Target {solution_target})", solution.id());
                    // Broadcast the solution.
                    self.broadcast_solution(solution).await;
                }
            } else {
                // Otherwise, sleep for a brief period of time, to await for puzzle state.
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            // If the Ctrl-C handler registered the signal, stop the prover.
            if self.shutdown.load(Ordering::Relaxed) {
                debug!("Shutting down the puzzle...");
                break;
            }
        }
    }

    /// Performs one iteration of the puzzle.
    fn puzzle_iteration<R: Rng + CryptoRng>(
        &self,
        epoch_hash: N::BlockHash,
        coinbase_target: u64,
        proof_target: u64,
        rng: &mut R,
    ) -> Option<(u64, Solution<N>)> {
        // Increment the puzzle instances.
        self.increment_puzzle_instances();

        debug!(
            "Proving 'Puzzle' for Epoch '{}' {}",
            fmt_id(epoch_hash),
            format!("(Coinbase Target {coinbase_target}, Proof Target {proof_target})").dimmed()
        );
        let address = match self.node_type {
            NodeType::ProverPoolWorker => self.pool_address.expect("Pool Address is not set"),
            NodeType::Prover | NodeType::ProverPool => self.address(),
            _ => unreachable!(),
        };

        // Compute the solution.
        let result = self.puzzle.prove(epoch_hash, address, rng.gen(), Some(proof_target)).ok().and_then(|solution| {
            self.puzzle.get_proof_target(&solution).ok().map(|solution_target| (solution_target, solution))
        });

        // Decrement the puzzle instances.
        self.decrement_puzzle_instances();
        // Return the result.
        result
    }

    /// Broadcasts the solution to the network.
    async fn broadcast_solution(&self, solution: Solution<N>) {
        match self.node_type {
            NodeType::ProverPoolWorker => {
                if let Err(err) = self
                    .http_client
                    .post(format!("{}/submit_solution", self.pool_base_url.as_ref().unwrap()))
                    .json(&SubmitSolutionRequest { address: self.address().to_string(), solution: solution.into() })
                    .send()
                    .await
                {
                    error!("Failed to submit solution: {}", err);
                }
            }
            _ => {
                // Prepare the unconfirmed solution message.
                let message = Message::UnconfirmedSolution(UnconfirmedSolution {
                    solution_id: solution.id(),
                    solution: Data::Object(solution),
                });
                // Propagate the "UnconfirmedSolution".
                self.propagate(message, &[]);
            }
        }
    }

    /// Returns the current number of puzzle instances.
    fn num_puzzle_instances(&self) -> u8 {
        self.puzzle_instances.load(Ordering::Relaxed)
    }

    /// Increments the number of puzzle instances.
    fn increment_puzzle_instances(&self) {
        self.puzzle_instances.fetch_add(1, Ordering::Relaxed);
        #[cfg(debug_assertions)]
        trace!("Number of Instances - {}", self.num_puzzle_instances());
    }

    /// Decrements the number of puzzle instances.
    fn decrement_puzzle_instances(&self) {
        self.puzzle_instances.fetch_sub(1, Ordering::Relaxed);
        #[cfg(debug_assertions)]
        trace!("Number of Instances - {}", self.num_puzzle_instances());
    }
}
