mod instance;
mod key;

use color_eyre::Result;

use std::{collections::HashMap, sync::Arc, time::Instant};

use runtime::{Instance, RuntimeConfig, RuntimeError};
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

impl WorkerPool {
    async fn acquire(
        &self,
        key: &PoolKey,
        config: RuntimeConfig,
    ) -> Result<Box<dyn Instance>, RuntimeError> {
        let mut idle = self.idle.lock().await;

        if let Some(instances) = idle.get_mut(key)
            && let Some(instance) = instances.pop()
        {
            return Ok(instance.instance);
        }
        drop(idle);

        let runtime = self.registry.get(key.runtime).ok_or_else(|| {
            RuntimeError::Start(format!("Unsupported runtime: {:?}", key.runtime))
        })?;

        runtime.start(config).await
    }

    async fn release(&self, key: PoolKey, instance: Box<dyn Instance>) -> Result<(), RuntimeError> {
        let mut idle = self.idle.lock().await;

        let count: usize = idle.values().map(Vec::len).sum();

        if count < self.config.max_idle_instances {
            idle.entry(key).or_default().push(IdleInstance {
                instance,
                last_used: Instant::now(),
            });

            return Ok(());
        }

        drop(idle);
        instance.stop().await
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Instant,
    };

    use async_trait::async_trait;
    use runtime::{Instance, Runtime, RuntimeConfig, RuntimeError, runtime::InvokeResult};
    use tokio::sync::{Mutex, Semaphore};

    use crate::{
        config::PoolConfig,
        registry::{RuntimeKind, RuntimeRegistry},
    };

    use super::{IdleInstance, PoolKey, WorkerPool};

    struct TestInstance {
        output: Vec<u8>,
    }

    #[async_trait]
    impl Instance for TestInstance {
        async fn invoke(&mut self, _payload: &[u8]) -> Result<InvokeResult, RuntimeError> {
            Ok(InvokeResult::Success {
                output: self.output.clone(),
            })
        }

        async fn stop(self: Box<Self>) -> Result<(), RuntimeError> {
            Ok(())
        }
    }

    struct StoppableInstance {
        stops: Arc<AtomicUsize>,
        fail: bool,
    }

    #[async_trait]
    impl Instance for StoppableInstance {
        async fn invoke(&mut self, _payload: &[u8]) -> Result<InvokeResult, RuntimeError> {
            Ok(InvokeResult::Success { output: Vec::new() })
        }

        async fn stop(self: Box<Self>) -> Result<(), RuntimeError> {
            self.stops.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(RuntimeError::Stop("test stop failure".to_owned()));
            }
            Ok(())
        }
    }

    struct TestRuntime {
        starts: Arc<AtomicUsize>,
        configs: Arc<Mutex<Vec<String>>>,
        fail: bool,
    }

    #[async_trait]
    impl Runtime for TestRuntime {
        async fn start(&self, config: RuntimeConfig) -> Result<Box<dyn Instance>, RuntimeError> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            self.configs.lock().await.push(config.artifact.clone());
            if self.fail {
                return Err(RuntimeError::Start("test failure".to_owned()));
            }
            Ok(Box::new(TestInstance {
                output: config.artifact.into_bytes(),
            }))
        }
    }

    fn key(artifact: &str) -> PoolKey {
        PoolKey {
            runtime: RuntimeKind::Wasm,
            artifact: artifact.to_owned(),
        }
    }

    fn pool(registry: RuntimeRegistry) -> WorkerPool {
        WorkerPool {
            registry: Arc::new(registry),
            config: PoolConfig::default(),
            semaphore: Semaphore::new(1),
            idle: Mutex::new(HashMap::new()),
        }
    }

    fn registry(fail: bool) -> (RuntimeRegistry, Arc<AtomicUsize>, Arc<Mutex<Vec<String>>>) {
        let starts = Arc::new(AtomicUsize::new(0));
        let configs = Arc::new(Mutex::new(Vec::new()));
        let mut registry = RuntimeRegistry::default();
        registry.register(
            RuntimeKind::Wasm,
            TestRuntime {
                starts: Arc::clone(&starts),
                configs: Arc::clone(&configs),
                fail,
            },
        );
        (registry, starts, configs)
    }

    #[tokio::test]
    async fn acquire_starts_runtime_with_supplied_config_when_no_instance_is_idle() {
        let (registry, starts, configs) = registry(false);
        let pool = pool(registry);
        let config = RuntimeConfig {
            artifact: "module-a".to_owned(),
        };

        let result = pool.acquire(&key("module-a"), config).await;

        assert!(result.is_ok());
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(*configs.lock().await, vec!["module-a"]);
    }

    #[tokio::test]
    async fn acquire_reuses_matching_idle_instance_without_starting_runtime() {
        let (registry, starts, _) = registry(false);
        let pool = pool(registry);
        pool.idle.lock().await.insert(
            key("module-a"),
            vec![IdleInstance {
                instance: Box::new(TestInstance {
                    output: b"reused".to_vec(),
                }),
                last_used: Instant::now(),
            }],
        );

        let result = pool
            .acquire(
                &key("module-a"),
                RuntimeConfig {
                    artifact: "module-a".to_owned(),
                },
            )
            .await;
        let mut instance = result.ok();
        let output = if let Some(instance) = instance.as_mut() {
            instance.invoke(b"input").await.ok()
        } else {
            None
        };

        assert!(matches!(output, Some(InvokeResult::Success { output }) if output == b"reused"));
        assert_eq!(starts.load(Ordering::SeqCst), 0);
        assert!(
            pool.idle
                .lock()
                .await
                .get(&key("module-a"))
                .is_some_and(Vec::is_empty)
        );
    }

    #[tokio::test]
    async fn acquire_does_not_reuse_instance_for_different_artifact() {
        let (registry, starts, _) = registry(false);
        let pool = pool(registry);
        pool.idle.lock().await.insert(
            key("module-a"),
            vec![IdleInstance {
                instance: Box::new(TestInstance {
                    output: b"reused".to_vec(),
                }),
                last_used: Instant::now(),
            }],
        );

        let result = pool
            .acquire(
                &key("module-b"),
                RuntimeConfig {
                    artifact: "module-b".to_owned(),
                },
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(
            pool.idle.lock().await.get(&key("module-a")).map(Vec::len),
            Some(1)
        );
    }

    #[tokio::test]
    async fn acquire_reports_unsupported_runtime() {
        let pool = pool(RuntimeRegistry::default());

        let result = pool
            .acquire(
                &key("module-a"),
                RuntimeConfig {
                    artifact: "module-a".to_owned(),
                },
            )
            .await;

        assert!(
            matches!(result, Err(RuntimeError::Start(message)) if message.contains("Unsupported runtime"))
        );
    }

    #[tokio::test]
    async fn acquire_propagates_runtime_start_failure() {
        let (registry, starts, _) = registry(true);
        let pool = pool(registry);

        let result = pool
            .acquire(
                &key("module-a"),
                RuntimeConfig {
                    artifact: "module-a".to_owned(),
                },
            )
            .await;

        assert!(matches!(result, Err(RuntimeError::Start(message)) if message == "test failure"));
        assert_eq!(starts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn release_caches_instance_with_current_timestamp_below_limit() {
        let pool = pool(RuntimeRegistry::default());
        let stops = Arc::new(AtomicUsize::new(0));
        let before = Instant::now();

        let result = pool
            .release(
                key("module-a"),
                Box::new(StoppableInstance {
                    stops: Arc::clone(&stops),
                    fail: false,
                }),
            )
            .await;
        let after = Instant::now();

        assert!(result.is_ok());
        assert_eq!(stops.load(Ordering::SeqCst), 0);
        let idle = pool.idle.lock().await;
        let cached_recent = idle
            .get(&key("module-a"))
            .and_then(|instances| instances.first())
            .is_some_and(|instance| instance.last_used >= before && instance.last_used <= after);
        drop(idle);
        assert!(cached_recent);
    }

    #[tokio::test]
    async fn release_stops_instance_when_global_idle_limit_is_reached() {
        let mut pool = pool(RuntimeRegistry::default());
        pool.config.max_idle_instances = 1;
        let stops = Arc::new(AtomicUsize::new(0));

        let first = pool
            .release(
                key("module-a"),
                Box::new(StoppableInstance {
                    stops: Arc::clone(&stops),
                    fail: false,
                }),
            )
            .await;
        let second = pool
            .release(
                key("module-b"),
                Box::new(StoppableInstance {
                    stops: Arc::clone(&stops),
                    fail: false,
                }),
            )
            .await;

        assert!(first.is_ok());
        assert!(second.is_ok());
        assert_eq!(stops.load(Ordering::SeqCst), 1);
        let idle = pool.idle.lock().await;
        assert_eq!(idle.get(&key("module-a")).map(Vec::len), Some(1));
        assert!(idle.get(&key("module-b")).is_none());
        drop(idle);
    }

    #[tokio::test]
    async fn release_stops_instance_when_idle_limit_is_zero() {
        let mut pool = pool(RuntimeRegistry::default());
        pool.config.max_idle_instances = 0;
        let stops = Arc::new(AtomicUsize::new(0));

        let result = pool
            .release(
                key("module-a"),
                Box::new(StoppableInstance {
                    stops: Arc::clone(&stops),
                    fail: false,
                }),
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(stops.load(Ordering::SeqCst), 1);
        assert!(pool.idle.lock().await.is_empty());
    }

    #[tokio::test]
    async fn release_propagates_stop_failure_when_full() {
        let mut pool = pool(RuntimeRegistry::default());
        pool.config.max_idle_instances = 0;
        let stops = Arc::new(AtomicUsize::new(0));

        let result = pool
            .release(
                key("module-a"),
                Box::new(StoppableInstance {
                    stops: Arc::clone(&stops),
                    fail: true,
                }),
            )
            .await;

        assert!(
            matches!(result, Err(RuntimeError::Stop(message)) if message == "test stop failure")
        );
        assert_eq!(stops.load(Ordering::SeqCst), 1);
        assert!(pool.idle.lock().await.is_empty());
    }
}
