use proto::api::registry::registry_service_server::RegistryServiceServer;
use tonic::transport::Server;
use tracing::info;

mod path;
mod registry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_target(false)
        .init();

    let addr = "[::1]:50001".parse().unwrap();
    let file_engine = registry::LocalBackend::default();

    info!("RegistryServiceServer listening on {}", addr);

    Server::builder()
        .add_service(RegistryServiceServer::new(file_engine))
        .serve(addr)
        .await?;

    Ok(())
}
