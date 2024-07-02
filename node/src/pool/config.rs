use anyhow::Context;
use reqwest::Url;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct PoolConfig {
    pub private_key: String,
    base_url: String,
    pub clickhouse_password: Option<String>,
}
impl PoolConfig {
    pub fn base_url(&self) -> Url {
        self.base_url.parse().with_context(|| format!("Invalid base URL: {}", self.base_url)).unwrap()
    }
}
