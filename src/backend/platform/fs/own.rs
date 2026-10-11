//! 后端**自己的**状态（数据目录 `~/.cc-monitor` 底下）的写：[`Own`] · [`OwnPath`] · [`OwnLock`]。
//!
//! - [`Own`] 在后端启动时造一次（[`Own::issue`]，根取 `platform::paths::data_home`），顺着入口递给写自家状态的那几处；
//!   别处拿不到这个值，也就写不出来。谁许造它由 `readonly_guard` 的构造判据钉（只许启动入口）。
//! - [`OwnPath`] 只能由 [`Own`] 从根往下拼（相对名、不许 `..`、不许绝对路径）⇒ 方法收的路径落不到根外。
//! - 方法只有这几件：建一层目录（0700，已在不动）· 拿目录的跨进程锁 · 原子写（要那个目录的锁）· 删文件 · 改名。
//!   原子写：旁名出生即只给本人（`O_EXCL` ＋ 0600）→ 写满 → `sync_all` → 原子挪过去；挪不过去就删掉自己建的旁名。
//!   旁名带 pid ＋ 线程 ＋ 进程内序号，同一进程几个线程同时写同一份也不撞。

use copy_core::copy_text;
use copy_core::said::Said;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// 写自家状态的句柄。克隆便宜（只有根）；造只经 [`Own::issue`]。
#[derive(Clone, Debug)]
pub(crate) struct Own {
    /// 数据目录；这台没有家目录（`HOME` / `USERPROFILE` 都没有）⇒ `None`，各写口照它们自己的「无主目录」那一句报。
    root: Option<PathBuf>,
}

/// 数据目录底下的一条路径。只能由 [`Own::root`] / [`Own::path`] / [`OwnPath::join`] 拼出来。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnPath(PathBuf);

/// 持着的那个目录的跨进程锁（`platform::lock`）。原子写要它，而且要的是**那份文件所在目录**的这一把。
pub(crate) struct OwnLock {
    dir: PathBuf,
    _held: crate::platform::lock::DirLock,
}

/// 相对名：至少一段、每段都是普通名字（没有 `..` · `.` · 根 · 盘符）。
fn relative_ok(rel: &str) -> bool {
    let p = Path::new(rel);
    p.components().next().is_some() && p.components().all(|c| matches!(c, Component::Normal(_)))
}

impl OwnPath {
    pub(crate) fn as_path(&self) -> &Path {
        &self.0
    }

    /// 往下拼一段相对名；不是相对名（空 · 绝对 · 含 `..`）⇒ 拒。
    pub(crate) fn join(&self, rel: &str) -> Result<OwnPath, Said> {
        if !relative_ok(rel) {
            return Err(copy_text("beOwnState.path.notRelative", &[("rel", rel)]).into());
        }
        Ok(OwnPath(self.0.join(rel)))
    }
}

impl Own {
    /// 启动时造一次：根是这台的数据目录。
    pub(crate) fn issue() -> Own {
        Own {
            root: crate::platform::paths::data_home(),
        }
    }

    /// 判据用：根指到一个临时目录。
    #[cfg(test)]
    pub(crate) fn issue_at(root: &Path) -> Own {
        Own {
            root: Some(root.to_path_buf()),
        }
    }

    /// 数据目录本身；这台没有家目录 ⇒ `None`。
    pub(crate) fn root(&self) -> Option<OwnPath> {
        self.root.clone().map(OwnPath)
    }

    /// 数据目录底下的 `rel`。没有家目录 ⇒ 拒（`beOwnState.path.noHome`）；`rel` 不是相对名 ⇒ 拒。
    pub(crate) fn path(&self, rel: &str) -> Result<OwnPath, Said> {
        self.root()
            .ok_or_else(|| Said::from(copy_text("beOwnState.path.noHome", &[])))?
            .join(rel)
    }

    /// 建**一层**目录，建的那一下就是 0700；已在的不动；父目录不在 ⇒ 照实报错。
    pub(crate) fn ensure_dir(&self, dir: &OwnPath) -> std::io::Result<()> {
        create_dir_private(dir.as_path())
    }

    /// 拿 `dir` 的跨进程锁（阻塞到拿到为止；`dir` 要先在）。
    pub(crate) fn lock(&self, dir: &OwnPath) -> Result<OwnLock, crate::platform::lock::LockFail> {
        Ok(OwnLock {
            dir: dir.0.clone(),
            _held: crate::platform::lock::hold(dir.as_path())?,
        })
    }

    /// 整份原子写 `body`。`held` 必须是 `path` 所在那个目录的锁，不是 ⇒ 拒、一个字节不落。
    pub(crate) fn write(&self, path: &OwnPath, held: &OwnLock, body: &[u8]) -> Result<(), Said> {
        if path.0.parent() != Some(held.dir.as_path()) {
            return Err(copy_text(
                "beOwnState.write.lockMismatch",
                &[("path", &path.0.display().to_string())],
            )
            .into());
        }
        atomic_write(&path.0, body)
    }

    /// 删一份文件（不删目录）。
    pub(crate) fn remove_file(&self, path: &OwnPath) -> std::io::Result<()> {
        std::fs::remove_file(&path.0)
    }

    /// 改名（两头都在数据目录底下）。
    pub(crate) fn rename(&self, from: &OwnPath, to: &OwnPath) -> std::io::Result<()> {
        std::fs::rename(&from.0, &to.0)
    }
}

/// 后端自家目录**建的那一下**给的权限位：只给本人（远端 SFTP 部署建目录用同一个数）。
pub(crate) const PRIVATE_DIR_MODE: u32 = 0o700;

/// 建**一层**目录、建的那一下就是 0700（unix：权限位随创建一起生效，没有「先按 umask 建出来再收窄」的那一段）；已在的不动。
/// 非 unix：照常建（目录的 ACL 按父目录继承）。[`Own::ensure_dir`] 的本体；文件管理面建暂存区那一处（第三层，下一刀搬）也直接用它。
pub(crate) fn create_dir_private(dir: &Path) -> std::io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        b.mode(PRIVATE_DIR_MODE);
    }
    match b.create(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

/// 进程内序号：同一线程连写两次也换一个旁名。
static SEQ: AtomicU64 = AtomicU64::new(0);

/// 原子写本体（全后端一份）。那一句只有路径与原因词，系统原话进 `raw`；都不带内容。
pub(crate) fn atomic_write(path: &Path, body: &[u8]) -> Result<(), Said> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(copy_text(
            "beOwnState.write.noParent",
            &[("path", &path.display().to_string())],
        )
        .into());
    };
    let tmp = dir.join(format!(
        "{}.{}.{:?}.{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        std::thread::current().id(),
        SEQ.fetch_add(1, Ordering::Relaxed),
    ));
    let tmp_s = tmp.display().to_string();
    let path_s = path.display().to_string();
    let io_said = |key: &str, e: &std::io::Error, with_path: bool| {
        let why = copy_core::io_reason(e.kind());
        let mut args: Vec<(&str, &str)> = vec![("tmp", &tmp_s), ("why", &why)];
        if with_path {
            args.push(("path", &path_s));
        }
        Said::with_raw(copy_text(key, &args), e)
    };
    // 建不出来 ⇒ 那个名字上的文件不是我们的，什么都不删。
    let mut f = creds_core::perm::create_private(&tmp)
        .map_err(|e| io_said("beOwnState.write.tmpCreateFailed", &e, false))?;
    let result = f
        .write_all(body)
        .and_then(|()| f.sync_all())
        .map_err(|e| io_said("beOwnState.write.tmpWriteFailed", &e, false))
        .and_then(|()| {
            drop(f);
            std::fs::rename(&tmp, path)
                .map_err(|e| io_said("beOwnState.write.renameFailed", &e, true))
        });
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
#[path = "../../../../tests/backend/platform/fs/own_tests.rs"]
mod tests;
