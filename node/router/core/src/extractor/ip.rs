pub use axum_client_ip::*;
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AxumClientIpSourceConfig {
    #[default]
    Insecure,
    Secure(SecureClientIpSource),
}
