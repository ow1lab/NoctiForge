use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use custom::CustomBuild;
use proto::api::{
    controlplane::{
        SetDigestToNameRequest, control_plane_service_client::ControlPlaneServiceClient,
    },
    registry::{self, RegistryPushRequest},
};
use registry::registry_service_client::RegistryServiceClient;
use rust::RustBuild;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, duplex};
use tonic::{Request, async_trait};
use tracing::{debug, error, info};

mod custom;
mod rust;

const CONFIG_FILE: &str = "Nocti.toml";

#[async_trait]
trait BuildService {
    async fn build(&self, project_path: PathBuf, temp_path: PathBuf) -> anyhow::Result<()>;
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Project {
    name: String,
    version: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Config {
    project: Project,
    build: Build,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
enum Build {
    #[serde(rename = "custom")]
    Custom(CustomBuild),
    #[serde(rename = "rust")]
    Rust(RustBuild),
}

pub async fn run(path: &str) -> Result<()> {
    let project_path = Path::new(path);
    info!("Running push command on path: {:?}", project_path);

    if !project_path.is_dir() || !project_path.exists() {
        error!("Provided path is invalid: {:?}", project_path);
        bail!("'path' does not exist or its a not folder");
    }

    let config_file_path = project_path.join(CONFIG_FILE);
    if !config_file_path.is_file() || !config_file_path.exists() {
        error!("Missing config file at: {:?}", config_file_path);
        bail!("'{}' does not exist or its a folder", CONFIG_FILE);
    }

    info!("Loading project config from: {:?}", config_file_path);
    let config_content = std::fs::read_to_string(config_file_path)?;
    let config: Config = toml::from_str(&config_content)?;
    debug!("Parsed config: {:?}", config);

    // Run the scripts
    let buildservice: Box<dyn BuildService + Send + Sync> = match config.build {
        Build::Custom(cb) => {
            debug!("Using custom build");
            Box::new(cb)
        }
        Build::Rust(rb) => {
            debug!("Using Rust build");
            Box::new(rb)
        }
    };

    debug!("Creating a temporary directory");
    let temp_dir = tempfile::Builder::new()
        .prefix("nocti-build")
        .tempdir()?;

    let temp_path = temp_dir.path().to_path_buf();

    info!("Starting build...");
    buildservice.build(project_path.to_path_buf(), temp_path).await?;
    info!("Build complete");

    let (writer, mut reader) = duplex(8 * 1024);
    info!("Creating in-memory tar archive...");

    tokio::spawn(async move {
        let temp_path = temp_dir.path();

        let mut builder = tokio_tar::Builder::new(writer);
        if let Err(e) = builder.append_dir_all(".", temp_path).await {
            error!("tar append_dir_all error: {}", e);
            return;
        }
        if let Err(e) = builder.finish().await {
            error!("tar finish error: {}", e);
        }
        debug!("Tarball creation task completed");
    });

    // Create a stream of RegistryPushRequest from reader
    let outbound = async_stream::stream! {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => {
                    debug!("Finished reading all tar data");
                    break;
                }
                Ok(n) => {
                    debug!("Read {} bytes from tar stream", n);
                    let req = RegistryPushRequest {
                        data: buf[..n].to_vec(),
                    };
                    yield req;
                }
                Err(e) => {
                    error!("Error reading from pipe: {}", e);
                    break;
                }
            }
        }
    };

    info!("Connecting to RegistryService...");
    let mut client = RegistryServiceClient::connect("http://localhost:50001").await?;
    info!("Sending tar data to registry...");
    let response = client.push(Request::new(outbound)).await?.into_inner();
    debug!("Registry responded with digest: {}", response.digest);

    let key = config.project.name;
    info!("Associating digest with project key: {}", key);

    let mut client = ControlPlaneServiceClient::connect("http://localhost:50002").await?;
    let request = SetDigestToNameRequest {
        key: key.clone(),
        digest: response.digest,
    };

    let response = client
        .set_digest_to_name(Request::new(request))
        .await?
        .into_inner();
    if response.success {
        info!("Successfully set digest for key '{}'", key);
        Ok(())
    } else {
        error!("Failed to associate digest with key '{}'", key);
        bail!("Something happened when setting digest to key")
    }
}
