//! Subagent JSONL 按需加载。
//!
//! Claude Code 的 Task/Agent tool_use 触发的 subagent 不出现在主 session JSONL，
//! 而是独立写到：
//!   `<encoded-cwd>/<parent-session-id>/subagents/agent-<hash>.jsonl`
//! 同目录还有 `agent-<hash>.meta.json`：`{agentType, description}`。
//!
//! 主 watcher 跳过这些文件；本模块提供一个 IPC 命令，前端在用户展开 Task 折叠
//! 卡时调用，按 (parent_jsonl_path, tool_use.description, tool_use.timestamp)
//! 定位并加载对应 subagent JSONL。
//!
//! 匹配策略：
//! 1. 用 `description` 在 `subagents/*.meta.json` 里查所有匹配项（精确字符串相等）
//! 2. 若多于 1 个 → 读对应 JSONL 首行 timestamp，取离 tool_use_timestamp 最近的
//! 3. 解析所选 JSONL 全部行并返回（错误行 skip）

use crate::messages::JsonlRecord;
use crate::parser::parse_line;
use serde::Deserialize;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct SubagentMeta {
    #[serde(rename = "agentType")]
    #[allow(dead_code)]
    agent_type: Option<String>,
    description: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
pub struct SubagentLoadResult {
    /// 命中的 jsonl 文件路径（用于前端 debug / 状态栏显示）
    pub path: String,
    /// agentId（从文件名 `agent-<id>.jsonl` 提取）
    pub agent_id: String,
    pub records: Vec<JsonlRecord>,
}

/// P7c-1：远端那条 —— daemon **只列候选**，筛选与挑选在这里（与本机共用 `pick_closest`）。
///
/// 读内容用的是**既有的** `--read-session`：subagent jsonl 就在
/// `<claude_dir>/projects/<slug>/<sid>/subagents/` 下，那条围栏本来就放行它。
async fn load_subagent_remote(
    origin: &str,
    parent_jsonl_path: &str,
    description: &str,
    tool_use_timestamp: &str,
) -> Result<SubagentLoadResult, String> {
    let cfg = crate::remote_history::require_cfg_by_label(origin)?;
    // 深度防御（与 `stream_read_remote_session` 同一条纪律）：路径来自前端，本侧先做廉价校验；
    // 真正的越权读由 daemon 的 `fence_under_projects` 兜底。
    if parent_jsonl_path.contains("..") || !parent_jsonl_path.ends_with(".jsonl") {
        return Err(format!("非法父会话路径: {parent_jsonl_path}"));
    }
    let args = format!(
        "--list-subagents {}",
        crate::ssh_source::shell_quote(parent_jsonl_path)
    );
    let lines = crate::remote_history::run_list_query(&cfg, &args).await?;
    let mut metas: Vec<(PathBuf, Option<String>)> = Vec::new();
    for line in &lines {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        // ★ **筛选在这里**，不在 daemon（`C1`：挑选逻辑只准有一份）。
        if v.get("description").and_then(|d| d.as_str()) != Some(description) {
            continue;
        }
        let Some(path) = v.get("path").and_then(|p| p.as_str()) else {
            continue;
        };
        let ts = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .map(str::to_string);
        metas.push((PathBuf::from(path), ts));
    }
    if metas.is_empty() {
        return Err(format!(
            "远端 [{origin}] 上没有 description={description:?} 的 subagent"
        ));
    }
    let picked = pick_closest(metas, tool_use_timestamp);
    let picked_str = picked.to_string_lossy().into_owned();
    let agent_id = extract_agent_id(&picked).unwrap_or_default();
    // 内容走**既有的** `--read-session`（原样透传字节，本侧走既有解析）。
    //
    // ⚠ **一条如实登记的边界**〔D 阶段补审〕：`run_list_query` 带 **30s 超时**与每行上限，
    // 而本机那条（`read_jsonl`）**没有超时**。subagent 通常是短命的侧任务、文件很小，
    // 但一个长跑的 subagent 可能撞上那 30s。
    // 真要收得把 `load_subagent` 改成**流式**（同 `stream_read_remote_session` 那条 channel 路），
    // 那会改它对前端的返回形状 —— 是另一件事，不在本件里顺手做。
    let raw = crate::remote_history::run_list_query(
        &cfg,
        &format!("--read-session {}", crate::ssh_source::shell_quote(&picked_str)),
    )
    .await?;
    let records = raw
        .iter()
        .filter_map(|l| serde_json::from_str::<JsonlRecord>(l.trim()).ok())
        .collect();
    Ok(SubagentLoadResult {
        path: picked_str,
        agent_id,
        records,
    })
}

#[tauri::command]
pub async fn load_subagent(
    parent_jsonl_path: String,
    description: String,
    tool_use_timestamp: String,
    origin: Option<String>,
) -> Result<SubagentLoadResult, String> {
    // P7c-1：远端会话的 subagent 记录在远端机器上 ⇒ 按 origin 分流。
    if let Some(o) = origin.as_deref().filter(|o| !o.is_empty()) {
        return load_subagent_remote(o, &parent_jsonl_path, &description, &tool_use_timestamp).await;
    }
    let parent = PathBuf::from(&parent_jsonl_path);
    let subagent_dir = derive_subagent_dir(&parent)
        .ok_or_else(|| format!("cannot derive subagent dir from {parent_jsonl_path}"))?;
    if !subagent_dir.is_dir() {
        return Err(format!("no subagent dir: {}", subagent_dir.display()));
    }

    let metas = list_meta_matches(&subagent_dir, &description)
        .map_err(|e| format!("scan {}: {e}", subagent_dir.display()))?;
    if metas.is_empty() {
        return Err(format!(
            "no subagent matches description={description:?} under {}",
            subagent_dir.display()
        ));
    }

    // 本机这条**自己读**首行时间戳，喂给共用的 `pick_closest`；
    // 远端那条的时间戳由 daemon 的 `--list-subagents` 一次带回来（少 N 次往返）。
    let metas: Vec<(PathBuf, Option<String>)> = metas
        .into_iter()
        .map(|p| {
            let ts = first_line_timestamp(&p);
            (p, ts)
        })
        .collect();
    let picked = pick_closest(metas, &tool_use_timestamp);
    let agent_id = extract_agent_id(&picked).unwrap_or_default();

    let records = read_jsonl(&picked).map_err(|e| format!("read {}: {e}", picked.display()))?;

    Ok(SubagentLoadResult {
        path: picked.to_string_lossy().into_owned(),
        agent_id,
        records,
    })
}

/// `<dir>/<file>.jsonl` → `<dir>/<file>/subagents`
fn derive_subagent_dir(parent_jsonl: &Path) -> Option<PathBuf> {
    let stem = parent_jsonl.file_stem()?.to_str()?;
    let dir = parent_jsonl.parent()?;
    Some(dir.join(stem).join("subagents"))
}

/// 扫 `dir/*.meta.json`，返回 description 精确匹配的 .jsonl 文件路径。
fn list_meta_matches(dir: &Path, description: &str) -> std::io::Result<Vec<PathBuf>> {
    let mut hits = Vec::new();
    for entry in fs::read_dir(dir)?.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !name.ends_with(".meta.json") {
            continue;
        }
        let raw = match fs::read_to_string(&p) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let meta: SubagentMeta = match serde_json::from_str(&raw) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.description.as_deref() == Some(description) {
            // .meta.json → .jsonl（strip_suffix 比 trim_end_matches 语义更清晰）
            if let Some(stem) = name.strip_suffix(".meta.json") {
                let jsonl = p.with_file_name(format!("{stem}.jsonl"));
                if jsonl.exists() {
                    hits.push(jsonl);
                }
            }
        }
    }
    Ok(hits)
}

/// 多个 description 相同的候选 → 取首行 timestamp 与 `tool_use_timestamp` 差距最小的。
///
/// ★★ **P7c-1：它吃 `(路径, 时间戳)` 对，不再自己去读文件。**
///
/// 原来它拿 `Vec<PathBuf>` 并在内部 `first_line_timestamp(p)` —— 那样**只有本机能用**
/// （远端的文件不在本机文件系统上）。改成吃对之后，本机与远端喂给它的是**同一种东西**，
/// 挑选逻辑因此只有一份 —— 定框 `C1` 要的正是这个（daemon 侧只列候选、不挑，
/// 由 `the_daemon_never_matches_or_ranks_subagents` 钉住）。
fn pick_closest(
    mut candidates: Vec<(PathBuf, Option<String>)>,
    tool_use_timestamp: &str,
) -> PathBuf {
    if candidates.len() == 1 {
        return candidates.remove(0).0;
    }
    let target = crate::utils::parse_iso8601_ms(tool_use_timestamp);
    candidates.sort_by_key(|(_, ts)| {
        let first_ts = ts.as_deref().and_then(crate::utils::parse_iso8601_ms);
        match (target, first_ts) {
            (Some(t), Some(f)) => (f - t).abs(),
            _ => i64::MAX,
        }
    });
    candidates.into_iter().next().unwrap().0
}

fn first_line_timestamp(path: &Path) -> Option<String> {
    let f = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(f);
    let mut buf = String::new();
    reader.read_line(&mut buf).ok()?;
    let v: serde_json::Value = serde_json::from_str(buf.trim()).ok()?;
    v.get("timestamp")?.as_str().map(str::to_string)
}

// P3 归并：parse_ts_ms 已搬到 utils::parse_iso8601_ms（多处复用）。
// 跨月 / 跨年 / 闰年单调性由 utils::days_from_civil 保证（utils 自带回归测试）。

fn extract_agent_id(jsonl_path: &Path) -> Option<String> {
    let stem = jsonl_path.file_stem()?.to_str()?;
    // 文件名形如 agent-<hash>
    stem.strip_prefix("agent-").map(str::to_string)
}

fn read_jsonl(path: &Path) -> std::io::Result<Vec<JsonlRecord>> {
    let f = fs::File::open(path)?;
    let reader = BufReader::new(f);
    let mut out = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
        match parse_line(&line) {
            Ok(Some(rec)) => out.push(rec),
            Ok(None) => {}
            Err(e) => tracing::warn!("subagent parse skip: {e}"),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    /// ★ P7c1-Y2：**挑选逻辑只有一份** —— 本机与远端两条路都喂同一个 `pick_closest`。
    ///
    /// 钉的是数据流：`pick_closest` 吃 `(路径, 时间戳)` 对（不再自己读文件），
    /// 而两条路各自把时间戳凑齐后交给它。
    #[test]
    fn both_paths_feed_the_same_picker() {
        let prod = guard_core::production_code(include_str!("subagent.rs"));
        // 定义 + 两个调用点 = 3 处。
        assert_eq!(
            prod.matches("pick_closest(").count(),
            3,
            "`pick_closest` 的定义 + 两个调用点 = 3 处。变了就是有人另起了一条挑选路。"
        );
        // 它不许再自己去读文件 —— 那样只有本机能用，远端那条会被逼着复制一份（`C1` 排除）。
        // ⚠ 窗口要**按函数边界**截，不能拍一个字节数：第一版取 900 字节，
        // 越过了函数末尾、吃进紧跟其后的 `first_line_timestamp` 定义 ⇒ 假红。
        let at = prod.find("fn pick_closest(").expect("找不到 pick_closest");
        let rest = &prod[at + "fn pick_closest(".len()..];
        let end = rest.find("\nfn ").unwrap_or(rest.len());
        let body = &rest[..end];
        assert!(
            !body.contains("first_line_timestamp("),
            "`pick_closest` 又自己去读文件了 —— 那样远端那条用不了它"
        );
    }

    /// ★ P7c1-Y3：远端那条真的**发命令**，而不是只把降级删了。
    #[test]
    fn the_remote_path_actually_asks_the_daemon() {
        let prod = guard_core::production_code(include_str!("subagent.rs"));
        assert!(
            prod.contains("--list-subagents "),
            "远端那条没调 daemon 的 `--list-subagents` —— 降级删了却没接上，点开就是报错"
        );
        // 内容走**既有**的 `--read-session`，不新造读口。
        assert!(
            prod.contains("--read-session "),
            "远端那条没复用既有的 `--read-session` —— 别为同一件事新造第二个读口"
        );
        assert!(
            prod.contains("if let Some(o) = origin.as_deref()"),
            "`load_subagent` 不再按 origin 分流"
        );
    }
}
