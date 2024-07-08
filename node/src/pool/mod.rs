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

pub mod config;
mod export;
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
    messages::{NodeType, UnconfirmedSolution},
    Heartbeat, Inbound, Outbound, Router, Routing, SYNC_LENIENCY,
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
};

use crate::handle::SubmitSolutionRequest;
use crate::pool::config::PoolConfig;
use crate::pool::export::ExportSolution;
use crate::pool::ws::WsConfig;
use crate::route::init_routes;
use aleo_std::StorageMode;
use anyhow::ensure;
use anyhow::Result;
use core::time::Duration;
use parking_lot::Mutex;
use parking_lot::RwLock;
use reqwest::Url;
use snarkos_node_router_core::extractor::ip::{AxumClientIpSourceConfig, SecureClientIpSource};
use snarkos_node_router_core::serve::{ServeAxum, ServeAxumConfig};
use snarkvm::prelude::VM;
use std::future::Future;
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::task::JoinHandle;

/// A pool is a light node, capable of dispatching puzzles to the pool workers
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

    /// The spawned handles.
    handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
    /// The shutdown signal.
    shutdown: Arc<AtomicBool>,
    pool_base_url: Url,
    ws_config: WsConfig,
    export: Arc<dyn ExportSolution>,
    p: std::marker::PhantomData<C>,
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
        config: PoolConfig,
    ) -> Result<Self> {
        // Initialize the signal handler.
        let signal_node = Self::handle_signals(shutdown.clone());

        // Initialize the ledger service.
        let ledger_service = Arc::new(ProverLedgerService::new());
        // Initialize the sync module.
        let sync = BlockSync::new(BlockSyncMode::Router, ledger_service.clone());
        // Determine if the pool should allow external peers.
        let allow_external_peers = true;

        // Initialize the node router.
        let router = Router::new(
            node_ip,
            // pretend to be Prover
            NodeType::Prover,
            account,
            trusted_peers,
            Self::MAXIMUM_NUMBER_OF_PEERS as u16,
            allow_external_peers,
            matches!(storage_mode, StorageMode::Development(_)),
        )
        .await?;
        // Initialize the node.
        let node = Self {
            router,
            sync: Arc::new(sync),
            genesis,
            puzzle: VM::<N, C>::new_puzzle()?,
            latest_epoch_hash: Default::default(),
            latest_block_header: Default::default(),
            handles: Default::default(),
            shutdown,
            pool_base_url: config.base_url(),
            ws_config: WsConfig::new(),
            export: config.get_export::<N>().await?,
            p: Default::default(),
        };
        // Initialize the routing.
        node.initialize_routing().await;

        // Initialize the puzzle.
        node.enable_http_server().await;
        // Initialize the notification message loop.
        node.handles.lock().push(crate::start_notification_message_loop());
        // Pass the node to the signal handler.
        let _ = signal_node.set(node.clone());
        // Return the node.
        Ok(node)
    }
    async fn enable_http_server(&self) {
        let config = ServeAxumConfig {
            title: "Aleo Prover Pool".to_string(),
            url: self.pool_base_url.clone(),
            ip_source: AxumClientIpSourceConfig::Secure(SecureClientIpSource::ConnectInfo),
        };
        let this = self.clone();
        let ws_config = self.ws_config.clone();
        tokio::spawn(async move {
            let routes = init_routes(Arc::new(this), ws_config);
            if let Err(err) = ServeAxum::new(config).serve(routes).await {
                error!("Failed to serve HTTP: {:?}", err);
            }
        });
    }

    /// Spawns a task with the given future; it should only be used for long-running tasks.
    pub fn spawn<T: Future<Output = ()> + Send + 'static>(&self, future: T) {
        self.handles.lock().push(tokio::spawn(future));
    }
    /// Broadcasts the solution to the network.
    async fn confirm_and_broadcast_solution(
        &self,
        peer_ip: SocketAddr,
        msg: &SubmitSolutionRequest,
        solution: Solution<N>,
    ) -> Result<()> {
        // Do not process unconfirmed solutions if the node is too far behind.

        ensure!(
            self.num_blocks_behind() <= SYNC_LENIENCY,
            "Skipped processing unconfirmed solution '{}' (node is syncing)",
            solution.id()
        );

        // Update the timestamp for the unconfirmed solution.
        let seen_before = self.router().cache.insert_inbound_solution(peer_ip, solution.id()).is_some();

        ensure!(!seen_before, "Skipping 'UnconfirmedSolution' from '{peer_ip}': seen before");

        ensure!(solution.address() == self.address(), "Peer '{peer_ip}' sent an invalid unconfirmed solution");
        // Clone the serialized message.
        let serialized = UnconfirmedSolution { solution_id: solution.id(), solution: Data::Object(solution.clone()) };
        // Handle the unconfirmed solution.
        ensure!(
            self.unconfirmed_solution(peer_ip, serialized.clone(), solution).await,
            "Peer '{peer_ip}' sent an invalid unconfirmed solution"
        );
        self.export.export_solution(peer_ip, msg, true).await?;

        Ok(())
    }
}

#[async_trait]
impl<N: Network, C: ConsensusStorage<N>> NodeInterface<N> for Pool<N, C> {
    /// Shuts down the node.
    async fn shut_down(&self) {
        info!("Shutting down...");

        // Shut down the node.
        trace!("Shutting down the node...");
        self.shutdown.store(true, Ordering::Release);

        // Abort the tasks.
        trace!("Shutting down the pool...");
        self.handles.lock().iter().for_each(|handle| handle.abort());

        // Shut down the router.
        self.router.shut_down().await;

        info!("Node has shut down.");
    }
}
