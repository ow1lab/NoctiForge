use anyhow::{Ok, Result};
use std::path::{Path, PathBuf};
use tokio::fs;

pub fn get_dir_path(digest: &str) -> PathBuf {
    get_pkgs_dir().join(digest)
}

pub fn get_instence_path(instance_id: &str) -> PathBuf {
    get_run_dir().join(instance_id)
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

fn get_run_dir() -> PathBuf {
    get_root_dir_path().join("run")
}

fn get_root_dir_path() -> PathBuf {
    Path::new("/var/lib/noctiforge/native_worker").to_path_buf()
}

fn get_pkgs_dir() -> PathBuf {
    get_root_dir_path().join("pkgs")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    // ==================== Path Functions Tests ====================

    #[test]
    fn test_get_root_dir_path() {
        let root = get_root_dir_path();
        assert_eq!(root, PathBuf::from("/var/lib/noctiforge/native_worker"));
    }

    #[test]
    fn test_get_run_dir() {
        let run_dir = get_run_dir();
        assert_eq!(run_dir, PathBuf::from("/var/lib/noctiforge/native_worker/run"));
    }

    #[test]
    fn test_get_pkgs_dir() {
        let pkgs_dir = get_pkgs_dir();
        assert_eq!(pkgs_dir, PathBuf::from("/var/lib/noctiforge/native_worker/pkgs"));
    }

    #[test]
    fn test_get_instence_path() {
        let instance_path = get_instence_path("test_instance");
        assert_eq!(
            instance_path,
            PathBuf::from("/var/lib/noctiforge/native_worker/run/test_instance")
        );
    }

    #[test]
    fn test_get_instence_path_with_special_chars() {
        let instance_path = get_instence_path("abc-123_xyz");
        assert!(instance_path.to_string_lossy().contains("abc-123_xyz"));
    }

    #[test]
    fn test_get_dir_path() {
        let dir_path = get_dir_path("digest123");
        assert_eq!(
            dir_path,
            PathBuf::from("/var/lib/noctiforge/native_worker/pkgs/digest123")
        );
    }

    #[test]
    fn test_get_dir_path_with_hash() {
        let digest = "sha256:abc123def456";
        let dir_path = get_dir_path(digest);
        assert!(dir_path.to_string_lossy().contains("sha256:abc123def456"));
    }

    // ==================== copy_dir_all Tests ====================

    #[tokio::test]
    async fn test_copy_dir_all_simple() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        // Create source directory with a file
        fs::create_dir_all(&src).await.unwrap();
        fs::write(src.join("test.txt"), b"hello").await.unwrap();

        // Copy
        let result = copy_dir_all(src.clone(), dst.clone()).await;
        assert!(result.is_ok());

        // Verify destination exists
        assert!(dst.exists());
        assert!(dst.join("test.txt").exists());
        
        // Verify content
        let content = fs::read_to_string(dst.join("test.txt")).await.unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn test_copy_dir_all_nested() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        
        // Create nested structure
        fs::create_dir_all(src.join("subdir")).await.unwrap();
        fs::write(src.join("file1.txt"), b"content1").await.unwrap();
        fs::write(src.join("file2.txt"), b"content2").await.unwrap();
        fs::write(src.join("subdir/file3.txt"), b"content3").await.unwrap();

        let dst = temp.path().join("dst");

        // Copy entire directory tree
        let result = copy_dir_all(src, dst.clone()).await;
        assert!(result.is_ok());

        // Verify all files and directories exist
        assert!(dst.exists());
        assert!(dst.join("file1.txt").exists());
        assert!(dst.join("file2.txt").exists());
        assert!(dst.join("subdir").exists());
        assert!(dst.join("subdir/file3.txt").exists());

        // Verify contents
        let content1 = fs::read_to_string(dst.join("file1.txt")).await.unwrap();
        assert_eq!(content1, "content1");
        
        let content3 = fs::read_to_string(dst.join("subdir/file3.txt")).await.unwrap();
        assert_eq!(content3, "content3");
    }

    #[tokio::test]
    async fn test_copy_dir_all_empty_directory() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("empty_src");
        let dst = temp.path().join("empty_dst");

        fs::create_dir_all(&src).await.unwrap();

        let result = copy_dir_all(src, dst.clone()).await;
        assert!(result.is_ok());
        assert!(dst.exists());
    }

    #[tokio::test]
    async fn test_copy_dir_all_nonexistent_source() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("nonexistent");
        let dst = temp.path().join("dst");

        let result = copy_dir_all(src, dst).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_copy_preserves_directory_structure() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        
        // Create complex directory structure
        fs::create_dir_all(src.join("a/b/c")).await.unwrap();
        fs::write(src.join("a/b/c/deep.txt"), b"deep content").await.unwrap();
        fs::write(src.join("a/shallow.txt"), b"shallow").await.unwrap();
        
        let dst = temp.path().join("dst");
        copy_dir_all(src, dst.clone()).await.unwrap();
        
        // Verify structure preserved
        assert!(dst.join("a").exists());
        assert!(dst.join("a/b").exists());
        assert!(dst.join("a/b/c").exists());
        assert!(dst.join("a/b/c/deep.txt").exists());
        assert!(dst.join("a/shallow.txt").exists());
    }

    #[tokio::test]
    async fn test_copy_multiple_files_same_directory() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        fs::create_dir_all(&src).await.unwrap();
        
        // Create multiple files
        for i in 0..10 {
            fs::write(src.join(format!("file{}.txt", i)), format!("content{}", i))
                .await
                .unwrap();
        }
        
        let dst = temp.path().join("dst");
        copy_dir_all(src, dst.clone()).await.unwrap();
        
        // Verify all files copied
        for i in 0..10 {
            let file_path = dst.join(format!("file{}.txt", i));
            assert!(file_path.exists());
            let content = fs::read_to_string(&file_path).await.unwrap();
            assert_eq!(content, format!("content{}", i));
        }
    }

    #[tokio::test]
    async fn test_copy_dir_all_creates_intermediate_directories() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("deeply/nested/dst");

        fs::create_dir_all(&src).await.unwrap();
        fs::write(src.join("file.txt"), b"test").await.unwrap();

        let result = copy_dir_all(src, dst.clone()).await;
        assert!(result.is_ok());
        assert!(dst.exists());
        assert!(dst.join("file.txt").exists());
    }

    #[tokio::test]
    async fn test_copy_dir_all_overwrites_existing_files() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        // Create source
        fs::create_dir_all(&src).await.unwrap();
        fs::write(src.join("file.txt"), b"new content").await.unwrap();

        // Create destination with old content
        fs::create_dir_all(&dst).await.unwrap();
        fs::write(dst.join("file.txt"), b"old content").await.unwrap();

        // Copy should overwrite
        copy_dir_all(src, dst.clone()).await.unwrap();
        
        let content = fs::read_to_string(dst.join("file.txt")).await.unwrap();
        assert_eq!(content, "new content");
    }

    #[tokio::test]
    async fn test_copy_dir_all_with_binary_files() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");

        fs::create_dir_all(&src).await.unwrap();
        
        // Create a binary file
        let binary_data: Vec<u8> = vec![0, 1, 2, 255, 254, 253];
        fs::write(src.join("binary.bin"), &binary_data).await.unwrap();

        copy_dir_all(src, dst.clone()).await.unwrap();
        
        let copied_data = fs::read(dst.join("binary.bin")).await.unwrap();
        assert_eq!(copied_data, binary_data);
    }

    #[tokio::test]
    async fn test_copy_dir_all_preserves_empty_subdirs() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        
        // Create empty subdirectories
        fs::create_dir_all(src.join("empty1")).await.unwrap();
        fs::create_dir_all(src.join("empty2/nested")).await.unwrap();
        fs::write(src.join("file.txt"), b"data").await.unwrap();

        let dst = temp.path().join("dst");
        copy_dir_all(src, dst.clone()).await.unwrap();
        
        assert!(dst.join("empty1").exists());
        assert!(dst.join("empty2").exists());
        assert!(dst.join("empty2/nested").exists());
        assert!(dst.join("file.txt").exists());
    }
}
