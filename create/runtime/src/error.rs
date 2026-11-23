use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Failed to start runtime: {0}")]
    Start(String),

    #[error("Execution failed: {0}")]
    Execution(String),

    #[error("Failed to stop runtime: {0}")]
    Stop(String),
}
