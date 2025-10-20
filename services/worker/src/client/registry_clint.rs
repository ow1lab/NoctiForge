use std::path::{Path, PathBuf};

use std::io::Cursor;
use anyhow::{Ok, Result};
use proto::api::registry::{registry_service_client::RegistryServiceClient, RegistryPullRequest};
use tokio_tar::Archive;
use tonic::Request;
use tokio::fs::create_dir;

use crate::path::get_dir_path;

pub struct RegistryClient {
    pub addr: String 
}

impl RegistryClient {
    pub fn new(addr: String) -> Self {
        Self { addr }
    }
}

impl RegistryClient {
    pub async fn get_tar_by_digest(&self, digest: &str) -> Result<PathBuf> {
        let dir_path = get_dir_path(digest);

        if dir_path.exists() {
            return Ok(dir_path);
        }

        let data = self.fetch_digest(digest).await?;
        self.extract_archive(&data, &dir_path).await?;
        Ok(dir_path)
    }

    async fn fetch_digest(&self, digest: &str) -> Result<Vec<u8>> {
        let mut client = RegistryServiceClient::connect(self.addr.clone()).await?;
        let mut response = client
            .pull(Request::new(RegistryPullRequest { digest: digest.to_string() } ))
            .await?
            .into_inner();

        let mut data = Vec::new();
        while let Some(message) = response.message().await? {
            data.extend_from_slice(&message.data);
        }

        Ok(data)
    }

    async fn extract_archive(&self, data: &[u8], dir_path: &Path) -> Result<()> {
        create_dir(dir_path).await?;
        let cursor = Cursor::new(&data);
        let mut archive = Archive::new(cursor);
        archive.unpack(&dir_path).await?;
        Ok(())
    }
}

