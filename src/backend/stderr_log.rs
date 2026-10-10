//! 脱离常驻那条载体的后端：stderr 落进一份有上限、滚动的文件（设置页「日志」里看得到）。
//! 脱离时 stdio 全 null，而全部 SSH 与本机中转都在这个进程里 ⇒ host key 警告、中转起不来的原因、watch 失败、panic 不落盘就没人听得见。
//!
//! # 形状
//!
//! - 谁决定写不写、写哪：宿主交 [`ENV`]（monitor 只在起脱离那条载体时交：`<monitor 数据目录>/logs/backend/stderr.log`，那一层目录由它建好）。
//!   没交 ⇒ 什么都不做（stdio 载体的 stderr 已经进 monitor 滚动日志；远端经 SSH exec 起的流模式没人交）。
//! - 接：`O_EXCL` 新建那份文件，fd 2 直接指过去（`platform::stderr_fd`）⇒ 本进程此后一切写 stderr 的（`tracing` · `eprintln!` · panic ·
//!   继承 stderr 的子进程）都同步落盘，进程 `exit` 那一刻前面的话一个字都不丢（没有线程、没有搬运）。
//! - 上限 ＋ 滚动：`tracing` 每写一行之前看一眼 fd 2 那份多长（[`stderr_writer`]，`main.rs` 装给 `tracing` 的写者）；过了 [`CAP_BYTES`] ⇒
//!   当前那份原子挪成旧的（盖掉上一份旧的）、`O_EXCL` 新建一份、fd 2 换过去，新那份第一行说清「上一份挪去了哪、再早的那一份丢了」。
//!   盘上恒 ≤ 两份。起来那一刻已有当前那份 ⇒ 先挪成旧的。两行 `tracing` 之间别人写的字节不触发检查 ⇒ 一份可以略超上限，不会无界。
//! - 换不过去不拖垮后端：建不了文件 / 换不了 fd ⇒ stderr 原样，照常服务。
//!
//! 后端自有状态（`readonly_guard` 第四层）：动词只用 `O_EXCL` 新建 · 原子挪（不建目录、不截断、不追加），对外口只从 `main.rs` 进。
//! 买不到：远端后端的 stderr · 真 Windows（非 unix 臂回 `Failed`）。

use copy_core::copy_text;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 宿主交的那一格：当前那份文件的**完整路径**。缺席 / 空白 ⇒ 不接。
pub const ENV: &str = crate::platform::child_env::STDERR_LOG;

/// 每份的字节上限（当前那份 ＋ 旧的那份 ⇒ 盘上 ≈ 两倍；按 `tracing` 行检查，见模块头注）。
pub const CAP_BYTES: u64 = 4 << 20;

/// 当前那份的路径 ⇒ 旧的那份的路径（`stderr.log` ⇒ `stderr.old.log`）。
pub(crate) fn old_path_of(cur: &Path) -> PathBuf {
    cur.with_extension("old.log")
}

/// 滚过一次之后，新那份的第一行。
pub(crate) fn roll_note(old: &Path) -> String {
    format!(
        "[stderr-log] 上一份挪成 {}（每份上限 {CAP_BYTES} 字节）；再早的那一份丢了\n",
        old.display()
    )
}

/// fd 2 那一头（生产 = 本进程真的 fd 2；判据用一个记账替身）。
pub(crate) trait Target: Send {
    /// 此刻那份多长（问不到 ⇒ `None`，不滚）。
    fn len(&self) -> Option<u64>;
    /// 此后的输出改落 `f` 那份。
    fn point_at(&mut self, f: &std::fs::File) -> Result<(), String>;
}

/// 生产那一头：本进程的 fd 2。
pub(crate) struct Fd2;

impl Target for Fd2 {
    fn len(&self) -> Option<u64> {
        crate::platform::stderr_fd::stderr_len()
    }
    fn point_at(&mut self, f: &std::fs::File) -> Result<(), String> {
        crate::platform::stderr_fd::point_stderr_at(f)
    }
}

/// 管那两份文件、决定什么时候滚的那一半。
pub(crate) struct Roller<T: Target> {
    cur: PathBuf,
    old: PathBuf,
    cap: u64,
    target: T,
}

impl<T: Target> Roller<T> {
    /// 起：已有当前那份 ⇒ 挪成旧的；新建一份当前的、把输出指过去。建不了 / 指不过去 ⇒ `Err`（输出原样）。
    pub(crate) fn start(cur: PathBuf, cap: u64, target: T) -> Result<Roller<T>, String> {
        let old = old_path_of(&cur);
        let mut r = Roller {
            cur,
            old,
            cap,
            target,
        };
        r.fresh()?;
        Ok(r)
    }

    /// 当前那份挪成旧的（不在就不挪），`O_EXCL` 新建一份当前的；真挪了一份 ⇒ 新那份第一行说清；把输出指过去。
    fn fresh(&mut self) -> Result<(), String> {
        let moved = match std::fs::rename(&self.cur, &self.old) {
            Ok(()) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            // 这几句只进 stderr 那一行（[`Installed::said`]）：那一句 ＋ 原话一起（`Said::logged`）。
            Err(e) => {
                return Err(crate::common::said::Said::with_raw(
                    copy_text(
                        "beStderrLog.roll.moveFailed",
                        &[
                            ("path", &self.cur.display().to_string()),
                            ("why", &copy_core::io_reason(e.kind())),
                        ],
                    ),
                    e,
                )
                .logged())
            }
        };
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.cur)
            .map_err(|e| {
                crate::common::said::Said::with_raw(
                    copy_text(
                        "beStderrLog.roll.createFailed",
                        &[
                            ("path", &self.cur.display().to_string()),
                            ("why", &copy_core::io_reason(e.kind())),
                        ],
                    ),
                    e,
                )
                .logged()
            })?;
        if moved {
            let _ = f.write_all(roll_note(&self.old).as_bytes());
        }
        self.target.point_at(&f)
    }

    /// 看一眼：过了上限就滚。滚不成 ⇒ 输出留在原来那份上（下一次再试）。
    pub(crate) fn check(&mut self) {
        if self.target.len().is_some_and(|n| n > self.cap) {
            let _ = self.fresh();
        }
    }
}

/// 装上之后那一个（本进程一份）。`None` = 没装（没被交路径 / 装不上）。
static ACTIVE: Mutex<Option<Roller<Fd2>>> = Mutex::new(None);

/// 装上之后的结局（给宿主那一句日志；装上了的话它本身落进那份文件）。
#[derive(Debug, PartialEq, Eq)]
pub enum Installed {
    /// 没被交路径 ⇒ stderr 原样不动。
    NotAsked,
    /// 接好了：此后 stderr 落在这份文件里。
    Logging(PathBuf),
    /// 被交了路径但接不上 ⇒ stderr 原样不动。
    Failed(String),
}

impl Installed {
    pub fn said(&self) -> String {
        match self {
            Installed::NotAsked => "[stderr] 没被交日志路径 ⇒ stderr 原样".to_string(),
            Installed::Logging(p) => format!(
                "[stderr] 本进程的输出落在 {}（每份上限 {} 字节，留一份旧的）",
                p.display(),
                CAP_BYTES
            ),
            Installed::Failed(why) => format!("[stderr] 被交了日志路径但接不上：{why}"),
        }
    }
}

/// 宿主交了 [`ENV`] ⇒ 把 fd 2 指到那份文件上。只从 `main.rs` 流模式那一处进（第四层那扇门）。
pub fn install_from_env(get: &dyn Fn(&str) -> Option<String>) -> Installed {
    let Some(path) = get(ENV).filter(|v| !v.trim().is_empty()) else {
        return Installed::NotAsked;
    };
    let cur = PathBuf::from(path);
    match Roller::start(cur.clone(), CAP_BYTES, Fd2) {
        Ok(r) => {
            *ACTIVE.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
            Installed::Logging(cur)
        }
        Err(e) => Installed::Failed(e),
    }
}

/// `tracing` 的写者（`main.rs` 装给它）：每写一行之前看一眼要不要滚，然后照旧写 stderr。
/// 没装 ⇒ 就是 `std::io::stderr()`。别的线程正在滚 ⇒ 这一行不等它（`try_lock`），下一行再看。
pub fn stderr_writer() -> std::io::Stderr {
    if let Ok(mut g) = ACTIVE.try_lock() {
        if let Some(r) = g.as_mut() {
            r.check();
        }
    }
    std::io::stderr()
}

#[cfg(test)]
#[path = "../../tests/backend/stderr_log_tests.rs"]
mod tests;
