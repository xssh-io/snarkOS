use std::net::SocketAddr;

use crate::{handle::SubmitSolutionRequest, model::PartialSolutionMessage};
use anyhow::{Context, Result};
use clickhouse_rs::{Block, ClientHandle};
use snarkos_node_bft::helpers::now;
use snarkvm::prelude::Network;
use tokio::sync::mpsc::{Receiver, Sender};

#[async_trait::async_trait]
pub trait ExportSolution: Send + Sync {
    async fn export_solution(
        &self,
        ip_addr: SocketAddr,
        solution: &SubmitSolutionRequest,
        verified: bool,
    ) -> Result<()>;
}

#[async_trait::async_trait]
impl ExportSolution for () {
    async fn export_solution(&self, _: SocketAddr, _: &SubmitSolutionRequest, _verified: bool) -> Result<()> {
        Ok(())
    }
}

pub struct ExportSolutionClickhouse<N: Network> {
    tx: Sender<(SocketAddr, SubmitSolutionRequest, bool)>,
    _network: Option<N>,
}
impl<N: Network> ExportSolutionClickhouse<N> {
    pub fn new(client: ClientHandle) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        tokio::spawn(Self::run(client, rx));
        Self { tx, _network: None }
    }
    async fn export_solution(
        client: &mut ClientHandle,
        solution: &SubmitSolutionRequest,
        ip_addr: SocketAddr,
        verified: bool,
    ) -> Result<()> {
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
        let table = if verified { "solution" } else { "solution_attempt" };

        client.insert(table, block).await?;
        Ok(())
    }
    async fn run(mut client: ClientHandle, mut rx: Receiver<(SocketAddr, SubmitSolutionRequest, bool)>) {
        while let Some((ip_addr, solution, verified)) = rx.recv().await {
            if let Err(e) = Self::export_solution(&mut client, &solution, ip_addr, verified).await {
                error!("Failed to export solution: {:?}", e);
            }
        }
    }
}

#[async_trait::async_trait]
impl<N: Network> ExportSolution for ExportSolutionClickhouse<N> {
    async fn export_solution(
        &self,
        ip_addr: SocketAddr,
        solution: &SubmitSolutionRequest,
        verified: bool,
    ) -> Result<()> {
        self.tx.send((ip_addr, solution.clone(), verified)).await.context("clickhouse closed")?;
        Ok(())
    }
}
