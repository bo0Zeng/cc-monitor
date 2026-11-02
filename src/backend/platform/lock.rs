//! **后端自有状态文件的跨进程锁** —— 后端这一侧只留造那一句；锁本身住 `platform_core::dir::hold`（monitor 那一把同一份）。
//!
//! # 它治什么
//!
//! 后端自有状态（写经 `platform::fs::Own`）每一份都是「读盘 → 改一格 → 临时文件 → 原子挪」。
//! 只靠进程内 `Mutex` 串不起它们：两个后端进程（本机常驻那一个 ＋ 一次性 CLI；
//! 远端上两台机器的 monitor 各自那条流）同时写同一份 ⇒ 后写的整份盖掉先写的。资产目录首建时还会生出两个 `self` id，
//! 输的那个成永不消失的幽灵机器；skill 装记录丢一条 ⇒ 那一趟装的文件从此卸不掉。
//!
//! # 锁住的是**那份状态文件所在的目录**，不是文件
//!
//! 文件本身每写一次就被原子挪换成一个新 inode ⇒ 锁文件 = 锁一个随时会被换掉的东西；锁目录只要**只读打开**它
//! ⇒ `Own` 的闭集一个写动词都不加。两臂（unix `flock(LOCK_EX)` · Windows 命名互斥量，同一线程可重入 ⇒ 调用方不许嵌套）
//! 与「等锁不是定时器」的边界写在 `platform_core::dir` 的头注里。

use copy_core::copy_text;
use copy_core::said::Said;
use platform_core::dir::{LockFail, LockStage};
use std::path::Path;

/// 持着的那把锁。落地即放。
pub use platform_core::dir::DirLock;

/// 拿 `dir` 那把锁（阻塞到拿到为止）。`dir` 必须已经在（调用方先建那一层 —— 建目录是 `Own` 的方法，本层不建）。
/// 拿不到 ⇒ 那一句（带原因词）＋ 系统原话。
pub fn hold(dir: &Path) -> Result<DirLock, Said> {
    platform_core::dir::hold(dir).map_err(|LockFail { stage, err }| {
        let shown = dir.display().to_string();
        let why = copy_core::io_reason(err.kind());
        let said = match stage {
            LockStage::Open => copy_text(
                "rsPlatformFs.lock.openFailed",
                &[("dir", &shown), ("why", &why)],
            ),
            LockStage::Lock => copy_text(
                "bePlatformLock.hold.lockFailed",
                &[("dir", &shown), ("why", &why)],
            ),
            LockStage::Wait => copy_text("bePlatformLock.hold.waitFailed", &[("dir", &shown)]),
        };
        Said::with_raw(said, &err)
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/platform/lock_tests.rs"]
mod tests;
