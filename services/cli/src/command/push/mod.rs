use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use custom::CustomBuild;
use proto::api::{controlplane::{control_plane_service_client::ControlPlaneServiceClient, SetDigestToNameRequest}, registry::{self, RegistryPushRequest}};
use rust::RustBuild;
use serde::Deserialize;
use tokio::io::{duplex, AsyncReadExt};
use tokio_tar::Builder;
use tonic::{async_trait, Request};
use registry::registry_service_client::RegistryServiceClient;

mod custom;
mod rust;

const CONFIG_FILE: &str = "Nocti.toml";

#[async_trait]
trait BuildService {
    async fn build(&self, project_path: PathBuf) -> Result<String>;
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Project {
    name: String,
    version: String,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct Config {
    project: Project,
    build: Build,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Build {
    #[serde(rename = "custom")]
    Custom(CustomBuild),
    #[serde(rename = "rust")]
    Rust(RustBuild),
}


pub async fn run(path: &str) -> Result<()> {
    let project_path = Path::new(path);
    if !project_path.is_dir() || !project_path.exists() {
        bail!("'path' does not exist or its a not folder");
    }

    let config_file_path = project_path.join(CONFIG_FILE);
    if !config_file_path.is_file() || !config_file_path.exists() {
        bail!("'{}' does not exist or its a folder", CONFIG_FILE);
    }

    let config_content = std::fs::read_to_string(config_file_path)?;
    let config: Config = toml::from_str(&config_content)?; 

    // Run the scripts
    let buildservice: Box<dyn BuildService + Send + Sync> = match config.build {
        Build::Custom( cb ) => Box::new(cb),
        Build::Rust( rb ) => Box::new(rb),
    };

    let path = buildservice.build(project_path.to_path_buf()).await?;
    let bin_folder = project_path.join(path);

    println!("bin_folder: {:?}", bin_folder);
    let (writer, mut reader) = duplex(8 * 1024);

    tokio::spawn(async move {
        let mut builder = Builder::new(writer);
        // Use `.await` since tokio_tar is async
        if let Err(e) = builder.append_dir_all(".", bin_folder).await {
            eprintln!("tar append_dir_all error: {}", e);
            return;
        }
        if let Err(e) = builder.finish().await {
            eprintln!("tar finish error: {}", e);
        }
        // When this task ends, `writer` is dropped, which causes EOF on reader side
    });

    // Create a stream of RegistryPushRequest from reader
    let outbound = async_stream::stream! {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => {
                    // EOF
                    break;
                }
                Ok(n) => {
                    // yield a chunk
                    let req = RegistryPushRequest {
                        data: buf[..n].to_vec(),
                    };
                    yield req;
                }
                Err(e) => {
                    eprintln!("Error reading from pipe: {}", e);
                    break;
                }
            }
        }
    };

    let mut client = RegistryServiceClient::connect("http://localhost:50001").await?;
    let response = client.push(Request::new(outbound)).await?.into_inner();

    let key = config.project.name;

    let mut client = ControlPlaneServiceClient::connect("http://localhost:50002").await?;
    let request = SetDigestToNameRequest { key: key.clone(), digest: response.digest };
    let response = client.set_digest_to_name(Request::new(request)).await?.into_inner();

    match response.success {
        true => Ok({}),
        false => bail!("Something happend when setting digiest to key")
    }
}
