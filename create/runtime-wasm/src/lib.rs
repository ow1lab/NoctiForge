use async_trait::async_trait;
use runtime::{Instance, Runtime, RuntimeConfig, RuntimeError};
use wasmtime::{Engine, Module, Store};

const ENTRYPOINT_EXPORT: &'static str = "run";

pub struct WasmEngine {
    engine: Engine,
}

impl WasmEngine {
    pub fn new() -> Self {
        Self {
            engine: Engine::default(),
        }
    }
}

pub struct WasmInstance {
    store: Store<()>,
    instance: wasmtime::Instance,
}

#[async_trait]
impl Runtime for WasmEngine {
    async fn start(&self, config: RuntimeConfig) -> Result<Box<dyn Instance>, RuntimeError> {
        let wasm = tokio::fs::read(&config.artifact)
            .await
            .map_err(|e| RuntimeError::Start(e.to_string()))?;

        let module =
            Module::new(&self.engine, &wasm).map_err(|e| RuntimeError::Start(e.to_string()))?;

        let mut store = Store::new(&self.engine, ());

        let instance = wasmtime::Instance::new(&mut store, &module, &[])
            .map_err(|e| RuntimeError::Start(e.to_string()))?;

        Ok(Box::new(WasmInstance { store, instance }))
    }
}

#[async_trait]
impl Instance for WasmInstance {
    async fn invoke(&mut self, _payload: &[u8]) -> Result<Vec<u8>, RuntimeError> {
        let run = self
            .instance
            .get_typed_func::<(), i32>(&mut self.store, ENTRYPOINT_EXPORT)
            .map_err(|e| RuntimeError::Execution(e.to_string()))?;

        let result = run
            .call_async(&mut self.store, ())
            .await
            .map_err(|e| RuntimeError::Execution(e.to_string()))?;

        Ok(result.to_le_bytes().to_vec())
    }

    async fn stop(self: Box<Self>) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_wasm_sandbox() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hello.wasm");

        let wasm = wat::parse_str(
            r#"
        (module
            (func (export "run") (result i32)
                i32.const 42
            )
        )
        "#,
        )
        .unwrap();
        std::fs::write(&path, wasm).unwrap();

        let engine = WasmEngine::new();

        let mut instance = engine
            .start(RuntimeConfig {
                artifact: path.into_string().expect("Can't convert"),
            })
            .await
            .unwrap();

        let result = instance.invoke(b"hello").await.unwrap();
        assert_eq!(i32::from_le_bytes(result.try_into().unwrap()), 42);
    }
}
