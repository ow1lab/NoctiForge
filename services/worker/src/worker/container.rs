use std::path::PathBuf;

use anyhow::{Context, Result};
use libcontainer::{
    container::{builder::ContainerBuilder, Container},
    syscall::{syscall::SyscallType},
};
use tokio::{fs::{self, DirBuilder, File}, io::{self, AsyncWriteExt, BufWriter}};
use url::Url;
use uuid::Uuid;
use crate::{path::get_instence_path, worker::spec::{get_spec, SysUserParms}};


pub struct ProccesContainer {
    container: Container,
    instance_id: String,
    sock_path: Url,
}

impl ProccesContainer {
    pub async fn new(root_path: PathBuf, handle_bin: PathBuf, sys_user: &SysUserParms) -> Result<Self> {
        _ = handle_bin;

        let instance_id = Uuid::new_v4().to_owned().to_string();
        let rootfs = Self::create_rootfs(&instance_id, handle_bin, sys_user).await?;

        let container = ContainerBuilder::new(instance_id.clone(), SyscallType::default())
            .with_root_path(root_path).expect("invalid root path")
            .as_init(rootfs.clone())
            .with_systemd(false)
            .build()?;

        // TODO: make this inte instance path
        let sock_path = format!("unix://{}/rootfs/run/app.sock", rootfs.to_string_lossy());
        let url = Url::parse(&sock_path)?;

        Ok(Self {
            container,
            instance_id,
            sock_path: url,
        })
    }

    async fn create_rootfs(instance_id: &str, handle_bin: PathBuf, sys_user: &SysUserParms) -> Result<PathBuf> {
        let path = PathBuf::from(get_instence_path(instance_id));

        if path.exists() {
            anyhow::bail!("Root filesystem path already exists: {}", path.display());
        }

        DirBuilder::new().create(&path).await?;

        let spec = get_spec(sys_user)?;

        // Create Spec
        let file = File::create(path.join("config.json")).await?;
        let mut writer = BufWriter::new(file);
        let json_bytes = serde_json::to_vec_pretty(&spec)?;
        writer.write_all(&json_bytes).await?;
        writer.flush().await?;

        let rootfs_path = path.join("rootfs");
        DirBuilder::new().create(&rootfs_path).await?;

        copy_dir_all(handle_bin, rootfs_path.join("app")).await?;

        DirBuilder::new().create(&rootfs_path.join("run")).await?;

        Ok(path)
    }

    pub fn start(&mut self) -> Result<()> {
        self.container.start().with_context(|| "failed to start container")?;
        Ok(())
    }

    pub fn get_url(&self) -> Url {
        self.sock_path.clone()
    }

    #[allow(dead_code)]
    pub async fn cleanup(&self) -> Result<()> {
        let path = PathBuf::from(get_instence_path(&self.instance_id));
        if path.exists() {
            tokio::fs::remove_dir_all(&path)
                .await
                .context("Failed to remove rootfs directory")?;
        }
        Ok(())
    }
}

pub async fn copy_dir_all(src: PathBuf, dst: PathBuf) -> io::Result<()> {
    let mut stack = vec![(src, dst)];

    while let Some((src, dst)) = stack.pop() {
        fs::create_dir_all(&dst).await?;

        let mut entries = fs::read_dir(&src).await?;

        while let Some(entry) = entries.next_entry().await? {
            let ft = entry.file_type().await?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if ft.is_dir() {
                stack.push((src_path, dst_path));
            } else {
                fs::copy(src_path, dst_path).await?;
            }
        }
    }

    Ok(())
}

