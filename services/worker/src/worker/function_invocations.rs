use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::{sync::Mutex, time::Instant};
use tracing::info;
use url::Url;
use anyhow::{Ok, Result};

use crate::worker::container::ProccesContainer;

pub struct Invocation {
    pub url: Url,
    pub last_accessed: Instant,
}

pub struct FunctionInvocations {
    root_path: PathBuf,
    functions: Arc<Mutex<HashMap<String, Arc<Mutex<Invocation>>>>>,
}

impl FunctionInvocations {
    pub fn new(root_path: PathBuf) -> Self {
        Self { 
            functions: Arc::new(Mutex::new(HashMap::new())),
            root_path,
        }
    }
}

impl FunctionInvocations {

    /// Get a process by instance_id
    pub async fn get(&self, instance_id: &str) -> Option<Arc<Mutex<Invocation>>> {
        self.get_internal(instance_id, true).await
    }

    pub async fn peek(&self, instance_id: &str) -> Option<Arc<Mutex<Invocation>>> {
        self.get_internal(instance_id, false).await
    }

    pub async fn get_internal(&self, instance_id: &str, touch: bool) -> Option<Arc<Mutex<Invocation>>> {
        let functions = self.functions.lock().await;

        let invocation = functions.get(instance_id)?.clone();

        if touch {
            let mut inv = invocation.lock().await;
            inv.last_accessed = Instant::now();
        }

        Some(invocation)
    }

    pub async fn keys(&self) -> Vec<String> {
        let functions = self.functions.lock().await;
        functions.keys().cloned().collect()
    }
    
    /// Insert a process (idempotent overwrite)
    pub async fn insert(
        &self,
        instance_id: String,
        url: Url,
    ) -> Arc<Mutex<Invocation>> {
        info!("inserting a new proccess with id {}", instance_id);
        let new_invocation = Arc::new(Mutex::new(Invocation{
            url,
            last_accessed: Instant::now(),
        }));
        let mut functions = self.functions.lock().await;
        functions.insert(instance_id, new_invocation.clone());
        new_invocation
    }
    
    pub async fn delete(&self, instance_id: &str) -> Result<()> {
        info!("deleting {}", instance_id);

        let removed = {
            let mut functions = self.functions.lock().await;
            functions.remove(instance_id)
        };

        if removed.is_none() {
            return Ok(());
        }

        let mut proc = ProccesContainer::load(&self.root_path, instance_id).await?;
        proc.cleanup().await?;

        Ok(())
    }

    pub async fn delete_all(&self) -> Result<()> {
        let keys: Vec<String> = {
            let functions = self.functions.lock().await;
            functions.keys().cloned().collect()
        };

        for key in keys {
            self.delete(&key).await?;
        }

        Ok(())
    }
}
