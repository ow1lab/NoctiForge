use std::pin::Pin;

use proto::api::registry::{
    registry_service_server::RegistryService,
    RegistryPullRequest,
    RegistryPullResponse,
    RegistryPushRequest,
    RegistryPushResponse,
};
use sha256::digest;
use tokio::fs::write;
use tokio_stream::{Stream, StreamExt};
use tonic::{
    Request,
    Streaming,
    Result,
    Response,
    Status
};

use crate::path::{get_registry_path};

#[derive(Default)]
pub struct LocalBackend {}

#[tonic::async_trait]
impl RegistryService for LocalBackend {
    type PullStream = Pin<Box<dyn Stream<Item = Result<RegistryPullResponse, Status>> + Send + 'static>>;

    async fn pull(
        &self,
        request: Request<RegistryPullRequest>,
    ) -> Result<Response<Self::PullStream>, Status> {
        _ = request;
        // Placeholder: return an empty stream
        let stream = tokio_stream::empty();
        Ok(Response::new(Box::pin(stream)))
    }

    async fn push(&self, request: Request<Streaming<RegistryPushRequest>>,
    ) -> Result<Response<RegistryPushResponse>, Status> {
        let mut request_data: Vec<u8> = vec![];
        let mut request_stream = request.into_inner();

        while let Some(request) = request_stream.next().await {
            let request = request.map_err(|err| Status::internal(err.to_string()))?;
            request_data.extend_from_slice(&request.data);
        }

        if request_data.is_empty() {
            return Err(Status::invalid_argument("missing `data` field"));
        }

        let digest = digest(request_data.clone());
        let request_path = get_registry_path(&digest);

        if !request_path.exists() {
            write(&request_path, &request_data).await.map_err(|err| {
                Status::internal(format!("failed to write store path: {:?}", err))
            })?;
        }

        Ok(Response::new(RegistryPushResponse { digest: digest }))
    }
}
