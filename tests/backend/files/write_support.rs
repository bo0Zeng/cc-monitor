//! [`crate::files::write::test_support`] 的体：写面判据（挂在上层的命令那一层）够到的那几样内部项，各包一层原样转交。
//! 只用本包 `[dependencies]` 里有的 crate。

use super::{Face, Perms, WriteRefusal};
use std::path::{Path, PathBuf};

/// [`super::rename_entry_as`]：`cross` 让判据走跨盘那一支（不动盘、当那一下系统回了 `CrossesDevices`）。
pub fn rename_entry_as(
    face: &Face,
    root: &Path,
    from: &Path,
    to: &Path,
    between: &mut dyn FnMut(),
    cross: bool,
) -> Result<PathBuf, WriteRefusal> {
    super::rename_entry_as(face, root, from, to, between, cross)
}

/// 本进程到此刻造过几个旁名序号（旁名是 `0..` 这个数）。
pub fn swap_seq() -> u64 {
    super::SWAP_SEQ.load(std::sync::atomic::Ordering::Relaxed)
}

/// [`super::keep_mode`]：两个动作由判据交进来。
pub fn keep_mode(
    bak: &Path,
    perms: Perms,
    chmod: impl FnOnce(&Path, Perms) -> std::io::Result<()>,
    remove: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<(), WriteRefusal> {
    super::keep_mode(bak, perms, chmod, remove)
}
