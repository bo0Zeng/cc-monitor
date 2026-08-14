//! F88a-remote（#52）：远端用量聚合（一次性查询子命令 `--usage`）。
//!
//! cc-monitor 通过**独立 SSH 连接**一次性 exec `<daemon> --usage`，daemon 在远端 CPU 上扫
//! `<claude_dir>/projects/**/*.jsonl`、**服务端聚合 token**（避免拉整库回本地），输出**每会话一行**
//! camelCase JSON（与 monitor `usage::SessionUsageRow` 形状严格一致、可直接反序列化，daemon 侧不带
//! `origin`——monitor 收到后盖上主机 label）：
//! `{sessionId, projectPath, projectName, buckets:[{model, day, totals:{input,cacheCreation,cacheRead,output,msgs}}]}`
//!
//! ★ **口径的唯一实现在共享 crate `usage-core`**（`usage_core::accumulate`）——
//! 本文件与本地 `src-tauri/src/usage.rs` **都调它**，不各写一遍。
//! 抽取仍在 `serde_json::Value` 上做（daemon 不引 `JsonlRecord`），同 `search.rs`↔`search_query.rs`。
//! **per-requestId（缺→uuid）逐字段 MAX**——一次 API 请求在 jsonl 落成多行时取各字段最大值；
//! `cache_*` 请求级逐行重复、`output` 流式（前占位、终结记录真总量）；`msgs` 每请求 +1。
//!
//! ⚠ **本段 08-06 订正过**：原文逐字写着「口径与本地 `usage.rs::accumulate_usage` 一字对齐」
//! 和「**改口径必须同步改本地 usage.rs（双写点）**」—— 那是 **U7-2 收口之前**的状态。
//! 收口之后口径只剩一处，而**这两句话没人回来改**：照它做的人会去维护一份
//! 根本不该存在的副本。⇒ 停滞式腐坏（世界变了、文本没动），本区反复记的那一族。
//! 现由 `the_usage_kou_jing_has_exactly_one_home` 钉着：两侧都必须调那个共享函数，
//! 且**两侧生产段都不许再出现 token 字段字面量**（那些字面量是口径本身，它们只许住在 `usage-core`）。
//!
//! 安全：路径严格限 `<claude_dir>/projects/`（canonicalize 前缀校验，复刻 history/search_query）；
//! 只读铁律（cc-monitor 不写远端）成立——本模块只 read_dir / read。

// U2：合并进 `common/paths.rs`（原来这里各有一份逐字相同的副本）。
use crate::common::paths::projects_root;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// ⚠ 这里原来有一份私有 `Totals`（与 `usage_core::Totals` **字段逐字相同**的副本）。
// `S2` 把 Codex 那半搬走之后编译器当场报「never constructed」—— 原来它**只有 Codex 那半在用**，
// Claude 这半早就走 `usage_core::accumulate` 了。⇒ 副本随搬迁自然消解，不是顺手删的。

/// `--usage`（无额外参数）。返回进程退出码（0 ok / 2 err），同 history_query::run 约定。
/// **先 Claude 后 Codex**，各输出每会话一行（Codex 行带 `agentKind` 标记）。
/// 无 Codex 会话 → Codex 段零输出、Claude 段不受影响（零回归）。
/// ⚠ Codex 那半 `S2` 已搬去 [`crate::agents::codex::usage`]——本文件只留 Claude 的口径与派发。
pub fn run(claude_dir: &Path, _args: &[String]) -> i32 {
    match aggregate(claude_dir).and_then(|()| crate::agents::codex::usage::aggregate()) {
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

}
