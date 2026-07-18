//! F88a-remote（#52）：远端用量聚合（一次性查询子命令 `--usage`）。
//!
//! cc-monitor 通过**独立 SSH 连接**一次性 exec `<daemon> --usage`，daemon 在远端 CPU 上扫
//! `<claude_dir>/projects/**/*.jsonl`、**服务端聚合 token**（避免拉整库回本地），输出**每会话一行**
//! camelCase JSON（与 monitor `usage::SessionUsageRow` 形状严格一致、可直接反序列化，daemon 侧不带
//! `origin`——monitor 收到后盖上主机 label）：
//! `{sessionId, projectPath, projectName, buckets:[{model, day, totals:{input,cacheCreation,cacheRead,output,msgs}}]}`
//!
//! ★ **口径与本地 `../src-tauri/src/usage.rs::accumulate_usage` 一字对齐**（daemon 无 `parse_line`/
//! `JsonlRecord`，故在 `serde_json::Value` 上抽取，同 `search.rs`↔`search_query.rs` 的移植先例）：
//! **per-requestId（缺→uuid）逐字段 MAX**——一次 API 请求在 jsonl 落成多条 assistant 记录，`input`/
//! `cache_*` 请求级逐行重复、`output` 流式（前占位、终结记录真总量）→ 逐字段 MAX（prompt 侧 max 无害、
//! output max=终结值）；`msgs` 每请求 +1；`/branch` 祖先复制保留 requestId → 跨会话按 requestId 去重。
//! **改口径必须同步改本地 usage.rs（双写点）。**
//!
//! 安全：路径严格限 `<claude_dir>/projects/`（canonicalize 前缀校验，复刻 history/search_query）；
//! 只读铁律（cc-monitor 不写远端）成立——本模块只 read_dir / read。

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// 逐字段 MAX 累加器（= 本地 `usage::UsageTotals` 的 Value 侧对应）。
#[derive(Default, Clone)]
struct Totals {
    input: u64,
    cache_creation: u64,
    cache_read: u64,
    output: u64,
    msgs: u32,
}

/// `--usage`（无额外参数）。返回进程退出码（0 ok / 2 err），同 history_query::run 约定。
pub fn run(claude_dir: &Path, _args: &[String]) -> i32 {
    match aggregate(claude_dir) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("cc-monitor-remote usage error: {e}");
            2
        }
    }
}

fn projects_root(claude_dir: &Path) -> PathBuf {
    claude_dir.join("projects")
}

/// 扫 projects/**/*.jsonl，按 requestId 逐字段 MAX 聚合，每有 usage 的会话输出一行 JSON。
fn aggregate(claude_dir: &Path) -> Result<(), String> {
    let root = projects_root(claude_dir);
    if !root.is_dir() {
        return Ok(()); // 无 projects → 无输出（exit 0）
    }
    let canon_root = root
        .canonicalize()
        .map_err(|e| format!("projects root unavailable: {e}"))?;
    let files: Vec<PathBuf> = WalkDir::new(&canon_root)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|x| x == "jsonl"))
        .map(|e| e.into_path())
        .collect();

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    // 跨全部会话文件的 requestId(缺→uuid) 去重集（防 /branch 复制重复计，同 usage.rs）。
    let mut seen_requests: HashSet<String> = HashSet::new();
    for path in files {
        // 防 symlink 逃逸：canonicalize 后仍须在 projects/ 下。
        let Ok(canon) = path.canonicalize() else {
            continue;
        };
        if !canon.starts_with(&canon_root) {
            continue;
        }
        if let Some(row) = analyze_session(&path, &mut seen_requests) {
            writeln!(out, "{row}").map_err(|e| format!("stdout write failed: {e}"))?;
        }
    }
    Ok(())
}

/// 扫一个 jsonl → 该会话的用量行 JSON（无任何 usage → None）。`seen_requests` 跨会话去重。
fn analyze_session(path: &Path, seen_requests: &mut HashSet<String>) -> Option<Value> {
    let session_id = path.file_stem()?.to_str()?.to_string();
    let content = std::fs::read_to_string(path).ok()?;
    let mut cwd: Option<String> = None;
    // 本文件内：requestId(缺→uuid) → (model, day, 逐字段 MAX totals)
    let mut per_req: HashMap<String, (String, String, Totals)> = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim_start_matches('\u{feff}').trim();
        if trimmed.is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue, // 畸形行跳过（不崩）
        };
        let rec_type = v.get("type").and_then(Value::as_str);
        // cwd 只从 user 记录取（严格对齐本地 usage.rs 的 `JsonlRecord::User { cwd }`——审计双写点）。
        if rec_type == Some("user") && cwd.is_none() {
            if let Some(c) = v.get("cwd").and_then(Value::as_str) {
                if !c.is_empty() {
                    cwd = Some(c.to_string());
                }
            }
        }
        if rec_type != Some("assistant") {
            continue;
        }
        let msg = match v.get("message") {
            Some(m) => m,
            None => continue,
        };
        let usage = match msg.get("usage") {
            Some(u) if u.is_object() => u,
            _ => continue,
        };
        // 键 = requestId（缺→uuid）。都无 → 跳过（无法去重/归属，同本地 fallback 但本地 uuid 恒有）。
        let key = v
            .get("requestId")
            .and_then(Value::as_str)
            .or_else(|| v.get("uuid").and_then(Value::as_str))
            .unwrap_or("")
            .to_string();
        if key.is_empty() {
            continue;
        }
        let model = msg
            .get("model")
            .and_then(Value::as_str)
            .filter(|m| !m.is_empty())
            .unwrap_or("unknown")
            .to_string();
        let day: String = v
            .get("timestamp")
            .and_then(Value::as_str)
            .map(|t| t.chars().take(10).collect())
            .unwrap_or_default();
        let field = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
        let entry = per_req
            .entry(key)
            .or_insert_with(|| (model, day, Totals::default()));
        let t = &mut entry.2;
        t.input = t.input.max(field("input_tokens"));
        t.cache_creation = t.cache_creation.max(field("cache_creation_input_tokens"));
        t.cache_read = t.cache_read.max(field("cache_read_input_tokens"));
        t.output = t.output.max(field("output_tokens"));
    }
    // flush：每 requestId 跨会话去重后，其逐字段 MAX 加进 (model, day) 桶（msgs+1/请求）。
    let mut buckets: HashMap<(String, String), Totals> = HashMap::new();
    for (key, (model, day, tmax)) in per_req {
        if !seen_requests.insert(key) {
            continue;
        }
        let b = buckets.entry((model, day)).or_default();
        b.input += tmax.input;
        b.cache_creation += tmax.cache_creation;
        b.cache_read += tmax.cache_read;
        b.output += tmax.output;
        b.msgs += 1;
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
    // 稳定序（天降序、同天按模型）——同本地 usage.rs。
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
        // origin 不带——monitor 侧收到后盖主机 label。
    }))
}

#[cfg(test)]
mod tests {
    use super::*; // 带入顶层 `std::io::Write`（writeln! 用）

    fn write_session(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        p
    }

    #[test]
    fn per_request_field_max_matches_local_kou_jing() {
        // 一次 requestId=r1 落 3 条：input/cache 逐行重复、output 流式(5→5→484)。
        // 正解=逐字段 MAX(input 2 / cache_read 19059 / output 484)、msgs=1。
        let tmp = std::env::temp_dir().join(format!("ccm-usage-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let a = |o: u64| {
            format!(
                r#"{{"type":"assistant","uuid":"u-{o}","requestId":"r1","timestamp":"2026-07-17T10:00:00Z","message":{{"model":"m","usage":{{"input_tokens":2,"cache_creation_input_tokens":8518,"cache_read_input_tokens":19059,"output_tokens":{o}}}}}}}"#
            )
        };
        let p = write_session(&tmp, "s1.jsonl", &[&a(5), &a(5), &a(484)]);
        let mut seen = HashSet::new();
        let row = analyze_session(&p, &mut seen).expect("has usage");
        let b = &row["buckets"][0]["totals"];
        assert_eq!(b["input"].as_u64(), Some(2));
        assert_eq!(b["cacheRead"].as_u64(), Some(19059));
        assert_eq!(b["output"].as_u64(), Some(484), "output 取终结值非占位");
        assert_eq!(b["msgs"].as_u64(), Some(1), "一请求算一条");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn malformed_line_skipped_and_no_usage_yields_none() {
        let tmp = std::env::temp_dir().join(format!("ccm-usage-test2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let p = write_session(
            &tmp,
            "s2.jsonl",
            &[
                "not json",
                r#"{"type":"user","cwd":"/p","message":{}}"#,
                r#"{"type":"assistant","uuid":"a","message":{"model":"m"}}"#, // 无 usage
            ],
        );
        let mut seen = HashSet::new();
        assert!(analyze_session(&p, &mut seen).is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
