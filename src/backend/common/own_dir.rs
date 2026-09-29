//! 〔HX1 · 4D · 主会话裁 HX1 拍板项 4〕**后端建自家目录的那一个函数**（`~/.cc-monitor` 与它底下后端自己的几层）。
//!
//! 出处：RK1 报备 §5.4「`~/.cc-monitor` 这一层目录若由中转第一个建出来，权限是 umask 默认（本机现打 0775）」→
//! 主会话裁「建自家目录收成一个小函数，五处都走它（0700、已存在不动）」。
//! 此前建那一层的有四份第四层模块各写一遍（`exit_policy` · `asset_catalog` · `relay/door` · `skill_ledger`）＋
//! 第三层的暂存区一份（`files_commit`），都按 umask 建。今天它们都调 [`ensure_private_dir`]；远端那一份（SFTP 部署建目录）
//! 不在这台机器的文件系统上、调不到本函数，用同一个 [`PRIVATE_DIR_MODE`]（`dial/sftp.rs::make_dir`）。
//!
//! # 它在 `readonly_guard` 第四层上（后端**自己的**状态，不是用户数据）
//!
//! 写口 [`ensure_private_dir`]：第四层别的模块调它不算越门（它们自己就是第四层）；第四层之外只有一扇门 ——
//! `control/files_commit.rs` 建暂存区那两层。动词只有「建目录」这一件（带着权限位一次建成，`DirBuilder`，只许住本模块）。

use std::path::Path;

/// 后端自家目录**建的那一下**给的权限位：只给本人。
pub const PRIVATE_DIR_MODE: u32 = 0o700;

/// 建**一层**自家目录，建的那一下就是 [`PRIVATE_DIR_MODE`]（unix：权限位随创建一起生效，没有「先按 umask 建出来再收窄」的那一段）。
/// **已在的不动**（那可能是用户自己设的）；父目录不在 ⇒ 照实报错（调用方都只建「家目录下那一层」，父目录本来就在）。
/// 非 unix：照常建（那边不是 unix 权限位这一问；目录的 ACL 按父目录继承）。
pub fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
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

#[cfg(test)]
#[path = "../../../tests/backend/common/own_dir_tests.rs"]
mod tests;
