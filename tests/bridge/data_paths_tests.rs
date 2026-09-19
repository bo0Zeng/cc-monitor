use super::*;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

struct TestDir(PathBuf);
impl TestDir {
    fn new(tag: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!(
            "ccm-data-paths-test-{}-{tag}-{n}",
            std::process::id(),
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        TestDir(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn probe_file_returns_exists_and_size_when_present() {
    let d = TestDir::new("probe-file");
    let p = d.path().join("x.json");
    fs::write(&p, "hello").unwrap();
    let info = probe_file(p.clone(), "x.json", "test");
    assert!(info.exists);
    assert_eq!(info.size_bytes, Some(5));
    assert_eq!(info.kind, "file");
}

#[test]
fn probe_file_returns_not_exists_when_absent() {
    let d = TestDir::new("probe-file-absent");
    let info = probe_file(d.path().join("absent.json"), "absent.json", "test");
    assert!(!info.exists);
    assert!(info.size_bytes.is_none());
}

#[test]
fn probe_dir_never_returns_size() {
    let d = TestDir::new("probe-dir");
    fs::write(d.path().join("a"), "abc").unwrap();
    let info = probe_dir(d.path().to_path_buf(), "x/", "test");
    assert!(info.exists);
    assert!(info.size_bytes.is_none());
    assert_eq!(info.kind, "dir");
}

#[test]
fn has_backup_in_dir_detects_ccm_backup_files() {
    let d = TestDir::new("backup-detect");
    // 没 backup
    assert!(!has_backup_in_dir(d.path()));
    // 加一个普通文件
    fs::write(d.path().join("profile.ps1"), "").unwrap();
    assert!(!has_backup_in_dir(d.path()));
    // 加 backup
    fs::write(d.path().join("profile.ps1.ccm-backup-12345"), "").unwrap();
    assert!(has_backup_in_dir(d.path()));
}
