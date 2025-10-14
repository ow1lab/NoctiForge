pub mod api {
    pub mod action {
        tonic::include_proto!("noctiforge.action");
    }
    pub mod registry {
        tonic::include_proto!("noctiforge.registry");
    }
    pub mod controlplane {
        tonic::include_proto!("noctiforge.controlplane");
    }
    pub mod worker {
        tonic::include_proto!("noctiforge.worker");
    }
}
