use std::{io::Cursor, pin::Pin};

use proto::api::registry::{
    RegistryPullRequest, RegistryPullResponse, RegistryPushRequest, RegistryPushResponse,
    registry_service_server::RegistryService,
};
use sha2::{Digest, Sha256};
use tokio::{
    fs::{read, write},
    io::AsyncReadExt,
};
use tokio_stream::{Stream, StreamExt};
use tokio_tar::Archive;
use tonic::{Request, Response, Result, Status, Streaming};
use tracing::{debug, error, info, instrument, warn};

use crate::path::get_registry_path;

const CHUNK_SIZE: usize = 64 * 1024;

#[derive(Default)]
pub struct LocalBackend {}

#[tonic::async_trait]
impl RegistryService for LocalBackend {
    type PullStream =
        Pin<Box<dyn Stream<Item = Result<RegistryPullResponse, Status>> + Send + 'static>>;

    #[instrument(
        name = "Registry pull",
        skip(self, request),
        fields(digest = %request.get_ref().digest)
    )]
    async fn pull(
        &self,
        request: Request<RegistryPullRequest>,
    ) -> Result<Response<Self::PullStream>, Status> {
        let req = request.into_inner();
        let request_path = get_registry_path(&req.digest);

        debug!(path = %request_path.display(), "Reading tar from registry path");

        let data = read(&request_path)
            .await
            .map_err(|err| {
                error!(
                    path = %request_path.display(),
                    error = %err,
                    "Failed to read from registry path"
                );
                Status::internal(format!("failed to read store path: {:?}", err))
            })?;
        let data_size = data.len();
        let chunk_count = (data_size + CHUNK_SIZE - 1) / CHUNK_SIZE;

        info!(
            digest = %req.digest,
            size_bytes = data_size,
            chunk_count = chunk_count,
            chunk_size = CHUNK_SIZE,
            "Successfully read tar, streaming chunks"
        );

        let stream = tokio_stream::iter(
            data.chunks(CHUNK_SIZE)
                .map(|chunk| {
                    Ok(RegistryPullResponse {
                        data: chunk.to_vec(),
                    })
                })
                .collect::<Vec<_>>(),
        );

        Ok(Response::new(Box::pin(stream)))
    }

    #[instrument(
        name = "Registry push",
        skip(self, request)
    )]
    async fn push(
        &self,
        request: Request<Streaming<RegistryPushRequest>>,
    ) -> Result<Response<RegistryPushResponse>, Status> {
        debug!("Starting to receive push stream");

        let mut request_data: Vec<u8> = vec![];
        let mut request_stream = request.into_inner();
        let mut chunk_count = 0;

        while let Some(request) = request_stream.next().await {
            let request = request.map_err(|err| {
                error!(error = %err, "Failed to receive stream chunk");
                Status::internal(err.to_string())
            })?;
            
            chunk_count += 1;
            request_data.extend_from_slice(&request.data);
            
            if chunk_count % 10 == 0 {
                debug!(
                    chunks_received = chunk_count,
                    total_bytes = request_data.len(),
                    "Receiving data..."
                );
            }
        }

        info!(
            total_chunks = chunk_count,
            total_bytes = request_data.len(),
            "Completed receiving all chunks"
        );

        if request_data.is_empty() {
            warn!("Received empty data");
            return Err(Status::invalid_argument("missing `data` field"));
        }

        debug!("Validating tar archive");
        let cursor = Cursor::new(&request_data);
        let mut archive = Archive::new(cursor);
        if let Err(err) = archive.entries() {
            error!(error = %err, "Invalid tar archive received");
            return Err(Status::invalid_argument(format!(
                "invalid tar archive: {}",
                err
            )));
        }

        debug!("Computing digest");
        let digest = get_digest(archive).await.map_err(|err| {
            error!(error = %err, "Failed to compute digest");
            err
        })?;

        info!(digest = %digest, "Computed digest successfully");

        let request_path = get_registry_path(&digest);

        if request_path.exists() {
            info!(
                digest = %digest,
                path = %request_path.display(),
                "Digest already exists in registry, skipping write"
            );
        } else {
            debug!(
                digest = %digest,
                path = %request_path.display(),
                size_bytes = request_data.len(),
                "Writing tar to registry"
            );
            
            write(&request_path, &request_data).await.map_err(|err| {
                error!(
                    path = %request_path.display(),
                    error = %err,
                    "Failed to write to registry path"
                );
                Status::internal(format!("failed to write store path: {:?}", err))
            })?;
            
            info!(
                digest = %digest,
                path = %request_path.display(),
                "Successfully written to registry"
            );
        }

        Ok(Response::new(RegistryPushResponse { digest }))
    }
}

async fn get_digest<T: std::marker::Unpin + tokio::io::AsyncRead>(
    mut archive: Archive<T>,
) -> Result<String> {
    let mut hasher = Sha256::new();

    let mut entries = archive.entries()?;
    while let Some(file) = entries.next().await {
        let mut entry = file?;
        let path = entry.path()?.to_string_lossy().to_string();

        if entry.header().entry_type().is_file() {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).await?;
            hasher.update(b"file:"); // prefix to differentiate files/folders
            hasher.update(path.as_bytes());
            hasher.update(&buf);
        } else if entry.header().entry_type().is_dir() {
            hasher.update(b"dir:"); // prefix for directories
            hasher.update(path.as_bytes());
        }
    }

    Ok(format!("{:x}", hasher.finalize()))
}
