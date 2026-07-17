//! F88a（#52）：会话用量聚合——按 (会话, 模型, 天) 累加 token。**只 token 不 $**（用户 2026-07-17 拍板：
//! cc-monitor 无 API key/不联网、定价会过期要维护 → 只做「已花费 token」这半，不做费用）。
//!
//! 数据源现成：`ApiMessage.usage`（`messages.rs:231`）挂在 assistant 记录上。本模块复用 history 的
//! 项目/会话遍历骨架（`history.rs:135/221`），每会话逐行**过 `parse_line`**（SS-16 唯一解析缝）累加。
//! 纯读纯算、不写任何 Claude 数据。用量视图按需触发（非 history 热路径），全扫可接受（后续可加增量缓存）。
//!
//! **硬边界**：只做「已花费」，**不做「配额还剩多少」**（`/usage` 5h/周窗口 = 账号级服务端数据，本地
//! jsonl 推不出）——UI 必须标死。context 窗% 的模型上限表在前端 `pricing.ts`（不在此模块）。

use crate::messages::{JsonlRecord, Usage};
use crate::parser::parse_line;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use tauri::ipc::Channel;

/// 一个 (会话/模型/天) 桶的 token 合计。u64 防大历史累加溢出。
#[derive(serde::Serialize, Clone, Default, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotals {
    pub input: u64,
    pub cache_creation: u64,
    pub cache_read: u64,
    pub output: u64,
    pub msgs: u32,
}

impl UsageTotals {
    fn add(&mut self, u: &Usage) {
        self.input += u.input_tokens as u64;
        self.cache_creation += u.cache_creation as u64;
        self.cache_read += u.cache_read as u64;
        self.output += u.output_tokens as u64;
        self.msgs += 1;
    }
}

/// 一条会话在某 (模型, 天) 下的用量。
#[derive(serde::Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UsageBucket {
    pub model: String,
    pub day: String, // "yyyy-mm-dd"（ISO timestamp 前 10 字符）
    pub totals: UsageTotals,
}

/// 一个会话的用量行（含项目归属；前端按 会话/天/项目/模型 四维 pivot）。wire camelCase。
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsageRow {
    pub session_id: String,
    pub project_path: String,
    pub project_name: String,
    pub buckets: Vec<UsageBucket>,
    /// None=本地；Some(host)=远端（对齐 HistorySessionEntry.origin）。本地聚合恒 None。
    pub origin: Option<String>,
}

/// 纯累加：逐行过 `parse_line`，把 assistant 记录的 usage 累进 (model, day) 桶，并捕获会话 cwd
/// （首条 user 记录）。抽出便于单测（不落文件）。model 缺失→`"unknown"`；day 取 timestamp 前 10 字符。
fn accumulate_usage(
    lines: impl Iterator<Item = String>,
    seen_uuids: &mut HashSet<String>,
) -> (HashMap<(String, String), UsageTotals>, Option<String>) {
    let mut buckets: HashMap<(String, String), UsageTotals> = HashMap::new();
    let mut cwd: Option<String> = None;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let rec = match parse_line(trimmed) {
            Ok(Some(r)) => r,
            _ => continue, // 解析失败/空行/Unrecognized 无 usage → 跳过，不崩
        };
        match &rec {
            JsonlRecord::User { cwd: c, .. } => {
                if cwd.is_none() {
                    if let Some(v) = c {
                        cwd = Some(v.clone());
                    }
                }
            }
            JsonlRecord::Assistant {
                uuid,
                message,
                timestamp,
                ..
            } => {
                if let Some(u) = &message.usage {
                    // F88a 审计修：`/branch` 建分支会把祖先记录（连 message.usage、**保留原 uuid**、只改
                    // sessionId）逐字段复制进新会话文件（见 history.rs::build_branch_records）。逐会话累加
                    // 无去重 → 共享前缀的 token 在源会话 + 每个分支各计一次 = 合计虚高。**跨聚合按 assistant
                    // uuid 去重**（首次遇到的会话计入，后续跳过）。普通 resume 追加同文件不复制、uuid 唯一，无影响。
                    if !seen_uuids.insert(uuid.clone()) {
                        continue;
                    }
                    let model = message
                        .model
                        .clone()
                        .filter(|m| !m.is_empty())
                        .unwrap_or_else(|| "unknown".to_string());
                    let day = timestamp.get(0..10).unwrap_or("").to_string();
                    buckets.entry((model, day)).or_default().add(u);
                }
            }
            _ => {}
        }
    }
    (buckets, cwd)
}

/// 扫一个会话 jsonl → 该会话的用量行。无任何 usage 的会话返 None（不报空行）。
fn analyze_usage_in_session(
    path: &Path,
    seen_uuids: &mut HashSet<String>,
) -> Option<SessionUsageRow> {
    let session_id = path.file_stem()?.to_str()?.to_string();
    let file = File::open(path).ok()?;
    let (buckets, cwd) = accumulate_usage(
        BufReader::new(file).lines().map_while(Result::ok),
        seen_uuids,
    );
    if buckets.is_empty() {
        return None;
    }
    let project_path = cwd.unwrap_or_default();
    let project_name = Path::new(&project_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&project_path)
        .to_string();
    let mut buckets: Vec<UsageBucket> = buckets
        .into_iter()
        .map(|((model, day), totals)| UsageBucket { model, day, totals })
        .collect();
    // 稳定序（天降序、同天按模型）——前端展示 + 测试确定性。
    buckets.sort_by(|a, b| b.day.cmp(&a.day).then_with(|| a.model.cmp(&b.model)));
    Some(SessionUsageRow {
        session_id,
        project_path,
        project_name,
        buckets,
        origin: None,
    })
}

/// F88a：聚合本地全部会话的用量，流式发行（每会话一行）。前端 drop channel → 取消。
/// 纯读、不需 SessionMap。走 history 同款项目/会话遍历骨架（`history.rs:141-162/221-237`）。
#[tauri::command]
pub async fn aggregate_usage_all(on_row: Channel<SessionUsageRow>) -> Result<u32, String> {
    tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let claude_dir = crate::paths::resolve_claude_dir().ok_or("claude dir not found")?;
        let projects_dir = crate::adapter::records_dir(&claude_dir);
        if !projects_dir.exists() {
            return Ok(0);
        }
        let mut count = 0u32;
        // 跨全部会话文件的 assistant uuid 去重集（防分支复制的祖先记录重复计，见 accumulate_usage）。
        let mut seen_uuids: HashSet<String> = HashSet::new();
        let proj_iter = std::fs::read_dir(&projects_dir)
            .map_err(|e| format!("read {}: {e}", projects_dir.display()))?;
        for proj in proj_iter.flatten() {
            let proj_path = proj.path();
            if !proj_path.is_dir() {
                continue;
            }
            let files = match std::fs::read_dir(&proj_path) {
                Ok(d) => d,
                Err(_) => continue,
            };
            for f in files.flatten() {
                let p = f.path();
                if !crate::adapter::has_record_ext(&p) {
                    continue;
                }
                if let Some(row) = analyze_usage_in_session(&p, &mut seen_uuids) {
                    if on_row.send(row).is_err() {
                        tracing::info!("aggregate_usage_all: cancelled at {count} sessions");
                        return Ok(count);
                    }
                    count += 1;
                }
            }
        }
        tracing::info!(
            "aggregate_usage_all: {count} sessions in {}ms",
            started.elapsed().as_millis()
        );
        Ok(count)
    })
    .await
    .map_err(|e| format!("spawn_blocking join: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    // JsonlRecord::User/Assistant 必需 uuid（无 default），缺了会落 Unrecognized（无 usage）。
    fn assistant(model: &str, day: &str, i: u32, cc: u32, cr: u32, o: u32) -> String {
        format!(
            r#"{{"type":"assistant","uuid":"u-{model}-{day}-{i}-{o}","timestamp":"{day}T10:00:00Z","message":{{"role":"assistant","content":[],"model":"{model}","usage":{{"input_tokens":{i},"cache_creation_input_tokens":{cc},"cache_read_input_tokens":{cr},"output_tokens":{o}}}}}}}"#
        )
    }
    fn user(cwd: &str) -> String {
        format!(
            r#"{{"type":"user","uuid":"uu","cwd":"{cwd}","timestamp":"2026-07-17T09:00:00Z","message":{{"role":"user","content":"hi"}}}}"#
        )
    }

    #[test]
    fn accumulates_per_model_and_day() {
        let lines = vec![
            user("/home/u/proj"),
            assistant("claude-opus-4-8", "2026-07-17", 100, 10, 500, 20),
            assistant("claude-opus-4-8", "2026-07-17", 50, 0, 200, 10),
            assistant("claude-haiku-4-5", "2026-07-18", 5, 0, 0, 3),
        ];
        let (buckets, cwd) = accumulate_usage(lines.into_iter(), &mut HashSet::new());
        assert_eq!(cwd.as_deref(), Some("/home/u/proj"));
        // opus/17 号两条合并
        let opus = &buckets[&("claude-opus-4-8".into(), "2026-07-17".into())];
        assert_eq!(opus.input, 150);
        assert_eq!(opus.cache_read, 700);
        assert_eq!(opus.output, 30);
        assert_eq!(opus.msgs, 2);
        // haiku/18 号独立桶
        let haiku = &buckets[&("claude-haiku-4-5".into(), "2026-07-18".into())];
        assert_eq!(haiku.input, 5);
        assert_eq!(haiku.msgs, 1);
        assert_eq!(buckets.len(), 2);
    }

    #[test]
    fn skips_lines_without_usage_and_unparseable() {
        let lines = vec![
            user("/p"),
            r#"{"type":"assistant","uuid":"a1","timestamp":"2026-07-17T10:00:00Z","message":{"role":"assistant","content":[],"model":"m"}}"#.to_string(), // 无 usage
            "not json at all".to_string(),
            "".to_string(),
            r#"{"type":"summary","summary":"x"}"#.to_string(), // 其它类型
            assistant("m", "2026-07-17", 1, 0, 0, 1),
        ];
        let (buckets, _) = accumulate_usage(lines.into_iter(), &mut HashSet::new());
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[&("m".into(), "2026-07-17".into())].msgs, 1);
    }

    #[test]
    fn missing_or_empty_model_falls_back_to_unknown() {
        let lines = vec![
            r#"{"type":"assistant","uuid":"a2","timestamp":"2026-07-17T10:00:00Z","message":{"role":"assistant","content":[],"usage":{"input_tokens":7,"output_tokens":2}}}"#.to_string(),
        ];
        let (buckets, _) = accumulate_usage(lines.into_iter(), &mut HashSet::new());
        let b = &buckets[&("unknown".into(), "2026-07-17".into())];
        assert_eq!(b.input, 7);
        assert_eq!(b.output, 2);
        assert_eq!(b.cache_read, 0);
    }

    #[test]
    fn empty_input_yields_empty() {
        let (buckets, cwd) = accumulate_usage(std::iter::empty(), &mut HashSet::new());
        assert!(buckets.is_empty());
        assert!(cwd.is_none());
    }

    #[test]
    fn dedups_same_uuid_across_sessions() {
        // F88a 审计修：分支复制——同一 assistant uuid 出现在两个会话文件里，跨聚合共享 seen 集只计一次。
        let mut seen = HashSet::new();
        let a = r#"{"type":"assistant","uuid":"shared","timestamp":"2026-07-17T10:00:00Z","message":{"role":"assistant","content":[],"model":"m","usage":{"input_tokens":100,"output_tokens":20}}}"#.to_string();
        let (b1, _) = accumulate_usage(std::iter::once(a.clone()), &mut seen);
        assert_eq!(b1[&("m".into(), "2026-07-17".into())].input, 100);
        // 第二个会话含同 uuid → 跳过（不重复计），合计不虚高。
        let (b2, _) = accumulate_usage(std::iter::once(a), &mut seen);
        assert!(b2.is_empty(), "同 uuid 跨会话应去重");
    }
}
