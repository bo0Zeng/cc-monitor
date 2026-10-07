//! observe 内部共享的文件元信息读取。

use std::path::Path;
use std::time::UNIX_EPOCH;

/// 文件 mtime 的毫秒时间戳；任何读取失败都退化成 `0`（排序时沉到最旧：读失败与「1970 年的文件」混成同一个值，要改得连同两个调用点的排序语义一起想）。
/// 住 observe：两个调用点（`history_query` / `search_query`）同属 observe，不够 `common/` 的「≥2 个上层用」门槛。
pub(crate) fn mtime_ms(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
