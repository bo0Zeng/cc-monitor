//! Claude 数据目录下的路径与文件元信息 —— **合并前逐字对照过，四份/两份完全相同。**

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// `<claude_dir>/projects`。
///
/// U2 之前这个三行函数在 `fork_write.rs` / `history_query.rs` / `search_query.rs` /
/// `usage_query.rs` 里**各有一份逐字相同的副本**，**外加 `watcher.rs::watch_loop` 里一处内联的
/// `claude_dir.join("projects")`** —— 一共**五**处。它本身不会写错，真问题是
/// 「`projects` 这个目录名在五个地方各写一遍」：哪天 Claude 改了布局就要改五处，
/// 而漏掉一处不会有任何东西变红。
///
/// > 第五处是 Phase D 审计逮出来的：我第一版只收了四个 `fn projects_root`，
/// > **grep 函数名找不到内联的那处**，于是「合并去重」承诺的性质（改布局只改一处）根本没拿到。
///
/// 生产段现在是单一来源。测试夹具里仍有一批 `join("projects")` —— **那些刻意不收**：
/// 测试自己搭目录时应当写字面量，走生产 helper 就成了拿自己对自己断言。
pub(crate) fn projects_root(claude_dir: &Path) -> PathBuf {
    claude_dir.join("projects")
}

/// 文件 mtime 的毫秒时间戳；任何读取失败都退化成 `0`。
///
/// U2 之前 `history_query.rs` / `search_query.rs` 各有一份逐字相同的副本。
///
/// ⚠ **`unwrap_or(0)` 是既有语义，本次搬家原样保留**：读不到 mtime 的文件排序时会沉到最旧。
/// 那是不是好设计另说（读失败与「1970 年的文件」被混成同一个值），但 U2 是纯重构，
/// 行为逐字不变 —— 要改得单独立项、连同两个调用点的排序语义一起想。
pub(crate) fn mtime_ms(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
