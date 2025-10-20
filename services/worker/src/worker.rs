use std::{collections::HashMap, path::{Path, PathBuf}, sync::Arc, time::Duration};

use anyhow::{Ok, Result};
use tokio::{process::Command, sync::Mutex, time::sleep};
use tonic::Request;
use url::Url;

use crate::{client::registry_clint::RegistryClient};
use proto::api::action::{function_runner_service_client::FunctionRunnerServiceClient, InvokeRequest};

const SERVER_STARTUP_TIMEOUT_MS: u64 = 3000;
const SERVER_STARTUP_RETRY_INTERVAL_MS: u64 = 10;

pub struct Config {
    pub is_dev: bool,
}

pub struct NativeWorker {
    function_urls: Arc<Mutex<HashMap<String, Url>>>,
    registry_service: RegistryClient,
    config: Config,
}

impl NativeWorker {
    pub fn new(registry_service: RegistryClient, server_config: Config) -> Result<Self> {
        Ok(Self {
            function_urls: Arc::new(Mutex::new(HashMap::new())),
            registry_service,
            config: server_config,
        })
    }
}

impl NativeWorker {
    pub async fn execute(&self, digest: String, body: String) -> Result<String> {
        let socket_path = self.get_available_handler_uri(digest).await?;
        let uri = format!("unix://{}", socket_path.path());
        let mut client = FunctionRunnerServiceClient::connect(uri).await?;
        let resp = client
            .invoke(Request::new(InvokeRequest {
                payload: Some(body),
            }))
            .await?
            .into_inner();
        Ok(resp.output)
    }

    
    async fn get_available_handler_uri(&self, digest: String) -> Result<Url> {
        // Check if URL is already cached
        {
            let urls = self.function_urls.lock().await;
            if let Some(url) = urls.get(&digest) {
                return Ok(url.clone());
            }
        }

        let dir_path = self.registry_service.get_tar_by_digest(&digest).await?;
        let url = self.start_handler(dir_path).await?;

        // Insert into cache
        self.function_urls.lock().await.insert(digest, url.clone());
        Ok(url)}

    async fn start_handler(&self, bin_path: PathBuf) -> Result<Url> {
        let bootstrap_path = bin_path.join("bootstrap");
        let uuid = uuid::Uuid::new_v4();

        let socket_path = match self.config.is_dev {
            true => Path::new("/tmp"),
            false => Path::new("/run"),
        }.join(uuid.to_string())
            .with_extension("sock");

        let socket_path_str = socket_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid socket path"))?;

        Command::new(&bootstrap_path)
            .env("SOCKET_PATH", socket_path_str)
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn handler: {}", e))?;

        let url = Url::from_file_path(&socket_path)
            .map_err(|_| anyhow::anyhow!("Failed to create URL from socket path"))?;

        self.wait_for_server_ready(&url).await?;
        Ok(url)
    }

    async fn wait_for_server_ready(&self, url: &Url) -> Result<()> {
        let max_attempts = (SERVER_STARTUP_TIMEOUT_MS / SERVER_STARTUP_RETRY_INTERVAL_MS) as u32;
        let retry_interval = Duration::from_millis(SERVER_STARTUP_RETRY_INTERVAL_MS);  

        for attempt in 1..= max_attempts {
            let uri = format!("unix://{}", url.path());
            match FunctionRunnerServiceClient::connect(uri).await {
                std::result::Result::Ok(_) => {
                    println!("Server is ready after {} attempts", attempt);
                    return Ok(());
                }
                Err(e) if attempt == max_attempts => {
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
