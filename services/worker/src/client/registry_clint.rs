use std::path::{Path, PathBuf};

use anyhow::{Ok, Result};
use proto::api::registry::{RegistryPullRequest, registry_service_client::RegistryServiceClient};
use tracing::{debug, info, instrument, warn};
use std::io::Cursor;
use tokio::fs::create_dir;
use tokio_tar::Archive;
use tonic::Request;

use crate::path::get_dir_path;

pub struct RegistryClient {
    pub addr: String,
}

impl RegistryClient {
    pub fn new(addr: String) -> Self {
        debug!(addr = %addr, "Creating RegistryClient");
        Self { addr }
    }
}

impl RegistryClient {
    #[instrument(skip(self), fields(addr = %self.addr))]
    pub async fn get_tar_by_digest(&self, digest: &str) -> Result<PathBuf> {
        let dir_path = get_dir_path(digest);

        if dir_path.exists() {
            debug!(digest = %digest, path = ?dir_path, "Using cached archive");
            return Ok(dir_path);
        }

        info!(digest = %digest, "Fetching archive from registry");
        let data = self.fetch_digest(digest).await?;

        debug!(digest = %digest, size_bytes = data.len(), "Archive downloaded, extracting");
        self.extract_archive(&data, &dir_path).await?;

        info!(digest = %digest, path = ?dir_path, "Archive extracted successfully");
        Ok(dir_path)
    }

    #[instrument(skip(self), fields(addr = %self.addr))]
    async fn fetch_digest(&self, digest: &str) -> Result<Vec<u8>> {
        let mut client = RegistryServiceClient::connect(self.addr.clone())
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to connect to registry");
                e
            })?;        

       let mut response = client
            .pull(Request::new(RegistryPullRequest {
                digest: digest.to_string(),
            }))
            .await
            .map_err(|e| {
                warn!(digest = %digest, error = %e, "Failed to pull from registry");
                e
            })?
            .into_inner();

        let mut data = Vec::new();
        while let Some(message) = response.message().await? {
            data.extend_from_slice(&message.data);
        }

        debug!(digest = %digest, total_bytes = data.len(), "Download complete");
        Ok(data)
    }

    #[instrument(skip(self, data))]
    async fn extract_archive(&self, data: &[u8], dir_path: &Path) -> Result<()> {
        create_dir(dir_path).await.map_err(|e| {
            warn!(path = ?dir_path, error = %e, "Failed to create directory");
            e
        })?;

        let cursor = Cursor::new(&data);
        let mut archive = Archive::new(cursor);

        archive.unpack(&dir_path).await.map_err(|e| {
            warn!(path = ?dir_path, error = %e, "Failed to extract archive");
            e
        })?;

        Ok(())
    }
}
