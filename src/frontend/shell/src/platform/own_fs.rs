//! **壳往盘上写的唯一一层**：按落点分组，选哪一组写在函数名上。
//!
//! - 自己的状态（[`OwnState`]）：monitor 数据目录（`CCM_DATA_DIR`，否则 `~/.cc-monitor/`）底下，设置 · 握手表 · 迁移记号这一类。
//!   根在 [`OwnState::issue`] 里取，方法只收相对名（没有 `..` · 根 · 盘符）⇒ 落不到根外。文件只给本人（0600），缺的目录建出来就是 0700。
//! - 自己的部署物（[`OwnBin`]）：数据目录底下的 `bin/`（终端里敲的 `ccm` 与文件窗口程序住这里）。方法只收落点目录里的一个名字；
//!   放一份 ＝ 先写 `.<名>.<pid>.partial`、置可执行位，再换名上位（Windows 上旧的正在跑 ⇒ 先挪成 `.<名>.<pid>.old`）。
//! - 终端设备（[`tty_write`]）：不是盘上的文件，往 `/dev/pts/N` 一类写控制序列。
//! - 日志目录（[`log_dir`]）：调用方给的那一层目录，只建目录。
//!
//! 用户的文件不在这里：经那台机器的后端写，或经 `host_core` 保留目标 ACL 的那一个替换。
//! `std::fs` 的写原语进了 `src/frontend/clippy.toml` 的禁用名单，放行只在本文件、各组那几个只做这一件事的小函数上。

use copy_core::copy_text;
use copy_core::said::Said;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// 写失败那一句（落点 ＋ 原因词，原话进复制详情）。
fn failed(at: &Path, e: &std::io::Error) -> Said {
    Said::with_raw(
        copy_text(
            "reason.fs.writeFailed",
            &[
                ("path", &at.display().to_string()),
                ("why", &copy_core::io_reason(e.kind())),
            ],
        ),
        e,
    )
}

/// 名字不是根底下的相对名 ⇒ 当成「参数不对」那一类写失败报。
fn not_relative(root: &Path, rel: &str) -> Said {
    failed(
        &root.join(rel),
        &std::io::Error::from(std::io::ErrorKind::InvalidInput),
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

/// 旁名序号：同一进程几个线程同时写同一份也不撞。
static SEQ: AtomicU64 = AtomicU64::new(0);

// ── 写原语：放行只在这几个小函数上 ──────────────────────────────

/// 新建一份只给本人的文件（已在 ⇒ 拒，`O_EXCL`）。
#[expect(
    clippy::disallowed_methods,
    reason = "写盘只经本文件：只给本人地新建一份"
)]
fn create_new_private(at: &Path) -> std::io::Result<std::fs::File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    super::fs::only_me_on_create(&mut opts);
    opts.open(at)
}

/// 建（或截断）一份只给本人的文件。
#[expect(
    clippy::disallowed_methods,
    reason = "写盘只经本文件：只给本人地建或截断一份"
)]
fn create_private(at: &Path) -> std::io::Result<std::fs::File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    super::fs::only_me_on_create(&mut opts);
    opts.open(at)
}

/// 换名（同一目录里，目标已在 ⇒ 覆盖）。
#[expect(
    clippy::disallowed_methods,
    reason = "写盘只经本文件：原子写与上位的换名"
)]
fn rename(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}

/// 删一份文件。
#[expect(clippy::disallowed_methods, reason = "写盘只经本文件：删自己的一份")]
fn remove_file(at: &Path) -> std::io::Result<()> {
    std::fs::remove_file(at)
}

/// 建一层目录（缺的上层一并建；已在不动）。
#[expect(
    clippy::disallowed_methods,
    reason = "写盘只经本文件：日志目录只建目录"
)]
fn create_dir_all(at: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(at)
}

/// 写满 → 落盘 → 换名到 `at`；中途失败删掉自己建的旁名。
fn write_atomic(at: &Path, bytes: &[u8]) -> Result<(), Said> {
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
    let wrote = create_new_private(&tmp).and_then(|mut f| {
        f.write_all(bytes)?;
        f.sync_all()
    });
    if let Err(e) = wrote {
        remove_file(&tmp).ok();
        return Err(failed(&tmp, &e));
    }
    rename(&tmp, at).map_err(|e| {
        remove_file(&tmp).ok();
        failed(at, &e)
    })
}

/// 删一份；本来就不在算删成了。
fn remove_if_there(at: &Path) -> Result<(), Said> {
    match remove_file(at) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(failed(at, &e)),
        _ => Ok(()),
    }
}

// ── 自己的状态 ──────────────────────────────

/// 写 monitor 自己的状态的句柄：根是数据目录。造只经 [`OwnState::issue`]（判据另有 `at`）。
#[derive(Clone, Debug)]
pub struct OwnState {
    root: PathBuf,
}

impl OwnState {
    /// 照这台的数据目录造；数据目录说不出（没有家目录 · `CCM_DATA_DIR` 不是绝对路径）⇒ `None`，调用方照「无主目录」报。
    pub fn issue() -> Option<OwnState> {
        crate::config::resolve_monitor_data_dir().map(OwnState::at)
    }

    fn at(root: PathBuf) -> OwnState {
        OwnState { root }
    }

    /// 根底下一条相对名的路径（读的那一侧也照它拼）；不是相对名 ⇒ 拒。
    pub fn path(&self, rel: &str) -> Result<PathBuf, Said> {
        if !relative_ok(rel) {
            return Err(not_relative(&self.root, rel));
        }
        Ok(self.root.join(rel))
    }

    /// 建一层目录（缺的上层一并建，建出来只给本人；已在不动）。回那一层的路径。
    pub fn dir(&self, rel: &str) -> Result<PathBuf, Said> {
        let at = self.path(rel)?;
        super::fs::ensure_private_dir(&at)?;
        Ok(at)
    }

    /// 所在目录缺 ⇒ 先建（只给本人）。
    fn parent_ready(at: &Path) -> Result<(), Said> {
        match at.parent() {
            Some(dir) => super::fs::ensure_private_dir(dir),
            None => Ok(()),
        }
    }

    /// 原子写一份（只给本人）；所在目录缺 ⇒ 先建。
    pub fn write(&self, rel: &str, bytes: &[u8]) -> Result<(), Said> {
        let at = self.path(rel)?;
        Self::parent_ready(&at)?;
        write_atomic(&at, bytes)
    }

    /// 建（或截断）一份只给本人的文件，交回句柄（锁旁件这一类：要的是那份文件本身，不是内容）。
    pub fn create(&self, rel: &str) -> Result<std::fs::File, Said> {
        let at = self.path(rel)?;
        Self::parent_ready(&at)?;
        create_private(&at).map_err(|e| failed(&at, &e))
    }

    /// 删一份；本来就不在算删成了。
    pub fn remove(&self, rel: &str) -> Result<(), Said> {
        remove_if_there(&self.path(rel)?)
    }
}

// ── 自己的部署物 ──────────────────────────────

/// 往数据目录的 `bin/` 放程序的句柄。造只经 [`OwnBin::issue`]（判据另有 `at`）。
#[derive(Clone, Debug)]
pub struct OwnBin {
    dir: PathBuf,
}

impl OwnBin {
    /// 落点目录 ＝ 数据目录底下的 `bin/`；数据目录说不出 ⇒ `None`。
    pub fn issue() -> Option<OwnBin> {
        crate::config::resolve_monitor_data_dir().map(|d| OwnBin::at(d.join("bin")))
    }

    fn at(dir: PathBuf) -> OwnBin {
        OwnBin { dir }
    }

    /// 落点目录（读的那一侧：比盘上那份、列残件）。
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn named(&self, name: &str) -> Result<PathBuf, Said> {
        if !single_name_ok(name) {
            return Err(not_relative(&self.dir, name));
        }
        Ok(self.dir.join(name))
    }

    /// 写一份暂存件 `.<名>.<pid>.partial`（只给本人、已置可执行位），回它的路径；还没上位。
    pub fn stage(&self, name: &str, bytes: &[u8]) -> Result<PathBuf, Said> {
        self.named(name)?;
        super::fs::ensure_private_dir(&self.dir)?;
        let tmp = self
            .dir
            .join(format!(".{name}.{}.partial", std::process::id()));
        let wrote = create_private(&tmp).and_then(|mut f| {
            f.write_all(bytes)?;
            f.sync_all()
        });
        if let Err(e) = wrote {
            remove_file(&tmp).ok();
            return Err(failed(&tmp, &e));
        }
        if let Err(s) = super::fs::make_executable(&tmp) {
            remove_file(&tmp).ok();
            return Err(s);
        }
        Ok(tmp)
    }

    /// 暂存件上位成 `<名>`，回落点。换不上（Windows 上旧的正在跑）⇒ 先把旧的挪成 `.<名>.<pid>.old` 再上位；
    /// 挪也挪不开 ⇒ 回第一次的错，暂存件留给调用方收。
    pub fn put(&self, name: &str, staged: &Path) -> Result<PathBuf, Said> {
        let dest = self.named(name)?;
        rename(staged, &dest)
            .or_else(|first| {
                let aside = self.dir.join(format!(".{name}.{}.old", std::process::id()));
                rename(&dest, &aside)
                    .and_then(|()| rename(staged, &dest))
                    .map_err(|_| first)
            })
            .map_err(|e| failed(&dest, &e))?;
        Ok(dest)
    }

    /// 暂存 ＋ 上位；上位不成 ⇒ 删掉暂存件再回错。
    pub fn place(&self, name: &str, bytes: &[u8]) -> Result<PathBuf, Said> {
        let staged = self.stage(name, bytes)?;
        self.put(name, &staged).inspect_err(|_| {
            remove_file(&staged).ok();
        })
    }

    /// 删落点目录里的一份（暂存件 · 挪开的旧版 · 落点本身）；本来就不在算删成了。
    pub fn remove(&self, name: &str) -> Result<(), Said> {
        remove_if_there(&self.named(name)?)
    }
}

// ── 终端设备 ──────────────────────────────

/// 往终端设备（`/dev/pts/N` 一类）写一段控制序列；不新建、不把它认作本进程的控制终端（`O_NOCTTY`）。
#[cfg(target_os = "linux")]
#[expect(
    clippy::disallowed_methods,
    reason = "写盘只经本文件：终端设备不是盘上的文件，只写不建"
)]
pub fn tty_write(tty: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    const O_NOCTTY: i32 = 0o400;
    std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(O_NOCTTY)
        .open(tty)
        .and_then(|mut f| f.write_all(bytes))
}

// ── 日志目录 ──────────────────────────────

/// 建调用方给的那一层日志目录（缺的上层一并建；已在不动）。
pub fn log_dir(dir: &Path) -> Result<(), Said> {
    create_dir_all(dir).map_err(|e| failed(dir, &e))
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/platform/own_fs_tests.rs"]
mod tests;
