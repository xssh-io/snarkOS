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

use crate::pool::ws::WsConfig;
use crate::route::init_routes;
use aleo_std::StorageMode;
use anyhow::Result;
use core::{marker::PhantomData, time::Duration};
use parking_lot::{Mutex, RwLock};
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
pub struct Pool<N: Network, C: ConsensusStorage<N>> {
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
    pool_base_url: String,
    http_client: reqwest::Client,
    ws_config: WsConfig,
    /// PhantomData.
    _phantom: PhantomData<C>,
}

impl<N: Network, C: ConsensusStorage<N>> Pool<N, C> {
    /// Initializes a new prover node.
    pub async fn new(
        node_ip: SocketAddr,
        account: Account<N>,
        trusted_peers: &[SocketAddr],
        genesis: Block<N>,
        storage_mode: StorageMode,
        shutdown: Arc<AtomicBool>,
        pool_base_url: String,
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
            NodeType::Pool,
            account,
            trusted_peers,
            Self::MAXIMUM_NUMBER_OF_PEERS as u16,
            allow_external_peers,
            matches!(storage_mode, StorageMode::Development(_)),
        )
        .await?;
        // Compute the maximum number of puzzle instances.
        let max_puzzle_instances = num_cpus::get().saturating_sub(2).clamp(1, 6);

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
            pool_base_url,
            http_client: reqwest::Client::new(),
            ws_config: WsConfig::new(),
            _phantom: Default::default(),
        };
        // Initialize the routing.
        node.initialize_routing().await;

        node.enable_http_request().await;

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
            url: self.pool_base_url.parse().expect("failed to parse url"),
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
impl<N: Network, C: ConsensusStorage<N>> NodeInterface<N> for Pool<N, C> {
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

impl<N: Network, C: ConsensusStorage<N>> Pool<N, C> {
    /// Initialize a new instance of the puzzle.
    async fn initialize_puzzle(&self) {
        self.enable_http_request().await;
    }

    /// Broadcasts the solution to the network.
    async fn broadcast_solution(&self, solution: Solution<N>) {
        // Prepare the unconfirmed solution message.
        let message = Message::UnconfirmedSolution(UnconfirmedSolution {
            solution_id: solution.id(),
            solution: Data::Object(solution),
        });
        // Propagate the "UnconfirmedSolution".
        self.propagate(message, &[]);
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
