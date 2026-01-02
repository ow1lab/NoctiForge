use std::path::PathBuf;

use serde::Deserialize;
use tonic::async_trait;

use super::BuildService;

#[derive(Debug, Deserialize)]
pub struct RustBuild {
    _output: String,
}

#[async_trait]
impl BuildService for RustBuild {
    async fn build(&self, _project_path: PathBuf, _temp_path: PathBuf) -> anyhow::Result<()> {
        todo!()
    }
}
