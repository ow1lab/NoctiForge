use std::{sync::Arc, time::Duration};

use anyhow::{Ok, Result};
use tokio::time::{Instant, sleep};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::worker::function_invocations::FunctionInvocations;

pub struct BackgroundConfig {
    pub time: Duration,
    pub resource_ttl: Duration, 
}

pub struct BackgroundJob {
    config: BackgroundConfig,
    cancel: CancellationToken,
    function_invocations: Arc<FunctionInvocations>,
}

impl BackgroundJob {
    pub fn new(
        config: BackgroundConfig,
        function_invocations: &Arc<FunctionInvocations>,
        ) -> Self {
        return Self {
            config,
            cancel: CancellationToken::new(),
            function_invocations: function_invocations.clone()
        }
    }

    pub async fn start(&mut self) {
        info!("Starting BackgroundJob");
        let cancel = self.cancel.clone();
        let time = self.config.time;
        let resource_ttl = self.config.resource_ttl;
        let function = self.function_invocations.clone();

        tokio::spawn(async move {
            while !cancel.is_cancelled() {
                sleep(time).await;
                for instance_id in function.keys().await {
                    if let Err(err) = execute(&instance_id, resource_ttl, &function).await {
                        tracing::error!(
                            "Something when worng with {}: {:?}",
                            instance_id,
                            err
                        );
                    }
                }
            }
        });
    }

    pub fn stop(&mut self) {
        info!("stopping BackgroundJob");
        self.cancel.cancel();
    }
}

pub async fn execute(instance_id: &str, resource_ttl: Duration, function: &FunctionInvocations) -> Result<()> {
    if let Some(inv) = function.peek(instance_id).await {
        let expired = {
            let inv = inv.lock().await;
            Instant::now() - inv.last_accessed > resource_ttl 
        };

        if expired {
            function.delete(instance_id).await?;
        }
    }

    Ok(())
}
