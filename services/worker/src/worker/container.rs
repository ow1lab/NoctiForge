use std::path::PathBuf;

use anyhow::Result;
use libcontainer::{container::{builder::ContainerBuilder, Container}, syscall::syscall::SyscallType};
use uuid::Uuid;

pub struct ProccesContainer {
    container: Container
}

impl ProccesContainer {
    pub fn new(handle_bin: PathBuf) -> Result<Self> {
        let instance_id = Uuid::new_v4().to_owned();

        let container = ContainerBuilder::new(
            instance_id.to_string(),
            SyscallType::default()
        )
        .with_root_path(format!("/run/noctiforge/youki/{}", instance_id))
            .expect("invalid root path")
        .with_pid_file(Some(format!("/var/run/noctiforge/{}.pid", instance_id)))
            .expect("invalid pid file")
        .as_init(handle_bin)
        .build()?;
            
        Ok(Self { container })
    }

    pub fn start(&mut self) -> Result<()> {
        todo!()
    }
}
