use std::time::Duration;

use anyhow::Result;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

pub struct BackgroundJob {
    cancel: CancellationToken,
    time: Duration
}

impl BackgroundJob {
    pub fn new(time: Duration) -> Self {
        return Self {
            cancel: CancellationToken::new(),
            time
        }
    }

    pub fn start(&mut self) {
        info!("Starting BackgroundJob");
        let cancel = self.cancel.clone();
        let time = self.time;

        tokio::spawn(async move {
            while !cancel.is_cancelled() {
                sleep(time).await;
                if let Err(e) = Self::execute().await {
                    error!("job error: {}", e)
                }
            }
        });
    }

    async fn execute() -> Result<()> {
        Ok(())
    }

    pub fn stop(&mut self) {
        info!("stopping BackgroundJob");
        self.cancel.cancel();
    }
}
