use std::time::Duration;

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub concurrency: usize,
    pub max_idle_instances: usize,
    pub idle_timeout: Duration,
    pub cleanup_interval: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            concurrency: 30,
            max_idle_instances: 60,
            idle_timeout: Duration::from_secs(300),
            cleanup_interval: Duration::from_secs(30),
        }
    }
}
