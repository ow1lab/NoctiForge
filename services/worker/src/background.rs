use std::{sync::Arc, time::Duration};

use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::worker::function_invocations::FunctionInvocations;

pub struct BackgroundJob {
    cancel: CancellationToken,
    time: Duration,
    function_invocations: Arc<FunctionInvocations>,
}

impl BackgroundJob {
    pub fn new(
        function_invocations: &Arc<FunctionInvocations>,
        time: Duration,
        ) -> Self {
        return Self {
            cancel: CancellationToken::new(),
            time,
            function_invocations: function_invocations.clone()
        }
    }

    pub async fn start(&mut self) {
        info!("Starting BackgroundJob");
        let cancel = self.cancel.clone();
        let time = self.time;
        let function = self.function_invocations.clone();

        tokio::spawn(async move {
            while !cancel.is_cancelled() {
                sleep(time).await;
                for proc in function.get_all().await.keys() {
                    info!("Checking id: {}", proc)
                }
            }
        });
    }

    pub fn stop(&mut self) {
        info!("stopping BackgroundJob");
        self.cancel.cancel();
    }
}
