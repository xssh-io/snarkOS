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
use snarkos_node_bft::ledger_service::CoreLedgerService;
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
use reqwest::Url;
use snarkos_node_router_core::extractor::ip::{AxumClientIpSourceConfig, SecureClientIpSource};
use snarkos_node_router_core::serve::{ServeAxum, ServeAxumConfig};
use snarkvm::prelude::Ledger;
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
    /// The ledger of the node.
    ledger: Ledger<N, C>,
    /// The router of the node.
    router: Router<N>,
    /// The sync module.
    sync: Arc<BlockSync<N>>,
    /// The genesis block.
    genesis: Block<N>,
    /// The puzzle.
    puzzle: Puzzle<N>,

    /// The spawned handles.
    handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
    /// The shutdown signal.
    shutdown: Arc<AtomicBool>,
    pool_base_url: Url,
    ws_config: WsConfig,
    export: Arc<dyn ExportSolution>,
}

impl<N: Network, C: ConsensusStorage<N>> Pool<N, C> {
    /// Initializes a new prover node.
    pub async fn new(
        node_ip: SocketAddr,
        account: Account<N>,
        trusted_peers: &[SocketAddr],
        genesis: Block<N>,
        cdn: Option<String>,
        storage_mode: StorageMode,
        shutdown: Arc<AtomicBool>,
        config: PoolConfig,
    ) -> Result<Self> {
        // Initialize the signal handler.
        let signal_node = Self::handle_signals(shutdown.clone());

        // Initialize the ledger.
        let ledger = Ledger::<N, C>::load(genesis.clone(), storage_mode.clone())?;

        // Initialize the CDN.
        if let Some(base_url) = cdn {
            // Sync the ledger with the CDN.
            if let Err((_, error)) =
                snarkos_node_cdn::sync_ledger_with_cdn(&base_url, ledger.clone(), shutdown.clone()).await
            {
                crate::log_clean_error(&storage_mode);
                return Err(error);
            }
        }

        // Initialize the ledger service.
        let ledger_service = Arc::new(CoreLedgerService::<N, C>::new(ledger.clone(), shutdown.clone()));
        // Initialize the sync module.
        let sync = BlockSync::new(BlockSyncMode::Router, ledger_service.clone());
        // Determine if the pool should allow external peers.
        let allow_external_peers = true;

        // Initialize the node router.
        let router = Router::new(
            node_ip,
            // pretend to be Client
            NodeType::Client,
            account,
            trusted_peers,
            Self::MAXIMUM_NUMBER_OF_PEERS as u16,
            allow_external_peers,
            matches!(storage_mode, StorageMode::Development(_)),
        )
        .await?;
        // Initialize the node.
        let node = Self {
            ledger: ledger.clone(),
            router,
            sync: Arc::new(sync),
            genesis,
            puzzle: ledger.puzzle().clone(),
            handles: Default::default(),
            shutdown,
            pool_base_url: config.base_url(),
            ws_config: WsConfig::new(),
            export: config.get_export::<N>().await?,
        };
        // Initialize the routing.
        node.initialize_routing().await;
        // Initialize the sync module.
        node.initialize_sync();
        // Initialize the puzzle.
        node.enable_http_server().await;
        // Initialize the notification message loop.
        node.handles.lock().push(crate::start_notification_message_loop());
        // Pass the node to the signal handler.
        let _ = signal_node.set(node.clone());
        // Return the node.
        Ok(node)
    }

    /// Returns the ledger.
    pub fn ledger(&self) -> &Ledger<N, C> {
        &self.ledger
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
    /// Initializes the sync pool.
    fn initialize_sync(&self) {
        // Start the sync loop.
        let node = self.clone();
        self.handles.lock().push(tokio::spawn(async move {
            loop {
                // If the Ctrl-C handler registered the signal, stop the node.
                if node.shutdown.load(Ordering::Acquire) {
                    info!("Shutting down block production");
                    break;
                }

                // Sleep briefly to avoid triggering spam detection.
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                // Perform the sync routine.
                node.sync.try_block_sync(&node).await;
            }
        }));
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
