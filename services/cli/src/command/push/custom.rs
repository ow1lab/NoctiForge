use std::{path::PathBuf, process::Stdio};

use serde::Deserialize;
use tokio::process::Command;
use tonic::async_trait;

use super::BuildService;

#[derive(Debug, Deserialize)]
pub struct CustomBuild {
    script: String,
    output: String,
}

#[async_trait]
impl BuildService for CustomBuild {
    async fn build(&self, project_path: PathBuf) -> anyhow::Result<String> {
        let status = Command::new("sh")
            .arg("-c")
            .arg(&self.script)
            .current_dir(project_path)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!("build script failed with exit code: {:?}", status.code());
        }

        Ok(self.output.clone())
    }
}
