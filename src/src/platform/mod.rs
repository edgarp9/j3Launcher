use std::io;
use std::path::Path;

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    windows::replace_file(from, to)
}

#[cfg(not(windows))]
pub fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::rename(from, to)?;
    sync_parent_dir(to)
}

#[cfg(not(windows))]
fn sync_parent_dir(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::File::open(parent)?.sync_all()
}

#[cfg(all(test, not(windows)))]
mod tests {
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::replace_file;

    static TEST_DIR_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(label: &str) -> io::Result<Self> {
            let sequence = TEST_DIR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "j3launcher-platform-{label}-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path)?;
            Ok(Self { path })
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn replace_file_replaces_target_after_syncable_rename() -> io::Result<()> {
        let dir = TestDir::new("replace")?;
        let source = dir.path().join("source.tmp");
        let target = dir.path().join("target.json");
        fs::write(&source, b"new")?;
        fs::write(&target, b"old")?;

        replace_file(&source, &target)?;

        assert_eq!(fs::read(&target)?, b"new");
        assert!(!source.exists());
        Ok(())
    }
}
