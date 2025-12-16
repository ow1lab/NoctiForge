use std::path::PathBuf;

use crate::{
    path::get_instence_path,
    worker::spec::{SysUserParms, get_spec},
};
use anyhow::{Context, Ok, Result};
use libcontainer::{
    container::{Container, ContainerStatus, builder::ContainerBuilder},
    syscall::syscall::SyscallType,
};
use tokio::{
    fs::{self, DirBuilder, File},
    io::{AsyncWriteExt, BufWriter},
};
use url::Url;

pub struct ProccesContainer {
    container: Container,
}

impl ProccesContainer {
    pub async fn new(
        digest: &str,
        handle_bin: PathBuf,
        root_path: PathBuf,
        sys_user: &SysUserParms,
    ) -> Result<Self> {
        // TODO: Support more creation of more func of the same type
        let instance_id = digest.to_string();

        let rootfs = Self::create_rootfs(&instance_id, handle_bin, sys_user).await?;
        let mut container = ContainerBuilder::new(instance_id.clone(), SyscallType::default())
            .with_root_path(root_path)
            .expect("invalid root path")
            .as_init(rootfs.clone())
            .with_detach(true)
            .with_systemd(false)
            .build()?;

        container.start()?;

        Ok(Self { container })
    }

    async fn create_rootfs(
        instance_id: &str,
        handle_bin: PathBuf,
        sys_user: &SysUserParms,
    ) -> Result<PathBuf> {
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

    pub fn get_url(&self) -> Result<Url> {
        let sock_path = format!(
            "unix://{}/rootfs/run/app.sock",
            self.container.bundle().to_string_lossy()
        );
        let url = Url::parse(&sock_path)?;
        Ok(url)
    }

    #[allow(dead_code)]
    pub async fn cleanup(&self) -> Result<()> {
        let path = self.container.bundle().as_path();
        if path.exists() {
            tokio::fs::remove_dir_all(&path)
                .await
                .context("Failed to remove rootfs directory")?;
        }
        Ok(())
    }
}

impl TryFrom<PathBuf> for ProccesContainer {
    type Error = anyhow::Error;

    fn try_from(value: PathBuf) -> std::result::Result<Self, Self::Error> {
        let mut container = Container::load(value)?;

        if container.status() != ContainerStatus::Running {
            container.start()?
        }
        Ok(Self { container })
    }
}

pub async fn copy_dir_all(src: PathBuf, dst: PathBuf) -> Result<()> {
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
