use anyhow::Result;

pub mod docker;

pub trait FunctionWorker: Send + Sync {
    fn execute(&self, package_digest: String) -> Result<(), Box<dyn std::error::Error>>;
}
