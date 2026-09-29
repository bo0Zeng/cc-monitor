//! Codex rollout 记录的后端侧解析（per-kind · 2D）。
//!
//! 〔MOD〕monitor 那份 `codex_record.rs` 搬进来之后（[`super::record`]：分类 ＋ 映射进渲染模型），信封助手只剩本模块这一份，
//! 两边都用它。与 aterm 的 CodexRecordParser/CodexTurnEndDetector **golden-parity**（同 Claude 那一家 `turn.rs` 套路）。
//! Codex 格式未文档、每几 minor churn → 宽容抽取、逐行不崩、未知/缺失安全默认、alias 归一 `turn_*`↔`task_*`。
//!
//! 记录信封（本机实测 codex-cli 0.144.6）：`{"timestamp","type","payload":{...}}`。顶层 `type` ∈
//! session_meta/turn_context/world_state/response_item/event_msg；后两者 `payload.type` 再细分。
//!
//! **本模块范围（渐进接线）**：DG4 = turn-end 边沿（event_msg task_complete/turn_complete → uuid=turn_id
//! 缺→envelope timestamp 回退）。DG5（usage：token_count）复用本模块的信封助手，接线时加。

// 〔AR1 订正〕DG5 那一拍本模块过半函数由用量聚合轴接线；那条轴随 `设计/50` 删了之后，发现那几个
// （`resolve_codex_dir` · `sessions_root` · `session_meta_cwd` · `codex_sid_from_path` · `unwrap_envelope`）
// 的读者是 `agents/codex/`，token 那几个（`is_token_count` · `last_token_delta` · `turn_context_model`）今天在
// 后端生产树里**零读者**，按 `设计/50 §3` 留给 codex 专项，别顺手清。turn-end 4 函数仍 staged（consumer
// = DG1 per-kind process_jsonl 派发 / DG3 wire），带 per-fn allow（Phase D 审计：blanket 会静默吞掉将来真死代码）。

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

/// event_msg 的 `payload.turn_id`（turn-end/started/aborted 用）。缺 → None。
#[allow(dead_code)] // staged：turn-end consumer（DG1/DG3）接线后摘。
pub(crate) fn turn_id(v: &Value) -> Option<&str> {
    unwrap_envelope(v)?.1.get("turn_id").and_then(Value::as_str)
}

/// 一条 Codex 记录是否为 **turn-end 边沿**：event_msg 且 payload.type（alias 归一后）== `task_complete`。
/// **golden-parity aterm `CodexTurnEndDetector`**。`turn_aborted` 明确**不算**（aterm 决策：中止轮静默
/// 不发 TurnEnd）。非 event_msg / 其它子型 / 缺失 → false（安全默认，不崩）。
#[allow(dead_code)] // staged：consumer = DG1 per-kind process_jsonl 派发 / DG3 wire，接线后摘。
pub fn is_codex_turn_end(v: &Value) -> bool {
    match unwrap_envelope(v) {
        Some(("event_msg", payload)) => {
            payload_type(payload).map(normalize_event) == Some("task_complete")
        }
        _ => false,
    }
}

/// turn-end 边沿的 **uuid**（= 客户端 dedup 键；喂 `Frame::TurnEnd`）。**Codex：turn_id，缺→envelope
/// timestamp 回退**（aterm trap③：某版/v1 alias 路径缺 turn_id，null 被当"非 end"会漏报最新完成轮）。
/// 两者皆缺 → None（无可去重键、不发帧）。非 turn-end → None。**与 aterm CodexTurnEndDetector 一字同**
/// （2026-07-19 双端逐字对拍 verbatim-equivalent）。
/// **共同待观察**（两端同步记）：真机 31/31 用 `turn_id`；若某版改字段名（如 `task_id`），两端都只读
/// `turn_id` → 都安全回退 envelope timestamp（非空键、不漏帧），届时两端同步加新键别名。
#[allow(dead_code)] // staged：consumer = DG1 per-kind process_jsonl 派发 / DG3 wire，接线后摘。
pub fn codex_turn_end_uuid(v: &Value) -> Option<&str> {
    if !is_codex_turn_end(v) {
        return None;
    }
    turn_id(v).or_else(|| envelope_ts(v))
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

/// 一条记录是否 event_msg `token_count`（用量事件）。
pub fn is_token_count(v: &Value) -> bool {
    matches!(unwrap_envelope(v), Some(("event_msg", p)) if payload_type(p) == Some("token_count"))
}

/// token_count 的 `payload.info.last_token_usage` 映射成 Claude 口径增量。缺→None。
///
/// ⚠ 字段名与减法**不在这里** —— 它们住 `token::codex_delta`（唯一权威源）。
/// 本函数只负责「从信封里把那个子对象挖出来」。这一层与 monitor 的
/// `codex_record.rs` 各自挖各自的（信封形状本就是两侧各读各的文件），
/// 但**口径必须同一份**：收口前两侧各写一遍减法、逐字相同、无判据钉住。
/// **实测 total_token_usage 严格单调、final==Σlast**、且偶发不可靠（某会话首事件 total=0）→ SUM last 更稳。
pub fn last_token_delta(v: &Value) -> Option<super::token::CodexDelta> {
    let usage = unwrap_envelope(v)?.1.get("info")?.get("last_token_usage")?;
    Some(super::token::codex_delta(usage))
}

/// turn_context 的 `payload.model`（用量按模型归桶；session_meta 只有 model_provider、model 在 turn_context）。
pub fn turn_context_model(v: &Value) -> Option<&str> {
    match unwrap_envelope(v) {
        Some(("turn_context", p)) => p.get("model").and_then(Value::as_str),
        _ => None,
    }
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
