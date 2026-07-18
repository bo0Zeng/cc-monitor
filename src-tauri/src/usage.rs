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
    /// 逐字段取 MAX（同一 requestId 的多条流式记录用）：一次 API 请求在 jsonl 落成多条 assistant
    /// 记录（thinking/text/各 tool_use 各一行），`message.usage` 挂**每一行**——但 `input`/`cache_*`
    /// 是**请求级、逐行完全相同**，`output` 是**流式**（前几条占位小值、终结记录才是真总量）。故按
    /// requestId 聚合时逐字段 MAX：prompt 侧近恒定→max 无害；output 单调→max=终结值。**两者皆正确。**
    /// 不动 `msgs`（msgs 在 flush 时按「每请求 +1」，见 accumulate_usage）。
    fn max_with(&mut self, u: &Usage) {
        self.input = self.input.max(u.input_tokens as u64);
        self.cache_creation = self.cache_creation.max(u.cache_creation as u64);
        self.cache_read = self.cache_read.max(u.cache_read as u64);
        self.output = self.output.max(u.output_tokens as u64);
    }

    /// 把一个「每请求 MAX」结果加进桶：各字段累加 + `msgs += 1`（一次请求算一条 assistant 轮次）。
    fn add_request(&mut self, req: &UsageTotals) {
        self.input += req.input;
        self.cache_creation += req.cache_creation;
        self.cache_read += req.cache_read;
        self.output += req.output;
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

/// 纯聚合：逐行过 `parse_line`，把 assistant 记录的 usage **按 requestId 聚合**进 (model, day) 桶，
/// 并捕获会话 cwd（首条 user 记录）。抽出便于单测（不落文件）。model 缺失→`"unknown"`；day 取 timestamp 前 10 字符。
///
/// **★ 为什么按 requestId 逐字段 MAX 而非逐条 uuid 加**（P88a 审计修，实测真实 jsonl）：一次 API 请求
/// （`requestId`）在 jsonl 落成**多条** assistant 记录（thinking/text/各 tool_use 各一行），`message.usage`
/// 挂**每一行**——`input`/`cache_*` 请求级逐行重复、`output` 流式（前几条占位、终结记录才是真总量）。
/// - 旧的**逐条 uuid 加** → 每请求算 N 次 → 全机超计 ~2.5×（cache_read 主导）。
/// - **per-requestId first-wins** → output 抓到占位 → 少计 ~29%（子代理转录尤甚）。
/// - **正解 = per-requestId 逐字段 MAX**（`max_with`）：prompt 侧近恒定 max 无害、output 单调 max=终结值。
///   `msgs` 按「每请求 +1」（一次请求 = 一条 assistant 轮次）。requestId 缺失（旧版 CC，实测 0.05%）→ 回退 uuid。
///   `/branch` 祖先复制保留 requestId → 跨会话按 requestId 去重（`seen_requests`），同旧 uuid 去重的意图。
fn accumulate_usage(
    lines: impl Iterator<Item = String>,
    seen_requests: &mut HashSet<String>,
) -> (HashMap<(String, String), UsageTotals>, Option<String>) {
    let mut buckets: HashMap<(String, String), UsageTotals> = HashMap::new();
    let mut cwd: Option<String> = None;
    // 本文件内：requestId(缺→uuid) → (model, day, 逐字段 MAX usage)。见函数 doc 为何 MAX。
    let mut per_req: HashMap<String, (String, String, UsageTotals)> = HashMap::new();
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
                request_id,
                message,
                timestamp,
                ..
            } => {
                if let Some(u) = &message.usage {
                    let key = request_id.clone().unwrap_or_else(|| uuid.clone());
                    let model = message
                        .model
                        .clone()
                        .filter(|m| !m.is_empty())
                        .unwrap_or_else(|| "unknown".to_string());
                    let day = timestamp.get(0..10).unwrap_or("").to_string();
                    per_req
                        .entry(key)
                        .or_insert_with(|| (model, day, UsageTotals::default()))
                        .2
                        .max_with(u);
                }
            }
            _ => {}
        }
    }
    // flush：每个 requestId 跨文件去重（seen_requests，防 /branch 祖先复制同 requestId 重复计），
    // 其「逐字段 MAX」加进 (model, day) 桶一次（msgs+1/请求）。同一 requestId 的记录 model/day 一致。
    for (key, (model, day, usage_max)) in per_req {
        if !seen_requests.insert(key) {
            continue;
        }
        buckets
            .entry((model, day))
            .or_default()
            .add_request(&usage_max);
    }
    (buckets, cwd)
}

/// 扫一个会话 jsonl → 该会话的用量行。无任何 usage 的会话返 None（不报空行）。
fn analyze_usage_in_session(
    path: &Path,
    seen_requests: &mut HashSet<String>,
) -> Option<SessionUsageRow> {
    let session_id = path.file_stem()?.to_str()?.to_string();
    let file = File::open(path).ok()?;
    let (buckets, cwd) = accumulate_usage(
        BufReader::new(file).lines().map_while(Result::ok),
        seen_requests,
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
        // 跨全部会话文件的 requestId(缺→uuid) 去重集（防 /branch 复制的祖先记录重复计，见 accumulate_usage）。
        let mut seen_requests: HashSet<String> = HashSet::new();
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
                if let Some(row) = analyze_usage_in_session(&p, &mut seen_requests) {
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
    /// 带 requestId 的 assistant 记录（uuid 逐条唯一，requestId 可复用同请求多条记录）。
    #[allow(clippy::too_many_arguments)]
    fn assistant_req(
        req: &str,
        n: u32,
        model: &str,
        day: &str,
        i: u32,
        cc: u32,
        cr: u32,
        o: u32,
    ) -> String {
        format!(
            r#"{{"type":"assistant","uuid":"u-{req}-{n}","requestId":"{req}","timestamp":"{day}T10:00:00Z","message":{{"role":"assistant","content":[],"model":"{model}","usage":{{"input_tokens":{i},"cache_creation_input_tokens":{cc},"cache_read_input_tokens":{cr},"output_tokens":{o}}}}}}}"#
        )
    }

    #[test]
    fn same_request_id_takes_field_max_not_sum_or_first() {
        // 一次 API 请求(requestId=r1)落 3 条记录：input/cache 请求级逐行重复，output 流式
        // (占位 5 → 占位 5 → 终结 484)。正解=逐字段 MAX(input 2 / cache_read 19059 / output 484)、msgs=1。
        // 反例：逐条 uuid 加 → input 6 / output 494 / msgs 3(超计)；first-wins → output 5(少计)。
        let lines = vec![
            user("/p"),
            assistant_req("r1", 1, "m", "2026-07-17", 2, 8518, 19059, 5),
            assistant_req("r1", 2, "m", "2026-07-17", 2, 8518, 19059, 5),
            assistant_req("r1", 3, "m", "2026-07-17", 2, 8518, 19059, 484),
        ];
        let (buckets, _) = accumulate_usage(lines.into_iter(), &mut HashSet::new());
        let b = &buckets[&("m".into(), "2026-07-17".into())];
        assert_eq!(b.input, 2, "input 请求级、MAX=2（非逐条加 6）");
        assert_eq!(b.cache_creation, 8518);
        assert_eq!(b.cache_read, 19059, "cache_read MAX=19059（非 3×）");
        assert_eq!(
            b.output, 484,
            "output 取终结值 484（非 first-wins 5、非逐条加 494）"
        );
        assert_eq!(b.msgs, 1, "一请求算一条 assistant 轮次（非 3 条记录）");
        assert_eq!(buckets.len(), 1);
    }

    #[test]
    fn distinct_request_ids_sum_across_requests() {
        // 不同 requestId = 不同请求 → 各自 MAX 后跨请求累加。
        let lines = vec![
            assistant_req("r1", 1, "m", "2026-07-17", 10, 0, 100, 20),
            assistant_req("r2", 1, "m", "2026-07-17", 10, 0, 100, 30),
        ];
        let (buckets, _) = accumulate_usage(lines.into_iter(), &mut HashSet::new());
        let b = &buckets[&("m".into(), "2026-07-17".into())];
        assert_eq!(b.input, 20); // 10+10
        assert_eq!(b.output, 50); // 20+30
        assert_eq!(b.msgs, 2);
    }

    #[test]
    fn dedups_same_request_id_across_sessions() {
        // /branch 复制同一 requestId 进另一会话 → 跨聚合按 requestId 去重、只计一次。
        let mut seen = HashSet::new();
        let a = assistant_req("rbr", 1, "m", "2026-07-17", 100, 0, 0, 20);
        let (b1, _) = accumulate_usage(std::iter::once(a.clone()), &mut seen);
        assert_eq!(b1[&("m".into(), "2026-07-17".into())].input, 100);
        let (b2, _) = accumulate_usage(std::iter::once(a), &mut seen);
        assert!(b2.is_empty(), "同 requestId 跨会话去重");
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
