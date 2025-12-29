use std::{net::SocketAddr, time::Duration};

use crate::background::BackgroundConfig;

#[derive(Debug, PartialEq)]
pub enum Environment {
    Development,
    Production,
}

pub struct ServerConfig {
    pub addr: SocketAddr,
    pub controlplane_clinet: String,
    pub registry_clinet: String,
    pub env: Environment,
    pub background_config: BackgroundConfig,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let addr = std::env::var("SERVER_ADDR")
            .unwrap_or_else(|_| "[::1]:50003".to_string())
            .parse()
            .expect("Invalid server address");

        let env = match cfg!(debug_assertions) {
            true => Environment::Development,
            false => Environment::Production,
        };

        let controlplane_clinet = std::env::var("CONTROLPLANE_CLINET")
            .unwrap_or_else(|_| "http://localhost:50002".to_string())
            .parse()
            .expect("Invalid controlplane address");

        let registry_clinet = std::env::var("REGISTRY_CLINET")
            .unwrap_or_else(|_| "http://localhost:50001".to_string())
            .parse()
            .expect("Invalid registry address");


        let time = std::env::var("BACGROUND_TIME")
            .map_err(|_| "Missing BACGROUND_TIME")
            .and_then(|s| s.parse::<u64>().map_err(|_| "Invalid BACGROUND_TIME"))
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(10));

        let resource_ttl = std::env::var("BACKGROUND_RESOURCE_TTL")
            .map_err(|_| "Missing BACKGROUND_RESOURCE_TTL")
            .and_then(|s| s.parse::<u64>().map_err(|_| "Invalid BACKGROUND_RESOURCE_TTL"))
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(30));

        Self {
            addr,
            controlplane_clinet,
            registry_clinet,
            env,
            background_config: BackgroundConfig {
                time,
                resource_ttl
            }
        }
    }
}
