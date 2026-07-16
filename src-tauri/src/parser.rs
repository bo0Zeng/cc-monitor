//! 单行 JSONL → `JsonlRecord`。剥 UTF-8 BOM（INVARIANT § 3）+ 跳空行。
//!
//! **F63 (issue #49)：这里是「零信息损失」的唯一关口。**
//! 六个生产调用点全过 `parse_line`（`lib.rs` live watcher / `history.rs` ×3 /
//! `remote_history.rs` / `search.rs` / `subagent.rs`），而它手里正好有原始字符串
//! ——**绕开点和修复点是同一个地方**。
//!
//! F63 之前的两条静默丢失路径：
//! - **未知 `type`** → `#[serde(other)] Unknown`（零字段）→ `is_displayable()` false
//!   → `lib.rs:1123` `Ok(_) => {}` 静默丢，连 warn 都没有（实测 8,774 条 / 5.6%）
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
                Ok(Some(salvage(&v, trimmed, format!("parse-failed: {e}"))))
            }
            // 连 JSON 都不是 → 真畸形，没东西可救，保持既有 Err 契约。
            Err(_) => Err(e),
        },
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
mod tests {
    use super::*;

    #[test]
    fn empty_line_returns_none() {
        assert!(parse_line("").unwrap().is_none());
        assert!(parse_line("   ").unwrap().is_none());
        assert!(parse_line("\n").unwrap().is_none());
    }

    #[test]
    fn bom_only_line_returns_none() {
        // 纯 BOM 行：trim 后空
        assert!(parse_line("\u{feff}").unwrap().is_none());
        assert!(parse_line("\u{feff}   \n").unwrap().is_none());
    }

    #[test]
    fn bom_prefix_does_not_corrupt_type() {
        // v1.7.8 教训：PS 5.1 Out-File -Encoding utf8 写 BOM，serde 不剥就解析失败 →
        // cc 集成"装上没用"7 个版本。parse_line 必须先剥 BOM 再 from_str。
        let bom_user =
            "\u{feff}{\"type\":\"custom-title\",\"customTitle\":\"hi\",\"sessionId\":\"s1\"}";
        let r = parse_line(bom_user).unwrap().expect("应该解析成功");
        assert!(
            matches!(r, JsonlRecord::CustomTitle { .. }),
            "BOM 后类型识别失败：{r:?}"
        );
    }

    #[test]
    fn malformed_json_returns_err() {
        // 半截 JSON / 非法语法 → Err，不 panic（caller 决定容错策略）。
        // F63 后仍 Err：连合法 JSON 都不是 = 没身份可救。
        assert!(parse_line("{ not json").is_err());
        assert!(parse_line("{\"type\":\"user\",").is_err());
    }

    // === F63 (issue #49)：看不懂的记录 —— 留原文 + 留链上的身份 ===

    /// ★ 护栏：`Unknown` **绝不出 parse_line**。走到这里说明后处理漏了。
    /// （F63 前这条测试的名字是 `unknown_type_falls_through_to_unknown_variant`，
    ///  断言的正是"Unknown 不应 emit"——那就是静默丢弃 8,774 条的那扇门。）
    #[test]
    fn unknown_type_is_salvaged_never_leaves_as_unknown() {
        let r = parse_line(r#"{"type":"some-future-record-type","foo":42}"#)
            .unwrap()
            .expect("合法 JSON 必须留下");
        assert!(
            !matches!(r, JsonlRecord::Unknown),
            "Unknown 不得出 parse_line，应被抢救成 Unrecognized"
        );
        match &r {
            JsonlRecord::Unrecognized {
                original_type,
                raw,
                reason,
                uuid,
                ..
            } => {
                assert_eq!(original_type.as_deref(), Some("some-future-record-type"));
                assert_eq!(reason, "unknown-type");
                assert!(raw.contains("\"foo\":42"), "原文必须一字节不改地留着");
                assert!(uuid.is_none(), "本样本无 uuid");
            }
            other => panic!("期望 Unrecognized，得到 {other:?}"),
        }
        assert!(
            r.is_displayable(),
            "必须 emit —— 否则链断，见 branching.ts:24"
        );
    }

    /// ★ 这条是 F63 真正要防的未来：Claude 发一个**带链身份**的新类型。
    /// 今天本机 771 会话里 7 个未知 type 的 uuid/parentUuid 全为 0（实测），
    /// 但一旦出现，丢掉它 = 它的 children 全成孤儿 root → 整棵误折叠。
    #[test]
    fn unknown_type_with_chain_identity_keeps_uuid_and_parent() {
        let r = parse_line(
            r#"{"type":"brand-new-2027","uuid":"u9","parentUuid":"u8","timestamp":"t9","payload":{"a":1}}"#,
        )
        .unwrap()
        .unwrap();
        match &r {
            JsonlRecord::Unrecognized {
                uuid,
                parent_uuid,
                timestamp,
                ..
            } => {
                assert_eq!(uuid.as_deref(), Some("u9"));
                assert_eq!(parent_uuid.as_deref(), Some("u8"), "链上的身份必须留住");
                assert_eq!(timestamp.as_deref(), Some("t9"));
            }
            other => panic!("期望 Unrecognized，得到 {other:?}"),
        }
    }

    /// 已知 `type` 但字段解析失败（serde 返回 Err）且原文仍是合法 JSON → 照样抢救。
    /// 实测本机 157,385 行里此路径仅 1 条（截断的 assistant），但 aterm 侧
    /// (`JsonlParser.kt:21`) 正是栽在这——任何一条解析失败就孤儿化其后整段。
    #[test]
    fn known_type_parse_failure_is_salvaged_with_identity() {
        // type=user 但缺必填 uuid 之外的东西：这里给 message 一个错形状
        let r = parse_line(
            r#"{"type":"user","uuid":"u1","parentUuid":"u0","timestamp":"t1","message":42}"#,
        )
        .unwrap()
        .expect("合法 JSON 必须留下，不得静默丢");
        match &r {
            JsonlRecord::Unrecognized {
                uuid,
                parent_uuid,
                original_type,
                reason,
                ..
            } => {
                assert_eq!(uuid.as_deref(), Some("u1"));
                assert_eq!(parent_uuid.as_deref(), Some("u0"), "链不能断");
                assert_eq!(original_type.as_deref(), Some("user"));
                assert!(
                    reason.starts_with("parse-failed:"),
                    "reason 要带 serde 原文，便于诊断：{reason}"
                );
            }
            other => panic!("期望 Unrecognized，得到 {other:?}"),
        }
    }

    /// ★ 零信息损失断言（#49 的形态②：「输入 N 条 → 输出覆盖 N 条」）。
    /// 每一行要么 Ok(Some)（认识 or 抢救），要么 Ok(None)（空行）——
    /// **没有任何一行被静默吞掉**；只有语法坏行显式 Err（可见、可记账）。
    #[test]
    fn zero_information_loss_over_mixed_fixture() {
        let fixture = vec![
            (
                r#"{"type":"user","uuid":"u1","timestamp":"t1","message":{"role":"user","content":"q"}}"#,
                "known",
            ),
            (
                r#"{"type":"mode","mode":"normal","sessionId":"s1"}"#,
                "salvaged",
            ), // 真实样本
            (
                r#"{"type":"pr-link","sessionId":"s1","prNumber":37,"timestamp":"t2"}"#,
                "salvaged",
            ), // 真实样本
            (
                r#"{"type":"agent-name","agentName":"x","sessionId":"s1"}"#,
                "salvaged",
            ), // 真实样本
            (
                r#"{"type":"user","uuid":"u2","timestamp":"t3","message":99}"#,
                "salvaged",
            ), // 已知类型解析失败
            ("", "empty"),
            ("   ", "empty"),
            ("{ not json", "err"),
        ];
        let (mut kept, mut skipped, mut errored) = (0, 0, 0);
        for (line, expect) in &fixture {
            match parse_line(line) {
                Ok(Some(r)) => {
                    kept += 1;
                    assert!(
                        !matches!(r, JsonlRecord::Unknown),
                        "Unknown 不得出 parse_line：{line}"
                    );
                    if *expect == "salvaged" {
                        assert!(
                            matches!(r, JsonlRecord::Unrecognized { .. }),
                            "该行应被抢救：{line}"
                        );
                        assert!(r.is_displayable(), "抢救出来就必须 emit：{line}");
                    }
                }
                Ok(None) => {
                    skipped += 1;
                    assert_eq!(*expect, "empty", "只有空行可以无声跳过：{line}");
                }
                Err(_) => {
                    errored += 1;
                    assert_eq!(*expect, "err", "只有语法坏行可以 Err：{line}");
                }
            }
        }
        assert_eq!(
            kept + skipped + errored,
            fixture.len(),
            "每一行都必须有交代"
        );
        assert_eq!(kept, 5);
        assert_eq!(skipped, 2);
        assert_eq!(errored, 1);
    }

    /// F63 长期对账工具：用真正的 `parse_line` 扫本机全部真实会话，量丢失面。
    /// 依赖本机数据故 `#[ignore]`。跑法：
    /// `cargo test f63_real_data_ledger -- --ignored --nocapture`
    ///
    /// 2026-07-16 基线（771 会话 / 643MB / 157,385 行）：
    ///   Ok 且 emit 132,489 · Ok 但故意不 emit 16,121 · **Unknown 丢弃 0**（F63 前 8,774）
    ///   · 抢救 8,774 · Err 1（截断行）
    #[test]
    #[ignore]
    fn f63_real_data_ledger() {
        use std::collections::BTreeMap;
        let Ok(home) = std::env::var("HOME") else {
            println!("无 HOME，跳过");
            return;
        };
        let root = std::path::PathBuf::from(home).join(".claude/projects");
        let (mut total, mut emitted, mut deliberate, mut leaked, mut errs) =
            (0u64, 0u64, 0u64, 0u64, 0u64);
        let mut salvaged: BTreeMap<String, u64> = BTreeMap::new();
        let mut stack = vec![root];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                    continue;
                }
                let Ok(content) = std::fs::read_to_string(&p) else {
                    continue;
                };
                for line in content.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    total += 1;
                    match parse_line(line) {
                        Ok(Some(JsonlRecord::Unknown)) => leaked += 1,
                        Ok(Some(JsonlRecord::Unrecognized { original_type, .. })) => {
                            *salvaged
                                .entry(original_type.unwrap_or_else(|| "<no-type>".into()))
                                .or_default() += 1;
                        }
                        Ok(Some(r)) => {
                            if r.is_displayable() {
                                emitted += 1
                            } else {
                                deliberate += 1
                            }
                        }
                        Ok(None) => {}
                        Err(_) => errs += 1,
                    }
                }
            }
        }
        println!("=== F63 真实数据对账 ===");
        println!("总行            {total:>9}");
        println!("Ok 且 emit      {emitted:>9}");
        println!("Ok 但故意不 emit {deliberate:>9}  (已知类型的明示决定，不算丢)");
        println!(
            "抢救 Unrecognized{:>9}  (F63 前这些是静默丢弃的)",
            salvaged.values().sum::<u64>()
        );
        for (t, n) in &salvaged {
            println!("                   {n:>7}  {t}");
        }
        println!("Err（语法坏行）  {errs:>9}  (可见、可记账)");
        println!("★ Unknown 泄漏  {leaked:>9}  (必须为 0)");
        // 自证扫到过东西：若 ~/.claude/projects 不存在/为空，total==0 → leaked 恒 0
        // → 测试会假绿。先断言扫到了行，再断言零泄漏。
        assert!(
            total > 0,
            "没扫到任何行——~/.claude/projects 不存在或为空，本对账工具无法自证，视为失败"
        );
        assert_eq!(leaked, 0, "Unknown 泄漏到 parse_line 出口 = 后处理有洞");
    }
}
