use std::net::SocketAddr;

use anyhow::Result;
use clickhouse_rs::{Block, ClientHandle};
use snarkos_node_bft::helpers::now;
use snarkvm::prelude::Network;

use crate::{handle::SubmitSolutionRequest, model::PartialSolutionMessage};

#[async_trait::async_trait]
pub trait ExportSolution: Send + Sync {
    async fn export_solution(&mut self, solution: &SubmitSolutionRequest, ip_addr: SocketAddr) -> Result<()>;
}

#[async_trait::async_trait]
impl ExportSolution for () {
    async fn export_solution(&mut self, _: &SubmitSolutionRequest, _: SocketAddr) -> Result<()> {
        Ok(())
    }
}

pub struct ExportSolutionClickhouse<N: Network> {
    client: ClientHandle,
    network: Option<N>,
}
impl<N: Network> ExportSolutionClickhouse<N> {
    pub fn new(client: ClientHandle) -> Self {
        Self { client, network: None }
    }
}

#[async_trait::async_trait]
impl<N: Network> ExportSolution for ExportSolutionClickhouse<N> {
    async fn export_solution(&mut self, solution: &SubmitSolutionRequest, ip_addr: SocketAddr) -> Result<()> {
        let submitter_address = solution.address.clone();
        let PartialSolutionMessage { solution_id, epoch_hash, address, counter } =
            solution.solution.partial_solution.clone();
        let block = Block::new()
            .column("datetime", vec![now()])
            .column("submitter_address", vec![submitter_address])
            .column("submitter_ip", vec![ip_addr.to_string()])
            .column("solution_id", vec![solution_id])
            .column("epoch_hash", vec![epoch_hash])
            .column("address", vec![address])
            .column("counter", vec![counter])
            .column("target", vec![solution.solution.target]);
        self.client.insert("solution", block).await?;
        Ok(())
    }
}
