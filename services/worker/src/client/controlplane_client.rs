use anyhow::{Ok, Result};
use proto::api::controlplane::{control_plane_service_client::ControlPlaneServiceClient, GetDigestByNameRequest};
use tonic::Request;

pub struct ControlPlaneClient {
    pub addr: String 
}

impl ControlPlaneClient {
    pub fn new(addr: String) -> Self {
        Self { addr }
    }
}

impl ControlPlaneClient {
    pub async fn get_digest(&self, key: String) -> Result<String> {
        let mut client = ControlPlaneServiceClient::connect(self.addr.clone()).await?;
        let response = client.get_digest_by_name(Request::new(GetDigestByNameRequest { key } )).await?.into_inner();
        Ok(response.digest)
    }
}

