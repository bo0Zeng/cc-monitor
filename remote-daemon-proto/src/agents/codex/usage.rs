//! Codex 用量聚合（DG5：per-kind `token_count` 路）。
//!
//! `S2` 从 `observe/usage_query.rs` 整段搬来（原 `aggregate_codex` / `analyze_codex_session`）。
//! **只改了两处名字之外的零行**：入口 `aggregate_codex` → [`aggregate`]（模块名已经说了是 codex），
//! 桶累加器由本地私有 `Totals` 换成 `usage_core::Totals`（**字段与 Default 逐字相同**，
//! 原来那份是共享 crate 里那个的副本）。抽取器函数名 [`analyze_codex_session`] **原样保留** ——
//! 它的两条测试跟着一起搬，断言一个字没改，那是"搬迁零行为变化"的证据。
//!
//! 口径的唯一权威源仍是 `usage_core::codex_delta`（经 [`super::parse::last_token_delta`]），
//! 本文件不碰 token 字段名 —— `the_usage_kou_jing_has_exactly_one_home` 的人群已跟着搬迁扩到这里。

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use usage_core::Totals;
use walkdir::WalkDir;

/// 扫本机 Codex 会话（`<codex_dir>/sessions/**/rollout-*.jsonl`）→ 每有 token_count 用量的会话输出一行
/// SessionUsageRow 形 JSON（+`agentKind:"codex"`）。无 Codex 会话根 → 无输出（零成本、Claude 段不受影响）。
/// 安全：路径限会话根之下（canonicalize 前缀校验，同 Claude 段）；只读。
pub(crate) fn aggregate() -> Result<(), String> {
    let Some(codex_dir) = super::parse::resolve_codex_dir() else {
        return Ok(());
    };
    let root = super::parse::sessions_root(&codex_dir);
    if !root.is_dir() {
        return Ok(());
    }
    let canon_root = match root.canonicalize() {
        Ok(r) => r,
        Err(_) => return Ok(()), // 会话根不可达 → 无输出（不报错）
    };
    // 日期分区树 sessions/YYYY/MM/DD/rollout-*.jsonl（root 下 4 层）；限 rollout-*.jsonl。
    let files: Vec<PathBuf> = WalkDir::new(&canon_root)
        .max_depth(4)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().is_file()
                && e.path().extension().is_some_and(|x| x == "jsonl")
                && e.path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("rollout-"))
        })
        .map(|e| e.into_path())
        .collect();

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for path in files {
        let Ok(canon) = path.canonicalize() else {
            continue;
        };
        if !canon.starts_with(&canon_root) {
            continue; // 防 symlink 逃逸
        }
        if let Some(row) = analyze_codex_session(&path) {
            writeln!(out, "{row}").map_err(|e| format!("stdout write failed: {e}"))?;
        }
    }
    Ok(())
}

/// 扫一个 Codex rollout jsonl → 用量行 JSON（无 token_count → None）。**口径镜像 monitor
/// `usage.rs::accumulate_codex_usage`（双写点，改口径须同步）**：event_msg `token_count` 的
/// `last_token_usage` 增量按 (model,天) **SUM**；字段映射由 `usage_core::codex_delta` 定
/// （`cacheCreation=0`、`output` **含 reasoning**，OpenAI 语义）；
/// **跳全零 no-op** 事件；model 取当时 `turn_context.model`；**无跨会话去重**（Codex 无 requestId、
/// 每 rollout 自成一会话、实测无 resume replay）。
fn analyze_codex_session(path: &Path) -> Option<Value> {
    let session_id = super::parse::codex_sid_from_path(path)?;
    let content = std::fs::read_to_string(path).ok()?;
    let mut cwd: Option<String> = None;
    let mut current_model = "unknown".to_string();
    let mut buckets: HashMap<(String, String), Totals> = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim_start_matches('\u{feff}').trim();
        if trimmed.is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue, // 畸形行跳过（不崩）
        };
        if cwd.is_none() {
            if let Some(c) = super::parse::session_meta_cwd(&v).filter(|c| !c.is_empty()) {
                cwd = Some(c.to_string());
            }
        }
        if let Some(m) = super::parse::turn_context_model(&v).filter(|m| !m.is_empty()) {
            current_model = m.to_string();
        }
        if super::parse::is_token_count(&v) {
            if let Some(d) = super::parse::last_token_delta(&v) {
                if d.is_noop() {
                    continue; // 全零 no-op（真机见会话起始 turn_context 前）→ 跳，免 ghost 桶
                }
                let day: String = super::parse::envelope_ts(&v)
                    .map(|t| t.chars().take(10).collect())
                    .unwrap_or_default();
                let b = buckets.entry((current_model.clone(), day)).or_default();
                b.input += d.input;
                b.cache_read += d.cache_read;
                b.output += d.output;
                b.msgs += 1;
            }
        }
    }
    if buckets.is_empty() {
        return None;
    }
    let project_path = cwd.unwrap_or_default();
    let project_name = Path::new(&project_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&project_path)
        .to_string();
    let mut bucket_arr: Vec<Value> = buckets
        .into_iter()
        .map(|((model, day), t)| {
            json!({
                "model": model,
                "day": day,
                "totals": {
                    "input": t.input,
                    "cacheCreation": t.cache_creation,
                    "cacheRead": t.cache_read,
                    "output": t.output,
                    "msgs": t.msgs,
                }
            })
        })
        .collect();
    bucket_arr.sort_by(|a, b| {
        let da = a["day"].as_str().unwrap_or("");
        let db = b["day"].as_str().unwrap_or("");
        let ma = a["model"].as_str().unwrap_or("");
        let mb = b["model"].as_str().unwrap_or("");
        db.cmp(da).then_with(|| ma.cmp(mb))
    });
    Some(json!({
        "sessionId": session_id,
        "projectPath": project_path,
        "projectName": project_name,
        "buckets": bucket_arr,
        // DG3 面：Codex 会话标记（monitor SessionUsageRow 现忽略未知字段、additive；未来消费显式 kind）。
        "agentKind": "codex",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 与 `observe/usage_query.rs` 的同名助手逐字相同（测试助手，两处各持一份不构成双写点）。
    fn write_session(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        p
    }

    /// DG5：Codex token_count SUM last + 字段映射（input−cached / cacheRead=cached / cacheCreation=0 /
    /// output）+ 全零 no-op 跳 + agentKind + sid 末36 UUID。**口径对拍 monitor F5 usage.rs（双写点）。**
    #[test]
    fn codex_usage_sums_last_maps_fields_and_marks_agent_kind() {
        let tmp = std::env::temp_dir().join(format!("ccm-cxusage-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let meta = r#"{"timestamp":"2026-07-18T08:00:00Z","type":"session_meta","payload":{"cwd":"/home/u/proj"}}"#;
        let tctx = r#"{"timestamp":"2026-07-18T08:00:00Z","type":"turn_context","payload":{"model":"gpt-5.6-terra"}}"#;
        let tc = |inp: u64, cached: u64, out: u64| {
            format!(
                r#"{{"timestamp":"2026-07-18T08:00:00Z","type":"event_msg","payload":{{"type":"token_count","info":{{"last_token_usage":{{"input_tokens":{inp},"cached_input_tokens":{cached},"output_tokens":{out},"reasoning_output_tokens":0,"total_tokens":0}}}}}}}}"#
            )
        };
        let z = tc(0, 0, 0); // 全零 no-op（turn_context 前）→ 应跳、不建 unknown ghost 桶
        let e1 = tc(12599, 10496, 565);
        let e2 = tc(2000, 1500, 100);
        // sid = 文件名末 36 字符 UUID。
        let name = "rollout-2026-07-18T08-00-00-019f75dd-875c-7c81-9eda-32f866b2c60f.jsonl";
        let p = write_session(&tmp, name, &[meta, &z, tctx, &e1, &e2]);
        let row = analyze_codex_session(&p).expect("has token_count usage");
        assert_eq!(row["sessionId"], "019f75dd-875c-7c81-9eda-32f866b2c60f");
        assert_eq!(row["projectPath"], "/home/u/proj");
        assert_eq!(row["projectName"], "proj");
        assert_eq!(row["agentKind"], "codex");
        assert_eq!(
            row["buckets"].as_array().unwrap().len(),
            1,
            "同 model+天一桶，全零不建桶"
        );
        let t = &row["buckets"][0]["totals"];
        assert_eq!(row["buckets"][0]["model"], "gpt-5.6-terra");
        assert_eq!(t["input"].as_u64(), Some((12599 - 10496) + (2000 - 1500))); // 2603
        assert_eq!(t["cacheRead"].as_u64(), Some(10496 + 1500)); // 11996
        assert_eq!(t["cacheCreation"].as_u64(), Some(0));
        assert_eq!(t["output"].as_u64(), Some(665));
        assert_eq!(t["msgs"].as_u64(), Some(2), "全零事件跳，msgs=2");
        // 防重复计红线：input+cacheRead == Σinput_tokens（总 prompt）。
        assert_eq!(
            t["input"].as_u64().unwrap() + t["cacheRead"].as_u64().unwrap(),
            12599 + 2000
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// DG5：无 token_count 的会话 → None（不报空行）；沿用 write_session 造纯 message 会话。
    #[test]
    fn codex_session_without_token_count_yields_none() {
        let tmp = std::env::temp_dir().join(format!("ccm-cxusage-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let meta =
            r#"{"timestamp":"2026-07-18T08:00:00Z","type":"session_meta","payload":{"cwd":"/p"}}"#;
        let msg = r#"{"timestamp":"2026-07-18T08:00:01Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]}}"#;
        let name = "rollout-2026-07-18T08-00-00-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee.jsonl";
        let p = write_session(&tmp, name, &[meta, msg]);
        assert!(analyze_codex_session(&p).is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
