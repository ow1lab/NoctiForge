mod client;
mod config;
mod path;
mod server;
mod worker;

use proto::api::worker::worker_service_server::WorkerServiceServer;
use tonic::transport::Server;
use tracing::{error, info};

use crate::client::controlplane_client::ControlPlaneClient;
use crate::client::registry_clint::RegistryClient;
use crate::config::Environment;
use crate::server::WorkerServer;
use crate::worker::{Config, NativeWorker};
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let _span = tracing::info_span!(
        "app",
        name = "worker",
        version = env!("CARGO_PKG_VERSION")
    ).entered();

    info!("Starting application");

    let config = config::ServerConfig::from_env();
    if config.env == config::Environment::Development {
        info!("Starting in Development mode");
    }

    let registry_clinet = RegistryClient::new(config.registry_clinet);
    let controlplane_client = ControlPlaneClient::new(config.controlplane_clinet);
    let function_worker = NativeWorker::new(
        registry_clinet,
        Config {
            is_dev: config.env == Environment::Development,
        },
    )?;

    let worker_server = WorkerServer::new(function_worker, controlplane_client);

    info!("Worker listening on {}", config.addr);

    Server::builder()
        .add_service(WorkerServiceServer::new(worker_server))
        .serve(config.addr)
        .await
        .map_err(|e| {
            error!("Server error: {}", e);
            e
        })?;

    info!("Server shut down gracefully");
    Ok(())
}
