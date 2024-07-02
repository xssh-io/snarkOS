use std::net::SocketAddr;

use anyhow::Result;
use chrono::Local;
use snarkvm::prelude::Network;

use crate::handle::SubmitSolutionRequest;

#[async_trait::async_trait]
pub trait ExportSolution: Send + Sync {
    fn export_solution(&self, solution: &SubmitSolutionRequest, ip_addr: SocketAddr) -> Result<()>;
}

#[async_trait::async_trait]
impl ExportSolution for () {
    fn export_solution(&self, _solution: &SubmitSolutionRequest, ip_addr: SocketAddr) -> Result<()> {
        Ok(())
    }
}

pub struct ExportSolutionClickhouse<N: Network> {
    client: clickhouse_rs::Client,
}
impl<N: Network> ExportSolutionClickhouse<N> {
    pub fn new(client: clickhouse_rs::Client) -> Self {
        Self { client }
    }
    pub async fn export_solution(&self, solution: &SubmitSolutionRequest, ip_addr: SocketAddr) -> Result<()> {
        let submitter_address = solution.address;
        let partial = solution.solution.partial_solution;
        let now = Local::now();
        let query = format!(
            "INSERT INTO solution (datetime, submitter_address, submitter_ip, solution_id, epoch_hash, address, counter, target) VALUES ({}, {}, {}, {}, {}, {}, {}, {})",
            now,
            submitter_address,
            ip_addr,
            partial.solution_id,
            partial.epoch_hash,
            partial.address,
            partial.counter,
            solution.solution.target,
        );
        self.client.execute(query).await?;
        Ok(())
        // let new_sol = Solution::new(partial_solution, target)
    }
}

#[async_trait::async_trait]
impl<N: Network> ExportSolution for ExportSolutionClickhouse<N> {
    fn export_solution(&self, solution: &SubmitSolutionRequest, ip_addr: SocketAddr) -> Result<()> {
        for export in self {
            export.export_solution(solution, ip_addr)?;
        }
        Ok(())
    }
}
