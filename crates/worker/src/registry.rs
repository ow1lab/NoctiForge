use std::{collections::HashMap, sync::Arc};

use runtime::Runtime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuntimeKind {
    Wasm,
}

#[derive(Default)]
pub struct RuntimeRegistry {
    runtimes: HashMap<RuntimeKind, Arc<dyn Runtime>>,
}

impl RuntimeRegistry {
    pub fn register(&mut self, kind: RuntimeKind, runtime: impl Runtime + 'static) {
        self.runtimes.insert(kind, Arc::new(runtime));
    }

    #[must_use]
    pub fn get(&self, kind: RuntimeKind) -> Option<Arc<dyn Runtime>> {
        self.runtimes.get(&kind).cloned()
    }

    #[must_use]
    pub fn supported(&self) -> Vec<RuntimeKind> {
        self.runtimes.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use runtime::{Instance, Runtime, RuntimeConfig, RuntimeError};

    use super::{RuntimeKind, RuntimeRegistry};

    struct TestRuntime;

    #[async_trait]
    impl Runtime for TestRuntime {
        async fn start(&self, _config: RuntimeConfig) -> Result<Box<dyn Instance>, RuntimeError> {
            Err(RuntimeError::Start("test runtime".to_owned()))
        }
    }

    #[test]
    fn new_registry_is_empty() {
        let registry = RuntimeRegistry::default();

        assert!(registry.get(RuntimeKind::Wasm).is_none());
        assert_eq!(registry.supported(), Vec::<RuntimeKind>::new());
    }

    #[test]
    fn register_makes_runtime_available() {
        let mut registry = RuntimeRegistry::default();
        registry.register(RuntimeKind::Wasm, TestRuntime);

        let first = registry.get(RuntimeKind::Wasm);
        let second = registry.get(RuntimeKind::Wasm);
        assert!(
            first
                .zip(second)
                .is_some_and(|(first, second)| Arc::ptr_eq(&first, &second))
        );
        assert_eq!(registry.supported(), vec![RuntimeKind::Wasm]);
    }

    #[test]
    fn registering_same_kind_replaces_runtime() {
        let mut registry = RuntimeRegistry::default();
        registry.register(RuntimeKind::Wasm, TestRuntime);
        let original = registry.get(RuntimeKind::Wasm);

        registry.register(RuntimeKind::Wasm, TestRuntime);
        let replacement = registry.get(RuntimeKind::Wasm);

        assert!(
            original
                .zip(replacement)
                .is_some_and(|(original, replacement)| !Arc::ptr_eq(&original, &replacement))
        );
        assert_eq!(registry.supported(), vec![RuntimeKind::Wasm]);
    }
}
