//! 没有 `/proc` 与 inotify 的平台（Windows · macOS 等）：一个诚实的空壳。
//!
//! [`writers_of`] 答 `None` ＝ 判不了（不是「没人在写」：那会把活会话报成没有）；[`watch_opens`] 不挂、答 `None`。
//! 调用方据此不宣告靠「开着即活」判活的那一类会话，并在日志里说一次。

use super::{OpenBatch, Writers};
use std::path::Path;

/// 本平台上没有这道耳朵 ⇒ 这个类型造不出来（[`watch_opens`] 恒答 `None`）。
pub(crate) enum OpenEar {}

/// 本平台不比路径（[`writers_of`] 判不了）：原样。
pub(crate) fn absolute(root: &Path) -> std::path::PathBuf {
    root.to_path_buf()
}

/// 见模块头注：判不了。
pub(crate) fn writers_of(wanted: &dyn Fn(&Path) -> bool) -> Option<Writers> {
    let _ = wanted;
    None
}

/// 见模块头注：不挂。
pub(crate) fn watch_opens(
    root: &Path,
    on_batch: impl FnMut(OpenBatch) -> bool + Send + 'static,
) -> Option<OpenEar> {
    let _ = on_batch;
    tracing::error!(
        "本平台认不出谁开着哪份文件写（{}）—— 靠「开着即活」判活的会话不宣告",
        root.display()
    );
    None
}
