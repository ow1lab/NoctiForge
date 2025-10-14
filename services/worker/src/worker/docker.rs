use crate::worker::FunctionWorker;
use anyhow::Result;

pub struct DockerWorker {
}

impl DockerWorker {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }
}

impl FunctionWorker for DockerWorker {
    fn execute(&self, package_digest: String) -> Result<(), Box<dyn std::error::Error>> {
        println!("Running in Docker with package: {}", package_digest);
        Ok(())
    }
}
