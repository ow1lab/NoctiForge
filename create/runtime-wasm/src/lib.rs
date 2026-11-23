use async_trait::async_trait;
use runtime::{Instance, Runtime, RuntimeConfig, RuntimeError, runtime::InvokeResult};
use wasmtime::{Engine, Module, Store};

const ENTRYPOINT_EXPORT: &str = "run";
const MEMORY_EXPORT: &str = "memory";
const STATUS_SUCCESS: i32 = 0;
const STATUS_PROBLEM: i32 = 1;

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
    async fn invoke(&mut self, _payload: &[u8]) -> Result<InvokeResult, RuntimeError> {
        let run = self
            .instance
            .get_typed_func::<(), (i32, i32, i32, i32, i32)>(&mut self.store, ENTRYPOINT_EXPORT)
            .map_err(|e| RuntimeError::Execution(e.to_string()))?;

        let (status, first_ptr, first_len, second_ptr, second_len) = run
            .call(&mut self.store, ())
            .map_err(|e| RuntimeError::Execution(e.to_string()))?;

        let memory = self
            .instance
            .get_memory(&mut self.store, MEMORY_EXPORT)
            .ok_or_else(|| RuntimeError::Execution("missing memory export".into()))?;

        let read_bytes = |ptr: i32, len: i32| -> Result<Vec<u8>, RuntimeError> {
            let ptr = usize::try_from(ptr)
                .map_err(|_| RuntimeError::Execution("negative pointer".into()))?;
            let len = usize::try_from(len)
                .map_err(|_| RuntimeError::Execution("negative length".into()))?;
            let end = ptr
                .checked_add(len)
                .ok_or_else(|| RuntimeError::Execution("output range overflow".into()))?;

            if end > memory.data_size(&self.store) {
                return Err(RuntimeError::Execution(
                    "output range is outside guest memory".into(),
                ));
            }

            let mut bytes = vec![0; len];
            memory
                .read(&self.store, ptr, &mut bytes)
                .map_err(|e| RuntimeError::Execution(e.to_string()))?;
            Ok(bytes)
        };

        match status {
            STATUS_SUCCESS => Ok(InvokeResult::Success {
                output: read_bytes(first_ptr, first_len)?,
            }),
            STATUS_PROBLEM => {
                let r#type = String::from_utf8(read_bytes(first_ptr, first_len)?)
                    .map_err(|e| RuntimeError::Execution(e.to_string()))?;
                let details = String::from_utf8(read_bytes(second_ptr, second_len)?)
                    .map_err(|e| RuntimeError::Execution(e.to_string()))?;
                Ok(InvokeResult::Problem { r#type, details })
            }
            _ => Err(RuntimeError::Execution(format!(
                "unknown guest result status: {status}"
            ))),
        }
    }

    async fn stop(self: Box<Self>) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invoke_returns_success() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hello.wasm");

        let wasm = wat::parse_str(
            r#"
              (module                                                                                                                                  
                 (memory (export "memory") 1)                                                                                                           
                 (data (i32.const 0) "hello")                                                                                                           
                                                                                                                                                        
                 (func (export "run") (result i32 i32 i32 i32 i32)                                                                                                  
                   i32.const 0  ;; success status
                   i32.const 0  ;; output pointer                                                                                                              
                   i32.const 5  ;; output length
                   i32.const 0  ;; unused second pointer
                   i32.const 0  ;; unused second length                                                                                                               
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
        match result {
            InvokeResult::Success { output } => assert_eq!(output, b"hello"),
            InvokeResult::Problem { .. } => panic!("expected success"),
        }
    }

    #[tokio::test]
    async fn invoke_returns_problem() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("problem.wasm");
        let wasm = wat::parse_str(
            r#"
            (module
                (memory (export "memory") 1)
                (data (i32.const 0) "about:blank")
                (data (i32.const 32) "invalid input")
                (func (export "run") (result i32 i32 i32 i32 i32)
                    i32.const 1
                    i32.const 0
                    i32.const 11
                    i32.const 32
                    i32.const 13
                )
            )
            "#,
        )
        .unwrap();
        std::fs::write(&path, wasm).unwrap();

        let engine = WasmEngine::new();
        let mut instance = engine
            .start(RuntimeConfig {
                artifact: path.into_string().unwrap(),
            })
            .await
            .unwrap();

        match instance.invoke(b"bad input").await.unwrap() {
            InvokeResult::Problem { r#type, details } => {
                assert_eq!(r#type, "about:blank");
                assert_eq!(details, "invalid input");
            }
            InvokeResult::Success { .. } => panic!("expected problem"),
        }
    }

    async fn invoke_wat(source: &str) -> Result<InvokeResult, RuntimeError> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wasm");
        std::fs::write(&path, wat::parse_str(source).unwrap()).unwrap();
        let engine = WasmEngine::new();
        let mut instance = engine
            .start(RuntimeConfig {
                artifact: path.into_string().unwrap(),
            })
            .await
            .unwrap();
        instance.invoke(b"").await
    }

    fn assert_execution_error(result: Result<InvokeResult, RuntimeError>, message: &str) {
        match result {
            Err(RuntimeError::Execution(error)) => assert!(
                error.contains(message),
                "expected error containing {message:?}, got {error:?}"
            ),
            _ => panic!("expected a runtime execution error"),
        }
    }

    #[tokio::test]
    async fn test_unknown_guest_status() {
        let result = invoke_wat(
            r#"(module
                (memory (export "memory") 1)
                (func (export "run") (result i32 i32 i32 i32 i32)
                    i32.const 2 i32.const 0 i32.const 0 i32.const 0 i32.const 0)
            )"#,
        )
        .await;
        assert_execution_error(result, "unknown guest result status: 2");
    }

    #[tokio::test]
    async fn test_output_outside_guest_memory() {
        let result = invoke_wat(
            r#"(module
                (memory (export "memory") 1)
                (func (export "run") (result i32 i32 i32 i32 i32)
                    i32.const 0 i32.const 65535 i32.const 2 i32.const 0 i32.const 0)
            )"#,
        )
        .await;
        assert_execution_error(result, "output range is outside guest memory");
    }

    #[tokio::test]
    async fn test_problem_requires_utf8() {
        let result = invoke_wat(
            r#"(module
                (memory (export "memory") 1)
                (data (i32.const 0) "\ff")
                (func (export "run") (result i32 i32 i32 i32 i32)
                    i32.const 1 i32.const 0 i32.const 1 i32.const 0 i32.const 0)
            )"#,
        )
        .await;
        assert_execution_error(result, "invalid utf-8");
    }

    #[tokio::test]
    async fn test_missing_memory_export() {
        let result = invoke_wat(
            r#"(module
                (func (export "run") (result i32 i32 i32 i32 i32)
                    i32.const 0 i32.const 0 i32.const 0 i32.const 0 i32.const 0)
            )"#,
        )
        .await;
        assert_execution_error(result, "missing memory export");
    }

    #[tokio::test]
    async fn test_guest_trap() {
        let result = invoke_wat(
            r#"(module
                (memory (export "memory") 1)
                (func (export "run") (result i32 i32 i32 i32 i32)
                    unreachable)
            )"#,
        )
        .await;
        assert_execution_error(result, "error while executing");
    }
}
