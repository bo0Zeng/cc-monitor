//! Codex rollout 记录的后端侧解析（per-kind · 2D）。
//!
//! monitor 那份 `codex_record.rs` 搬进来之后（[`super::record`]：分类 ＋ 映射进渲染模型），信封助手只剩本模块这一份，
//! 两边都用它。
//! Codex 格式未文档、每几 minor churn → 宽容抽取、逐行不崩、未知/缺失安全默认、alias 归一 `turn_*`↔`task_*`。
//!
//! 记录信封（本机实测 codex-cli 0.144.6）：`{"timestamp","type","payload":{...}}`。顶层 `type` ∈
//! session_meta/turn_context/world_state/response_item/event_msg；后两者 `payload.type` 再细分。
//!
//! 本模块只留信封助手与会话定位（`resolve_codex_dir` · `sessions_root` · `session_meta_cwd` · `codex_sid_from_path`），
//! 读者都在 `agents/codex/`。轮次边沿与用量那几个函数今天零读者，删了（这一家今天不报轮次边沿）。

use serde_json::Value;
use std::path::{Path, PathBuf};

/// 解包信封 `{type, payload}` → `(顶层 type, payload)`。缺 type / payload 非对象 → `None`。
pub fn unwrap_envelope(v: &Value) -> Option<(&str, &Value)> {
    let top = v.get("type")?.as_str()?;
    let payload = v.get("payload").filter(|p| p.is_object())?;
    Some((top, payload))
}

/// payload 的 `type` 子判别（response_item/event_msg 用）。
pub(crate) fn payload_type(payload: &Value) -> Option<&str> {
    payload.get("type").and_then(Value::as_str)
}

/// alias 归一：`turn_started`→`task_started`、`turn_complete`→`task_complete`（EventMsg v1 别名，新旧
/// 版本都吃）。其它原样。
pub(crate) fn normalize_event(t: &str) -> &str {
    match t {
        "turn_started" => "task_started",
        "turn_complete" => "task_complete",
        other => other,
    }
}

/// 信封顶层 `timestamp`（turn-end uuid 回退键 / usage 归天）。缺 → None。
pub fn envelope_ts(v: &Value) -> Option<&str> {
    v.get("timestamp").and_then(Value::as_str)
}

// ─── DG5：usage（event_msg `token_count`）+ 会话定位 helpers（镜像 monitor F5 / adapter Codex 侧）───

/// `$CODEX_HOME`（优先、非空）| `~/.codex`。缺 HOME → None。**backend 在会话主机本地解**（同 monitor adapter）。
pub fn resolve_codex_dir() -> Option<PathBuf> {
    if let Some(h) = std::env::var_os("CODEX_HOME").filter(|h| !h.is_empty()) {
        return Some(PathBuf::from(h));
    }
    crate::platform::paths::home_dir().map(|h| h.join(".codex"))
}

/// Codex 会话记录根 `<codex_dir>/sessions`（日期分区树 `YYYY/MM/DD/rollout-*.jsonl` 在其下）。
pub fn sessions_root(codex_dir: &Path) -> PathBuf {
    codex_dir.join("sessions")
}

/// session_meta 的 `payload.cwd`（用量行 projectPath；Codex 无 cwd-项目目录）。
pub fn session_meta_cwd(v: &Value) -> Option<&str> {
    match unwrap_envelope(v) {
        Some(("session_meta", p)) => p.get("cwd").and_then(Value::as_str),
        _ => None,
    }
}

/// Codex sid = rollout 文件名的 UUID（`rollout-<ts>-<uuid>` → `<uuid>`）。**校验强度对齐 monitor
/// `adapter::codex_sid_from_rollout`（Phase D 审计修 parity 发散）**：须 `rollout-` 前缀 + 末 36 字符〔散文墓碑〕
/// 过 UUID 形校验，否则 `None`（→ 调用方跳过该文件，同 monitor，避免畸形名吐幽灵行）。末 36 用 `get`
/// （非字节切片）→ 非字符边界安全返 None、不 panic（比 monitor 的 `&rest[..]` 切片更稳）。
pub fn codex_sid_from_path(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let rest = stem.strip_prefix("rollout-")?;
    let uuid = rest.get(rest.len().checked_sub(36)?..)?;
    is_uuid(uuid).then(|| uuid.to_string())
}

/// UUID 形校验（`8-4-4-4-12` hex；对齐 monitor `adapter::is_uuid`）。
fn is_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(p, n)| p.len() == n && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/parse_tests.rs"]
mod tests;
