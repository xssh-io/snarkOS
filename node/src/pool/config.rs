use crate::pool::export::{ExportSolution, ExportSolutionClickhouse};
use anyhow::Context;
use anyhow::Result;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use snarkvm::prelude::Network;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
pub struct PoolConfig {
    pub private_key: String,
    base_url: String,
    pub clickhouse_password: Option<String>,
    clickhouse_host: Option<String>,
    clickhouse_port: String,
}
impl PoolConfig {
    pub fn base_url(&self) -> Url {
        self.base_url.parse().with_context(|| format!("Invalid base URL: {}", self.base_url)).unwrap()
    }
    pub async fn get_export<N: Network>(&self) -> Result<Arc<dyn ExportSolution>> {
        let host = match &self.clickhouse_host {
            Some(host) => host,
            None => "localhost",
        };
        let port = &self.clickhouse_port;
        if let Some(password) = &self.clickhouse_password {
            let url = format!("tcp://default:{}@{}:{}/clicks?compression=lz4&ping_timeout=42ms", password, host, port);

            let clickhouse = clickhouse_rs::Pool::new(url);
            let handle = clickhouse.get_handle().await?;
            return Ok(Arc::new(ExportSolutionClickhouse::<N>::new(handle)) as _);
        }

        Ok(Arc::new(()) as _)
    }
}
