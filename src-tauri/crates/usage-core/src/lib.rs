//! Claude 会话用量口径的**唯一实现**。
//!
//! # 为什么要这个 crate
//!
//! 在它之前，同一套口径写了两遍：`src-tauri/src/usage.rs::accumulate_usage`（monitor，
//! 走 `parse_line`/`JsonlRecord`）与 `remote-daemon-proto/src/observe/usage_query.rs::analyze_session`
//! （daemon，在裸 `serde_json::Value` 上抽取）。后者的头注逐字写着
//! **「改口径必须同步改本地 usage.rs（双写点）」**。
//!
//! ## 那个双写**没有任何护栏**，而且已经漂了
//!
//! daemon 侧有一条名叫 `per_request_field_max_matches_local_kou_jing` 的测试 ——
//! **它根本不跨轨**：只调 daemon 自己的 `analyze_session`，断言的是人手写下的期望数字，
//! 从不碰 monitor 的 `usage.rs`（那个测试模块里对「本地 usage.rs」的唯一提及是一句 doc 注释）。
//! 名字里的 `matches_local` 是一句没有判据的声明。
//!
//! 实测已经漂开的一处：**daemon 剥 BOM（`\u{feff}`），monitor 零 BOM 处理**
//! （`parse.rs` / `usage.rs` 里都没有）⇒ 带 BOM 的首行 daemon 计入、monitor 跳过。
//!
//! ## 为什么内核吃裸 JSON 而不是 `JsonlRecord`
//!
//! 让 daemon 反向长出 `parse_line`/`JsonlRecord` 会把一个 Linux-only 静态 musl 二进制
//! 拖上 monitor 的类型体系。反过来对 monitor 几乎无成本 —— **它自己的 Codex 用量轴
//! 早就「直读 rawJson、不经 `JsonlRecord`」**（`usage.rs` 头注原话）。
//! 取两侧都拿得到的最小公共形态 = 裸 JSON。
//!
//! # 口径（这里是唯一定义处）
//!
//! - 键 = `requestId`，缺则 `uuid`；两者都无 ⇒ **跳过**（无法去重也无法归属）。
//! - 同一个键的多条 assistant 记录 **逐字段取 MAX**：一次 API 请求在 jsonl 里落成多条，
//!   `input`/`cache_*` 请求级逐行重复、`output` 流式（前面是占位、终结记录才是真总量）。
//! - `msgs` **每请求 +1**（不是每行 +1）。
//! - 跨会话按键去重（`/branch` 祖先复制会保留同一个 `requestId`）。
//! - 桶键 = `(model, day)`；`model` 空或缺 ⇒ `"unknown"`；`day` = 时间戳前 10 字符。

use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// 一个 `(model, day)` 桶里的累加量。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub input: u64,
    pub cache_creation: u64,
    pub cache_read: u64,
    pub output: u64,
    pub msgs: u32,
}

impl Totals {
    /// 逐字段取 MAX（同一 requestId 内合并）。
    fn max_with(&mut self, u: &Value) {
        let g = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
        self.input = self.input.max(g("input_tokens"));
        self.cache_creation = self.cache_creation.max(g("cache_creation_input_tokens"));
        self.cache_read = self.cache_read.max(g("cache_read_input_tokens"));
        self.output = self.output.max(g("output_tokens"));
    }

    /// 把一个「已合并完的请求」加进桶：四个量相加，`msgs` **+1**。
    fn add_request(&mut self, r: &Totals) {
        self.input += r.input;
        self.cache_creation += r.cache_creation;
        self.cache_read += r.cache_read;
        self.output += r.output;
        self.msgs += 1;
    }
}

/// 扫一个会话的结果。
#[derive(Debug, Default)]
pub struct SessionUsage {
    /// `(model, day)` → 累加量。
    pub buckets: HashMap<(String, String), Totals>,
    /// 该会话的 `cwd`（取**第一条** user 记录的，同两侧原实现）。
    pub cwd: Option<String>,
}

/// 累加一个会话的用量。
///
/// `lines`：该会话 jsonl 的逐行原文。**BOM 与首尾空白由本函数处理** ——
/// 调用方不要各自再剥一遍（两侧此前剥法不同，正是漂移的来源）。
///
/// `seen_requests`：**跨会话**去重集合，由调用方在一轮扫描内持有并复用。
pub fn accumulate<I, S>(lines: I, seen_requests: &mut HashSet<String>) -> SessionUsage
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out = SessionUsage::default();
    // 本会话内：键 → (model, day, 逐字段 MAX)
    let mut per_req: HashMap<String, (String, String, Totals)> = HashMap::new();

    for line in lines {
        let trimmed = line.as_ref().trim_start_matches('\u{feff}').trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(trimmed) else {
            continue; // 畸形行跳过，绝不 panic
        };
        let rec_type = v.get("type").and_then(Value::as_str);

        // cwd 只从 user 记录取，且只取第一条。
        if rec_type == Some("user") && out.cwd.is_none() {
            if let Some(c) = v.get("cwd").and_then(Value::as_str) {
                if !c.is_empty() {
                    out.cwd = Some(c.to_string());
                }
            }
        }
        if rec_type != Some("assistant") {
            continue;
        }
        let Some(msg) = v.get("message") else {
            continue;
        };
        let Some(usage) = msg.get("usage").filter(|u| u.is_object()) else {
            continue;
        };
        // 键 = requestId（缺→uuid）。都无 ⇒ 跳过。
        let key = v
            .get("requestId")
            .and_then(Value::as_str)
            .or_else(|| v.get("uuid").and_then(Value::as_str))
            .unwrap_or("");
        if key.is_empty() {
            continue;
        }
        let model = msg
            .get("model")
            .and_then(Value::as_str)
            .filter(|m| !m.is_empty())
            .unwrap_or("unknown")
            .to_string();
        let day = v
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|t| t.get(0..10))
            .unwrap_or("")
            .to_string();
        per_req
            .entry(key.to_string())
            .or_insert((model, day, Totals::default()))
            .2
            .max_with(usage);
    }

    // flush：跨会话去重后，每个请求的「逐字段 MAX」进桶一次。
    for (key, (model, day, req_max)) in per_req {
        if !seen_requests.insert(key) {
            continue;
        }
        out.buckets
            .entry((model, day))
            .or_default()
            .add_request(&req_max);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assistant(req: Option<&str>, uuid: &str, out_tokens: u64) -> String {
        let rid = req
            .map(|r| format!(r#""requestId":"{r}","#))
            .unwrap_or_default();
        format!(
            r#"{{"type":"assistant",{rid}"uuid":"{uuid}","timestamp":"2026-07-17T10:00:00Z",
               "message":{{"model":"m","usage":{{"input_tokens":2,"cache_read_input_tokens":19059,
               "cache_creation_input_tokens":0,"output_tokens":{out_tokens}}}}}}}"#
        )
        .replace('\n', "")
    }

    /// 口径主线：一次请求落三条，`output` 流式 5→5→484 ⇒ 逐字段 MAX、`msgs` 只 +1。
    #[test]
    fn one_request_three_records_takes_field_max_and_counts_once() {
        let mut seen = HashSet::new();
        let u = accumulate(
            [
                assistant(Some("r1"), "u1", 5),
                assistant(Some("r1"), "u2", 5),
                assistant(Some("r1"), "u3", 484),
            ],
            &mut seen,
        );
        let t = u.buckets[&("m".into(), "2026-07-17".into())];
        assert_eq!(
            (t.input, t.cache_read, t.output, t.msgs),
            (2, 19059, 484, 1),
            "output 必须是终结值不是占位；msgs 一请求算一条"
        );
    }

    /// 键的 fallback 第二段：缺 `requestId` ⇒ 按 `uuid` 归并。
    #[test]
    fn falls_back_to_uuid_when_request_id_is_absent() {
        let mut seen = HashSet::new();
        let u = accumulate(
            [
                assistant(None, "u1", 100),
                assistant(None, "u1", 150),
                assistant(None, "u2", 200),
            ],
            &mut seen,
        );
        let t = u.buckets[&("m".into(), "2026-07-17".into())];
        assert_eq!(
            (t.output, t.msgs),
            (350, 2),
            "同 uuid 合成一请求（MAX 150）、不同 uuid 各算一请求 ⇒ 150+200，msgs=2"
        );
    }

    /// 跨会话去重：`/branch` 祖先复制会保留同一个 `requestId`。
    #[test]
    fn the_same_request_id_is_counted_once_across_sessions() {
        let mut seen = HashSet::new();
        let a = accumulate([assistant(Some("r1"), "u1", 484)], &mut seen);
        let b = accumulate([assistant(Some("r1"), "u9", 484)], &mut seen);
        assert_eq!(a.buckets.len(), 1);
        assert!(
            b.buckets.is_empty(),
            "第二个会话里重复的 requestId 必须被挡下，否则 /branch 会把用量算两遍"
        );
    }

    /// ★ **BOM**：这是两侧此前**真的漂开**的那一处。
    ///
    /// daemon 剥 `\u{feff}`，monitor 零 BOM 处理 ⇒ 带 BOM 的首行 daemon 计入、monitor 跳过。
    /// 内核统一剥，两侧从此一致。
    #[test]
    fn a_leading_bom_does_not_hide_the_first_record() {
        let mut seen = HashSet::new();
        let u = accumulate(
            [format!("\u{feff}{}", assistant(Some("r1"), "u1", 484))],
            &mut seen,
        );
        assert_eq!(
            u.buckets.len(),
            1,
            "带 BOM 的首行被吞了 —— 那正是 monitor 侧此前的行为"
        );
    }

    /// 畸形行不许拖垮整个会话。
    #[test]
    fn malformed_lines_are_skipped_not_fatal() {
        let mut seen = HashSet::new();
        let u = accumulate(
            [
                "not json".to_string(),
                String::new(),
                assistant(Some("r1"), "u1", 484),
            ],
            &mut seen,
        );
        assert_eq!(u.buckets.len(), 1);
    }

    /// ★ 有 `requestId`、无 `uuid` ⇒ **计入**。这是 U7-2 的第二处**刻意收敛**。
    ///
    /// 此前 monitor 走 `parse_line`，而 `JsonlRecord::Assistant` 的 `uuid` 是必填
    /// （无 default）⇒ 这种记录落 `Unrecognized` 被丢；daemon 一直按 `requestId` 计入。
    /// 两侧就此分叉，而那个双写没有任何护栏。内核统一按「requestId 优先」⇒ 跟 daemon 一致。
    ///
    /// 可达性低（Claude Code 实际每条都写 uuid），但**分叉是真的**，钉住免得再漂回去。
    #[test]
    fn a_record_with_a_request_id_but_no_uuid_is_counted() {
        let mut seen = HashSet::new();
        let u = accumulate(
            [
                r#"{"type":"assistant","requestId":"r1","timestamp":"2026-07-17T10:00:00Z",
                 "message":{"model":"m","usage":{"output_tokens":9}}}"#
                    .replace('\n', ""),
            ],
            &mut seen,
        );
        assert_eq!(
            u.buckets[&("m".into(), "2026-07-17".into())].output,
            9,
            "无 uuid 但有 requestId 的记录被丢了 —— 那是 monitor 此前的行为，与 daemon 分叉"
        );
    }

    /// 两者都无键 ⇒ 跳过（无法去重也无法归属）。
    #[test]
    fn a_record_with_neither_request_id_nor_uuid_is_skipped() {
        let mut seen = HashSet::new();
        let u = accumulate(
            [r#"{"type":"assistant","timestamp":"2026-07-17T10:00:00Z",
                 "message":{"model":"m","usage":{"output_tokens":9}}}"#
                .replace('\n', "")],
            &mut seen,
        );
        assert!(u.buckets.is_empty());
    }

    /// `cwd` 取第一条 user 记录的。
    #[test]
    fn cwd_comes_from_the_first_user_record() {
        let mut seen = HashSet::new();
        let u = accumulate(
            [
                r#"{"type":"user","cwd":"/first"}"#.to_string(),
                r#"{"type":"user","cwd":"/second"}"#.to_string(),
            ],
            &mut seen,
        );
        assert_eq!(u.cwd.as_deref(), Some("/first"));
    }
}
