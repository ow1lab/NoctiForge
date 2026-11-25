mod instance;
mod key;

use std::{collections::HashMap, sync::Arc};

use tokio::sync::{Mutex, Semaphore};

use crate::{config::PoolConfig, registry::RuntimeRegistry};

use instance::IdleInstance;
pub use key::PoolKey;

pub struct WorkerPool {
    registry: Arc<RuntimeRegistry>,
    config: PoolConfig,
    semaphore: Semaphore,
    idle: Mutex<HashMap<PoolKey, Vec<IdleInstance>>>,
}
