use serde::Deserialize;

use super::BuildService;

#[derive(Debug, Deserialize)]
pub struct RustBuild {
    output: String,
}

impl BuildService for RustBuild {
    fn build(&self) -> anyhow::Result<String> {
        _ = self.output;
        todo!()
    }
}
