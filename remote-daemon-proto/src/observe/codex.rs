//! Codex rollout 记录的 daemon 侧解析（per-kind · 2D）。
//!
//! **daemon↔monitor 不共享代码**（deliberate）：本模块在 `serde_json::Value` 上**独立重镜像** monitor
//! `codex_record.rs` 的防御抽取，与 aterm 的 CodexRecordParser/CodexTurnEndDetector **golden-parity**
//! （同 `turn_detect`/`usage_query` 套路）。Codex 格式未文档、每几 minor churn → 宽容抽取、逐行不崩、
//! 未知/缺失安全默认、alias 归一 `turn_*`↔`task_*`。
//!
//! 记录信封（本机实测 codex-cli 0.144.6）：`{"timestamp","type","payload":{...}}`。顶层 `type` ∈
//! session_meta/turn_context/world_state/response_item/event_msg；后两者 `payload.type` 再细分。
//!
//! **本模块范围（渐进接线）**：DG4 = turn-end 边沿（event_msg task_complete/turn_complete → uuid=turn_id
//! 缺→envelope timestamp 回退）。DG5（usage：token_count）复用本模块的信封助手，接线时加。

// DG5 起本模块过半函数已被 usage_query 接线（发现/用量 helpers）；仅 turn-end 4 函数仍 staged（consumer
// = DG1 per-kind process_jsonl 派发 / DG3 wire），故 blanket 模块 allow 收窄为那 4 个上的 per-fn allow
// （Phase D 审计建议：blanket 会静默吞掉将来真死代码）。

use serde_json::Value;
use std::path::{Path, PathBuf};

/// 解包信封 `{type, payload}` → `(顶层 type, payload)`。缺 type / payload 非对象 → `None`。
/// 与 monitor `codex_record::unwrap_envelope` 同语义。
pub fn unwrap_envelope(v: &Value) -> Option<(&str, &Value)> {
    let top = v.get("type")?.as_str()?;
    let payload = v.get("payload").filter(|p| p.is_object())?;
    Some((top, payload))
}

/// payload 的 `type` 子判别（response_item/event_msg 用）。
fn payload_type(payload: &Value) -> Option<&str> {
    payload.get("type").and_then(Value::as_str)
}

/// alias 归一：`turn_started`→`task_started`、`turn_complete`→`task_complete`（EventMsg v1 别名，新旧
/// 版本都吃）。其它原样。与 monitor `codex_record::normalize_event` 同。
#[allow(dead_code)] // staged：仅 turn-end（is_codex_turn_end）用；consumer DG1/DG3 接线后摘。
fn normalize_event(t: &str) -> &str {
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
fn turn_id(v: &Value) -> Option<&str> {
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

/// `$CODEX_HOME`（优先、非空）| `~/.codex`。缺 HOME → None。**daemon 在会话主机本地解**（同 monitor adapter）。
pub fn resolve_codex_dir() -> Option<PathBuf> {
    if let Some(h) = std::env::var_os("CODEX_HOME").filter(|h| !h.is_empty()) {
        return Some(PathBuf::from(h));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".codex"))
}

/// Codex 会话记录根 `<codex_dir>/sessions`（日期分区树 `YYYY/MM/DD/rollout-*.jsonl` 在其下）。
pub fn sessions_root(codex_dir: &Path) -> PathBuf {
    codex_dir.join("sessions")
}

/// 一条记录是否 event_msg `token_count`（用量事件）。
pub fn is_token_count(v: &Value) -> bool {
    matches!(unwrap_envelope(v), Some(("event_msg", p)) if payload_type(p) == Some("token_count"))
}

/// token_count 的 `payload.info.last_token_usage` 三元组 `(input_tokens, cached_input_tokens, output_tokens)`。
/// 缺→None。**Codex `input_tokens` 含 cached**——映射时 `input−cached` 防与 cacheRead 重复计（见 usage_query）。
/// **实测 total_token_usage 严格单调、final==Σlast**、且偶发不可靠（某会话首事件 total=0）→ SUM last 更稳。
pub fn last_token_usage_fields(v: &Value) -> Option<(u64, u64, u64)> {
    let usage = unwrap_envelope(v)?.1.get("info")?.get("last_token_usage")?;
    let g = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
    Some((
        g("input_tokens"),
        g("cached_input_tokens"),
        g("output_tokens"),
    ))
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
/// `adapter::codex_sid_from_rollout`（Phase D 审计修 parity 发散）**：须 `rollout-` 前缀 + 末 36 字符
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
mod tests {
    use super::*;
    use serde_json::json;

    fn event(sub: &str, extra: Value) -> Value {
        let mut payload = json!({ "type": sub });
        if let (Some(o), Some(e)) = (payload.as_object_mut(), extra.as_object()) {
            for (k, val) in e {
                o.insert(k.clone(), val.clone());
            }
        }
        json!({ "timestamp": "2026-07-19T08:00:00Z", "type": "event_msg", "payload": payload })
    }

    /// 基准正例：task_complete + turn_id → turn-end，uuid=turn_id。
    #[test]
    fn task_complete_is_turn_end_uuid_is_turn_id() {
        let v = event("task_complete", json!({ "turn_id": "t-1" }));
        assert!(is_codex_turn_end(&v));
        assert_eq!(codex_turn_end_uuid(&v), Some("t-1"));
    }

    /// v1 alias：turn_complete 归一为 task_complete → 同样是 turn-end。
    #[test]
    fn turn_complete_alias_is_turn_end() {
        let v = event("turn_complete", json!({ "turn_id": "t-2" }));
        assert!(is_codex_turn_end(&v));
        assert_eq!(codex_turn_end_uuid(&v), Some("t-2"));
    }

    /// trap③ 回退：turn_id 缺 → uuid 回退到 envelope timestamp（否则漏报最新完成轮）。
    #[test]
    fn missing_turn_id_falls_back_to_envelope_timestamp() {
        let v = event("task_complete", json!({}));
        assert!(is_codex_turn_end(&v));
        assert_eq!(codex_turn_end_uuid(&v), Some("2026-07-19T08:00:00Z"));
    }

    /// turn_aborted 明确不算 turn-end（aterm 决策：中止轮静默不发）。
    #[test]
    fn turn_aborted_is_not_turn_end() {
        let v = event("turn_aborted", json!({ "turn_id": "t-3" }));
        assert!(!is_codex_turn_end(&v));
        assert_eq!(codex_turn_end_uuid(&v), None);
    }

    /// 其它 event_msg 子型（token_count/task_started/agent_message…）非 turn-end。
    #[test]
    fn other_events_are_not_turn_end() {
        for sub in [
            "token_count",
            "task_started",
            "turn_started",
            "agent_message",
            "user_message",
        ] {
            assert!(
                !is_codex_turn_end(&event(sub, json!({}))),
                "{sub} 不该是 turn-end"
            );
        }
    }

    /// 非 event_msg 顶层（response_item/session_meta/…）+ 坏信封 → 非 turn-end、不崩。
    #[test]
    fn non_event_and_malformed_are_not_turn_end() {
        assert!(!is_codex_turn_end(&json!({
            "type": "response_item", "payload": { "type": "message", "role": "assistant" }
        })));
        assert!(!is_codex_turn_end(
            &json!({ "type": "session_meta", "payload": { "cwd": "/p" } })
        ));
        // 坏信封：无 payload / payload 非对象 / 无 type。
        assert!(!is_codex_turn_end(&json!({ "type": "event_msg" })));
        assert!(!is_codex_turn_end(
            &json!({ "type": "event_msg", "payload": "x" })
        ));
        assert!(!is_codex_turn_end(
            &json!({ "payload": { "type": "task_complete" } })
        ));
        assert!(!is_codex_turn_end(&json!("not even an object")));
    }

    /// **不是** Claude turn-end 路：Claude 的 assistant+end_turn 形状在 Codex 探测下 → false
    /// （per-kind 隔离，两路各判各的）。
    #[test]
    fn claude_shape_is_not_codex_turn_end() {
        let claude = json!({
            "type": "assistant", "uuid": "u", "message": { "stop_reason": "end_turn" }
        });
        assert!(!is_codex_turn_end(&claude));
    }

    /// Phase D 审计修：sid 提取校验对齐 monitor（rollout- 前缀 + 末36 UUID 形），畸形名 → None（跳过、
    /// 不吐幽灵行），合法名 → 末36 UUID。补 daemon 侧此前缺的畸形名覆盖。
    #[test]
    fn codex_sid_from_path_validates_like_monitor() {
        let p = |n: &str| PathBuf::from(n);
        assert_eq!(
            codex_sid_from_path(&p(
                "rollout-2026-07-18T08-00-00-019f75dd-875c-7c81-9eda-32f866b2c60f.jsonl"
            ))
            .as_deref(),
            Some("019f75dd-875c-7c81-9eda-32f866b2c60f")
        );
        for bad in [
            "rollout-garbage.jsonl",                                 // 剥前缀后 <36
            "notrollout-019f75dd-875c-7c81-9eda-32f866b2c60f.jsonl", // 无 rollout- 前缀
            "rollout-2026-07-18T08-00-00-zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz.jsonl", // 末36 结构对但非 hex
        ] {
            assert!(codex_sid_from_path(&p(bad)).is_none(), "畸形名应跳: {bad}");
        }
    }

    #[test]
    fn is_uuid_matches_8_4_4_4_12_hex() {
        assert!(is_uuid("019f75dd-875c-7c81-9eda-32f866b2c60f"));
        assert!(!is_uuid("019f75dd-875c-7c81-9eda-32f866b2c60")); // 末段 11 位
        assert!(!is_uuid("zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz")); // 非 hex
        assert!(!is_uuid("019f75dd875c7c819eda32f866b2c60f")); // 无分隔
    }
}
