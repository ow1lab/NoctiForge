use std::{path::PathBuf, time::Duration};

use anyhow::{Ok, Result};
use libcontainer::syscall::Syscall;
use tokio::time::sleep;
use tonic::Request;
use tracing::{debug, info, instrument, warn};
use url::Url;

use crate::{
    client::registry_clint::RegistryClient,
    worker::{
        container::{self, ProccesContainer},
        spec::SysUserParms,
    },
};
use proto::api::action::{
    InvokeRequest, function_runner_service_client::FunctionRunnerServiceClient,
};

const SERVER_STARTUP_TIMEOUT_MS: u64 = 3000;
const SERVER_STARTUP_RETRY_INTERVAL_MS: u64 = 10;

pub struct Config {
    pub is_dev: bool,
}

#[derive(Clone)]
pub struct NativeWorker {
    registry_service: RegistryClient,
    root_path: PathBuf,
    sysuser: SysUserParms,
}

impl NativeWorker {
    pub fn new(
        registry_service: RegistryClient,
        root_path: PathBuf,
        syscall: &dyn Syscall,
        server_config: Config,
    ) -> Result<Self> {
        info!(is_dev = server_config.is_dev, "Creating NativeWorker");
        Ok(Self {
            registry_service,
            root_path,
            sysuser: SysUserParms {
                uid: syscall.get_euid().as_raw(),
                gid: syscall.get_egid().as_raw(),
            },
        })
    }
}

impl NativeWorker {
    #[instrument(name = "function_execute", level = "debug", skip(self, body), fields(digest = %digest, body_size = body.len()))]
    pub async fn execute(&self, digest: String, body: String) -> Result<String> {
        debug!("Executing function");

        let uri = self.get_available_handler_uri(digest.clone()).await?;

        debug!(uri = %uri, "Connecting to function handler");
        let mut client = FunctionRunnerServiceClient::connect(uri.to_string())
            .await
            .map_err(|e| {
                warn!(digest = %digest, error = %e, "Failed to connect to function handler");
                e
            })?;

        let resp = client
            .invoke(Request::new(InvokeRequest {
                payload: Some(body),
            }))
            .await
            .map_err(|e| {
                warn!(digest = %digest, error = %e, "Function invocation failed");
                e
            })?
            .into_inner();

        debug!(digest = %digest, output_size = resp.output.len(), "Function execution completed");
        Ok(resp.output)
    }

    pub async fn clean_all_handlers(&self) -> Result<()> {
        let containers = ProccesContainer::get_all(&self.root_path).await?;
        for mut container in containers {
            container.cleanup().await?;
        }
        Ok(())
    }

    async fn get_available_handler_uri(&self, digest: String) -> Result<Url> {
        // TODO: This is a workaround i don't like as there could be a very low way that this
        // fails. 
        let short_digest = &digest[..16];

        let proc = if ProccesContainer::exist(&self.root_path, short_digest) {
            info!(short_digest = %short_digest, "Loading existing handler");
            ProccesContainer::load(&self.root_path, short_digest).await?
        } else {
            info!(short_digest = %short_digest, "Creating new handler");
            let dir_path = self.registry_service.get_tar_by_digest(&digest).await?;
            container::ProccesContainer::new(
                short_digest,
                dir_path,
                self.root_path.clone(),
                &self.sysuser,
            )
            .await?
        };

        let url = proc.get_url()?;
        self.wait_for_server_ready(&url).await?;

        Ok(url)
    }

    async fn wait_for_server_ready(&self, url: &Url) -> Result<()> {
        let max_attempts = (SERVER_STARTUP_TIMEOUT_MS / SERVER_STARTUP_RETRY_INTERVAL_MS) as u32;
        let retry_interval = Duration::from_millis(SERVER_STARTUP_RETRY_INTERVAL_MS);

        let uri = url.to_string();
        debug!(max_attempts, uri, "Waiting for server to be ready");

        for attempt in 1..=max_attempts {
            match FunctionRunnerServiceClient::connect(uri.clone()).await {
                std::result::Result::Ok(_) => {
                    debug!(attempts = attempt, "Server is ready");
                    return Ok(());
                }
                Err(e) if attempt == max_attempts => {
                    warn!(
                        attempts = max_attempts,
                        error = %e,
                        "Server failed to start"
                    );
                    return Err(anyhow::anyhow!(
                        "Server failed to start after {} attempts: {}",
                        max_attempts,
                        e
                    ));
                }
                Err(_) => {
                    sleep(retry_interval).await;
                }
            }
        }

        unreachable!("Loop should have returned")
    }
}
