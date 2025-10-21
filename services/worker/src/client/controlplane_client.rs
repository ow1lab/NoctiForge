use anyhow::{Ok, Result};
use proto::api::controlplane::{
    GetDigestByNameRequest, control_plane_service_client::ControlPlaneServiceClient,
};
use tonic::Request;
use tracing::{debug, instrument, warn};

pub struct ControlPlaneClient {
    pub addr: String,
}

impl ControlPlaneClient {
    pub fn new(addr: String) -> Self {
        debug!(addr = %addr, "Creating ControlPlaneClient");
        Self { addr }
    }
}

impl ControlPlaneClient {
    #[instrument(skip(self), fields(addr = %self.addr))]
    pub async fn get_digest(&self, key: String) -> Result<String> {
        debug!(key = %key, "Fetching digest from control plane");
        let mut client = ControlPlaneServiceClient::connect(self.addr.clone())
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to connect to control plane");
                e
            })?;        

        let response = client
            .get_digest_by_name(Request::new(GetDigestByNameRequest { key: key.clone() }))
            .await
            .map_err(|e| {
                warn!(key = %key, error = %e, "Failed to get digest by name");
                e
            })?
            .into_inner();

        debug!(key = %key, digest = %response.digest, "Successfully retrieved digest");
        
        Ok(response.digest)
    }
}
