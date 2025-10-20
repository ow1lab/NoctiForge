use std::{io::Cursor, pin::Pin};

use proto::api::registry::{
    registry_service_server::RegistryService,
    RegistryPullRequest,
    RegistryPullResponse,
    RegistryPushRequest,
    RegistryPushResponse,
};
use sha2::{Digest, Sha256};
use tokio::{fs::{read, write}, io::AsyncReadExt};
use tokio_stream::{Stream, StreamExt};
use tokio_tar::Archive;
use tonic::{
    Request,
    Streaming,
    Result,
    Response,
    Status
};

use crate::path::{get_registry_path};

const CHUNK_SIZE: usize = 64 * 1024;

#[derive(Default)]
pub struct LocalBackend {}

#[tonic::async_trait]
impl RegistryService for LocalBackend {
    type PullStream = Pin<Box<dyn Stream<Item = Result<RegistryPullResponse, Status>> + Send + 'static>>;

    async fn pull(
        &self,
        request: Request<RegistryPullRequest>,
    ) -> Result<Response<Self::PullStream>, Status> {
        let req = request.into_inner();

        let request_path = get_registry_path(&req.digest);
        println!("Getting tar from {:?}", request_path);

        let data = read(request_path).await.map_err(|err| {
            Status::internal(format!("failed to read store path: {:?}", err))
        })?;


        let stream = tokio_stream::iter(
            data.chunks(CHUNK_SIZE).map(|chunk| {
                Ok(RegistryPullResponse {
                    data: chunk.to_vec(),
                })
            })
            .collect::<Vec<_>>()
        );

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

        let cursor = Cursor::new(&request_data);
        let mut archive = Archive::new(cursor);
        if let Err(err) = archive.entries() {
            return Err(Status::invalid_argument(format!(
                "invalid tar archive: {}",
                err
            )));
        }

        let digest = get_digist(archive).await?; 
        let request_path = get_registry_path(&digest);

        if !request_path.exists() {
            write(&request_path, &request_data).await.map_err(|err| {
                Status::internal(format!("failed to write store path: {:?}", err))
            })?;
        }

        Ok(Response::new(RegistryPushResponse { digest }))
    }
}

async fn get_digist<T: std::marker::Unpin + tokio::io::AsyncRead>(mut archive: Archive<T>) -> Result<String>
{
    let mut hasher = Sha256::new();

    let mut entries = archive.entries()?;
    while let Some(file) = entries.next().await {
        let mut entry = file?;
        let path = entry
            .path()?
            .to_string_lossy()
            .to_string();

        if entry.header().entry_type().is_file() {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).await?;
            hasher.update(b"file:");        // prefix to differentiate files/folders
            hasher.update(path.as_bytes());
            hasher.update(&buf);
        } else if entry.header().entry_type().is_dir() {
            hasher.update(b"dir:");         // prefix for directories
            hasher.update(path.as_bytes());
        }
    }

    Ok(format!("{:x}", hasher.finalize()))
}
