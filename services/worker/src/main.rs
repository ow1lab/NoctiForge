mod server;
mod config;
mod worker;

use std::sync::Arc;

use proto::api::worker::worker_service_server::WorkerServiceServer;
use tonic::transport::Server;

use crate::server::WorkerServer;
use crate::worker::docker::DockerWorker;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = config::ServerConfig::from_env();

    let function_worker = Arc::new(DockerWorker::new()?);
    let worker_server = WorkerServer::new(function_worker);

    println!("ControlPlaneService listening on {}", config.addr);

    Server::builder()
        .add_service(WorkerServiceServer::new(worker_server))
        .serve(config.addr)
        .await?;

    Ok(())
}
