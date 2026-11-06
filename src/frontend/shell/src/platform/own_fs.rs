//! **壳往盘上写的唯一一层**：按落点分组，选哪一组写在函数名上。
//!
//! - 自己的状态（[`OwnState`]）：monitor 数据目录（`CCM_DATA_DIR`，否则 `~/.cc-monitor/`）底下，设置 · 握手表 · 迁移记号这一类。
//!   根在 [`OwnState::issue`] 里取（入口造一次、句柄往下传），方法只收相对名（没有 `..` · 根 · 盘符）⇒ 落不到根外。文件只给本人（0600），缺的目录建出来就是 0700。
//! - 自己的部署物（[`OwnBin`]）：家目录底下后端落点那一层 `~/.cc-monitor/bin/`（终端里敲的 `ccm` 与文件窗口程序住这里，
//!   与远端部署 · 别名块认的是同一处，`relay_route_core::BACKEND_LANDING_REL`）。方法只收落点目录里的一个名字；
//!   放一份 ＝ 先写 `.<名>.<pid>.partial`、置可执行位，再换名上位（Windows 上旧的正在跑 ⇒ 先挪成 `.<名>.<pid>.old`）。
//! - 终端设备（[`tty_write`]）：不是盘上的文件，往 `/dev/pts/N` 一类写控制序列。
//! - 日志目录（[`log_dir`]）：调用方给的那一层目录，只建目录（只给本人）。
//!
//! 用户的文件不在这里：经那台机器的后端写，或经 `host_core` 保留目标 ACL 的那一个替换。
//! 失败回 `io::Error`（带落点）：说给人听的那一句由调用方照它那一处的文案取。
//! `std::fs` 的写原语进了 `src/frontend/clippy.toml` 的禁用名单，放行只在本文件、各组那几个只做这一件事的小函数上。

use crate::detail::Shell as _;
use copy_core::{Discard as _, Why};
use std::io::{Error, ErrorKind, Result, Write as _};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// 带上落点的下层错（种类照旧）。
fn at_path(at: &Path, e: &Error) -> Error {
    Error::new(e.kind(), format!("{}: {e}", at.display()))
}

/// 名字不是根底下的相对名（或不是落点目录里的一个名字）。
fn not_relative(root: &Path, rel: &str) -> Error {
    Error::new(
        ErrorKind::InvalidInput,
        format!("{}: {rel:?} is not a plain relative name", root.display()),
    )
}

/// 相对名：至少一段、每段都是普通名字。
fn relative_ok(rel: &str) -> bool {
    let p = Path::new(rel);
    p.components().next().is_some() && p.components().all(|c| matches!(c, Component::Normal(_)))
}

/// 一个文件名：恰好一段普通名字。
fn single_name_ok(name: &str) -> bool {
    relative_ok(name) && Path::new(name).components().count() == 1
}

/// 建一层只给本人的目录（缺的上层一并建；已在不动）。
fn private_dir(dir: &Path) -> Result<()> {
    super::fs::ensure_private_dir(dir).map_err(|s| s.into_io())
}

/// 旁名序号：同一进程几个线程同时写同一份也不撞。
static SEQ: AtomicU64 = AtomicU64::new(0);

// ── 写原语：放行只在这几个小函数上 ──────────────────────────────

/// 建（或截断）一份只给本人的文件。
#[expect(
    clippy::disallowed_methods,
    reason = "写盘只经本文件：只给本人地建或截断一份"
)]
fn create_private(at: &Path) -> Result<std::fs::File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    super::fs::only_me_on_create(&mut opts);
    opts.open(at)
}

/// 换名（同一目录里，目标已在 ⇒ 覆盖）。
#[expect(
    clippy::disallowed_methods,
    reason = "写盘只经本文件：部署物上位的换名"
)]
fn rename(from: &Path, to: &Path) -> Result<()> {
    std::fs::rename(from, to)
}

/// 删一份文件。
#[expect(clippy::disallowed_methods, reason = "写盘只经本文件：删自己的一份")]
fn remove_file(at: &Path) -> Result<()> {
    std::fs::remove_file(at)
}

/// 写满 → 落盘。
fn fill(mut f: std::fs::File, bytes: &[u8]) -> Result<()> {
    f.write_all(bytes)?;
    f.sync_all()
}

/// 整份写到 `at`：旁名 `.<名>.<pid>.<序>.tmp` 起在这里，「建旁名 → 写满 → 落盘 → 换上去 → 失败删旁名」只经全仓那一份
/// （`platform_core::atomic::write_atomic`，后端同一份）。
fn write_state(at: &Path, bytes: &[u8]) -> Result<()> {
    let dir = at.parent().unwrap_or(Path::new("."));
    let name = at
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    let tmp = dir.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    platform_core::atomic::write_atomic(at, &tmp, platform_core::atomic::create_new_private, |f| {
        f.write_all(bytes)
    })
    .map_err(|fail| match fail.stage {
        platform_core::atomic::Stage::Replace => at_path(at, &fail.err),
        _ => at_path(&tmp, &fail.err),
    })
}

/// 删一份；本来就不在算删成了。
fn remove_if_there(at: &Path) -> Result<()> {
    match remove_file(at) {
        Err(e) if e.kind() != ErrorKind::NotFound => Err(at_path(at, &e)),
        _ => Ok(()),
    }
}

// ── 自己的状态 ──────────────────────────────

/// 写 monitor 自己的状态的句柄：根是数据目录。生产段造只经 [`OwnState::issue`]（入口造一次往下传）；
/// 判据经 `issue_with` 喂环境（只在测试档里有）。
#[derive(Clone, Debug)]
pub struct OwnState {
    root: PathBuf,
}

impl OwnState {
    /// 照这台的数据目录造；数据目录说不出（没有家目录 · `CCM_DATA_DIR` 不是绝对路径）⇒ `None`，调用方照「无主目录」报。
    pub fn issue() -> Option<OwnState> {
        crate::config::resolve_monitor_data_dir().map(OwnState::at)
    }

    /// 同 [`OwnState::issue`]，环境由 `get` 答（`CCM_DATA_DIR` · 家目录那几个变量）：判据在临时目录上造句柄用。
    #[cfg(test)]
    pub(crate) fn issue_with(get: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> Option<OwnState> {
        let data = get(crate::config::DATA_DIR_ENV).and_then(|v| v.into_string().ok());
        crate::config::monitor_data_dir_from(data.as_deref(), creds_core::store::home_dir_from(get))
            .map(OwnState::at)
    }

    fn at(root: PathBuf) -> OwnState {
        OwnState { root }
    }

    /// 根（数据目录本身）：读的那一侧 · 锁整个目录用。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 根底下一条相对名的路径（读的那一侧也照它拼）；不是相对名 ⇒ 拒。
    pub fn path(&self, rel: &str) -> Result<PathBuf> {
        if !relative_ok(rel) {
            return Err(not_relative(&self.root, rel));
        }
        Ok(self.root.join(rel))
    }

    /// 建一层目录（缺的上层一并建，建出来只给本人；已在不动）。回那一层的路径。
    pub fn dir(&self, rel: &str) -> Result<PathBuf> {
        let at = self.path(rel)?;
        private_dir(&at)?;
        Ok(at)
    }

    /// 所在目录缺 ⇒ 先建（只给本人）。
    fn parent_ready(at: &Path) -> Result<()> {
        at.parent().map_or(Ok(()), private_dir)
    }

    /// 原子写一份（只给本人）；所在目录缺 ⇒ 先建。
    pub fn write(&self, rel: &str, bytes: &[u8]) -> Result<()> {
        let at = self.path(rel)?;
        Self::parent_ready(&at)?;
        write_state(&at, bytes)
    }

    /// 删一份；本来就不在算删成了。
    pub fn remove(&self, rel: &str) -> Result<()> {
        remove_if_there(&self.path(rel)?)
    }
}

// ── 自己的部署物 ──────────────────────────────

/// 往后端落点那一层（`~/.cc-monitor/bin/`）放程序的句柄。生产段造只经 [`OwnBin::issue`]；判据经 `issue_with` 喂环境（只在测试档里有）。
#[derive(Clone, Debug)]
pub struct OwnBin {
    dir: PathBuf,
}

impl OwnBin {
    /// 落点目录 ＝ 家目录底下后端落点（`relay_route_core::BACKEND_LANDING_REL`）所在那一层；家目录问不到 ⇒ `None`。
    pub fn issue() -> Option<OwnBin> {
        crate::data_paths::home().and_then(OwnBin::under_home)
    }

    /// 同 [`OwnBin::issue`]，家目录由 `get` 答：判据在临时目录上造句柄用。
    #[cfg(test)]
    pub(crate) fn issue_with(get: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> Option<OwnBin> {
        creds_core::store::home_dir_from(get).and_then(OwnBin::under_home)
    }

    fn under_home(home: PathBuf) -> Option<OwnBin> {
        let (rel, _) = relay_route_core::BACKEND_LANDING_REL.rsplit_once('/')?;
        Some(OwnBin::at(rel.split('/').fold(home, |p, seg| p.join(seg))))
    }

    fn at(dir: PathBuf) -> OwnBin {
        OwnBin { dir }
    }

    /// 落点目录底下的一层（`slots` 这一类）：同一套放法，名字是一段。
    pub fn sub(&self, name: &str) -> Result<OwnBin> {
        Ok(OwnBin::at(self.named(name)?))
    }

    /// 落点目录（读的那一侧：比盘上那份、列残件）。
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn named(&self, name: &str) -> Result<PathBuf> {
        if !single_name_ok(name) {
            return Err(not_relative(&self.dir, name));
        }
        Ok(self.dir.join(name))
    }

    /// 写一份暂存件 `.<名>.<pid>.partial`（只给本人、已置可执行位），回它的路径；还没上位。
    pub fn stage(&self, name: &str, bytes: &[u8]) -> Result<PathBuf> {
        self.named(name)?;
        private_dir(&self.dir)?;
        let tmp = self
            .dir
            .join(format!(".{name}.{}.partial", std::process::id()));
        let ready = create_private(&tmp)
            .and_then(|f| fill(f, bytes))
            .map_err(|e| at_path(&tmp, &e))
            .and_then(|()| super::fs::make_executable(&tmp).map_err(|s| s.into_io()));
        if let Err(e) = ready {
            remove_file(&tmp).discard(Why::CleanupAfterFailure);
            return Err(e);
        }
        Ok(tmp)
    }

    /// 暂存件上位成 `<名>`，回落点。换不上（Windows 上旧的正在跑）⇒ 先把旧的挪成 `.<名>.<pid>.old` 再上位；
    /// 挪也挪不开 ⇒ 回第一次的错，暂存件留给调用方收。
    pub fn put(&self, name: &str, staged: &Path) -> Result<PathBuf> {
        let dest = self.named(name)?;
        rename(staged, &dest)
            .or_else(|first| {
                let aside = self.dir.join(format!(".{name}.{}.old", std::process::id()));
                rename(&dest, &aside)
                    .and_then(|()| rename(staged, &dest))
                    .map_err(|_| first)
            })
            .map_err(|e| at_path(&dest, &e))?;
        Ok(dest)
    }

    /// 暂存 ＋ 上位；上位不成 ⇒ 删掉暂存件再回错。
    pub fn place(&self, name: &str, bytes: &[u8]) -> Result<PathBuf> {
        let staged = self.stage(name, bytes)?;
        self.put(name, &staged).inspect_err(|_| {
            remove_file(&staged).discard(Why::CleanupAfterFailure);
        })
    }

    /// 删落点目录里的一份（暂存件 · 挪开的旧版 · 落点本身）；本来就不在算删成了。
    pub fn remove(&self, name: &str) -> Result<()> {
        remove_if_there(&self.named(name)?)
    }
}

// ── 终端设备 ──────────────────────────────

/// 往终端设备（`/dev/pts/N` 一类）写一段控制序列；不新建、不把它认作本进程的控制终端（`O_NOCTTY`）。
/// 只给同层的控制台那一族用（改标题）。
#[cfg(target_os = "linux")]
#[expect(
    clippy::disallowed_methods,
    reason = "写盘只经本文件：终端设备不是盘上的文件，只写不建"
)]
pub(super) fn tty_write(tty: &Path, bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    const O_NOCTTY: i32 = 0o400;
    std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(O_NOCTTY)
        .open(tty)
        .and_then(|mut f| f.write_all(bytes))
}

// ── 日志目录 ──────────────────────────────

/// 建调用方给的那一层日志目录（缺的上层一并建，建出来只给本人；已在不动）。
pub fn log_dir(dir: &Path) -> Result<()> {
    private_dir(dir)
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/platform/own_fs_tests.rs"]
mod tests;
