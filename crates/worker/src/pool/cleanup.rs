use std::sync::Arc;

use tokio::time::Instant;

use super::WorkerPool;

impl WorkerPool {
    pub async fn cleanup_idle(&self) {
        let now = Instant::now();
        let mut expired = Vec::new();

        {
            let mut idle = self.idle.lock().await;
            idle.retain(|_, instances| {
                expired.extend(instances.extract_if(.., |instance| {
                    now.saturating_duration_since(instance.last_used) >= self.config.idle_timeout
                }));

                !instances.is_empty()
            });
        }

        for idle in expired {
            if let Err(error) = idle.instance.stop().await {
                tracing::warn!(%error, "Failed to stop idle instance");
            }
        }
    }

    pub async fn run_cleanup(self: Arc<Self>) {
        let mut interval = tokio::time::interval(self.config.cleanup_interval);

        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            self.cleanup_idle().await;
        }
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
        time::Duration,
    };

    use async_trait::async_trait;
    use runtime::{Instance, RuntimeError, runtime::InvokeResult};
    use tokio::{
        sync::{Mutex, Semaphore},
        time::Instant,
    };

    use super::super::{IdleInstance, PoolKey, WorkerPool};
    use crate::{
        config::PoolConfig,
        registry::{RuntimeKind, RuntimeRegistry},
    };

    struct TrackedInstance {
        stops: Arc<AtomicUsize>,
        fail: bool,
    }

    #[async_trait]
    impl Instance for TrackedInstance {
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

    fn key(artifact: &str) -> PoolKey {
        PoolKey {
            runtime: RuntimeKind::Wasm,
            artifact: artifact.to_owned(),
        }
    }

    fn pool(timeout: Duration) -> WorkerPool {
        WorkerPool {
            registry: Arc::new(RuntimeRegistry::default()),
            config: PoolConfig {
                idle_timeout: timeout,
                cleanup_interval: Duration::from_secs(10),
                ..PoolConfig::default()
            },
            semaphore: Semaphore::new(1),
            idle: Mutex::new(HashMap::new()),
        }
    }

    fn idle(stops: &Arc<AtomicUsize>, last_used: Instant, fail: bool) -> IdleInstance {
        IdleInstance {
            instance: Box::new(TrackedInstance {
                stops: Arc::clone(stops),
                fail,
            }),
            last_used,
        }
    }

    #[tokio::test(start_paused = true)]
    async fn cleanup_removes_expired_instances_and_empty_keys_but_keeps_fresh_ones() {
        let pool = pool(Duration::from_secs(5));
        let expired_stops = Arc::new(AtomicUsize::new(0));
        let fresh_stops = Arc::new(AtomicUsize::new(0));
        let now = Instant::now();
        let mut entries = pool.idle.lock().await;
        entries.insert(
            key("mixed"),
            vec![
                idle(&expired_stops, now - Duration::from_secs(5), false),
                idle(&fresh_stops, now - Duration::from_secs(4), false),
            ],
        );
        entries.insert(
            key("expired"),
            vec![idle(&expired_stops, now - Duration::from_secs(6), false)],
        );
        drop(entries);

        pool.cleanup_idle().await;

        assert_eq!(expired_stops.load(Ordering::SeqCst), 2);
        assert_eq!(fresh_stops.load(Ordering::SeqCst), 0);
        let entries = pool.idle.lock().await;
        assert_eq!(entries.get(&key("mixed")).map(Vec::len), Some(1));
        assert!(!entries.contains_key(&key("expired")));
        drop(entries);
    }

    #[tokio::test(start_paused = true)]
    async fn cleanup_continues_stopping_after_stop_failure() {
        let pool = pool(Duration::ZERO);
        let stops = Arc::new(AtomicUsize::new(0));
        pool.idle.lock().await.insert(
            key("module"),
            vec![
                idle(&stops, Instant::now(), true),
                idle(&stops, Instant::now(), false),
            ],
        );

        pool.cleanup_idle().await;

        assert_eq!(stops.load(Ordering::SeqCst), 2);
        assert!(pool.idle.lock().await.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn run_cleanup_repeats_at_configured_interval() {
        let pool = Arc::new(pool(Duration::from_secs(5)));
        let stops = Arc::new(AtomicUsize::new(0));
        pool.idle
            .lock()
            .await
            .insert(key("module"), vec![idle(&stops, Instant::now(), false)]);
        let task = tokio::spawn(Arc::clone(&pool).run_cleanup());
        tokio::task::yield_now().await;

        tokio::time::advance(Duration::from_secs(5)).await;
        assert_eq!(stops.load(Ordering::SeqCst), 0);
        tokio::time::advance(Duration::from_secs(5)).await;
        tokio::task::yield_now().await;

        assert_eq!(stops.load(Ordering::SeqCst), 1);
        assert!(pool.idle.lock().await.is_empty());
        task.abort();
    }
}
