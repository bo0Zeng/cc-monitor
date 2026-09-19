//! 单行 JSONL → `JsonlRecord`。剥 UTF-8 BOM（INVARIANT § 3）+ 跳空行。
//!
//! **F63 (issue #49)：这里是「零信息损失」的唯一关口。**
//! 六个生产调用点全过 `parse_line`（`lib.rs` live watcher / `history.rs` ×3 /
//! `remote_history.rs` / `search.rs` / `subagent.rs`），而它手里正好有原始字符串
//! ——**绕开点和修复点是同一个地方**。
//!
//! F63 之前的两条静默丢失路径：
//! - **未知 `type`** → `#[serde(other)] Unknown`（零字段）→ `is_displayable()` false
//!   → `lib.rs::batch_to_payloads` 里那个 `Ok(_) => {}` 静默丢，连 warn 都没有（实测 8,774 条 / 5.6%）
//! - **已知 `type` 但字段解析失败** → `Err` → `history.rs` / `remote_history.rs`
//!   的 `_ => continue` 静默丢（实测 1 条 / 157,385 行）
//!
//! 两条殊途同归：记录从集合消失 → children 的 parentUuid 指向集合外 →
//! `branching.ts:100-106` 判孤儿 root → `:48-50` 整棵误折叠。
//!
//! F63 起：能解成合法 JSON 的行**一律留下**（抢救原文 + uuid/parentUuid/timestamp
//! 组 `Unrecognized`）；只有连 JSON 语法都不成立的行才仍返回 `Err`。

use crate::messages::JsonlRecord;

/// 解析单行 JSONL。
///
/// - `Ok(None)`：空行 / 纯 BOM 行。
/// - `Ok(Some(_))`：认识的类型；**或** F63 抢救出的 `Unrecognized`（留原文+身份）。
/// - `Err(_)`：原文**连合法 JSON 都不是**（半截行 / 语法坏）——没身份可救，
///   caller 决定容错策略。**`Unknown` 绝不出这个出口**（护栏见测试）。
pub fn parse_line(raw: &str) -> Result<Option<JsonlRecord>, serde_json::Error> {
    let trimmed = raw.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    match serde_json::from_str::<JsonlRecord>(trimmed) {
        // 未知 `type`：serde 落到 Unknown（`#[serde(other)]` 编译期强制 unit variant，
        // 加不了字段）→ 在这里抢救。from_str::<JsonlRecord> 已成功 ⇒ 必是合法 JSON。
        Ok(JsonlRecord::Unknown) => {
            let v: serde_json::Value = serde_json::from_str(trimmed)?;
            // U-CC1：**记一笔，不 warn**。刻意不 warn 那个决定是对的（实测 20,526 条 `mode`
            // 会刷屏），但「刻意不 warn」不等于「刻意不可观测」—— 见 `drift_ledger` 头注。
            crate::drift_ledger::record(
                crate::drift_ledger::DriftFace::UnknownRecordType,
                v.get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(""),
                Some(trimmed),
            );
            Ok(Some(salvage(&v, trimmed, "unknown-type".to_string())))
        }
        Ok(record) => Ok(Some(record)),
        Err(e) => match serde_json::from_str::<serde_json::Value>(trimmed) {
            // 合法 JSON，但我们的 schema 认不出（如已知 type 缺必填字段 / 字段形状
            // 变了）→ 照样抢救身份，不丢链。**这类值得警惕**（多半是 Claude 改了
            // 已知类型的格式），故 warn 一条保留可观测性——F63 前这条路径走上层
            // `tracing::warn`，抢救后不再 Err，故在此补回。实测仅 1/157,385，不刷屏；
            // **刻意不对 unknown-type 分支 warn**（那是预期内的，实测 6472 条会刷屏）。
            Ok(v) => {
                tracing::warn!("已知类型解析失败，抢救为 Unrecognized（不丢链）: {e}");
                // U-CC1：这一类**值得警惕**（多半是 CC 改了已知类型的形状）。
                // 键按 `type` 分组，这样诊断面能直接告诉你「是哪个类型变了」。
                crate::drift_ledger::record(
                    crate::drift_ledger::DriftFace::KnownTypeParseFailed,
                    v.get("type")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(""),
                    Some(trimmed),
                );
                Ok(Some(salvage(&v, trimmed, format!("parse-failed: {e}"))))
            }
            // 连 JSON 都不是 → 真畸形，没东西可救，保持既有 Err 契约。
            Err(_) => Err(e),
        },
    }
}

/// Phase 2 F1a：**按 agent kind 派发**的单行解析。Claude 走 [`parse_line`]（F63 缝不动、字节不变、
/// 零回归）；Codex 走 `serde_json` + [`crate::codex_record::to_jsonl_record`]（消息映射进 `JsonlRecord`、
/// event/token_count 等落 `Unrecognized` 保 raw）。契约同 [`parse_line`]：空行 `Ok(None)`、认识
/// `Ok(Some)`、连 JSON 都不是 `Err`。发现层枚举时已知 kind（见 `adapter::records_roots`/`kind_of_path`）。
pub fn parse_for_kind(
    kind: crate::adapter::AgentKind,
    raw: &str,
) -> Result<Option<JsonlRecord>, serde_json::Error> {
    match kind {
        crate::adapter::AgentKind::ClaudeCode => parse_line(raw),
        crate::adapter::AgentKind::Codex => {
            let trimmed = raw.trim_start_matches('\u{feff}').trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            let v: serde_json::Value = serde_json::from_str(trimmed)?;
            Ok(Some(crate::codex_record::to_jsonl_record(&v, trimmed)))
        }
    }
}

/// F63：从已解析的 `Value` 抢救链上身份 + 原文，组 `Unrecognized`。
///
/// 只取**链需要的**四个字段（uuid / parentUuid / timestamp / type）——其余靠 `raw`
/// 原文保底。刻意不做 schema 猜测：SS-1 账本「留逃生口就够，别建完整统一格式」。
fn salvage(v: &serde_json::Value, raw: &str, reason: String) -> JsonlRecord {
    let s = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    JsonlRecord::Unrecognized {
        uuid: s("uuid"),
        parent_uuid: s("parentUuid"),
        timestamp: s("timestamp"),
        original_type: s("type"),
        raw: raw.to_owned(),
        reason,
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/parser_tests.rs"]
mod tests;
