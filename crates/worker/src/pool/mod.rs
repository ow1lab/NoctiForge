mod cleanup;
mod instance;
mod key;

use color_eyre::Result;

use std::{collections::HashMap, sync::Arc};

use runtime::{Instance, RuntimeConfig, RuntimeError, runtime::InvokeResult};
use tokio::{
    sync::{Mutex, Semaphore},
    time::Instant,
};

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
    /// Invokes an instance for the given pool key.
    ///
    /// # Errors
    /// Returns an error if acquiring or invoking the instance fails.
    pub async fn invoke(
        &self,
        key: PoolKey,
        config: RuntimeConfig,
        payload: &[u8],
    ) -> Result<InvokeResult, RuntimeError> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| RuntimeError::Execution(e.to_string()))?;

        let mut instance = self.acquire(&key, config).await?;

        match instance.invoke(payload).await {
            Ok(result) => {
                if let Err(error) = self.release(key, instance).await {
                    tracing::warn!(%error, "Failed to release instance");
                }

                Ok(result)
            }
            Err(error) => {
                if let Err(stop_error) = instance.stop().await {
                    tracing::warn!(%stop_error, "Failed to stop instance");
                }

                Err(error)
            }
        }
    }

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

    struct InvokingInstance {
        payloads: Arc<Mutex<Vec<Vec<u8>>>>,
        stops: Arc<AtomicUsize>,
        fail_invoke: bool,
        problem: bool,
        fail_stop: bool,
    }

    #[async_trait]
    impl Instance for InvokingInstance {
        async fn invoke(&mut self, payload: &[u8]) -> Result<InvokeResult, RuntimeError> {
            self.payloads.lock().await.push(payload.to_vec());
            if self.fail_invoke {
                return Err(RuntimeError::Execution("invoke failed".to_owned()));
            }
            if self.problem {
                return Ok(InvokeResult::Problem {
                    r#type: "test problem".to_owned(),
                    details: "details".to_owned(),
                });
            }
            Ok(InvokeResult::Success {
                output: payload.to_vec(),
            })
        }

        async fn stop(self: Box<Self>) -> Result<(), RuntimeError> {
            self.stops.fetch_add(1, Ordering::SeqCst);
            if self.fail_stop {
                return Err(RuntimeError::Stop("stop failed".to_owned()));
            }
            Ok(())
        }
    }

    fn invoking_instance(
        payloads: &Arc<Mutex<Vec<Vec<u8>>>>,
        stops: &Arc<AtomicUsize>,
        fail_invoke: bool,
        problem: bool,
        fail_stop: bool,
    ) -> Box<dyn Instance> {
        Box::new(InvokingInstance {
            payloads: Arc::clone(payloads),
            stops: Arc::clone(stops),
            fail_invoke,
            problem,
            fail_stop,
        })
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
    async fn invoke_starts_instance_and_caches_it_after_success() {
        let (registry, starts, _) = registry(false);
        let pool = pool(registry);

        let result = pool
            .invoke(
                key("module-a"),
                RuntimeConfig {
                    artifact: "module-a".to_owned(),
                },
                b"input",
            )
            .await;

        assert!(matches!(result, Ok(InvokeResult::Success { output }) if output == b"module-a"));
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(
            pool.idle.lock().await.get(&key("module-a")).map(Vec::len),
            Some(1)
        );
    }

    #[tokio::test]
    async fn invoke_passes_payload_to_idle_instance_and_releases_it() {
        let pool = pool(RuntimeRegistry::default());
        let payloads = Arc::new(Mutex::new(Vec::new()));
        let stops = Arc::new(AtomicUsize::new(0));
        pool.idle.lock().await.insert(
            key("module-a"),
            vec![IdleInstance {
                instance: invoking_instance(&payloads, &stops, false, false, false),
                last_used: Instant::now(),
            }],
        );

        let result = pool
            .invoke(
                key("module-a"),
                RuntimeConfig {
                    artifact: "module-a".to_owned(),
                },
                b"request",
            )
            .await;

        assert!(matches!(result, Ok(InvokeResult::Success { output }) if output == b"request"));
        assert_eq!(*payloads.lock().await, vec![b"request".to_vec()]);
        assert_eq!(stops.load(Ordering::SeqCst), 0);
        assert_eq!(
            pool.idle.lock().await.get(&key("module-a")).map(Vec::len),
            Some(1)
        );
    }

    #[tokio::test]
    async fn invoke_problem_result_is_released_as_successful_invocation() {
        let pool = pool(RuntimeRegistry::default());
        let payloads = Arc::new(Mutex::new(Vec::new()));
        let stops = Arc::new(AtomicUsize::new(0));
        pool.idle.lock().await.insert(
            key("module-a"),
            vec![IdleInstance {
                instance: invoking_instance(&payloads, &stops, false, true, false),
                last_used: Instant::now(),
            }],
        );

        let result = pool
            .invoke(
                key("module-a"),
                RuntimeConfig {
                    artifact: "module-a".to_owned(),
                },
                b"request",
            )
            .await;

        assert!(
            matches!(result, Ok(InvokeResult::Problem { r#type, details }) if r#type == "test problem" && details == "details")
        );
        assert_eq!(stops.load(Ordering::SeqCst), 0);
        assert_eq!(
            pool.idle.lock().await.get(&key("module-a")).map(Vec::len),
            Some(1)
        );
    }

    #[tokio::test]
    async fn invoke_failure_stops_instance_and_returns_original_error() {
        let pool = pool(RuntimeRegistry::default());
        let payloads = Arc::new(Mutex::new(Vec::new()));
        let stops = Arc::new(AtomicUsize::new(0));
        pool.idle.lock().await.insert(
            key("module-a"),
            vec![IdleInstance {
                instance: invoking_instance(&payloads, &stops, true, false, true),
                last_used: Instant::now(),
            }],
        );

        let result = pool
            .invoke(
                key("module-a"),
                RuntimeConfig {
                    artifact: "module-a".to_owned(),
                },
                b"request",
            )
            .await;

        assert!(
            matches!(result, Err(RuntimeError::Execution(message)) if message == "invoke failed")
        );
        assert_eq!(stops.load(Ordering::SeqCst), 1);
        assert!(
            pool.idle
                .lock()
                .await
                .get(&key("module-a"))
                .is_some_and(Vec::is_empty)
        );
    }

    #[tokio::test]
    async fn invoke_returns_result_even_if_releasing_instance_fails() {
        let mut pool = pool(RuntimeRegistry::default());
        pool.config.max_idle_instances = 0;
        let payloads = Arc::new(Mutex::new(Vec::new()));
        let stops = Arc::new(AtomicUsize::new(0));
        pool.idle.lock().await.insert(
            key("module-a"),
            vec![IdleInstance {
                instance: invoking_instance(&payloads, &stops, false, false, true),
                last_used: Instant::now(),
            }],
        );

        let result = pool
            .invoke(
                key("module-a"),
                RuntimeConfig {
                    artifact: "module-a".to_owned(),
                },
                b"request",
            )
            .await;

        assert!(matches!(result, Ok(InvokeResult::Success { output }) if output == b"request"));
        assert_eq!(stops.load(Ordering::SeqCst), 1);
        assert!(
            pool.idle
                .lock()
                .await
                .get(&key("module-a"))
                .is_some_and(Vec::is_empty)
        );
    }

    #[tokio::test]
    async fn invoke_propagates_acquire_failure() {
        let pool = pool(RuntimeRegistry::default());

        let result = pool
            .invoke(
                key("module-a"),
                RuntimeConfig {
                    artifact: "module-a".to_owned(),
                },
                b"request",
            )
            .await;

        assert!(
            matches!(result, Err(RuntimeError::Start(message)) if message.contains("Unsupported runtime"))
        );
        assert!(pool.idle.lock().await.is_empty());
    }

    #[tokio::test]
    async fn invoke_returns_execution_error_when_semaphore_is_closed() {
        let (registry, starts, _) = registry(false);
        let pool = pool(registry);
        pool.semaphore.close();

        let result = pool
            .invoke(
                key("module-a"),
                RuntimeConfig {
                    artifact: "module-a".to_owned(),
                },
                b"request",
            )
            .await;

        assert!(matches!(result, Err(RuntimeError::Execution(_))));
        assert_eq!(starts.load(Ordering::SeqCst), 0);
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
