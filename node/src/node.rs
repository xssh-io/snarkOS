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

use crate::{traits::NodeInterface, Client, Pool, Prover, Validator, Worker};
use snarkos_account::Account;
use snarkos_node_router::messages::NodeType;
use snarkvm::prelude::{
    block::Block,
    store::helpers::{memory::ConsensusMemory, rocksdb::ConsensusDB},
    Address, Network, PrivateKey, ViewKey,
};

use crate::config::PoolConfig;
use aleo_std::StorageMode;
use anyhow::Result;
use std::{
    net::SocketAddr,
    sync::{atomic::AtomicBool, Arc},
};

pub enum Node<N: Network> {
    /// A validator is a full node, capable of validating blocks.
    Validator(Arc<Validator<N, ConsensusDB<N>>>),
    /// A prover is a light node, capable of producing proofs for consensus.
    Prover(Arc<Prover<N, ConsensusMemory<N>>>),
    /// A prover pool is a collection of prover nodes.
    Pool(Arc<Pool<N, ConsensusMemory<N>>>),
    /// A client node is a full node, capable of querying with the network.
    Client(Arc<Client<N, ConsensusDB<N>>>),
    Worker(Arc<Worker<N, ConsensusMemory<N>>>),
}

impl<N: Network> Node<N> {
    /// Initializes a new validator node.
    pub async fn new_validator(
        node_ip: SocketAddr,
        bft_ip: Option<SocketAddr>,
        rest_ip: Option<SocketAddr>,
        rest_rps: u32,
        account: Account<N>,
        trusted_peers: &[SocketAddr],
        trusted_validators: &[SocketAddr],
        genesis: Block<N>,
        cdn: Option<String>,
        storage_mode: StorageMode,
        allow_external_peers: bool,
        dev_txs: bool,
        shutdown: Arc<AtomicBool>,
    ) -> Result<Self> {
        Ok(Self::Validator(Arc::new(
            Validator::new(
                node_ip,
                bft_ip,
                rest_ip,
                rest_rps,
                account,
                trusted_peers,
                trusted_validators,
                genesis,
                cdn,
                storage_mode,
                allow_external_peers,
                dev_txs,
                shutdown,
            )
            .await?,
        )))
    }

    /// Initializes a new prover node.
    pub async fn new_prover(
        node_ip: SocketAddr,
        account: Account<N>,
        trusted_peers: &[SocketAddr],
        genesis: Block<N>,
        storage_mode: StorageMode,
        shutdown: Arc<AtomicBool>,
        pool_base_url: Option<String>,
    ) -> Result<Self> {
        Ok(Self::Prover(Arc::new(
            Prover::new(node_ip, account, trusted_peers, genesis, storage_mode, shutdown, pool_base_url).await?,
        )))
    }
    pub async fn new_worker(account: Account<N>, shutdown: Arc<AtomicBool>, pool_base_url: String) -> Result<Self> {
        Ok(Self::Worker(Worker::new(account, shutdown, pool_base_url).await?))
    }
    /// Initializes a new prover pool.
    pub async fn new_pool(
        node_ip: SocketAddr,
        account: Account<N>,
        trusted_peers: &[SocketAddr],
        genesis: Block<N>,
        storage_mode: StorageMode,
        shutdown: Arc<AtomicBool>,
        config: PoolConfig,
    ) -> Result<Self> {
        Ok(Self::Pool(Arc::new(
            Pool::new(node_ip, account, trusted_peers, genesis, storage_mode, shutdown, config).await?,
        )))
    }
    /// Initializes a new client node.
    pub async fn new_client(
        node_ip: SocketAddr,
        rest_ip: Option<SocketAddr>,
        rest_rps: u32,
        account: Account<N>,
        trusted_peers: &[SocketAddr],
        genesis: Block<N>,
        cdn: Option<String>,
        storage_mode: StorageMode,
        shutdown: Arc<AtomicBool>,
    ) -> Result<Self> {
        Ok(Self::Client(Arc::new(
            Client::new(node_ip, rest_ip, rest_rps, account, trusted_peers, genesis, cdn, storage_mode, shutdown)
                .await?,
        )))
    }

    /// Returns the node type.
    pub fn node_type(&self) -> NodeType {
        match self {
            Self::Validator(validator) => validator.node_type(),
            Self::Prover(prover) => prover.node_type(),
            Self::Pool(pool) => pool.node_type(),
            Self::Client(client) => client.node_type(),
            Self::Worker(worker) => worker.node_type(),
        }
    }

    /// Returns the account private key of the node.
    pub fn private_key(&self) -> &PrivateKey<N> {
        match self {
            Self::Validator(node) => node.private_key(),
            Self::Prover(node) => node.private_key(),
            Self::Pool(node) => node.private_key(),
            Self::Client(node) => node.private_key(),
            Self::Worker(node) => node.private_key(),
        }
    }

    /// Returns the account view key of the node.
    pub fn view_key(&self) -> &ViewKey<N> {
        match self {
            Self::Validator(node) => node.view_key(),
            Self::Prover(node) => node.view_key(),
            Self::Pool(node) => node.view_key(),
            Self::Client(node) => node.view_key(),
            Self::Worker(node) => node.view_key(),
        }
    }

    /// Returns the account address of the node.
    pub fn address(&self) -> Address<N> {
        match self {
            Self::Validator(node) => node.address(),
            Self::Prover(node) => node.address(),
            Self::Pool(node) => node.address(),
            Self::Client(node) => node.address(),
            Self::Worker(node) => node.address(),
        }
    }

    /// Returns `true` if the node is in development mode.
    pub fn is_dev(&self) -> bool {
        match self {
            Self::Validator(node) => node.is_dev(),
            Self::Prover(node) => node.is_dev(),
            Self::Pool(node) => node.is_dev(),
            Self::Client(node) => node.is_dev(),
            Self::Worker(node) => node.is_dev(),
        }
    }
}
