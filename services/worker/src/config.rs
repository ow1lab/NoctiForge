use std::net::SocketAddr;

pub struct ServerConfig {
    pub addr: SocketAddr
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let addr = std::env::var("SERVER_ADDR")
            .unwrap_or_else(|_| "[::1]:50003".to_string())
            .parse()
            .expect("Invalid server address");
        Self { addr }
    }
}

