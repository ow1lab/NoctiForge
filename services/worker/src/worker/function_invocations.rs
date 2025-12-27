use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;
use tracing::info;
use url::Url;
use anyhow::{Ok, Result};

use crate::worker::container::ProccesContainer;

pub struct Invocation {
    pub url: Url
}

pub struct FunctionInvocations {
    root_path: PathBuf,
    functions: Arc<RwLock<HashMap<String, Arc<Invocation>>>>,
}

impl FunctionInvocations {
    pub fn new(root_path: PathBuf) -> Self {
        Self { 
            functions: Arc::new(RwLock::new(HashMap::new())),
            root_path,
        }
    }
}

impl FunctionInvocations {

    /// Get a process by instance_id
    pub async fn get(&self, instance_id: &str) -> Option<Arc<Invocation>> {
        let functions = self.functions.read().await;
        functions.get(instance_id).cloned()
    }
    /// Get all processes
    pub async fn get_all(&self) -> HashMap<String,Arc<Invocation>> {
        let functions = self.functions.read().await;
        functions.clone()
    }

    /// Insert a process (idempotent overwrite)
    pub async fn insert(
        &self,
        instance_id: String,
        url: Url,
    ) -> Arc<Invocation> {
        info!("inserting a new proccess with id {}", instance_id);
        let new_invocation = Arc::new(Invocation{
            url
        });
        let mut functions = self.functions.write().await;
        functions.insert(instance_id, new_invocation.clone());
        new_invocation
    }
    
    pub async fn delete(&self, instance_id: &str) -> Result<()> {
        info!("deleting {}", instance_id);
        let mut func_proc = ProccesContainer::load(&self.root_path, instance_id).await?;
        func_proc.cleanup().await?;

        let mut functions = self.functions.write().await;
        functions.remove(instance_id);

        Ok(())
    }

    pub async fn delete_all(&self) -> Result<()> {
        let keys: Vec<String> = {
            let functions = self.functions.read().await;
            functions.keys().cloned().collect()
        };

        for key in keys {
            self.delete(&key).await?;
        }

        Ok(())
    }
}
