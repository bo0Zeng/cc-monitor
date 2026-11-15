//! 送出清单：从树根走一遍，排掉的不进，产出要送的相对路径。

use std::io;
use std::path::{Path, PathBuf};

use crate::ExcludeList;

/// 从 `root` 走一遍，返回要送的相对路径（只文件与符号链接，排好序）。
/// 排掉的目录整棵不进；符号链接不跟进去，指向树外 · 解不开 · 指向被排掉的东西的不送。
pub fn send_list(root: &Path, ex: &ExcludeList) -> io::Result<Vec<PathBuf>> {
    let real_root = root.canonicalize()?;
    let mut out = Vec::new();
    walk(&real_root, &real_root, PathBuf::new(), ex, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(
    real_root: &Path,
    dir: &Path,
    rel: PathBuf,
    ex: &ExcludeList,
    out: &mut Vec<PathBuf>,
) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel = rel.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_dir() {
            if !ex.is_excluded(&rel, true) {
                walk(real_root, &path, rel, ex, out)?;
            }
        } else if ft.is_file() {
            if !ex.is_excluded(&rel, false) {
                out.push(rel);
            }
        } else if ft.is_symlink() && !ex.is_excluded(&rel, false) && link_ok(real_root, &path, ex) {
            out.push(rel);
        }
    }
    Ok(())
}

/// 链接解得开、落在树里一份没被排掉的普通文件上。指向目录的一律不送（带 `-L` 会把目录里排掉的带出去）。
fn link_ok(real_root: &Path, link: &Path, ex: &ExcludeList) -> bool {
    let Ok(target) = link.canonicalize() else {
        return false;
    };
    let Ok(inside) = target.strip_prefix(real_root) else {
        return false;
    };
    target.is_file() && !ex.is_excluded(inside, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 改进程当前目录的只此一条；别的测试不碰当前目录，但仍拿这把锁防日后有人加。
    static CWD_LOCK: Mutex<()> = Mutex::new(());

    /// 私有临时目录（0700），测试结束删掉。
    struct Tmp(PathBuf);
    impl Tmp {
        fn new() -> Tmp {
            static N: AtomicU32 = AtomicU32::new(0);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .subsec_nanos();
            let p = std::env::temp_dir().join(format!(
                "devsched-policy-{}-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed),
                nanos
            ));
            fs::create_dir(&p).unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
            Tmp(p)
        }
        fn file(&self, rel: &str) {
            let p = self.0.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, b"x").unwrap();
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn rels(v: &[&str]) -> Vec<PathBuf> {
        v.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn env_star_with_cwd_at_root_sends_only_source() {
        let t = Tmp::new();
        for f in [".env.example", "api/.env", ".planned-build/x", "src/a.rs"] {
            t.file(f);
        }
        let _g = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(&t.0).unwrap();
        let got = send_list(&t.0, &ExcludeList::parse(".env*"));
        std::env::set_current_dir(old).unwrap();
        assert_eq!(got.unwrap(), rels(&["src/a.rs"]));
    }

    #[test]
    fn listed_dir_pruned_whole_and_output_sorted() {
        let t = Tmp::new();
        for f in [
            "z.txt",
            "b/c.rs",
            "a.rs",
            ".git/HEAD",
            ".git/objects/ab/cd",
            "sub/.git/config",
        ] {
            t.file(f);
        }
        let got = send_list(&t.0, &ExcludeList::parse(".git")).unwrap();
        assert_eq!(got, rels(&["a.rs", "b/c.rs", "z.txt"]));
    }

    #[test]
    fn symlinks_inside_kept_outside_or_to_excluded_dropped() {
        let outside = Tmp::new();
        outside.file("secret.txt");
        let t = Tmp::new();
        t.file("src/a.rs");
        t.file(".env");
        symlink("src/a.rs", t.0.join("in-link")).unwrap();
        symlink(outside.0.join("secret.txt"), t.0.join("out-abs")).unwrap();
        // 相对路径逃出树根、落到一份真在的文件上。
        let out_name = outside.0.file_name().unwrap().to_str().unwrap();
        symlink(
            format!("../../{out_name}/secret.txt"),
            t.0.join("src/out-rel"),
        )
        .unwrap();
        symlink(".env", t.0.join("env-link")).unwrap();
        symlink("missing", t.0.join("dangling")).unwrap();
        let got = send_list(&t.0, &ExcludeList::parse("")).unwrap();
        assert_eq!(got, rels(&["in-link", "src/a.rs"]));
    }

    #[test]
    fn links_to_directories_never_sent() {
        // 指向目录（树根本身 · 上级 · 树里的目录）的链接一律不送：带 `-L` 送时会把目录里排掉的东西带出去。
        let t = Tmp::new();
        t.file("src/a.rs");
        t.file("src/.env");
        symlink("src", t.0.join("dir-link")).unwrap();
        symlink(".", t.0.join("self")).unwrap();
        symlink("..", t.0.join("src/up")).unwrap();
        let got = send_list(&t.0, &ExcludeList::parse("")).unwrap();
        assert_eq!(got, rels(&["src/a.rs"]));
    }

    #[test]
    fn root_given_through_symlink_still_works() {
        let t = Tmp::new();
        t.file("real/src/a.rs");
        symlink(t.0.join("real"), t.0.join("alias")).unwrap();
        let got = send_list(&t.0.join("alias"), &ExcludeList::parse("")).unwrap();
        assert_eq!(got, rels(&["src/a.rs"]));
    }
}
