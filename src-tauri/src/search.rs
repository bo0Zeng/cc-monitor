//! 历史会话全文搜索（issue #6）。
//!
//! ## 设计：启动后台建内存索引 + substring 查询
//!
//! 历史浏览器原本只能按项目名 / 标题过滤（前端本地 filter）。本模块补"搜会话
//! **内容**"：扫描 `<claude_dir>/projects/**/*.jsonl`，把每条 user/assistant 的
//! 文本抽出来建内存索引，`search_history` IPC 做大小写不敏感 substring 查询。
//!
//! ## 搜什么（"只搜有用内容"）
//!
//! - **默认**：user 输入文本 + assistant 回复文本（text block）。CLI 注入的包装
//!   （`<system-reminder>` / `<task-notification>` / `[Request interrupted by user]`
//!   等）按 INVARIANT § 20 的同一意图剥掉 —— 搜索只命中真内容。
//! - **可选**（`include_tools=true`）：tool_use（名字+入参）/ tool_result（输出）/
//!   thinking。前端一个复选框控制。
//! - **范围 / 时间筛选**（v2.7.1）：`scope`（all/user/assistant，按记录类型过滤）+
//!   `after_ms`（只搜该时刻之后的消息）。两者在扫描时过滤，不影响两级匹配性能。
//!
//! ## 性能（off 热路径 + 两级匹配 + 截断）
//!
//! 1. **后台构建**：`build_blocking` 在独立线程跑（启动后延迟一会儿，先让首屏 replay
//!    跑完不抢磁盘）。索引未就绪时 `search_history` 返回 `status="indexing"`。
//! 2. **两级匹配**：粗筛用预先小写化的 `*_lc` 做 SIMD 优化的 `str::contains`（扫全部
//!    消息，快）；只有命中的 ≤limit 条才跑 `find_ci`（在原文上 char 对齐定位，给
//!    snippet 用）—— 贵的活只在结果上做。
//! 3. **截断**：tool 输出可能是几百 KB 的文件 dump；索引时按字符截断（MAIN/TOOL_CAP），
//!    把内存与扫描成本封顶。
//! 4. **snippet 三段返回**：`{before, matched, after}`，前端把 matched 包 `<mark>`。
//!    不跨 Rust char 索引 / JS UTF-16 索引传 offset（中文 / emoji 安全）。
//!
//! 内存：每条消息存原文 + 小写副本（2×文本）。典型用户几百会话 < ~100MB，符合 issue
//! 预算。重建走 `rebuild_search_index`（手动刷新）。

use crate::messages::JsonlRecord;
use crate::parser::parse_line;
use crate::paths;
use crate::utils::{parse_iso8601_ms, systime_to_ms};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use walkdir::WalkDir;

/// 单条 main 文本索引上限（字符）。user/assistant 正文极少超，但兜底防超长粘贴。
const MAIN_CAP: usize = 20_000;
/// 单条 tool 文本索引上限（字符）。tool_result 可能是大文件 dump，必须封顶。
const TOOL_CAP: usize = 4_000;
/// snippet 命中点前后各保留的字符数。
const SNIPPET_CTX: usize = 48;
/// 单会话最多返回的命中条目数（防一个会话刷屏；hit_count 仍报全量）。
const PER_SESSION_CAP: usize = 30;

// === wire 类型（camelCase，契约测试守护） ===

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    /// "ready" | "indexing"
    pub status: String,
    /// 全局命中总数（可能 > 返回的 hit 条数）
    pub total_hits: u32,
    /// 返回的会话组数
    pub session_count: u32,
    /// 是否因 limit / 每会话上限截断了返回
    pub truncated: bool,
    /// 已索引的会话数（status=indexing 时给 UI 显示进度）
    pub indexed_sessions: u32,
    /// 已索引的消息数
    pub indexed_messages: u32,
    pub sessions: Vec<SessionHits>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SessionHits {
    pub session_id: String,
    pub project_path: String,
    pub project_name: String,
    pub jsonl_path: String,
    /// ai-title / 首条 user 摘要 / sid 前 8 位 之一
    pub title: String,
    // **C03 大整数策略**：量纲是**毫秒时间戳**——2^53-1 ms ≈ **28.5 万年**。
    #[cfg_attr(test, ts(type = "number"))]
    pub updated_at: i64,
    /// 本会话命中总数（可能 > 返回的 hits 长度）
    pub hit_count: u32,
    pub hits: Vec<Hit>,
    /// issue #28：数据来源。`None` = 本地（不序列化，前端无 `[host]` 前缀）；
    /// `Some(label)` = 远端机器 label，前端据此加 `[host]` 前缀 + 点击走远端 viewer。
    /// daemon 的 `--search` 输出**不含** origin（远端无身份概念）；由 monitor fan-out
    /// 反序列化后补上。
    #[cfg_attr(test, ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    /// 消息 uuid，前端打开 viewer 后据此滚动定位 + 高亮
    pub uuid: String,
    // **C03 大整数策略**：量纲是**毫秒时间戳**——2^53-1 ms ≈ **28.5 万年**。
    // （这一处是**守卫指出来的**：`Hit` 是 `SessionHits` 的传递依赖，我没逐字段读它。）
    #[cfg_attr(test, ts(type = "number"))]
    pub ts_ms: i64,
    /// "user" | "assistant" | "tool"
    pub kind: String,
    /// 命中点之前的上下文（已折叠换行）
    pub before: String,
    /// 命中的原文片段（前端包 <mark>）
    pub matched: String,
    /// 命中点之后的上下文
    pub after: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SearchIndexStatus {
    pub ready: bool,
    pub indexed_sessions: u32,
    pub indexed_messages: u32,
    // **C03 大整数策略**：量纲是**毫秒时间戳**——2^53-1 ms ≈ **28.5 万年**。
    #[cfg_attr(test, ts(type = "number"))]
    pub built_at_ms: i64,
}

// === 内部索引数据 ===

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    User,
    Assistant,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::User => "user",
            Kind::Assistant => "assistant",
        }
    }
}

struct MsgDoc {
    uuid: String,
    ts_ms: i64,
    kind: Kind,
    /// user/assistant 正文（原文，给 snippet）
    main: String,
    /// main 的小写副本（给粗筛 contains）
    main_lc: String,
    /// tool 内容（原文，可能空）
    tool: String,
    tool_lc: String,
}

struct SessionDoc {
    session_id: String,
    project_path: String,
    project_name: String,
    jsonl_path: String,
    title: String,
    updated_at: i64,
    msgs: Vec<MsgDoc>,
}

struct IndexData {
    ready: bool,
    sessions: Vec<SessionDoc>,
    total_messages: usize,
    built_at_ms: i64,
}

/// `app.manage` 的全局搜索索引 State。
pub struct SearchIndex {
    inner: RwLock<IndexData>,
}

impl SearchIndex {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(IndexData {
                ready: false,
                sessions: Vec::new(),
                total_messages: 0,
                built_at_ms: 0,
            }),
        }
    }

    /// 后台线程调：扫描全部 jsonl 建索引。**阻塞**，调用方放独立线程里跑。
    ///
    /// 启动时先 sleep 一会儿，避开首屏 replay 的磁盘 / CPU 争用（索引不是关键路径，
    /// 晚几秒就绪没关系，UI 在那之前显示"索引中"）。
    pub fn build_blocking(&self, claude_dir: &Path, startup_delay: Duration) {
        if !startup_delay.is_zero() {
            std::thread::sleep(startup_delay);
        }
        let started = Instant::now();
        let projects_dir = crate::adapter::records_dir(claude_dir);
        if !projects_dir.is_dir() {
            let mut data = self.inner.write();
            data.ready = true;
            data.built_at_ms = crate::utils::now_ms();
            tracing::info!("search index: no projects dir, empty index ready");
            return;
        }

        // 收集所有 jsonl 路径（projects/<encoded>/<sid>.jsonl，max_depth=2）
        let files: Vec<PathBuf> = WalkDir::new(&projects_dir)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file() && crate::adapter::has_record_ext(e.path()))
            .map(|e| e.into_path())
            .collect();

        let sessions = build_sessions_parallel(files);
        let total_messages: usize = sessions.iter().map(|s| s.msgs.len()).sum();

        let session_count = sessions.len();
        {
            let mut data = self.inner.write();
            data.sessions = sessions;
            data.total_messages = total_messages;
            data.ready = true;
            data.built_at_ms = crate::utils::now_ms();
        }
        tracing::info!(
            "[perf] search index built: {} sessions, {} messages in {}ms",
            session_count,
            total_messages,
            started.elapsed().as_millis()
        );
    }

    fn status(&self) -> SearchIndexStatus {
        let data = self.inner.read();
        SearchIndexStatus {
            ready: data.ready,
            indexed_sessions: data.sessions.len() as u32,
            indexed_messages: data.total_messages as u32,
            built_at_ms: data.built_at_ms,
        }
    }

    /// 执行查询。query 已 trim；空 query 返回空结果。
    ///
    /// - `scope`：None=全部消息；Some(Kind)=只搜该类型记录（只 user / 只 assistant）。
    /// - `after_ms`：>0 时只搜 ts_ms >= after_ms 的消息（时间范围筛选）；0=不限。
    fn query(
        &self,
        query: &str,
        include_tools: bool,
        scope: Option<Kind>,
        after_ms: i64,
        limit: usize,
    ) -> SearchResponse {
        let data = self.inner.read();
        let indexed_sessions = data.sessions.len() as u32;
        let indexed_messages = data.total_messages as u32;

        if !data.ready {
            return SearchResponse {
                status: "indexing".into(),
                total_hits: 0,
                session_count: 0,
                truncated: false,
                indexed_sessions,
                indexed_messages,
                sessions: Vec::new(),
            };
        }

        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return SearchResponse {
                status: "ready".into(),
                total_hits: 0,
                session_count: 0,
                truncated: false,
                indexed_sessions,
                indexed_messages,
                sessions: Vec::new(),
            };
        }

        // 会话按 updated_at desc 排序，让有限的 snippet 预算优先给最近的会话。
        let mut order: Vec<usize> = (0..data.sessions.len()).collect();
        order.sort_by(|&a, &b| {
            data.sessions[b]
                .updated_at
                .cmp(&data.sessions[a].updated_at)
        });

        let mut total_hits: u32 = 0;
        let mut kept: usize = 0;
        let mut truncated = false;
        let mut out: Vec<SessionHits> = Vec::new();

        for &si in &order {
            let sd = &data.sessions[si];
            let mut hits: Vec<Hit> = Vec::new();
            let mut session_hit_count: u32 = 0;

            for m in &sd.msgs {
                // 字段过滤：scope 指定时只搜该类型记录（只 user / 只 Claude）。
                if let Some(k) = scope {
                    if m.kind != k {
                        continue;
                    }
                }
                // 时间范围：after_ms>0 时只搜该时刻之后的消息。
                if after_ms > 0 && m.ts_ms < after_ms {
                    continue;
                }
                // 粗筛：先看 main，再（可选）看 tool。
                let in_main = m.main_lc.contains(&q);
                let in_tool = include_tools && !m.tool_lc.is_empty() && m.tool_lc.contains(&q);
                if !in_main && !in_tool {
                    continue;
                }
                session_hit_count += 1;
                total_hits += 1;

                // 只给"还在预算内"的命中构造 snippet（贵活只做这些）。
                if kept < limit && hits.len() < PER_SESSION_CAP {
                    let (kind, text) = if in_main {
                        (m.kind.as_str(), &m.main)
                    } else {
                        ("tool", &m.tool)
                    };
                    let (before, matched, after) = make_snippet(text, &q);
                    hits.push(Hit {
                        uuid: m.uuid.clone(),
                        ts_ms: m.ts_ms,
                        kind: kind.to_string(),
                        before,
                        matched,
                        after,
                    });
                    kept += 1;
                } else {
                    truncated = true;
                }
            }

            if session_hit_count > 0 {
                out.push(SessionHits {
                    session_id: sd.session_id.clone(),
                    project_path: sd.project_path.clone(),
                    project_name: sd.project_name.clone(),
                    jsonl_path: sd.jsonl_path.clone(),
                    title: sd.title.clone(),
                    updated_at: sd.updated_at,
                    hit_count: session_hit_count,
                    hits,
                    origin: None, // 本地结果无 origin（远端结果由 fan-out 补）
                });
            }
        }

        SearchResponse {
            status: "ready".into(),
            total_hits,
            session_count: out.len() as u32,
            truncated,
            indexed_sessions,
            indexed_messages,
            sessions: out,
        }
    }
}

impl Default for SearchIndex {
    fn default() -> Self {
        Self::new()
    }
}

// === 并行构建 ===

fn build_sessions_parallel(files: Vec<PathBuf>) -> Vec<SessionDoc> {
    if files.is_empty() {
        return Vec::new();
    }
    let nthreads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, 4)
        .min(files.len());

    if nthreads <= 1 {
        return files.iter().filter_map(|p| build_one(p)).collect();
    }

    // 把文件平均切成 nthreads 份，每线程独立建 Vec<SessionDoc> 再拼接。
    let chunk_size = files.len().div_ceil(nthreads);
    let chunks: Vec<&[PathBuf]> = files.chunks(chunk_size).collect();
    let mut result: Vec<SessionDoc> = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = chunks
            .into_iter()
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .filter_map(|p| build_one(p))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for h in handles {
            if let Ok(mut docs) = h.join() {
                result.append(&mut docs);
            }
        }
    });
    result
}

/// 解析一个 jsonl → SessionDoc。无任何可索引内容时返回 None。
fn build_one(path: &Path) -> Option<SessionDoc> {
    let session_id = crate::adapter::session_id_from_path(path)?;
    let file = File::open(path).ok()?;
    let updated_at = file
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .map(systime_to_ms)
        .unwrap_or(0);

    let reader = BufReader::new(file);
    let mut msgs: Vec<MsgDoc> = Vec::new();
    let mut cwd: Option<String> = None;
    let mut ai_title: Option<String> = None;
    let mut first_user_excerpt = String::new();

    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let rec = match parse_line(trimmed) {
            Ok(Some(r)) => r,
            _ => continue,
        };
        match rec {
            JsonlRecord::User {
                uuid,
                timestamp,
                message,
                cwd: c,
                ..
            } => {
                if cwd.is_none() {
                    if let Some(v) = c {
                        if !v.is_empty() {
                            cwd = Some(v);
                        }
                    }
                }
                let main = clean_user_text(&extract_text_blocks(&message.content));
                let tool = extract_tool_text(&message.content, false);
                if first_user_excerpt.is_empty() && !main.is_empty() {
                    first_user_excerpt = truncate_chars(&main, 120);
                }
                push_msg(&mut msgs, uuid, &timestamp, Kind::User, main, tool);
            }
            JsonlRecord::Assistant {
                uuid,
                timestamp,
                message,
                ..
            } => {
                let main = extract_text_blocks(&message.content);
                let tool = extract_tool_text(&message.content, true);
                push_msg(&mut msgs, uuid, &timestamp, Kind::Assistant, main, tool);
            }
            JsonlRecord::AiTitle { ai_title: t, .. } => ai_title = Some(t),
            JsonlRecord::CustomTitle {
                custom_title: t, ..
            } => ai_title = Some(t),
            _ => {}
        }
    }

    if msgs.is_empty() {
        return None;
    }

    let project_path = cwd.unwrap_or_default();
    let project_name = Path::new(&project_path)
        .file_name()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| project_path.clone());
    let title = ai_title
        .filter(|s| !s.trim().is_empty())
        .or_else(|| (!first_user_excerpt.is_empty()).then(|| first_user_excerpt.clone()))
        .unwrap_or_else(|| session_id.chars().take(8).collect());

    Some(SessionDoc {
        session_id,
        project_path,
        project_name,
        jsonl_path: path.to_string_lossy().into_owned(),
        title,
        updated_at,
        msgs,
    })
}

/// 构造一条 MsgDoc 并入列。main 与 tool 都空则跳过（无可搜内容）。
fn push_msg(
    msgs: &mut Vec<MsgDoc>,
    uuid: String,
    timestamp: &str,
    kind: Kind,
    main: String,
    tool: String,
) {
    let main = truncate_chars_plain(&main, MAIN_CAP);
    let tool = truncate_chars_plain(&tool, TOOL_CAP);
    if main.is_empty() && tool.is_empty() {
        return;
    }
    msgs.push(MsgDoc {
        uuid,
        ts_ms: parse_iso8601_ms(timestamp).unwrap_or(0),
        kind,
        main_lc: main.to_lowercase(),
        main,
        tool_lc: tool.to_lowercase(),
        tool,
    });
}

// === 文本抽取 ===

/// 抽 content 里所有 text block（或裸字符串）。assistant 正文 / user 正文都用它。
fn extract_text_blocks(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let mut out = String::new();
            for b in arr {
                if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(s) = b.get("text").and_then(|t| t.as_str()) {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(s);
                    }
                }
            }
            out
        }
        _ => String::new(),
    }
}

/// 抽 tool 相关内容（可选搜索）。
/// - assistant：tool_use（name + input JSON）+ thinking
/// - user：tool_result 的 content
fn extract_tool_text(content: &Value, is_assistant: bool) -> String {
    let Value::Array(arr) = content else {
        return String::new();
    };
    let mut out = String::new();
    let mut push = |s: &str| {
        if s.is_empty() {
            return;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(s);
    };
    for b in arr {
        match b.get("type").and_then(|t| t.as_str()) {
            Some("tool_use") if is_assistant => {
                if let Some(name) = b.get("name").and_then(|t| t.as_str()) {
                    push(name);
                }
                if let Some(input) = b.get("input") {
                    push(&stringify_json(input));
                }
            }
            Some("thinking") if is_assistant => {
                if let Some(t) = b.get("thinking").and_then(|t| t.as_str()) {
                    push(t);
                }
            }
            Some("tool_result") if !is_assistant => {
                if let Some(c) = b.get("content") {
                    push(&stringify_json(c));
                }
            }
            _ => {}
        }
    }
    out
}

/// 把 JSON 值压成可搜索的纯文本（string 直接取；array/object 取其中字符串叶子）。
fn stringify_json(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let mut out = String::new();
            for item in arr {
                // tool_result.content 常是 [{type:"text", text:"..."}]
                let s = if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                    t.to_string()
                } else {
                    stringify_json(item)
                };
                if !s.is_empty() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&s);
                }
            }
            out
        }
        Value::Object(_) => v.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
    }
}

/// 去掉 CLI 注入的 prompt 包装 + ESC 中断标记（INVARIANT § 20 同一意图，搜索用从宽）。
fn clean_user_text(s: &str) -> String {
    let mut out = s.to_string();
    for tag in [
        "task-notification",
        "system-reminder",
        "local-command-caveat",
        "local-command-stdout",
        "local-command-stderr",
    ] {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        while let (Some(i), Some(j)) = (out.find(&open), out.find(&close)) {
            if j > i {
                out.replace_range(i..j + close.len(), "");
            } else {
                break;
            }
        }
    }
    let trimmed = out.trim();
    // 纯 ESC 中断标记 → 不是真用户内容
    if trimmed.starts_with("[Request interrupted by user") {
        return String::new();
    }
    trimmed.to_string()
}

// === snippet ===

/// 在 `text` 里大小写不敏感地定位 `needle_lc`（已小写），返回前/中/后三段。
/// 找不到（理论上不会，调用前已 contains 粗筛过）则退化成开头窗口。
fn make_snippet(text: &str, needle_lc: &str) -> (String, String, String) {
    match find_ci(text, needle_lc) {
        Some((start, end)) => {
            let before_raw = &text[..start];
            let matched = &text[start..end];
            let after_raw = &text[end..];
            (
                tail_chars(before_raw, SNIPPET_CTX),
                collapse_ws(matched),
                head_chars(after_raw, SNIPPET_CTX),
            )
        }
        None => (
            String::new(),
            String::new(),
            head_chars(text, SNIPPET_CTX * 2),
        ),
    }
}

/// 大小写不敏感子串查找：返回原文里匹配区间的 (起始字节, 结束字节)。
/// 逐字符比对原文的小写展开 vs needle（needle 已小写）。只对粗筛命中的 ≤limit 条跑，
/// O(n·m) 可接受。能正确处理 CJK（无大小写）与 ASCII，多字符小写展开也对齐到原文字符边界。
fn find_ci(hay: &str, needle_lc: &str) -> Option<(usize, usize)> {
    if needle_lc.is_empty() {
        return None;
    }
    let needle: Vec<char> = needle_lc.chars().collect();
    let hay_idx: Vec<(usize, char)> = hay.char_indices().collect();
    let n = hay_idx.len();

    for i in 0..n {
        let mut hi = i; // 原文字符下标
        let mut ni = 0; // needle 字符下标
        let mut pending: Vec<char> = Vec::new(); // 当前原文字符的小写展开缓冲
        let mut pi = 0;
        let start_byte = hay_idx[i].0;
        let mut end_byte = start_byte;
        let mut ok = true;

        while ni < needle.len() {
            if pi >= pending.len() {
                if hi >= n {
                    ok = false;
                    break;
                }
                pending.clear();
                pending.extend(hay_idx[hi].1.to_lowercase());
                pi = 0;
                end_byte = if hi + 1 < n {
                    hay_idx[hi + 1].0
                } else {
                    hay.len()
                };
                hi += 1;
            }
            if pending[pi] != needle[ni] {
                ok = false;
                break;
            }
            pi += 1;
            ni += 1;
        }
        if ok && ni == needle.len() {
            return Some((start_byte, end_byte));
        }
    }
    None
}

/// 取字符串末尾 n 个字符（折叠空白），不足则全取；截断时前缀 …。
fn tail_chars(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let truncated = chars.len() > n;
    let slice: String = chars[chars.len().saturating_sub(n)..].iter().collect();
    let collapsed = collapse_ws(&slice);
    if truncated {
        format!("…{collapsed}")
    } else {
        collapsed
    }
}

/// 取字符串开头 n 个字符（折叠空白），截断时后缀 …。
fn head_chars(s: &str, n: usize) -> String {
    let mut out = String::new();
    let mut count = 0;
    for ch in s.chars() {
        if count >= n {
            out.push('…');
            break;
        }
        out.push(ch);
        count += 1;
    }
    collapse_ws_keep_ellipsis(&out)
}

/// 折叠所有空白（含换行）为单空格，trim。
fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 同 collapse_ws 但保留尾部 … 。
fn collapse_ws_keep_ellipsis(s: &str) -> String {
    let has_ellipsis = s.ends_with('…');
    let core = if has_ellipsis {
        &s[..s.len() - '…'.len_utf8()]
    } else {
        s
    };
    let collapsed = collapse_ws(core);
    if has_ellipsis {
        format!("{collapsed}…")
    } else {
        collapsed
    }
}

/// 按字符截断（保留换行原样，用于 excerpt），加 … 。
fn truncate_chars(s: &str, n: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= n {
            out.push('…');
            break;
        }
        out.push(if ch == '\n' || ch == '\r' { ' ' } else { ch });
    }
    out
}

/// 按字符硬截断（不加 …，用于索引正文封顶）。
fn truncate_chars_plain(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    s.chars().take(n).collect()
}

// === IPC ===

/// 全文搜索历史会话。query 大小写不敏感 substring 匹配。
///
/// - `include_tools`：是否附加搜索 tool_use / tool_result / thinking 内容。
/// - `scope`：搜索范围 `"all"`（默认）/ `"user"`（只我的输入）/ `"assistant"`（只 Claude 回复）。
/// - `after_ms`：时间范围下界（epoch ms）；只搜该时刻之后的消息，0 / 缺省 = 不限。
/// - `limit`：返回的命中条数上限（total_hits 仍报全量）。
///
/// 本地内存索引查询（CPU，spawn_blocking）与远端 fan-out（SSH，async）**并发**，
/// 合并成一个 `SearchResponse`（issue #28）。本地大索引几十 ms、远端 SSH 几百 ms，
/// 并发让总延迟≈max 而非和。
#[tauri::command]
pub async fn search_history(
    query: String,
    include_tools: bool,
    scope: Option<String>,
    after_ms: Option<i64>,
    limit: Option<usize>,
    index: tauri::State<'_, std::sync::Arc<SearchIndex>>,
) -> Result<SearchResponse, String> {
    let index = index.inner().clone();
    let limit = limit.unwrap_or(300).clamp(1, 2000);
    let after_ms = after_ms.unwrap_or(0).max(0);
    let scope_kind = match scope.as_deref() {
        Some("user") => Some(Kind::User),
        Some("assistant") => Some(Kind::Assistant),
        _ => None, // "all" / None / 未知值 → 不过滤
    };

    // 本地（CPU）与远端（SSH）并发跑，再合并。
    let q_local = query.clone();
    let local_task = tokio::task::spawn_blocking(move || {
        index.query(&q_local, include_tools, scope_kind, after_ms, limit)
    });
    let remote_task = crate::remote_history::search_remote_all(
        &query,
        include_tools,
        scope.as_deref(),
        after_ms,
        limit,
    );
    let (local_res, remote) = tokio::join!(local_task, remote_task);
    let local = local_res.map_err(|e| format!("spawn_blocking join: {e}"))?;
    Ok(merge_search_results(local, remote))
}

/// 合并本地索引结果与远端 fan-out 结果（issue #28）：拼接 sessions 后按 updatedAt desc
/// 重排，`total_hits`/`session_count` 重算。无远端 → 原样返回本地（含 indexing 态）。
/// 本地 indexing 但有远端结果时 status=ready（不丢远端；本地结果待索引就绪后下次搜索补上）。
fn merge_search_results(local: SearchResponse, remote: Vec<SessionHits>) -> SearchResponse {
    if remote.is_empty() {
        return local;
    }
    let remote_hits: u32 = remote.iter().map(|s| s.hit_count).sum();
    let mut sessions = local.sessions;
    sessions.extend(remote);
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    SearchResponse {
        // remote 非空 → 必有结果，status 一律 ready（不让 indexing 吞掉远端结果）。
        status: "ready".into(),
        total_hits: local.total_hits + remote_hits,
        session_count: sessions.len() as u32,
        truncated: local.truncated,
        indexed_sessions: local.indexed_sessions,
        indexed_messages: local.indexed_messages,
        sessions,
    }
}

/// 查索引状态（UI 显示"索引中 / 已就绪"，无需发查询）。
#[tauri::command]
pub fn get_search_index_status(
    index: tauri::State<'_, std::sync::Arc<SearchIndex>>,
) -> SearchIndexStatus {
    index.status()
}

/// 手动重建索引（历史浏览器"重新索引"按钮 / 大量新会话后）。
#[tauri::command]
pub async fn rebuild_search_index(
    index: tauri::State<'_, std::sync::Arc<SearchIndex>>,
) -> Result<SearchIndexStatus, String> {
    let index = index.inner().clone();
    tokio::task::spawn_blocking(move || {
        let claude_dir = paths::resolve_claude_dir().ok_or("claude dir not found")?;
        // 重建前先标记未就绪，让查询返回 indexing
        {
            let mut data = index.inner.write();
            data.ready = false;
        }
        index.build_blocking(&claude_dir, Duration::ZERO);
        Ok(index.status())
    })
    .await
    .map_err(|e| format!("spawn_blocking join: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_ci_ascii_case_insensitive() {
        assert_eq!(find_ci("Hello World", "world"), Some((6, 11)));
        assert_eq!(find_ci("HELLO", "hell"), Some((0, 4)));
        assert_eq!(find_ci("abc", "xyz"), None);
    }

    #[test]
    fn find_ci_cjk() {
        let s = "你好世界 docker 部署";
        // "docker" 起始字节：4 个 CJK ×3 字节 + 1 空格 = 13
        let (start, end) = find_ci(s, "docker").unwrap();
        assert_eq!(&s[start..end], "docker");
    }

    #[test]
    fn make_snippet_three_parts() {
        let text = "请问怎么用 Docker 部署这个服务到生产环境";
        let (before, matched, after) = make_snippet(text, "docker");
        assert_eq!(matched, "Docker");
        assert!(before.contains("怎么用"));
        assert!(after.contains("部署"));
    }

    #[test]
    fn extract_text_blocks_string_and_array() {
        assert_eq!(extract_text_blocks(&Value::String("hi".into())), "hi");
        let arr = serde_json::json!([
            {"type":"text","text":"line1"},
            {"type":"tool_use","name":"Bash","input":{}},
            {"type":"text","text":"line2"}
        ]);
        assert_eq!(extract_text_blocks(&arr), "line1\nline2");
    }

    #[test]
    fn extract_tool_text_assistant_and_user() {
        let asst = serde_json::json!([
            {"type":"tool_use","name":"Bash","input":{"command":"ls -la"}}
        ]);
        let t = extract_tool_text(&asst, true);
        assert!(t.contains("Bash"));
        assert!(t.contains("ls -la"));

        let user = serde_json::json!([
            {"type":"tool_result","content":[{"type":"text","text":"file output here"}]}
        ]);
        let t2 = extract_tool_text(&user, false);
        assert!(t2.contains("file output here"));
    }

    #[test]
    fn clean_user_text_strips_noise_and_interrupt() {
        assert_eq!(
            clean_user_text("<system-reminder>noise</system-reminder>真问题"),
            "真问题"
        );
        assert_eq!(clean_user_text("[Request interrupted by user]"), "");
    }

    #[test]
    fn truncate_chars_plain_caps() {
        assert_eq!(truncate_chars_plain("hello", 3), "hel");
        assert_eq!(truncate_chars_plain("你好世界", 2), "你好");
        assert_eq!(truncate_chars_plain("hi", 10), "hi");
    }

    #[test]
    fn query_scope_and_time_filter() {
        let idx = SearchIndex::new();
        {
            let mut d = idx.inner.write();
            let mk = |uuid: &str, ts: i64, kind: Kind, main: &str| MsgDoc {
                uuid: uuid.into(),
                ts_ms: ts,
                kind,
                main: main.into(),
                main_lc: main.to_lowercase(),
                tool: String::new(),
                tool_lc: String::new(),
            };
            d.sessions = vec![SessionDoc {
                session_id: "s1".into(),
                project_path: "/x".into(),
                project_name: "x".into(),
                jsonl_path: "/a.jsonl".into(),
                title: "t".into(),
                updated_at: 100,
                msgs: vec![
                    mk("u1", 100, Kind::User, "deploy docker now"),
                    mk("a1", 200, Kind::Assistant, "use docker compose"),
                ],
            }];
            d.total_messages = 2;
            d.ready = true;
        }
        // 全部：两条都命中
        assert_eq!(idx.query("docker", false, None, 0, 300).total_hits, 2);
        // 只 user：只 u1
        let user = idx.query("docker", false, Some(Kind::User), 0, 300);
        assert_eq!(user.total_hits, 1);
        assert_eq!(user.sessions[0].hits[0].uuid, "u1");
        // 只 assistant：只 a1
        let asst = idx.query("docker", false, Some(Kind::Assistant), 0, 300);
        assert_eq!(asst.total_hits, 1);
        assert_eq!(asst.sessions[0].hits[0].uuid, "a1");
        // 时间 >=150：只 a1（u1 的 ts=100 被滤掉）
        let recent = idx.query("docker", false, None, 150, 300);
        assert_eq!(recent.total_hits, 1);
        assert_eq!(recent.sessions[0].hits[0].uuid, "a1");
    }

    /// 契约测试：wire 全 camelCase，前端 TS interface 字段名须一致。
    #[test]
    fn search_response_camel_case_contract() {
        let resp = SearchResponse {
            status: "ready".into(),
            total_hits: 5,
            session_count: 1,
            truncated: false,
            indexed_sessions: 10,
            indexed_messages: 200,
            sessions: vec![SessionHits {
                session_id: "s1".into(),
                project_path: "/x".into(),
                project_name: "x".into(),
                jsonl_path: "/a.jsonl".into(),
                title: "t".into(),
                updated_at: 1,
                hit_count: 2,
                hits: vec![Hit {
                    uuid: "u1".into(),
                    ts_ms: 1,
                    kind: "user".into(),
                    before: "b".into(),
                    matched: "m".into(),
                    after: "a".into(),
                }],
                origin: None,
            }],
        };
        let j = serde_json::to_string(&resp).unwrap();
        for k in [
            "\"totalHits\"",
            "\"sessionCount\"",
            "\"indexedSessions\"",
            "\"indexedMessages\"",
            "\"sessionId\"",
            "\"projectPath\"",
            "\"projectName\"",
            "\"jsonlPath\"",
            "\"updatedAt\"",
            "\"hitCount\"",
            "\"tsMs\"",
        ] {
            assert!(j.contains(k), "wire 缺 {k}: {j}");
        }
        for snake in [
            "\"total_hits\"",
            "\"session_id\"",
            "\"jsonl_path\"",
            "\"ts_ms\"",
        ] {
            assert!(!j.contains(snake), "wire 漏改 {snake}: {j}");
        }
    }

    // === #28 远端搜索合并 ===

    fn mk_session(sid: &str, updated: i64, hit_count: u32, origin: Option<&str>) -> SessionHits {
        SessionHits {
            session_id: sid.into(),
            project_path: "/p".into(),
            project_name: "p".into(),
            jsonl_path: format!("/{sid}.jsonl"),
            title: sid.into(),
            updated_at: updated,
            hit_count,
            hits: vec![],
            origin: origin.map(str::to_string),
        }
    }

    fn resp(status: &str, total: u32, sessions: Vec<SessionHits>) -> SearchResponse {
        SearchResponse {
            status: status.into(),
            total_hits: total,
            session_count: sessions.len() as u32,
            truncated: false,
            indexed_sessions: 1,
            indexed_messages: 1,
            sessions,
        }
    }

    /// daemon 的 `--search` 输出（camelCase，无 origin）能反序列化成 SessionHits。
    #[test]
    fn session_hits_deserializes_from_daemon_json() {
        let line = r#"{"sessionId":"s9","projectPath":"/home/pi/p","projectName":"p","jsonlPath":"/home/pi/.claude/projects/p/s9.jsonl","title":"标题","updatedAt":123,"hitCount":2,"hits":[{"uuid":"u1","tsMs":5,"kind":"user","before":"b","matched":"m","after":"a"}]}"#;
        let sh: SessionHits = serde_json::from_str(line).expect("daemon json deserializes");
        assert_eq!(sh.session_id, "s9");
        assert_eq!(sh.hit_count, 2);
        assert_eq!(sh.hits.len(), 1);
        assert_eq!(
            sh.origin, None,
            "daemon 不发 origin → None（由 fan-out 补）"
        );
    }

    /// 合并：拼接 + updatedAt desc 重排 + 总数相加；远端 origin 保留。
    #[test]
    fn merge_orders_and_sums() {
        let local = resp("ready", 3, vec![mk_session("local-old", 100, 3, None)]);
        let remote = vec![
            mk_session("rem-new", 300, 2, Some("pi")),
            mk_session("rem-mid", 200, 1, Some("wsl")),
        ];
        let merged = merge_search_results(local, remote);
        assert_eq!(merged.status, "ready");
        assert_eq!(merged.total_hits, 3 + 2 + 1);
        assert_eq!(merged.session_count, 3);
        // updatedAt desc：rem-new(300) > rem-mid(200) > local-old(100)
        let ids: Vec<&str> = merged
            .sessions
            .iter()
            .map(|s| s.session_id.as_str())
            .collect();
        assert_eq!(ids, vec!["rem-new", "rem-mid", "local-old"]);
        assert_eq!(merged.sessions[0].origin.as_deref(), Some("pi"));
        assert_eq!(merged.sessions[2].origin, None);
    }

    /// 无远端 → 原样返回本地（含 indexing 态不被改写）。
    #[test]
    fn merge_no_remote_returns_local_verbatim() {
        let local = resp("indexing", 0, vec![]);
        let merged = merge_search_results(local, vec![]);
        assert_eq!(merged.status, "indexing");
        assert_eq!(merged.session_count, 0);
    }

    /// 本地 indexing 但有远端结果 → status=ready（不丢远端）。
    #[test]
    fn merge_indexing_local_with_remote_is_ready() {
        let local = resp("indexing", 0, vec![]);
        let remote = vec![mk_session("rem", 50, 4, Some("pi"))];
        let merged = merge_search_results(local, remote);
        assert_eq!(merged.status, "ready");
        assert_eq!(merged.total_hits, 4);
        assert_eq!(merged.sessions.len(), 1);
    }
}
