//! 登记根下的布局：只认 `--root` 给的一个目录，节点的一切都在它下面。
//!
//! ```text
//! <根>/toolchains/   Rust（cargo · rustup）· node · JDK · android-sdk/
//! <根>/projects/<项目>/<树>/   工作树与产物（项目之间不串）
//! <根>/cache/        sccache · cargo 注册表 · gradle · npm
//! <根>/t/<4 位短号>/  每趟的临时目录
//! <根>/log/          节点日志（上限 50 MB，超了从最旧的删）
//! <根>/state/        锁 · 进程记录 · 套接字 · 节点二进制本身
//! ```

use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// Unix 套接字路径的上限（`sun_path` 108 字节，连结尾的 0）。
pub const SUN_PATH_MAX: usize = 108;
/// 测试在每趟临时目录下起私有 tmux 等，预计最深的 socket 后缀有这么长（带 tmux 的测试路径一超 108 字节就全红）。
pub const SOCKET_SUFFIX_RESERVE: usize = 80;
/// `log/` 的上限。
pub const LOG_CAP_BYTES: u64 = 50 * 1024 * 1024;
/// 作业往这个变量指的文件里一行写一个没跑的段名；它的值同时是这趟的记号（扫漏出组的进程照它认）。
pub const SKIPPED_VAR: &str = "DEVSCHED_SKIPPED";

/// 这条路径当得了 Unix 套接字地址（< 108 字节）。
pub fn unix_path_ok(p: &Path) -> bool {
    p.as_os_str().as_bytes().len() < SUN_PATH_MAX
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Root {
    base: PathBuf,
}

/// 建每趟临时目录没成。
#[derive(Debug)]
pub enum TmpError {
    /// 「临时目录 ＋ 预计最深的 socket 后缀」≥ 108 字节：停因「临时目录路径超长」。
    TooLong {
        path: PathBuf,
        len: usize,
    },
    Io(io::Error),
}

impl std::fmt::Display for TmpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TmpError::TooLong { path, len } => write!(
                f,
                "临时目录 {} 有 {len} 字节，再接 {SOCKET_SUFFIX_RESERVE} 字节的 socket 后缀就到 {SUN_PATH_MAX} 了",
                path.display()
            ),
            TmpError::Io(e) => write!(f, "建临时目录没成：{e}"),
        }
    }
}

/// 一趟的临时目录：`<根>/t/<短号>`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunTmp {
    pub short: String,
    pub dir: PathBuf,
}

impl RunTmp {
    /// `DEVSCHED_SKIPPED` 指的那份文件（也是这趟的记号）。
    pub fn skipped_file(&self) -> PathBuf {
        self.dir.join("skipped")
    }

    /// 读出作业写下的跳过的段：一行一个，空行不算，重复的只留第一次。
    pub fn read_skipped(&self) -> Vec<String> {
        let Ok(s) = fs::read_to_string(self.skipped_file()) else {
            return Vec::new();
        };
        let mut out: Vec<String> = Vec::new();
        for l in s.lines().map(str::trim).filter(|l| !l.is_empty()) {
            if !out.iter().any(|x| x == l) {
                out.push(l.to_string());
            }
        }
        out
    }

    pub fn remove(&self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// 另起一个线程删掉这个目录（删大树不挡帧循环）；起不了线程（撞了线程线）就当场删。
pub fn remove_in_background(p: PathBuf) {
    let q = p.clone();
    if std::thread::Builder::new()
        .name("del".into())
        .spawn(move || {
            let _ = fs::remove_dir_all(&q);
        })
        .is_err()
    {
        let _ = fs::remove_dir_all(&p);
    }
}

static SEQ: AtomicU32 = AtomicU32::new(0);

/// 4 位三十六进制短号（不是路径字节的编码，只是每趟临时目录的名字）。
fn short_id(n: u32) -> String {
    let mut v = n % (36 * 36 * 36 * 36);
    let mut s = ['0'; 4];
    for c in s.iter_mut().rev() {
        *c = char::from_digit(v % 36, 36).unwrap_or('0');
        v /= 36;
    }
    s.iter().collect()
}

impl Root {
    pub fn new(base: impl Into<PathBuf>) -> Root {
        Root { base: base.into() }
    }
    pub fn base(&self) -> &Path {
        &self.base
    }
    pub fn toolchains(&self) -> PathBuf {
        self.base.join("toolchains")
    }
    pub fn projects(&self) -> PathBuf {
        self.base.join("projects")
    }
    pub fn cache(&self) -> PathBuf {
        self.base.join("cache")
    }
    /// `clean` 要删的那棵树先改名成的地方：同一项目目录下 `.del-<树>.<节点进程号>.<序号>`（`.` 起头，调度端列树时不算树）。
    pub fn trash_for(&self, tree_dir: &Path, seq: u64) -> PathBuf {
        let name = tree_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // 带节点进程号：节点重启后序号从头数，也不撞上回没删完的那份
        tree_dir.with_file_name(format!(".del-{name}.{}.{seq}", std::process::id()))
    }
    /// 各项目目录下上回没删完的 `.del-*`。
    pub fn leftover_trash(&self) -> Vec<PathBuf> {
        let Ok(rd) = fs::read_dir(self.projects()) else {
            return Vec::new();
        };
        rd.flatten()
            .filter_map(|p| fs::read_dir(p.path()).ok())
            .flat_map(|d| d.flatten())
            .filter(|e| e.file_name().to_string_lossy().starts_with(".del-"))
            .map(|e| e.path())
            .collect()
    }
    pub fn tmp(&self) -> PathBuf {
        self.base.join("t")
    }
    pub fn log(&self) -> PathBuf {
        self.base.join("log")
    }
    pub fn state(&self) -> PathBuf {
        self.base.join("state")
    }
    pub fn android_sdk(&self) -> PathBuf {
        self.toolchains().join("android-sdk")
    }
    pub fn lock_file(&self) -> PathBuf {
        self.state().join("node.lock")
    }
    pub fn socket(&self) -> PathBuf {
        self.state().join("n.sock")
    }
    pub fn jobs_dir(&self) -> PathBuf {
        self.state().join("jobs")
    }

    /// 建齐各目录；根与 `state/` 只给自己。cargo 注册表照布局住 `cache/`（`CARGO_HOME/registry` 是指过去的链接）。
    pub fn ensure(&self) -> io::Result<()> {
        fs::create_dir_all(&self.base)?;
        fs::set_permissions(&self.base, fs::Permissions::from_mode(0o700))?;
        for d in [
            self.toolchains(),
            self.projects(),
            self.cache(),
            self.tmp(),
            self.log(),
            self.state(),
            self.jobs_dir(),
        ] {
            fs::create_dir_all(d)?;
        }
        let cargo = self.toolchains().join("cargo");
        fs::create_dir_all(&cargo)?;
        let reg = cargo.join("registry");
        if fs::symlink_metadata(&reg).is_err() {
            let target = self.cache().join("cargo-registry");
            fs::create_dir_all(&target)?;
            std::os::unix::fs::symlink(&target, &reg)?;
        }
        Ok(())
    }

    /// 作业的工作目录 `<根>/projects/<项目>/<树>/<相对目录>`；名字里带 `..`、绝对路径、`/` 的拒。
    pub fn job_cwd(&self, project: &str, tree: &str, rel: &str) -> Result<PathBuf, String> {
        for (what, name) in [("项目名", project), ("树名", tree)] {
            if name.is_empty() || name.contains('/') || name == "." || name == ".." {
                return Err(format!("{what}「{name}」不是一段目录名"));
            }
        }
        let relp = Path::new(rel);
        if relp
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err(format!("相对目录「{rel}」出了树"));
        }
        Ok(self.projects().join(project).join(tree).join(relp))
    }

    /// 建这一趟的临时目录 `<根>/t/<4 位短号>`；路径太长 ⇒ 不建、报超长。
    pub fn make_run_tmp(&self) -> Result<RunTmp, TmpError> {
        let probe = self.tmp().join("xxxx");
        let len = probe.as_os_str().as_bytes().len();
        if len + SOCKET_SUFFIX_RESERVE >= SUN_PATH_MAX {
            return Err(TmpError::TooLong { path: probe, len });
        }
        fs::create_dir_all(self.tmp()).map_err(TmpError::Io)?;
        let seed = std::process::id().wrapping_mul(2654435761)
            ^ std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0);
        for _ in 0..64 {
            let short = short_id(seed.wrapping_add(SEQ.fetch_add(7919, Ordering::Relaxed)));
            let dir = self.tmp().join(&short);
            match fs::create_dir(&dir) {
                Ok(()) => {
                    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
                    return Ok(RunTmp { short, dir });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(TmpError::Io(e)),
            }
        }
        Err(TmpError::Io(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "短号用满",
        )))
    }

    /// 作业与探测用的 PATH：根下 Rust 工具链的 bin 在前，再接照工具清单装进来的各 `toolchains/<名>/bin`，再接 `base`（作业帧带的 PATH；没带就是节点自己的）。
    /// hello 报的能力与程序表也照这一份查，报的与作业找得到的是同一套。
    pub fn job_path(&self, base: Option<&std::ffi::OsStr>) -> OsString {
        let mut p = OsString::from(self.toolchains().join("cargo/bin"));
        // 照工具清单装进来的（调度端 --prepare）：toolchains/<工具名>/bin，按名字排，没有 bin 的跳过
        let mut tools: Vec<PathBuf> = fs::read_dir(self.toolchains())
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                !devsched_wire::TOOLCHAINS_NOT_TOOLS
                    .iter()
                    .any(|n| e.file_name() == *n)
            })
            .map(|e| e.path().join("bin"))
            .filter(|b| b.is_dir())
            .collect();
        tools.sort();
        for b in tools {
            p.push(":");
            p.push(b);
        }
        let node_path = std::env::var_os("PATH");
        if let Some(b) = base.or(node_path.as_deref()).filter(|b| !b.is_empty()) {
            p.push(":");
            p.push(b);
        }
        p
    }

    /// 节点替作业设的变量（只这三类，其余照作业帧原样）：TMPDIR 与 HOME（这趟的临时目录）· 工具链与缓存（连 PATH 前头的根下工具链；
    /// 节点的 sccache server 起着就带它的端口）· DEVSCHED_SKIPPED。`frame_path` 是作业帧带的 PATH。
    pub fn job_env(
        &self,
        run: &RunTmp,
        frame_path: Option<&std::ffi::OsStr>,
        sccache_port: Option<u16>,
    ) -> Vec<(String, OsString)> {
        // HOME 也指这趟的临时目录：很多工具默认要它；不带开发机的家目录出去，帧里带了也盖掉
        let mut v: Vec<(String, OsString)> = vec![
            ("TMPDIR".into(), run.dir.clone().into()),
            ("HOME".into(), run.dir.clone().into()),
        ];
        v.extend(self.tool_env_with(frame_path));
        if let Some(p) = sccache_port {
            v.push(("SCCACHE_SERVER_PORT".into(), p.to_string().into()));
        }
        v.push((SKIPPED_VAR.into(), run.skipped_file().into()));
        v
    }

    /// 工具链与缓存类（PATH 接节点自己的）：节点的 sccache server 也照这一份起。
    pub fn tool_env(&self) -> Vec<(String, OsString)> {
        self.tool_env_with(None)
    }

    fn tool_env_with(&self, frame_path: Option<&std::ffi::OsStr>) -> Vec<(String, OsString)> {
        let mut v: Vec<(String, OsString)> = vec![
            ("PATH".into(), self.job_path(frame_path)),
            ("CARGO_HOME".into(), self.toolchains().join("cargo").into()),
            (
                "RUSTUP_HOME".into(),
                self.toolchains().join("rustup").into(),
            ),
            (
                "GRADLE_USER_HOME".into(),
                self.cache().join("gradle").into(),
            ),
            ("npm_config_cache".into(), self.cache().join("npm").into()),
            ("SCCACHE_DIR".into(), self.cache().join("sccache").into()),
        ];
        if self.android_sdk().is_dir() {
            v.push(("ANDROID_HOME".into(), self.android_sdk().into()));
        }
        v
    }

    /// 删掉 `t/` 下这几趟的临时目录（起来清上次残留用：只删 `state/jobs/` 记着的，不整个清 `t/`，
    /// 共用这个根的别的东西正在用的不误删）。短号带 `/` 或 `..` 的不认。返回删了几个。
    pub fn clear_tmp_of(&self, shorts: &[String]) -> usize {
        let mut n = 0;
        for s in shorts {
            if s.is_empty() || s.contains('/') || s == "." || s == ".." {
                continue;
            }
            let p = self.tmp().join(s);
            if p.is_dir() && !p.is_symlink() && fs::remove_dir_all(&p).is_ok() {
                n += 1;
            }
        }
        n
    }

    /// `log/` 超了上限 ⇒ 从最旧的删，删到不超；`keep`（正写着的那一份）不删。返回删了几份。
    pub fn prune_logs_except(&self, cap: u64, keep: &Path) -> usize {
        let Ok(rd) = fs::read_dir(self.log()) else {
            return 0;
        };
        let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = rd
            .flatten()
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                m.is_file().then(|| {
                    (
                        m.modified().unwrap_or(std::time::UNIX_EPOCH),
                        m.len(),
                        e.path(),
                    )
                })
            })
            .collect();
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        files.sort();
        let mut n = 0;
        for (_, len, p) in files {
            if total <= cap {
                break;
            }
            if p != keep && fs::remove_file(&p).is_ok() {
                total -= len;
                n += 1;
            }
        }
        n
    }
}

#[cfg(test)]
pub mod testdir {
    use std::path::PathBuf;

    /// 测试用的私有临时目录（`/tmp/dn<4 位>`，只给自己，测完删）。名字要短：根下 `t/xxxx` 再接 80 字节要 < 108，
    /// 经调度跑时 TMPDIR 本身就是远端的 `<根>/t/xxxx`，在它下面再套一层就超了 ⇒ 直接放 `/tmp`。
    pub struct TestDir(pub PathBuf);
    impl TestDir {
        pub fn new(_tag: &str) -> TestDir {
            use std::os::unix::fs::PermissionsExt;
            loop {
                let n = super::SEQ
                    .fetch_add(1, super::Ordering::Relaxed)
                    .wrapping_add(std::process::id().wrapping_mul(7919));
                let p = PathBuf::from(format!("/tmp/dn{}", super::short_id(n)));
                match std::fs::create_dir(&p) {
                    Ok(()) => {
                        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700))
                            .unwrap();
                        return TestDir(p);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => panic!("建测试目录 {}：{e}", p.display()),
                }
            }
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testdir::TestDir;
    use super::*;

    #[test]
    fn too_long_root_refuses_run_tmp() {
        let d = TestDir::new("root-long");
        // 根本身就占掉 ≥ 108 − 80 − "/t/xxxx" 的字节
        let mut base = d.0.clone();
        while base.as_os_str().len() + "/t/xxxx".len() + SOCKET_SUFFIX_RESERVE < SUN_PATH_MAX {
            base = base.join("deeper");
        }
        let r = Root::new(&base);
        match r.make_run_tmp() {
            Err(TmpError::TooLong { .. }) => {}
            other => panic!("该报超长：{other:?}"),
        }
        assert!(!r.tmp().exists(), "超长时不该建 t/");
    }

    #[test]
    fn short_root_makes_four_char_tmp_and_env() {
        let d = TestDir::new("root-ok");
        let r = Root::new(&d.0);
        r.ensure().unwrap();
        let a = r.make_run_tmp().unwrap();
        let b = r.make_run_tmp().unwrap();
        assert_eq!(a.short.len(), 4);
        assert_ne!(a.short, b.short);
        assert!(a.dir.is_dir());
        assert_eq!(a.dir.parent().unwrap(), r.tmp());
        let env = r.job_env(&a, Some(std::ffi::OsStr::new("/usr/bin")), None);
        let get = |k: &str| {
            env.iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| PathBuf::from(v))
        };
        assert_eq!(get("TMPDIR").unwrap(), a.dir);
        assert_eq!(get("HOME").unwrap(), a.dir, "HOME 是这趟的临时目录");
        assert!(get("CARGO_HOME").unwrap().starts_with(r.toolchains()));
        assert!(get("SCCACHE_DIR").unwrap().starts_with(r.cache()));
        assert_eq!(get(SKIPPED_VAR).unwrap(), a.skipped_file());
        assert_eq!(
            get("PATH").unwrap(),
            PathBuf::from(format!(
                "{}:/usr/bin",
                r.toolchains().join("cargo/bin").display()
            ))
        );
        assert!(get("ANDROID_HOME").is_none(), "没有 android-sdk 不设");
        fs::create_dir_all(r.android_sdk()).unwrap();
        assert!(get("SCCACHE_SERVER_PORT").is_none(), "没起 server 不设端口");
        assert!(r
            .job_env(&a, None, None)
            .iter()
            .any(|(n, _)| n == "ANDROID_HOME"));
        let with_port = r.job_env(&a, None, Some(4321));
        assert!(with_port
            .iter()
            .any(|(n, v)| n == "SCCACHE_SERVER_PORT" && v == "4321"));
        // 注册表住 cache/
        assert!(fs::symlink_metadata(r.toolchains().join("cargo/registry"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            r.clear_tmp_of(&[a.short.clone(), "..".into(), "nope".into()]),
            1
        );
        assert!(b.dir.exists(), "没点名的不删");
        assert!(!a.dir.exists());
    }

    #[test]
    fn skipped_lines_dedup_and_trim() {
        let d = TestDir::new("root-skip");
        let r = Root::new(&d.0);
        let t = r.make_run_tmp().unwrap();
        fs::write(t.skipped_file(), "mobile\n\n windows \nmobile\n").unwrap();
        assert_eq!(
            t.read_skipped(),
            vec!["mobile".to_string(), "windows".to_string()]
        );
    }

    #[test]
    fn job_cwd_stays_in_tree() {
        let r = Root::new("/r");
        assert_eq!(
            r.job_cwd("p", "w", "a/b").unwrap(),
            PathBuf::from("/r/projects/p/w/a/b")
        );
        assert_eq!(
            r.job_cwd("p", "w", "").unwrap(),
            PathBuf::from("/r/projects/p/w")
        );
        assert!(r.job_cwd("p", "w", "../x").is_err());
        assert!(r.job_cwd("p", "w", "/etc").is_err());
        assert!(r.job_cwd("..", "w", "").is_err());
        assert!(r.job_cwd("p", "a/b", "").is_err());
    }

    #[test]
    fn logs_pruned_oldest_first() {
        let d = TestDir::new("root-log");
        let r = Root::new(&d.0);
        r.ensure().unwrap();
        for (i, name) in ["a", "b", "c"].iter().enumerate() {
            let p = r.log().join(name);
            fs::write(&p, vec![b'x'; 100]).unwrap();
            let t =
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000 + i as u64 * 10);
            fs::File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_modified(t)
                .unwrap();
        }
        let keep = r.log().join("a");
        assert_eq!(
            r.prune_logs_except(150, &keep),
            2,
            "正写着的最旧那份不删，删 b · c"
        );
        assert!(keep.exists());
        assert!(!r.log().join("b").exists() && !r.log().join("c").exists());
        assert_eq!(r.prune_logs_except(150, &keep), 0);
    }

    #[test]
    fn job_path_has_each_tool_bin_after_cargo() {
        let d = TestDir::new("root-tools");
        let r = Root::new(&d.0);
        for t in ["tmux", "node", "nobin"] {
            fs::create_dir_all(r.toolchains().join(t)).unwrap();
        }
        fs::create_dir_all(r.toolchains().join("tmux/bin")).unwrap();
        fs::create_dir_all(r.toolchains().join("node/bin")).unwrap();
        fs::create_dir_all(r.toolchains().join("cargo/bin")).unwrap();
        fs::create_dir_all(r.toolchains().join("rustup/bin")).unwrap();
        let p = r.job_path(Some(std::ffi::OsStr::new("/usr/bin")));
        let tc = r.toolchains();
        assert_eq!(
            p,
            OsString::from(format!(
                "{}:{}:{}:/usr/bin",
                tc.join("cargo/bin").display(),
                tc.join("node/bin").display(),
                tc.join("tmux/bin").display()
            ))
        );
    }

    #[test]
    fn socket_path_limit() {
        assert!(unix_path_ok(Path::new("/short/n.sock")));
        let long = "/".to_string() + &"x".repeat(SUN_PATH_MAX);
        assert!(!unix_path_ok(Path::new(&long)));
    }
}
