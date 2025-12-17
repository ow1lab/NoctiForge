use std::path::PathBuf;

use crate::{
    path::copy_dir_all,
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
    fn bundle(&self) -> PathBuf;
    fn status(&self) -> ContainerStatus;
    fn start(&mut self) -> Result<()>;
}

// Wrapper implementation for real Container
pub struct RealContainerWrapper(Container);

impl ContainerWrapper for RealContainerWrapper {
    fn bundle(&self) -> PathBuf {
        self.0.bundle().to_path_buf()
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
        Self::new_with_deps(
            digest,
            handle_bin,
            root_path,
            sys_user,
            &LibcontainerOps,
        ).await
    }


    pub async fn load(root_path: &PathBuf, instance_id: &str) -> Result<Self> {
        Self::load_with_deps(root_path, instance_id, &LibcontainerOps).await
    }

    async fn load_with_deps(root_path: &PathBuf, instance_id: &str, ops: &impl ContainerOps) -> Result<Self> {
        let mut container = ops.load_container(root_path.join("state").join(instance_id))?;

        if container.status() != ContainerStatus::Running {
            ops.start_container(container.as_mut())?;
        }
        Ok(Self { container })
    }

    async fn new_with_deps(
        digest: &str,
        handle_bin: PathBuf,
        root_path: PathBuf,
        sys_user: &SysUserParms,
        ops: &impl ContainerOps,
    ) -> Result<Self> {
        let instance_id = digest.to_string();

        let rootfs = Self::create_rootfs(
            &instance_id,
            handle_bin,
            sys_user,
            root_path.join("run")
        ).await?;
        
        let mut container = ops.build_container(
            instance_id.clone(),
            root_path.join("state"),
            rootfs.clone(),
        )?;

        ops.start_container(container.as_mut())?;

        Ok(Self { container })
    }

    async fn create_rootfs(
        instance_id: &str,
        handle_bin: PathBuf,
        sys_user: &SysUserParms,
        run_path: PathBuf,
    ) -> Result<PathBuf> {
        let path = run_path.join(instance_id);

        if path.exists() {
            anyhow::bail!("Root filesystem path already exists: {}", path.display());
        }

        // TODO: need to look at this and see if we should create the folder a head of time?
        DirBuilder::new().recursive(true).create(&path).await?;

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
            self.container.bundle().display()
        );
        let url = Url::parse(&sock_path)?;
        Ok(url)
    }

    pub fn exist(root_path: &PathBuf, instance_id: &str) -> bool {
        root_path.join("state").join(instance_id).exists()
    }

    #[allow(dead_code)]
    pub async fn cleanup(&self) -> Result<()> {
        let path = self.container.bundle();
        if path.exists() {
            tokio::fs::remove_dir_all(&path)
                .await
                .context("Failed to remove rootfs directory")?;
        }
        Ok(())
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

        // Create mocks
        let mut mock_ops = MockContainerOps::new();
        
        // Set up container expectations
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
        let result = ProccesContainer::new_with_deps(
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
        
        // Mock path resolver
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
        let result = ProccesContainer::new_with_deps(
            "test_digest_123",
            handle_bin,
            root_path,
            &sys_user,
            &mock_ops,
        ).await;

        assert!(
            result.is_ok(),
            "ProcessContainer failed with error: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_create_rootfs_fails_if_path_exists() {
        let temp = TempDir::new().unwrap();
        let handle_bin = temp.path().join("bin");
        let root_path = temp.path().join("root");
        let instance_path = root_path.join("run").join("test");
        
        // Create the instance path beforehand
        fs::create_dir_all(&instance_path).await.unwrap();
        fs::create_dir_all(&handle_bin).await.unwrap();
        fs::create_dir_all(&root_path).await.unwrap();
        fs::write(handle_bin.join("app"), b"test").await.unwrap();

        let mut mock_ops = MockContainerOps::new();
        
        // Container ops should NOT be called since we fail early
        mock_ops
            .expect_build_container()
            .times(0);

        let sys_user = SysUserParms { uid: 0, gid: 0 };
        let result = ProccesContainer::new_with_deps(
            "test",
            handle_bin,
            root_path,
            &sys_user,
            &mock_ops,
        ).await;
        
        // Should fail with "already exists" error
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_try_from_with_running_container() {
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

        let result = ProccesContainer::load_with_deps(
            &container_path,
            "container",
            &mock_ops,
        ).await;
        
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_try_from_with_stopped_container() {
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

        let result = ProccesContainer::load_with_deps(
            &container_path,
            "contaienr",
            &mock_ops,
        ).await;
        
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
