use async_trait::async_trait

use crate::{config::RuntimeConfig, error::RuntimeError};

#[async_trait]
pub trait Runtime: Send + Sync {
    async fn start(
        &self,
        config: RuntimeConfig
    ) -> Result<Box<dyn Instance>, RuntimeError>;
}

pub trait Instance: Send + Sync {
    async fn invoke(
        &mut self,
        payload: &[u8]
    ) -> Result<Vec<u8>, RuntimeError>;

    async fn stop(self: Box<Self>) -> Result<(), RuntimeError>;
}
