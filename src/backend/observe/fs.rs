//! observe 内部共享的文件元信息读取。

use std::path::Path;
use std::time::UNIX_EPOCH;

/// 文件 mtime 的毫秒时间戳；任何读取失败都退化成 `0`。
///
/// # 它为什么从 `common/` 搬到这里（U3）
///
/// U2 把它放进 `common/`，而 `common/mod.rs` 的门槛第一条是「**≥2 个上层用**（按**层**数）」。
/// 当时 `observe/`/`control/` 还不存在，这条判不了，我**如实登记了「U3 必须复查」**而不是含糊过去。
///
/// U3 建出两层后一查：两个调用点（`history_query` / `search_query`）**同属 observe**。
/// ⇒ 门槛不成立，搬回 observe 内部共享。
///
/// **这是我自己定的规则第一次要求我改自己的代码。** `common/` 的门槛只有在「写规则的人
/// 也照它办」时才有约束力 —— 留在 common/ 并说「反正 control 将来可能用」，
/// 正是那份门槛里被逐字禁掉的那句话。
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
