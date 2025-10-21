use proto::api::worker::{ExecuteRequest, ExecuteResponse, worker_service_server::WorkerService};
use tonic::{Request, Response, Status};
use tracing::{debug, info, instrument, warn};

use crate::{client::controlplane_client::ControlPlaneClient, worker::NativeWorker};

pub struct WorkerServer {
    function_worker: NativeWorker,
    controlplane_client: ControlPlaneClient,
}

impl WorkerServer {
    pub fn new(function_worker: NativeWorker, controlplane_client: ControlPlaneClient) -> Self {
        debug!("Creating WorkerServer");
        Self {
            function_worker,
            controlplane_client,
        }
    }
}

#[tonic::async_trait]
impl WorkerService for WorkerServer {
    #[instrument(skip(self, request), fields(action = %request.get_ref().action))]
    async fn execute(
        &self,
        request: Request<ExecuteRequest>,
    ) -> Result<Response<ExecuteResponse>, Status> {
        let req = request.into_inner();

        info!(action = %req.action, "Executing request");

        debug!(action = %req.action, "Fetching digest from control plane");
        let digest = self
            .controlplane_client
            .get_digest(req.action.clone())
            .await
            .map_err(|e| {
                warn!(action = %req.action, error = %e, "Failed to communicate with control plane");
                Status::internal(format!("Failed to commnicate with controlplane: {:?}", e))
            })?;

        debug!(action = %req.action, digest = %digest, body_size = req.body.len(), "Executing function");
        let output = self
            .function_worker
            .execute(digest, req.body)
            .await
            .map_err(|e| {
                warn!(action = %req.action, error = %e, "Execution failed");
                Status::internal(format!("Execution failed: {:?}", e))
            })?;        

        info!(action = %req.action, output_size = output.len(), "Execution completed successfully");

        Ok(Response::new(ExecuteResponse {
            status: "Ok".to_string(),
            resp: output,
        }))
    }
}
