use std::net::SocketAddr;

use crate::{handle::SubmitSolutionRequest, model::PartialSolutionMessage};
use anyhow::{Context, Result};
use chrono::Utc;
use clickhouse_rs::{Block, ClientHandle};
use snarkvm::prelude::Network;
use tokio::sync::mpsc::{Receiver, Sender};

#[async_trait::async_trait]
pub trait ExportSolution<N: Network>: Send + Sync {
    async fn export_solution(
        &self,
        ip_addr: SocketAddr,
        solution: &SubmitSolutionRequest<N>,
        verified: bool,
    ) -> Result<()>;
}

#[async_trait::async_trait]
impl<N: Network> ExportSolution<N> for () {
    async fn export_solution(
        &self,
        _: SocketAddr,
        _: &SubmitSolutionRequest<N>,
        _verified: bool
    ) -> Result<()> {
        Ok(())
    }
}

pub struct ExportSolutionClickhouse<N: Network> {
    tx: Sender<(SocketAddr, SubmitSolutionRequest<N>, bool)>,
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
        solution: &SubmitSolutionRequest<N>,
        ip_addr: SocketAddr,
        verified: bool,
    ) -> Result<()> {
        let submitter_address = solution.address.clone();
        let PartialSolutionMessage { solution_id, epoch_hash, address, counter } =
            solution.solution.partial_solution.clone();
        let block_height = solution.header.height();
        let block = Block::new()
            .column("datetime", vec![Utc::now().with_timezone(&chrono_tz::Tz::UTC)])
            .column("submitter_address", vec![submitter_address])
            .column("submitter_ip", vec![ip_addr.to_string()])
            .column("solution_id", vec![solution_id])
            .column("epoch_hash", vec![epoch_hash])
            .column("address", vec![address])
            .column("counter", vec![counter])
            .column("target", vec![solution.solution.target])
            .column("block_height", vec![block_height]);
        let table = if verified { "solution" } else { "solution_attempt" };

        client.insert(table, block).await?;
        Ok(())
    }
    async fn run(mut client: ClientHandle, mut rx: Receiver<(SocketAddr, SubmitSolutionRequest<N>, bool)>) {
        while let Some((ip_addr, solution, verified)) = rx.recv().await {
            if let Err(e) = Self::export_solution(&mut client, &solution, ip_addr, verified).await {
                error!("Failed to export solution: {:?}", e);
            }
        }
    }
}

#[async_trait::async_trait]
impl<N: Network> ExportSolution<N> for ExportSolutionClickhouse<N> {
    async fn export_solution(
        &self,
        ip_addr: SocketAddr,
        solution: &SubmitSolutionRequest<N>,
        verified: bool,
    ) -> Result<()> {
        self.tx.send((ip_addr, solution.clone(), verified)).await.context("clickhouse closed")?;
        Ok(())
    }
}
