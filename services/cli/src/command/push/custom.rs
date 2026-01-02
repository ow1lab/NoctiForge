use std::{path::PathBuf, process::Stdio};

use serde::Deserialize;
use tokio::process::Command;
use tonic::async_trait;

use super::BuildService;

#[derive(Debug, Deserialize)]
pub struct CustomBuild {
    script: String,
}

#[async_trait]
impl BuildService for CustomBuild {
    async fn build(&self, project_path: PathBuf, temp_path: PathBuf) -> anyhow::Result<()> {
        let status = Command::new("sh")
            .arg("-c")
            .arg(&self.script)
            .current_dir(project_path)
            .env("OUTPUT", temp_path)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!("build script failed with exit code: {:?}", status.code());
        }

        Ok(())
    }
}
