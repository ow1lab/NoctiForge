use std::net::SocketAddr;

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
        Self {
            addr,
            controlplane_clinet,
            registry_clinet,
            env,
        }
    }
}
