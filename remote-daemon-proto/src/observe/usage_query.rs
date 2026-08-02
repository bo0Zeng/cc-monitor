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

// U2：合并进 `common/paths.rs`（原来这里各有一份逐字相同的副本）。
use crate::common::paths::projects_root;
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
/// **先 Claude（projects/）后 Codex（sessions/）**，各输出每会话一行（Codex 行带 `agentKind:"codex"`）。
/// 无 `~/.codex` → Codex 段零输出、Claude 段不受影响（零回归）。
pub fn run(claude_dir: &Path, _args: &[String]) -> i32 {
    match aggregate(claude_dir).and_then(|()| aggregate_codex()) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("cc-monitor-remote usage error: {e}");
            2
        }
    }
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

    // U7-2：口径**不在这里**了 —— 唯一实现在共享 crate `usage-core`，monitor 侧
    // （`src-tauri/src/usage.rs`）用的是同一个函数。
    //
    // 此前这里与 monitor 各写一遍，本文件头注逐字写着「改口径必须同步改本地 usage.rs
    // （双写点）」，而那个双写**没有任何护栏**：名叫
    // `per_request_field_max_matches_local_kou_jing` 的测试只调本文件自己的实现、
    // 断言人手写下的数字，从不碰 monitor。实测已经漂开一处（BOM）。
    let usage = usage_core::accumulate(content.lines(), seen_requests);
    let buckets = usage.buckets;
    let cwd = usage.cwd;

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

// ─── DG5：Codex 用量聚合（per-kind token_count 路；镜像 monitor F5 / usage.rs accumulate_codex_usage）───

/// 扫本机 Codex 会话（`<codex_dir>/sessions/**/rollout-*.jsonl`）→ 每有 token_count 用量的会话输出一行
/// SessionUsageRow 形 JSON（+`agentKind:"codex"`）。无 `~/.codex/sessions` → 无输出（零成本、Claude 段不受影响）。
/// 安全：路径限 `<codex_dir>/sessions/`（canonicalize 前缀校验，同 Claude 段）；只读。
fn aggregate_codex() -> Result<(), String> {
    let Some(codex_dir) = crate::observe::codex::resolve_codex_dir() else {
        return Ok(());
    };
    let root = crate::observe::codex::sessions_root(&codex_dir);
    if !root.is_dir() {
        return Ok(());
    }
    let canon_root = match root.canonicalize() {
        Ok(r) => r,
        Err(_) => return Ok(()), // sessions 不可达 → 无输出（不报错）
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
/// `last_token_usage` 增量按 (model,天) **SUM**；字段映射 `input=input_tokens−cached`（防与 cacheRead
/// 重复计）/ `cacheRead=cached` / `cacheCreation=0` / `output=output_tokens`（**含 reasoning**，OpenAI 语义）；
/// **跳全零 no-op** 事件；model 取当时 `turn_context.model`；**无跨会话去重**（Codex 无 requestId、
/// 每 rollout 自成一会话、实测无 resume replay）。
fn analyze_codex_session(path: &Path) -> Option<Value> {
    let session_id = crate::observe::codex::codex_sid_from_path(path)?;
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
            if let Some(c) = crate::observe::codex::session_meta_cwd(&v).filter(|c| !c.is_empty()) {
                cwd = Some(c.to_string());
            }
        }
        if let Some(m) = crate::observe::codex::turn_context_model(&v).filter(|m| !m.is_empty()) {
            current_model = m.to_string();
        }
        if crate::observe::codex::is_token_count(&v) {
            if let Some((inp, cached, out_tok)) = crate::observe::codex::last_token_usage_fields(&v)
            {
                if inp == 0 && cached == 0 && out_tok == 0 {
                    continue; // 全零 no-op（真机见会话起始 turn_context 前）→ 跳，免 ghost 桶
                }
                let day: String = crate::observe::codex::envelope_ts(&v)
                    .map(|t| t.chars().take(10).collect())
                    .unwrap_or_default();
                let b = buckets.entry((current_model.clone(), day)).or_default();
                b.input += inp.saturating_sub(cached);
                b.cache_read += cached;
                b.output += out_tok;
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

    /// golden-parity（daemon-03）：aterm `UsageAggregator` 键 = `requestId ?: uuid ?: r`。
    /// 缺 requestId → 按 **uuid** 归并：同 uuid 逐字段 MAX（一请求），不同 uuid = 不同请求
    /// （msgs 各 +1、桶内相加）。锁死 fallback 链的第二段（现有测只覆盖 requestId 存在）。
    #[test]
    fn uuid_fallback_keying_matches_aterm() {
        let tmp = std::env::temp_dir().join(format!("ccm-usage-uuid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        // 无 requestId：u1 两条(output 100→150，MAX=150)、u2 一条(output 200)。同 model/day。
        let a = |uuid: &str, out: u64| {
            format!(
                r#"{{"type":"assistant","uuid":"{uuid}","timestamp":"2026-07-17T10:00:00Z","message":{{"model":"m","usage":{{"input_tokens":1,"output_tokens":{out}}}}}}}"#
            )
        };
        let p = write_session(
            &tmp,
            "s1.jsonl",
            &[&a("u1", 100), &a("u1", 150), &a("u2", 200)],
        );
        let mut seen = HashSet::new();
        let row = analyze_session(&p, &mut seen).expect("has usage");
        let b = &row["buckets"][0]["totals"];
        // u1 MAX=150 + u2=200 = 350；两个不同 uuid = 两请求。
        assert_eq!(b["output"].as_u64(), Some(350), "同 uuid MAX、异 uuid 相加");
        assert_eq!(b["msgs"].as_u64(), Some(2), "两个 uuid = 两请求");
        assert_eq!(
            b["input"].as_u64(),
            Some(2),
            "input 也 uuid 分组 MAX 后相加(1+1)"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// golden-parity（daemon-03）：`/branch` 祖先复制保留 requestId → **跨会话按 requestId
    /// 去重**（同 requestId 在两个 jsonl 只算一次，防分支重复计）。对拍 aterm 跨流去重 +
    /// 本地 usage.rs。第二个文件的重复 requestId 被 `seen_requests` 挡下 → 该文件无净新增。
    #[test]
    fn branch_cross_session_dedup_counts_once() {
        let tmp = std::env::temp_dir().join(format!("ccm-usage-branch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let rec = r#"{"type":"assistant","uuid":"x","requestId":"r1","timestamp":"2026-07-17T10:00:00Z","message":{"model":"m","usage":{"input_tokens":10,"output_tokens":400}}}"#;
        let s1 = write_session(&tmp, "s1.jsonl", &[rec]);
        // s2 = 分支复制：同 requestId r1（uuid 不同也无所谓，键是 requestId）。
        let s2_rec = r#"{"type":"assistant","uuid":"y","requestId":"r1","timestamp":"2026-07-17T10:00:00Z","message":{"model":"m","usage":{"input_tokens":10,"output_tokens":400}}}"#;
        let s2 = write_session(&tmp, "s2.jsonl", &[s2_rec]);
        let mut seen = HashSet::new();
        // 先 s1：r1 首见 → 计入。
        let r1 = analyze_session(&s1, &mut seen).expect("s1 has usage");
        assert_eq!(r1["buckets"][0]["totals"]["output"].as_u64(), Some(400));
        assert_eq!(r1["buckets"][0]["totals"]["msgs"].as_u64(), Some(1));
        // 再 s2：r1 已在 seen → 去重 → 该会话无净新增 usage → None（不重复计 400）。
        let r2 = analyze_session(&s2, &mut seen);
        assert!(r2.is_none(), "分支重复 requestId 跨会话只算一次");
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

    /// 审计 quality-重要：多 (model,day) 桶拆分 + 稳定序（此前 4 测全单桶、只查 buckets[0]，
    /// 排序比较器 `db.cmp(da).then(ma.cmp(mb))`=天降序·同天模型升序 在 >1 桶下无测）。
    #[test]
    fn multi_bucket_split_and_stable_sort() {
        let tmp = std::env::temp_dir().join(format!("ccm-usage-multi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let rec = |rid: &str, model: &str, day: &str, out: u64| {
            format!(
                r#"{{"type":"assistant","uuid":"{rid}","requestId":"{rid}","timestamp":"{day}T10:00:00Z","message":{{"model":"{model}","usage":{{"output_tokens":{out}}}}}}}"#
            )
        };
        // 三桶：17/a、17/b、18/a（各不同 requestId 免去重）。
        let p = write_session(
            &tmp,
            "s.jsonl",
            &[
                &rec("r1", "a", "2026-07-17", 10),
                &rec("r2", "b", "2026-07-17", 20),
                &rec("r3", "a", "2026-07-18", 30),
            ],
        );
        let mut seen = HashSet::new();
        let row = analyze_session(&p, &mut seen).expect("has usage");
        let b = row["buckets"].as_array().unwrap();
        assert_eq!(b.len(), 3, "三 (model,day) 桶");
        let key = |i: usize| {
            (
                b[i]["day"].as_str().unwrap(),
                b[i]["model"].as_str().unwrap(),
            )
        };
        // 稳定序：天降序，同天模型升序 → [18/a, 17/a, 17/b]。
        assert_eq!(key(0), ("2026-07-18", "a"));
        assert_eq!(key(1), ("2026-07-17", "a"));
        assert_eq!(key(2), ("2026-07-17", "b"));
        assert_eq!(b[0]["totals"]["output"].as_u64(), Some(30));
        assert_eq!(b[0]["totals"]["msgs"].as_u64(), Some(1));
        let _ = std::fs::remove_dir_all(&tmp);
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
