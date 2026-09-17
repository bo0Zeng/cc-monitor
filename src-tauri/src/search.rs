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
//!
//! ## 🔴 口径不住在这里 —— 住 `search-core`（`K-R100`）
//!
//! 抽取 / 匹配 / snippet 的 12 个助手、4 个口径常量、snippet 预算与预算顺序，
//! **一份都不在本文件**：它们住 `crates/search-core`，daemon 的 `--search`
//! （`src/backend/observe/search_query.rs`）调的是**同一份**。
//! 本文件只剩「怎么扫盘、怎么建索引、怎么组装 wire 类型」这些**本地特有**的活。
//! 收口前两侧各写一遍那 12 个助手（`K-R85` 实测逐字相同），而 `CROSS_EDGES` 里
//! search 零命中 ⇒ **没有任何判据在拦着它们漂开**。判据现在有了：
//! `tests::the_search_kou_jing_has_exactly_one_home`。

use crate::messages::JsonlRecord;
use crate::parser::parse_line;
use crate::paths;
use crate::utils::{parse_iso8601_ms, systime_to_ms};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use walkdir::WalkDir;

// 🔴 口径常量**一个都不在这里**——它们就是口径本身，两侧各写一个字面量 = 两份口径。
// `MAIN_CAP` / `TOOL_CAP` / `SNIPPET_CTX` / `PER_SESSION_CAP` / `DEFAULT_LIMIT` 住
// `search_core`，daemon 用的是同一份。
use search_core::{MAIN_CAP, TOOL_CAP};

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
    /// 🔴 **整份结果被全局 `limit` 砍过** —— 本地与**每一台远端**任意一处发生就是 true
    /// （`K-R100` 之前这里只装本地那一半，远端截断在界面上一个字不说）。
    /// ⚠ **不含**「单会话超 `PER_SESSION_CAP` 条」：那是「这个会话话多」，不是结果被砍，
    /// 由 `SessionHits::hits_truncated` 与卡片上那行「还有 N 条」分别承担。
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
    /// 🔴 **本会话有命中被「全局 snippet 预算用完」挡下了** —— `K-R100`。
    ///
    /// 没有这一格时，`hits: []` 与「这个会话没什么可看的」在下游**同形**：前端只能拿
    /// `hitCount > hits.length` 反推，而那个式子对**两个完全不同的原因**给出同一个答案
    /// （① 整份结果被 `--limit` 砍了 ② 这个会话超过 30 条只列前 30）。
    /// ① 该让用户知道「缩小范围 / 加大 limit」，② 只需要「点进去看」。
    /// daemon 侧同名字段由 `--search` 逐会话吐出（它才知道自己是哪一种），
    /// monitor 合并时 OR 进 `SearchResponse::truncated`。
    ///
    /// 兼容：旧 daemon 不发这个字段 ⇒ `serde(default)` = false（退化成收口前的行为，不炸）。
    #[serde(default)]
    pub hits_truncated: bool,
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

        // 🔴 snippet 预算按**最近优先**花 —— 排序与预算判定都在 `search_core`，
        // daemon 的 `--search` 调的是同一份（`K-R100`；理由与读数见 `sort_by_recency` 文档注释）。
        let mut order: Vec<usize> = (0..data.sessions.len()).collect();
        search_core::sort_by_recency(&mut order, |&i| data.sessions[i].updated_at);

        let mut total_hits: u32 = 0;
        let mut budget = search_core::SnippetBudget::new(limit);
        let mut out: Vec<SessionHits> = Vec::new();

        for &si in &order {
            let sd = &data.sessions[si];
            let mut hits: Vec<Hit> = Vec::new();
            let mut session_hit_count: u32 = 0;
            // 本会话有没有命中是**因为全局预算用完**而拿不到 snippet（≠ 单会话超 30 条）。
            let mut session_starved = false;

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
                // 🔴 判定在 `search_core::SnippetBudget`，两侧同一份；它还分得清
                // 「预算用完」与「单会话满 30 条」——收口前这两件事挤在一个 bool 里。
                match budget.take(hits.len()) {
                    search_core::SnippetVerdict::Give => {
                        let (kind, text) = if in_main {
                            (m.kind.as_str(), &m.main)
                        } else {
                            ("tool", &m.tool)
                        };
                        let (before, matched, after) = search_core::make_snippet(text, &q);
                        hits.push(Hit {
                            uuid: m.uuid.clone(),
                            ts_ms: m.ts_ms,
                            kind: kind.to_string(),
                            before,
                            matched,
                            after,
                        });
                    }
                    search_core::SnippetVerdict::BudgetExhausted => session_starved = true,
                    search_core::SnippetVerdict::SessionCapped => {}
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
                    hits_truncated: session_starved,
                    origin: None, // 本地结果无 origin（远端结果由 fan-out 补）
                });
            }
        }

        SearchResponse {
            status: "ready".into(),
            total_hits,
            session_count: out.len() as u32,
            // 「整份结果被砍了」= 预算真的被榨干过。单会话超 `PER_SESSION_CAP`
            // **不算**（那只是「这个会话话多」，卡片上那行「还有 N 条」已经说了）。
            truncated: budget.starved(),
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
                let main = search_core::clean_user_text(&search_core::extract_text_blocks(
                    &message.content,
                ));
                let tool = search_core::extract_tool_text(&message.content, false);
                if first_user_excerpt.is_empty() && !main.is_empty() {
                    first_user_excerpt = search_core::truncate_excerpt(&main, 120);
                }
                push_msg(&mut msgs, uuid, &timestamp, Kind::User, main, tool);
            }
            JsonlRecord::Assistant {
                uuid,
                timestamp,
                message,
                ..
            } => {
                let main = search_core::extract_text_blocks(&message.content);
                let tool = search_core::extract_tool_text(&message.content, true);
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
    let title = search_core::session_title(ai_title.as_deref(), &first_user_excerpt, &session_id);

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
    let main = search_core::truncate_plain(&main, MAIN_CAP);
    let tool = search_core::truncate_plain(&tool, TOOL_CAP);
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

// === 文本抽取 / snippet / 截断：**一份都不在这里** ===
//
// 🔴 那 12 个助手（`extract_text_blocks` · `extract_tool_text` · `stringify_json` ·
// `clean_user_text` · `make_snippet` · `find_ci` · `tail_chars` · `head_chars` ·
// `collapse_ws` · `collapse_ws_keep_ellipsis` · `truncate_plain` · `truncate_excerpt`）
// 与它们的单元测试全部住 `crates/search-core`。本文件调它，daemon 也调它 —— **同一份**。
// 别在这里「顺手再写一个小的」：那就是 `K-R100` 收口前的形状（两份、逐字同、零判据）。

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
    let limit = search_core::clamp_limit(limit.unwrap_or(search_core::DEFAULT_LIMIT));
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
    // 🔴 `K-R100`：远端也会被自己的 `--limit` 砍。收口前这里逐字写的是
    // `truncated: local.truncated` ⇒ **远端截断在界面上一个字不说**
    // （实测一次查询 13 个命中会话里 10 个 `hitCount>0` 而 `hits: []`，状态行照旧只报总数）。
    let remote_starved = remote.iter().any(|s| s.hits_truncated);
    let mut sessions = local.sessions;
    sessions.extend(remote);
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    SearchResponse {
        // remote 非空 → 必有结果，status 一律 ready（不让 indexing 吞掉远端结果）。
        status: "ready".into(),
        total_hits: local.total_hits + remote_hits,
        session_count: sessions.len() as u32,
        truncated: local.truncated || remote_starved,
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

/// `K-R100`：搜索**口径**只许有一个家 —— `crates/search-core`。
///
/// # 失效方向（先说这个，因为最容易做偏）
///
/// 🔴 **不许判「两边源码文本一样」**。那判的是**写法**，而且这两份今天本来就逐字相同
/// （`K-R85` 09-12 实测）⇒ 那样一条判据会**恒绿**，一天都不会响。
/// 本模块判的是**同一份实现**：core 真的持有 · 两侧都不许自己再有一份 · 两侧都得真调它。
///
/// 第 ③ 刀（「改 core 一处、两侧行为都跟着变」）**不在这里** —— 那是行为，
/// 判不了源码。它由两侧各一条**期望值取自 core、实际值来自本侧生产管线**的行为判据承担：
/// `tests::the_snippet_window_comes_from_core`（monitor）
/// ＋ `src/backend/observe/search_query.rs` 里的同名那条（daemon）。
#[cfg(test)]
mod kou_jing_guard {
    /// 收口前在两侧**各写一遍**的那 12 个助手（`K-R85` 逐条实测同名同形）。
    /// ⚠ 人群按「口径可能住在哪」取，不是按「当初动过哪几行」取。
    const HELPERS: &[&str] = &[
        "extract_text_blocks",
        "extract_tool_text",
        "stringify_json",
        "clean_user_text",
        "make_snippet",
        "find_ci",
        "tail_chars",
        "head_chars",
        "collapse_ws",
        "collapse_ws_keep_ellipsis",
        "truncate_plain",
        "truncate_excerpt",
    ];

    /// 口径常量 —— **它们就是口径本身**，两侧任何一处再写一遍就是第二份口径。
    const CONSTS: &[&str] = &[
        "MAIN_CAP",
        "TOOL_CAP",
        "SNIPPET_CTX",
        "PER_SESSION_CAP",
        "DEFAULT_LIMIT",
    ];

    /// 两侧都必须真的调到的东西（不只是「import 了」）。
    const MUST_CALL: &[&str] = &[
        "search_core::make_snippet",
        "search_core::sort_by_recency",
        "search_core::clean_user_text",
        "search_core::session_title",
    ];

    #[test]
    fn the_search_kou_jing_has_exactly_one_home() {
        // ── ① core 确实持有口径，否则下面两条退化成「哪里都没有」，零命中地绿。
        let core = guard_core::production_code(include_str!("../crates/search-core/src/lib.rs"));
        let missing: Vec<&str> = HELPERS
            .iter()
            .copied()
            .filter(|h| !core.contains(&format!("pub fn {h}(")))
            .collect();
        assert!(
            missing.is_empty(),
            "`search-core` 生产段里找不到这些助手：{missing:?}\n             口径搬走了还是抽取坏了？本条此刻无效。"
        );
        let missing_c: Vec<&str> = CONSTS
            .iter()
            .copied()
            .filter(|c| !core.contains(&format!("pub const {c}: usize")))
            .collect();
        assert!(
            missing_c.is_empty(),
            "`search-core` 生产段里找不到这些口径常量：{missing_c:?}"
        );
        assert!(
            core.contains("pub struct SnippetBudget") && core.contains("pub enum SnippetVerdict"),
            "snippet 预算（含「预算用完」vs「单会话满」这一拆）必须住 core —— \
             它一旦回到两侧，`hits: []` 又会变成一个装两件事的值"
        );
        assert!(
            core.contains("pub fn sort_by_recency"),
            "预算顺序必须住 core：收口前 monitor 按 `updated_at desc`、daemon 按 readdir，\
             `--limit 50` 下两侧给出的 3 个会话**只重合 1 个**"
        );

        // ── ② 两侧：不许自己再有一份，且必须真的调 core。
        for (name, raw) in [
            ("monitor src/search.rs", include_str!("search.rs")),
            (
                "daemon observe/search_query.rs",
                include_str!("../../src/backend/observe/search_query.rs"),
            ),
        ] {
            let prod = guard_core::production_code(raw);
            let redefined: Vec<&str> = HELPERS
                .iter()
                .copied()
                .filter(|h| prod.contains(&format!("fn {h}(")))
                .collect();
            assert!(
                redefined.is_empty(),
                "{name} 的生产段里又长出了这些助手的定义：{redefined:?}\n                 🔴 那就是 `K-R100` 收口前的形状：两份实现、逐字相同、**零判据对拍**。\n                 「今天没漂」不是保障 —— 下一次谁改一侧，另一侧静默留在原地，\n                 而搜索结果不一致**不会报错**（本地一份 snippet、远端另一份，谁都不抛异常）。"
            );
            let reconst: Vec<&str> = CONSTS
                .iter()
                .copied()
                .filter(|c| prod.contains(&format!("const {c}")))
                .collect();
            assert!(
                reconst.is_empty(),
                "{name} 的生产段里重新定义了口径常量：{reconst:?}\n                 那些数**就是口径**，只许住 `search-core`。在这里写一个同样的字面量，\n                 两边漂开时**搜索结果会静默不一致**。"
            );
            for call in MUST_CALL {
                assert!(
                    prod.contains(call),
                    "{name} 不再调 `{call}` —— 它要么自己算了一遍（第二份口径），\n                     要么口径搬家了而本条没跟。"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ⚠ `K-R100` 的性能台架（同进程配对：`search_core::make_snippet` vs 收口前那份
    // 逐字相同的本地副本）**跑完就删了**，读数落在 `evidence/K-R100-deathvalue.md`。
    // 它是一次性量具，不该留在门禁里（留下就成了一条没人跑、也没人维护的 `#[ignore]`）。

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
                hits_truncated: true,
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
            "\"hitsTruncated\"",
            "\"tsMs\"",
        ] {
            assert!(j.contains(k), "wire 缺 {k}: {j}");
        }
        for snake in [
            "\"total_hits\"",
            "\"session_id\"",
            "\"jsonl_path\"",
            "\"ts_ms\"",
            "\"hits_truncated\"",
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
            hits_truncated: false,
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

    // ── `K-R100` 的行为判据（本侧那一半；daemon 侧有同形的三条）─────────────

    /// 建一棵 `<claude_dir>/projects/<proj>/<sid>.jsonl`，mtime 按给定毫秒设。
    fn corpus(tag: &str, sessions: &[(&str, u64, usize)]) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("ccm-kr100-mon-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        for (sid, mtime_ms, n) in sessions {
            let proj = tmp.join("projects").join(format!("p-{sid}"));
            std::fs::create_dir_all(&proj).unwrap();
            let jsonl = proj.join(format!("{sid}.jsonl"));
            let lines: Vec<String> = (0..*n)
                .map(|i| {
                    format!(
                        r#"{{"type":"user","uuid":"{sid}-{i}","timestamp":"2026-01-01T00:00:0{}Z","cwd":"/w","message":{{"role":"user","content":"命中 docker 第 {i} 条"}}}}"#,
                        i % 10
                    )
                })
                .collect();
            std::fs::write(&jsonl, lines.join("\n")).unwrap();
            let f = File::options().write(true).open(&jsonl).unwrap();
            f.set_modified(std::time::UNIX_EPOCH + Duration::from_millis(*mtime_ms))
                .unwrap();
        }
        tmp
    }
    fn built(dir: &Path) -> SearchIndex {
        let idx = SearchIndex::new();
        idx.build_blocking(dir, Duration::ZERO);
        idx
    }

    /// `KR100D2`：**预算按最近优先花**，与 daemon 同一份 `search_core::sort_by_recency`。
    /// 死值验①（把某一侧换回「文件系统先走到的顺序」）当场红。
    #[test]
    fn the_snippet_budget_goes_to_the_most_recent_sessions() {
        let dir = corpus(
            "order",
            &[("old", 1_000, 3), ("mid", 2_000, 3), ("new", 3_000, 3)],
        );
        let idx = built(&dir);
        let r = idx.query("docker", false, None, 0, 300);
        let ids: Vec<&str> = r.sessions.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["new", "mid", "old"],
            "会话按最近优先排（= 预算顺序 = 展示顺序）"
        );

        // 预算只够 2 条 ⇒ 都花在最新那个会话上，与 daemon 侧同名判据逐条同形。
        let r = idx.query("docker", false, None, 0, 2);
        let newest = r.sessions.iter().find(|s| s.session_id == "new").unwrap();
        assert_eq!(newest.hits.len(), 2);
        for s in r.sessions.iter().filter(|s| s.session_id != "new") {
            assert!(s.hits.is_empty());
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `KR100D3`（本地半）：截断自己说出来，且**不与「本会话就这么点命中」同形**。
    #[test]
    fn truncation_is_stated_not_left_to_an_empty_array() {
        let dir = corpus("trunc", &[("a", 3_000, 2), ("b", 2_000, 5)]);
        let idx = built(&dir);
        let r = idx.query("docker", false, None, 0, 2);
        assert!(r.truncated, "整份结果被全局预算砍过 ⇒ 状态行要说得出");
        let a = r.sessions.iter().find(|s| s.session_id == "a").unwrap();
        let b = r.sessions.iter().find(|s| s.session_id == "b").unwrap();
        assert!(!a.hits_truncated, "a 全给到了");
        assert_eq!((b.hit_count, b.hits.len()), (5, 0));
        assert!(
            b.hits_truncated,
            "🔴 `hitCount>0` 而 `hits: []` 必须自己说出「我被砍了」"
        );
        // 反空真：预算充足时一格都不许亮。
        let r = idx.query("docker", false, None, 0, 300);
        assert!(!r.truncated);
        assert!(r.sessions.iter().all(|s| !s.hits_truncated));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `KR100D3` 的合并半：**远端截断不许在合并那一步被丢掉**。
    /// 收口前这里逐字 `truncated: local.truncated`。
    #[test]
    fn remote_truncation_survives_the_merge() {
        let local = resp("ready", 1, vec![mk_session("loc", 100, 1, None)]);
        assert!(!local.truncated);
        let mut rem = mk_session("rem", 200, 12, Some("pi"));
        rem.hits_truncated = true; // daemon 说的：它被自己的 --limit 砍了
        let merged = merge_search_results(local, vec![rem]);
        assert!(
            merged.truncated,
            "远端截断在合并处被丢掉了 —— 那正是「远端截断界面一个字不说」的成因"
        );
        // 反空真：远端没截断时不许乱亮。
        let local2 = resp("ready", 1, vec![mk_session("loc", 100, 1, None)]);
        let merged2 = merge_search_results(local2, vec![mk_session("rem", 200, 1, Some("pi"))]);
        assert!(!merged2.truncated);
    }

    /// `KR100D1` 第 ③ 刀（本侧那一半）：**改 `search_core` 一处，本侧真跑出来的东西跟着变**。
    ///
    /// 期望值取自 `search_core::SNIPPET_CTX`，实际值来自本文件的生产管线
    /// （`build_blocking` → `query` → `search_core::make_snippet`）。
    /// · 改 core 的 `SNIPPET_CTX` ⇒ 两头一起动，本条仍绿（＝行为确实跟着变）；
    /// · 本侧哪天写回一个自己的 `const SNIPPET_CTX = 48` ⇒ 实际不动、期望动 ⇒ **当场红**。
    /// daemon 侧有一条同形的（`observe/search_query.rs::the_snippet_window_comes_from_core`）。
    #[test]
    fn the_snippet_window_comes_from_core() {
        let ctx = search_core::SNIPPET_CTX;
        let filler = "x".repeat(ctx * 4);
        let dir = std::env::temp_dir().join(format!("ccm-kr100-mon-ctx-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let proj = dir.join("projects").join("p");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(
            proj.join("s.jsonl"),
            format!(
                r#"{{"type":"user","uuid":"u","timestamp":"2026-01-01T00:00:00Z","cwd":"/w","message":{{"role":"user","content":"{filler}docker{filler}"}}}}"#
            ),
        )
        .unwrap();
        let r = built(&dir).query("docker", false, None, 0, 300);
        let h = &r.sessions[0].hits[0];
        assert_eq!(
            h.before.chars().count(),
            ctx + 1,
            "snippet 前窗必须等于 `search_core::SNIPPET_CTX`（={ctx}）+ 省略号"
        );
        assert_eq!(h.after.chars().count(), ctx + 1);
        std::fs::remove_dir_all(&dir).ok();
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
