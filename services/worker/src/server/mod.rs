use std::sync::Arc;

use proto::api::worker::{worker_service_server::WorkerService, ExecuteRequest, ExecuteResponse};
use tonic::{Request, Response, Status};

use crate::worker::FunctionWorker;

pub struct WorkerServer {
    function_worker: Arc<dyn FunctionWorker + Send + Sync>,
}

impl WorkerServer {
    pub fn new(function_worker: Arc<dyn FunctionWorker + Send + Sync>) -> Self {
        Self { function_worker }
    }
}

#[tonic::async_trait]
impl WorkerService for WorkerServer {
    async fn execute(
        &self,
        request: Request<ExecuteRequest>
    ) -> Result<Response<ExecuteResponse>, Status> {
       _ = request.into_inner();
       self.function_worker
            .execute("123".to_string())
            .map_err(|e| Status::internal(format!("Execution failed: {:?}", e)))?;
        Ok(Response::new(ExecuteResponse{
            status: "Ok".to_string(),
            resp: "Anwser".to_string()
        }))
    }
}
