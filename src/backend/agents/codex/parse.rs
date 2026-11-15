//! Codex rollout 记录的后端侧解析（per-kind · 2D）。
//!
//! monitor 那份 `codex_record.rs` 搬进来之后（[`super::record`]：分类 ＋ 映射进渲染模型），信封助手只剩本模块这一份，
//! 两边都用它。
//! Codex 格式未文档、每几 minor churn → 宽容抽取、逐行不崩、未知/缺失安全默认、alias 归一 `turn_*`↔`task_*`。
//!
//! 记录信封（本机实测 codex-cli 0.144.6）：`{"timestamp","type","payload":{...}}`。顶层 `type` ∈
//! session_meta/turn_context/world_state/response_item/event_msg；后两者 `payload.type` 再细分。
//!
//! 本模块住三样：信封助手 · 会话定位（家那个环境变量名 [`HOME_ENV`] · `sessions_root` · `session_meta_cwd` · `codex_sid_from_path`，
//! 读者都在 `agents/codex/`）· Codex 自报两窗额度的读法（rollout 那一格 · 回包头那一族同一形；回包头那一读经 [`super::relay`]
//! 的默认上游那一格交注册表，中转记账用它）。轮次边沿与 token 用量那几个函数零读者，删了（这一家今天不报轮次边沿）。

use crate::agents::{QuotaReading, QuotaWindow};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// 解包信封 `{type, payload}` → `(顶层 type, payload)`。缺 type / payload 非对象 → `None`。
pub(crate) fn unwrap_envelope(v: &Value) -> Option<(&str, &Value)> {
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
pub(crate) fn envelope_ts(v: &Value) -> Option<&str> {
    v.get("timestamp").and_then(Value::as_str)
}

// ─── DG5：usage（event_msg `token_count`）+ 会话定位 helpers（镜像 monitor F5 / adapter Codex 侧）───

/// 这一家认哪个 Codex 家的那个环境变量名（一号一个家）。读它的：[`super::home`] · 删会话那一条命令（`sessions_cli`）。
pub(crate) const HOME_ENV: &str = "CODEX_HOME";

/// Codex 会话记录根 `<codex_dir>/sessions`（日期分区树 `YYYY/MM/DD/rollout-*.jsonl` 在其下）。
pub(crate) fn sessions_root(codex_dir: &Path) -> PathBuf {
    codex_dir.join(SESSIONS_DIR)
}

const SESSIONS_DIR: &str = "sessions";
/// 归档了的 rollout 平铺住的那个目录（`<codex_dir>/archived_sessions/rollout-*.jsonl`，文件名同归档前；codex-rs `ARCHIVED_SESSIONS_SUBDIR`）。
pub(crate) const ARCHIVED_DIR: &str = "archived_sessions";

/// 归档根 `<codex_dir>/archived_sessions`（平铺一层）。
pub(crate) fn archived_root(codex_dir: &Path) -> PathBuf {
    codex_dir.join(ARCHIVED_DIR)
}

/// session_meta 的 `payload.cwd`（用量行 projectPath；Codex 无 cwd-项目目录）。
pub(crate) fn session_meta_cwd(v: &Value) -> Option<&str> {
    match unwrap_envelope(v) {
        Some(("session_meta", p)) => p.get("cwd").and_then(Value::as_str),
        _ => None,
    }
}

pub(crate) fn codex_sid_from_path(path: &Path) -> Option<String> {
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

// ─── 额度：Codex 自报的两窗（rollout `token_count.rate_limits` · 回包头 `x-codex-*`）───
//
// 形状照 openai/codex（核到 rust-v0.167.0）的 `codex-rs/protocol/src/protocol.rs`（`RateLimitSnapshot` · `RateLimitWindow`）与
// `codex-rs/codex-api/src/rate_limits.rs`（回包头那一族）：`primary` / `secondary` 各一窗，`used_percent`（0–100）·
// `window_minutes` · `resets_at`（unix 秒）；0.4x 旧版没有 `resets_at`、是 `resets_in_seconds`（从那一行的时刻起算）。
// `limit_id` 缺或 `codex` ⇒ 默认那一族；按模型另算的那几族（`x-<limit_id>-primary-…`）今天不读。

/// 默认那一族的 `limit_id`（回包头的前缀 `x-codex-` 也由它来）。
pub(crate) const DEFAULT_LIMIT: &str = "codex";

/// 两窗在快照里的那两格。
pub(crate) const SIDES: [&str; 2] = ["primary", "secondary"];

/// 5 小时 · 一周，按分钟（Codex 报窗口时长的单位）。
const FIVE_HOURS_MIN: i64 = 5 * 60;
const WEEK_MIN: i64 = 7 * 24 * 60;

/// 窗口名：照 Codex 自己报的时长 `<分钟>m`；说不出时长 ⇒ 那一格的名字（`primary` · `secondary`，没有语义位）。
fn window_name(side: &str, minutes: Option<i64>) -> String {
    minutes.map_or_else(|| side.to_string(), |m| format!("{m}m"))
}

/// 窗口名 → 语义位（注册表 `DefaultUpstream.window_slot` 那一格）：5 小时那一窗 `5h`、一周那一窗 `7d`，别的 ⇒ `None`。
/// 时长照 Codex 自己的认法（`codex_tui::chatwidget::rate_limits::get_limits_duration`）：与那个时长差在 ±5% 以内就算（上游报的分钟数不必恰好整）。
pub(crate) fn slot_of(name: &str) -> Option<&'static str> {
    let m = name.strip_suffix('m')?.parse::<i64>().ok()?;
    let near = |expected: i64| (m * 100 - expected * 100).abs() <= expected * 5;
    if near(FIVE_HOURS_MIN) {
        Some(cmd_wire::quota::SLOT_5H)
    } else if near(WEEK_MIN) {
        Some(cmd_wire::quota::SLOT_7D)
    } else {
        None
    }
}

/// 窗口名 → 窗口键（注册表 `DefaultUpstream.window_key` 那一格）：报了时长的每一窗都有键（收到几窗出几行，不丢窗）——
/// 认得出语义位 ⇒ 键就是语义位；别的时长 ⇒ 照时长通用地写（[`crate::common::time::span_key`]）；说不出时长 ⇒ 没有键（不拿上游的字段名当窗口名）。
pub(crate) fn key_of(name: &str) -> Option<String> {
    slot_of(name).map(str::to_string).or_else(|| {
        name.strip_suffix('m')?
            .parse::<i64>()
            .ok()
            .and_then(crate::common::time::span_key)
    })
}

/// 一窗：用量从百分点折成比例（与别家同一值域）。
pub(crate) fn window(
    side: &str,
    used_percent: f64,
    minutes: Option<i64>,
    resets_at: Option<u64>,
) -> QuotaWindow {
    QuotaWindow {
        name: window_name(side, minutes),
        used: Some(used_percent / 100.0),
        resets_at,
        warned_at: None,
    }
}

/// 一份 `RateLimitSnapshot`（rollout `token_count.rate_limits` 那一格）⇒ 它报的两窗；`at` ＝ 那一行的时刻（旧形的相对秒数从它起算）。
/// 不是对象 · 别的一族 · 一窗都没带 ⇒ `None`。一行怎么认作 `token_count` 在 [`super::record::quota_said`]。
pub(crate) fn snapshot_windows(limits: &Value, at: Option<u64>) -> Option<Vec<QuotaWindow>> {
    let limits = Some(limits).filter(|r| r.is_object())?;
    if limits
        .get("limit_id")
        .and_then(Value::as_str)
        .is_some_and(|id| id != DEFAULT_LIMIT)
    {
        return None;
    }
    let windows: Vec<QuotaWindow> = SIDES
        .iter()
        .filter_map(|side| {
            let w = limits.get(*side).filter(|w| w.is_object())?;
            let used = w.get("used_percent")?.as_f64().filter(|n| n.is_finite())?;
            let resets = w
                .get("resets_at")
                .and_then(Value::as_u64)
                .or_else(|| at?.checked_add(w.get("resets_in_seconds")?.as_u64()?));
            Some(window(
                side,
                used,
                w.get("window_minutes").and_then(Value::as_i64),
                resets,
            ))
        })
        .collect();
    (!windows.is_empty()).then_some(windows)
}

/// 回包头 ⇒ 额度读数（注册表 `DefaultUpstream.quota` 那一格）。没有那一族 ⇒ `None`。
/// 只读数：被不被拒由通用层一处判（[`crate::agents::judged`]），这里的 `refused` 恒假。
/// Codex 不说限流器的结论 ⇒ `status` 恒缺；没有超额那一档。
pub(crate) fn quota_of_headers(
    _status: u16,
    headers: &[(String, String)],
    _now: u64,
) -> Option<QuotaReading> {
    use crate::agents::header;
    let at = |side: &str, tail: &str| header(headers, &format!("x-{DEFAULT_LIMIT}-{side}-{tail}"));
    let windows: Vec<QuotaWindow> = SIDES
        .iter()
        .filter_map(|side| {
            let used = at(side, "used-percent")?
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())?;
            let minutes = at(side, "window-minutes").and_then(|s| s.parse::<i64>().ok());
            let resets = at(side, "reset-at").and_then(|s| s.parse::<u64>().ok());
            Some(window(side, used, minutes, resets))
        })
        .collect();
    if windows.is_empty() {
        return None;
    }
    Some(QuotaReading {
        status: None,
        refused: false,
        limiting: None,
        resets_at: None,
        windows,
        overage: None,
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/parse_tests.rs"]
mod tests;
