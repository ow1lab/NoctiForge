use serde::Deserialize;

use super::BuildService;

#[derive(Debug, Deserialize)]
pub struct CustomBuild {
    script: String,
    output: String,
}

impl BuildService for CustomBuild {
    fn build(&self) -> anyhow::Result<String> {
        _ = self.script;
        _ = self.output;
        todo!()
    }
}
