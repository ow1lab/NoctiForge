pub mod api {
    pub mod action {
        tonic::include_proto!("noctifunc.action");
    }
    pub mod registry {
        tonic::include_proto!("noctifunc.registry");
    }
}
