use std::path::Path;

use anyhow::{bail, Result};
use custom::CustomBuild;
use rust::RustBuild;
use serde::Deserialize;

mod custom;
mod rust;

const CONFIG_FILE: &str = "Nocti.toml";

trait BuildService {
    fn build(&self) -> Result<String>;
}

#[derive(Debug, Deserialize)]
struct Project {
    name: String,
    version: String,
}

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
    let buildservice: Box<dyn BuildService> = match config.build {
        Build::Custom( cb ) => Box::new(cb),
        Build::Rust( rb ) => Box::new(rb),
    };

    let path = buildservice.build()?;

    // zip It
    let zip_builder = async_zip::ZipFileBuilder::new();


    // push to registry
    Ok(())
}
