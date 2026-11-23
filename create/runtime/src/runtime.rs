use async_trait::async_trait;

use crate::{config::RuntimeConfig, error::RuntimeError};

pub enum InvokeResult {
    Success { output: Vec<u8> },
    Problem { r#type: String, details: String },
}

#[async_trait]
pub trait Runtime: Send + Sync {
    async fn start(&self, config: RuntimeConfig) -> Result<Box<dyn Instance>, RuntimeError>;
}

#[async_trait]
pub trait Instance: Send + Sync {
    async fn invoke(&mut self, payload: &[u8]) -> Result<InvokeResult, RuntimeError>;
    async fn stop(self: Box<Self>) -> Result<(), RuntimeError>;
}
