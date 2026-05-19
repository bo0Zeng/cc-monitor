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
pub struct SubagentLoadResult {
    /// 命中的 jsonl 文件路径（用于前端 debug / 状态栏显示）
    pub path: String,
    /// agentId（从文件名 `agent-<id>.jsonl` 提取）
    pub agent_id: String,
    pub records: Vec<JsonlRecord>,
}

#[tauri::command]
pub fn load_subagent(
    parent_jsonl_path: String,
    description: String,
    tool_use_timestamp: String,
) -> Result<SubagentLoadResult, String> {
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

/// 多个 description 相同的候选 → 读每个文件首行 timestamp，取与 tool_use_timestamp 差距最小的。
fn pick_closest(mut candidates: Vec<PathBuf>, tool_use_timestamp: &str) -> PathBuf {
    if candidates.len() == 1 {
        return candidates.remove(0);
    }
    let target = parse_ts_ms(tool_use_timestamp);
    candidates.sort_by_key(|p| {
        let first_ts = first_line_timestamp(p).and_then(|s| parse_ts_ms(&s));
        match (target, first_ts) {
            (Some(t), Some(f)) => (f - t).abs(),
            _ => i64::MAX,
        }
    });
    candidates.into_iter().next().unwrap()
}

fn first_line_timestamp(path: &Path) -> Option<String> {
    let f = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(f);
    let mut buf = String::new();
    reader.read_line(&mut buf).ok()?;
    let v: serde_json::Value = serde_json::from_str(buf.trim()).ok()?;
    v.get("timestamp")?.as_str().map(str::to_string)
}

/// 解析 ISO 8601 至 unix ms。失败返回 None；只用于排序。
/// 形如 `2026-05-11T03:17:35.109Z` 或 `2026-05-11T03:17:35Z`。
///
/// 用 Howard Hinnant 的 days_from_civil 算法把 (y,m,d) 映射到 unix epoch
/// 起算的天数 —— 这条公式跨月跨年单调。原实现 `(y*12+m)*31+d` 在月末/月
/// 初边界不单调（例如 2026-01-31 与 2026-02-01 之间会产生异常大的差），
/// 多 subagent candidate 排序会选错。
fn parse_ts_ms(s: &str) -> Option<i64> {
    let s = s.trim_end_matches('Z');
    let (date, time) = s.split_once('T')?;
    let mut y_m_d = date.split('-');
    let y: i64 = y_m_d.next()?.parse().ok()?;
    let mo: i64 = y_m_d.next()?.parse().ok()?;
    let d: i64 = y_m_d.next()?.parse().ok()?;
    let (hms, frac) = time.split_once('.').unwrap_or((time, "0"));
    let mut hms_p = hms.split(':');
    let h: i64 = hms_p.next()?.parse().ok()?;
    let mi: i64 = hms_p.next()?.parse().ok()?;
    let se: i64 = hms_p.next()?.parse().ok()?;
    let ms: i64 = frac.parse().unwrap_or(0);

    let days = days_from_civil(y, mo, d);
    Some(days * 86_400_000 + h * 3_600_000 + mi * 60_000 + se * 1000 + ms)
}

/// Howard Hinnant 的 days_from_civil：把 (y,m,d) 转换为相对 1970-01-01 的天数。
/// 跨月/跨年/闰年都单调。参考 http://howardhinnant.github.io/date_algorithms.html
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // Mar=0..Feb=11
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

#[cfg(test)]
mod ts_tests {
    use super::parse_ts_ms;

    #[test]
    fn cross_month_monotonic() {
        let a = parse_ts_ms("2026-01-31T23:59:59.000Z").unwrap();
        let b = parse_ts_ms("2026-02-01T00:00:00.000Z").unwrap();
        assert!(b > a, "Feb 1 ms ({b}) must be greater than Jan 31 ms ({a})");
        assert_eq!(b - a, 1000, "diff should be exactly 1 second across month boundary");
    }

    #[test]
    fn cross_year_monotonic() {
        let a = parse_ts_ms("2025-12-31T23:59:59.000Z").unwrap();
        let b = parse_ts_ms("2026-01-01T00:00:00.000Z").unwrap();
        assert_eq!(b - a, 1000);
    }

    #[test]
    fn leap_year() {
        let a = parse_ts_ms("2024-02-28T00:00:00.000Z").unwrap();
        let b = parse_ts_ms("2024-02-29T00:00:00.000Z").unwrap();
        let c = parse_ts_ms("2024-03-01T00:00:00.000Z").unwrap();
        assert_eq!(b - a, 86_400_000);
        assert_eq!(c - b, 86_400_000);
    }
}

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
