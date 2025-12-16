use std::path::PathBuf;

use crate::{
    path::{copy_dir_all, get_instence_path},
    worker::spec::{get_spec, SysUserParms},
};
use anyhow::{Context, Result};
use libcontainer::{
    container::{Container, ContainerStatus, builder::ContainerBuilder},
    syscall::syscall::SyscallType,
};
use tokio::{
    fs::{DirBuilder, File},
    io::{AsyncWriteExt, BufWriter},
};
use url::Url;

// Trait for abstracting container operations - enables mocking
#[cfg_attr(test, mockall::automock)]
pub trait ContainerOps {
    fn build_container(
        &self,
        instance_id: String,
        root_path: PathBuf,
        rootfs: PathBuf,
    ) -> Result<Box<dyn ContainerWrapper>>;
    
    fn start_container(&self, container: &mut dyn ContainerWrapper) -> Result<()>;
    
    fn load_container(&self, path: PathBuf) -> Result<Box<dyn ContainerWrapper>>;
}

// Trait to wrap Container methods we need
#[cfg_attr(test, mockall::automock)]
pub trait ContainerWrapper: Send + Sync {
    fn bundle(&self) -> PathBuf;  // Changed from &Path to PathBuf
    fn status(&self) -> ContainerStatus;
    fn start(&mut self) -> Result<()>;
}

// Wrapper implementation for real Container
pub struct RealContainerWrapper(Container);

impl ContainerWrapper for RealContainerWrapper {
    fn bundle(&self) -> PathBuf {
        self.0.bundle().to_path_buf()  // Convert to owned PathBuf
    }
    
    fn status(&self) -> ContainerStatus {
        self.0.status()
    }
    
    fn start(&mut self) -> Result<()> {
        self.0.start()?;
        Ok(())
    }
}

// Real implementation using libcontainer
pub struct LibcontainerOps;

impl ContainerOps for LibcontainerOps {
    fn build_container(
        &self,
        instance_id: String,
        root_path: PathBuf,
        rootfs: PathBuf,
    ) -> Result<Box<dyn ContainerWrapper>> {
        let container = ContainerBuilder::new(instance_id, SyscallType::default())
            .with_root_path(root_path)
            .expect("invalid root path")
            .as_init(rootfs)
            .with_detach(true)
            .with_systemd(false)
            .build()?;
        Ok(Box::new(RealContainerWrapper(container)))
    }
    
    fn start_container(&self, container: &mut dyn ContainerWrapper) -> Result<()> {
        container.start()?;
        Ok(())
    }
    
    fn load_container(&self, path: PathBuf) -> Result<Box<dyn ContainerWrapper>> {
        let container = Container::load(path)?;
        Ok(Box::new(RealContainerWrapper(container)))
    }
}

pub struct ProccesContainer {
    container: Box<dyn ContainerWrapper>,
}

impl ProccesContainer {
    pub async fn new(
        digest: &str,
        handle_bin: PathBuf,
        root_path: PathBuf,
        sys_user: &SysUserParms,
    ) -> Result<Self> {
        Self::new_with_ops(
            digest,
            handle_bin,
            root_path,
            sys_user,
            &LibcontainerOps,
        ).await
    }

    // Internal constructor that accepts container operations - testable!
    async fn new_with_ops(
        digest: &str,
        handle_bin: PathBuf,
        root_path: PathBuf,
        sys_user: &SysUserParms,
        ops: &impl ContainerOps,
    ) -> Result<Self> {
        let instance_id = digest.to_string();

        let rootfs = Self::create_rootfs(&instance_id, handle_bin, sys_user).await?;
        
        let mut container = ops.build_container(
            instance_id.clone(),
            root_path,
            rootfs.clone(),
        )?;

        ops.start_container(container.as_mut())?;

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
            self.container.bundle().display()  // Now works with PathBuf
        );
        let url = Url::parse(&sock_path)?;
        Ok(url)
    }

    #[allow(dead_code)]
    pub async fn cleanup(&self) -> Result<()> {
        let path = self.container.bundle();  // Now PathBuf
        if path.exists() {
            tokio::fs::remove_dir_all(&path)
                .await
                .context("Failed to remove rootfs directory")?;
        }
        Ok(())
    }
    
    // Refactored to use ContainerOps trait
    fn try_from_with_ops(value: PathBuf, ops: &impl ContainerOps) -> Result<Self> {
        let mut container = ops.load_container(value)?;

        if container.status() != ContainerStatus::Running {
            ops.start_container(container.as_mut())?;
        }
        Ok(Self { container })
    }
}

impl TryFrom<PathBuf> for ProccesContainer {
    type Error = anyhow::Error;

    fn try_from(value: PathBuf) -> std::result::Result<Self, Self::Error> {
        Self::try_from_with_ops(value, &LibcontainerOps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    // ==================== Mocked Container Tests ====================

    #[tokio::test]
    async fn test_process_container_build_success() {
        let temp = TempDir::new().unwrap();
        let handle_bin = temp.path().join("bin");
        let root_path = temp.path().join("root");
        
        // Setup test filesystem
        fs::create_dir_all(&handle_bin).await.unwrap();
        fs::create_dir_all(&root_path).await.unwrap();
        fs::write(handle_bin.join("app"), b"#!/bin/sh\necho test").await.unwrap();

        // Create mock
        let mut mock_ops = MockContainerOps::new();
        
        // Set up expectations
        mock_ops
            .expect_build_container()
            .times(1)
            .returning(move |_instance_id, _root_path, _rootfs| {
                let mut mock = MockContainerWrapper::new();
                mock.expect_bundle()
                    .return_const(PathBuf::from("/tmp/test_bundle"));
                Ok(Box::new(mock))
            });
        
        mock_ops
            .expect_start_container()
            .times(1)
            .returning(|_| Ok(()));

        // Now we can actually test the full flow!
        let sys_user = SysUserParms { uid: 0, gid: 0 };
        let result = ProccesContainer::new_with_ops(
            "test_digest",
            handle_bin,
            root_path,
            &sys_user,
            &mock_ops,
        ).await;
        
        assert!(result.is_ok());
        let container = result.unwrap();
        
        // Test get_url works with mocked container
        let url_result = container.get_url();
        assert!(url_result.is_ok());
    }

    #[tokio::test]
    async fn test_container_get_url() {
        let mut mock_container = MockContainerWrapper::new();
        
        mock_container
            .expect_bundle()
            .return_const(PathBuf::from("/tmp/test_container"));
        
        let container = ProccesContainer {
            container: Box::new(mock_container),
        };
        
        let url = container.get_url().unwrap();
        assert_eq!(url.scheme(), "unix");
        assert!(url.path().contains("test_container"));
        assert!(url.path().contains("rootfs/run/app.sock"));
    }

    #[tokio::test]
    async fn test_container_ops_called_with_correct_params() {
        let temp = TempDir::new().unwrap();
        let handle_bin = temp.path().join("bin");
        let root_path = temp.path().join("root");
        
        fs::create_dir_all(&handle_bin).await.unwrap();
        fs::create_dir_all(&root_path).await.unwrap();
        fs::write(handle_bin.join("app"), b"test").await.unwrap();

        let mut mock_ops = MockContainerOps::new();
        
        let expected_instance = "test_digest_123".to_string();
        let expected_instance_clone = expected_instance.clone();
        
        mock_ops
            .expect_build_container()
            .withf(move |instance_id, _root, _rootfs| {
                instance_id == &expected_instance_clone
            })
            .times(1)
            .returning(|_, _, _| {
                let mut mock = MockContainerWrapper::new();
                mock.expect_bundle()
                    .return_const(PathBuf::from("/tmp/test"));
                Ok(Box::new(mock))
            });
        
        mock_ops
            .expect_start_container()
            .times(1)
            .returning(|_| Ok(()));

        let sys_user = SysUserParms { uid: 0, gid: 0 };
        let result = ProccesContainer::new_with_ops(
            "test_digest_123",
            handle_bin,
            root_path,
            &sys_user,
            &mock_ops,
        ).await;
        
        // Now it should succeed with mocks
        assert!(result.is_ok());
    }

    #[test]
    fn test_try_from_with_running_container() {
        let temp = TempDir::new().unwrap();
        let container_path = temp.path().join("container");
        
        let mut mock_ops = MockContainerOps::new();
        
        // Mock a container that's already running
        mock_ops
            .expect_load_container()
            .times(1)
            .returning(|_| {
                let mut mock = MockContainerWrapper::new();
                mock.expect_status()
                    .return_const(ContainerStatus::Running);
                mock.expect_bundle()
                    .return_const(PathBuf::from("/tmp/running"));
                Ok(Box::new(mock))
            });
        
        // Should NOT call start_container for running container
        mock_ops
            .expect_start_container()
            .times(0);

        let result = ProccesContainer::try_from_with_ops(
            container_path,
            &mock_ops,
        );
        
        assert!(result.is_ok());
    }

    #[test]
    fn test_try_from_with_stopped_container() {
        let temp = TempDir::new().unwrap();
        let container_path = temp.path().join("container");
        
        let mut mock_ops = MockContainerOps::new();
        
        // Mock a stopped container
        mock_ops
            .expect_load_container()
            .times(1)
            .returning(|_| {
                let mut mock = MockContainerWrapper::new();
                mock.expect_status()
                    .return_const(ContainerStatus::Stopped);
                mock.expect_bundle()
                    .return_const(PathBuf::from("/tmp/stopped"));
                Ok(Box::new(mock))
            });
        
        // SHOULD call start_container for stopped container
        mock_ops
            .expect_start_container()
            .times(1)
            .returning(|_| Ok(()));

        let result = ProccesContainer::try_from_with_ops(
            container_path,
            &mock_ops,
        );
        
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_cleanup_removes_directory() {
        let temp = TempDir::new().unwrap();
        let bundle_path = temp.path().join("test_bundle");
        fs::create_dir_all(&bundle_path).await.unwrap();
        
        let bundle_clone = bundle_path.clone();
        let mut mock_container = MockContainerWrapper::new();
        mock_container
            .expect_bundle()
            .return_const(bundle_clone);
        
        let container = ProccesContainer {
            container: Box::new(mock_container),
        };
        
        let result = container.cleanup().await;
        assert!(result.is_ok());
        assert!(!bundle_path.exists());
    }
}
