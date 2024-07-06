use crate::pool::export::{ExportSolution, ExportSolutionClickhouse};
use anyhow::Context;
use anyhow::Result;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use snarkvm::prelude::Network;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
pub struct PoolConfig {
    base_url: String,
    pub clickhouse_url: Option<String>,
}
impl PoolConfig {
    pub fn base_url(&self) -> Url {
        self.base_url.parse().with_context(|| format!("Invalid base URL: {}", self.base_url)).unwrap()
    }
    pub async fn get_export<N: Network>(&self) -> Result<Arc<dyn ExportSolution>> {
        if let Some(url) = &self.clickhouse_url {
            let clickhouse = clickhouse_rs::Pool::new(url.clone());
            let handle = clickhouse.get_handle().await?;
            return Ok(Arc::new(ExportSolutionClickhouse::<N>::new(handle)) as _);
        }

        Ok(Arc::new(()) as _)
    }
}
