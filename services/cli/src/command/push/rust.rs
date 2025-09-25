use std::path::PathBuf;

use serde::Deserialize;
use tonic::async_trait;

use super::BuildService;

#[derive(Debug, Deserialize)]
pub struct RustBuild {
    output: String,
}

#[async_trait]
impl BuildService for RustBuild {
    async fn build(&self, project_path: PathBuf) -> anyhow::Result<String> {
        _ = self.output;
        _ = project_path;
        todo!()
    }
}
