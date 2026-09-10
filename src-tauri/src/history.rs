//! 历史会话浏览器后端：扫描 `<claude_dir>/projects/**/*.jsonl`，提供
//! list / delete / metadata 增删改 / resume / **F62 从某轮建分支** IPC 命令。
//!
//! ## 与 watcher 的关系
//!
//! watcher.rs / event_replay.rs 只关心**活跃 session**（PID 还在跑）。本模块是
//! 用户**显式触发**的拉取（点"历史"按钮才扫一次），不监听变化，不维护内存索引。
//!
//! ## 两级懒加载
//!
//! 历史浏览器项目组默认折叠，因此后端分两级 IPC：
//!  1. `list_history_projects` —— 项目级元数据，**不读 jsonl 内容**，每项目仅 1 个
//!     1-line read 拿 cwd + 文件 stat 拿 mtime + 数 dir entries。500 个项目 < 50ms
//!  2. `stream_history_sessions_in_project` —— 用户展开某项目时才调，流式 Channel
//!     边解析边发，前端逐条增量渲染。（v2.2 起取代非流式 `list_history_sessions_in_project`）
//!
//! ## 用户元数据
//!
//! star / 重命名 / 隐藏 这些信息**不能改 jsonl**（Claude Code 的数据保持零侵入），
//! 单独存到 `<monitor_data_dir>/history-metadata.json`，结构见 `HistoryMetadata`。
//!
//! ## 物理删除
//!
//! 用户明确选了"物理删除 .jsonl 文件"。前端二次确认后调 `delete_history_session`，
//! 直接 `std::fs::remove_file`。Claude Code 自己也不再能 resume 这个会话。

use crate::messages::{ApiMessage, JsonlRecord};
use crate::parser::parse_line;
use crate::paths;
use crate::session_map::SessionMap;
use crate::utils::{now_ms, systime_to_ms};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// === 数据结构 ===

/// 项目级元数据 —— 首次 list 时返回，**不含**任何 session 内容。
/// P1.2：全字段 camelCase wire，前端 TS interface 字段名一致。
#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct HistoryProject {
    /// 真实工作目录路径（从某个 jsonl 的首条 user 消息的 cwd 取）
    pub project_path: String,
    /// 项目名 = cwd 最后一段
    pub project_name: String,
    /// 编码后的目录名（位于 `<claude_dir>/projects/` 之下），前端调用
    /// `stream_history_sessions_in_project` 时传回来作 key
    pub project_dir: String,
    pub session_count: u32,
    pub starred_count: u32,
    pub hidden_count: u32,
    /// 该项目下任意 jsonl 文件的最大 mtime（ms）
    // **C03 大整数策略**：量纲是**毫秒时间戳**——2^53-1 ms ≈ **28.5 万年**。
    #[cfg_attr(test, ts(type = "number"))]
    pub last_activity: i64,
    /// 该项目下是否有 session 当前 PID 还活着
    pub has_live: bool,
    /// issue #16：数据来源。None=本地；Some(host)=远端（前端组头显示 [host] 徽标，
    /// 展开时改调 stream_remote_history_sessions）。
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct HistorySessionEntry {
    pub session_id: String,
    pub project_path: String,
    pub project_name: String,
    pub ai_title: Option<String>,
    pub first_user_excerpt: String,
    // **C03 大整数策略**：量纲是**毫秒时间戳**——2^53-1 ms ≈ **28.5 万年**。
    #[cfg_attr(test, ts(type = "number"))]
    pub started_at: i64,
    // **C03 大整数策略**：量纲是**毫秒时间戳**——2^53-1 ms ≈ **28.5 万年**。
    #[cfg_attr(test, ts(type = "number"))]
    pub updated_at: i64,
    pub jsonl_path: String,
    pub is_live: bool,
    pub message_count_approx: u32,
    /// Batch11-F32：CC 后台分身会话（⚙ 徽标——防 resume 误选克隆）。
    pub is_bg: bool,
    // 用户元数据合并进来，前端一次拿全
    pub starred: bool,
    pub custom_title: Option<String>,
    pub hidden: bool,
    // issue #12: fork 关系。若本 session 是从某 parent session 用 /branch 分叉来的，
    // 这两个字段记 parent session 的 id 和被 fork 处的 messageUuid。
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forked_from_session_id: Option<String>,
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forked_from_message_uuid: Option<String>,
    /// issue #16：数据来源。None=本地；Some(host)=远端（前端据此禁用 resume/delete、
    /// 查看走 stream_read_remote_session）。
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct HistoryMetadata {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub entries: HashMap<String, EntryMetadata>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
pub struct EntryMetadata {
    #[serde(default)]
    pub starred: bool,
    #[serde(default, rename = "customTitle", alias = "custom_title")]
    pub custom_title: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    // **C03 大整数策略**：量纲是**毫秒时间戳**——2^53-1 ms ≈ **28.5 万年**。
    #[cfg_attr(test, ts(type = "number"))]
    #[serde(default, rename = "updatedAt", alias = "updated_at")]
    pub updated_at: i64,
    /// A4：上次用本工具（cc-monitor）起该会话时选的账号名（DESIGN §3 源②）。
    /// live 探测不到时会话徽章回退用它。None = 从未用本工具带账号起过（旧文件缺此字段亦为 None）。
    #[serde(default, rename = "lastAccount", alias = "last_account")]
    pub last_account: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MetadataPatch {
    #[serde(default)]
    pub starred: Option<bool>,
    #[serde(default, rename = "customTitle", alias = "custom_title")]
    pub custom_title: Option<Option<String>>,
    #[serde(default)]
    pub hidden: Option<bool>,
    // plain serde default（非 double_option）：缺键 / JSON `null` 都 → None（不改）；
    // 只有给字符串才 → Some(Some(s))。**JSON `null` 到不了 Some(None)**——清空走"空/空白串
    // → Some(Some("")) → update 里 filter 掉"（见 update_history_metadata），不靠 null。
    #[serde(default, rename = "lastAccount", alias = "last_account")]
    pub last_account: Option<Option<String>>,
}

// === IPC 命令 ===

/// 项目级元数据列表 —— **不读 jsonl 内容**。首次打开历史浏览器时调。
/// 每个项目仅 1 个 1-line read（拿 cwd） + N 个文件 stat（拿 mtime / count）。
///
/// v2.2 (issue #12)：改 async + spawn_blocking，避免 sync IO 阻塞 Tauri IPC
/// 派发线程 —— 加载期间其他 IPC（拉前 / 切设置）能正常响应。
#[tauri::command]
pub async fn list_history_projects(
    map: tauri::State<'_, Arc<SessionMap>>,
) -> Result<Vec<HistoryProject>, String> {
    let map = map.inner().clone();
    tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let claude_dir = paths::resolve_claude_dir().ok_or("claude dir not found")?;
        let projects_dir = crate::adapter::records_dir(&claude_dir);
        if !projects_dir.exists() {
            return Ok(Vec::new());
        }
        let metadata = load_metadata().unwrap_or_default();

        let mut out: Vec<HistoryProject> = Vec::new();
        let proj_iter = match std::fs::read_dir(&projects_dir) {
            Ok(d) => d,
            Err(e) => return Err(format!("read {}: {e}", projects_dir.display())),
        };

        for proj in proj_iter.flatten() {
            let proj_path = proj.path();
            if !proj_path.is_dir() {
                continue;
            }
            if let Some(hp) = analyze_project_dir(&proj_path, &metadata, &map) {
                out.push(hp);
            }
        }

        // Phase 2 F1a-3：追加 Codex 合成项目（按 session_meta.cwd 内存分组；Codex 未启用 → 空、零回归）。
        out.extend(codex_projects());

        // live → starred → last_activity desc（同 UI 顺序，前端可再排但默认就是这个）
        out.sort_by(|a, b| {
            b.has_live
                .cmp(&a.has_live)
                .then_with(|| (b.starred_count > 0).cmp(&(a.starred_count > 0)))
                .then(b.last_activity.cmp(&a.last_activity))
        });

        tracing::info!(
            "list_history_projects: {} projects in {}ms",
            out.len(),
            started.elapsed().as_millis()
        );
        Ok(out)
    })
    .await
    .map_err(|e| format!("spawn_blocking join: {e}"))?
}

// ─── Phase 2 F1a-3：Codex 历史枚举（Codex 无 `projects/<cwd>` 目录 → 按 session_meta.cwd 内存分组成
// 合成「项目」，塞进现有 HistoryProject shape → 前端零改、Codex 会话入列）───

/// Codex 一个会话的 list 元信息。`pub(crate)` 供 usage.rs（F5 用量）复用枚举。
pub(crate) struct CodexSessionInfo {
    pub(crate) sid: String,
    pub(crate) path: PathBuf,
    /// session_meta.cwd（分组键；缺 → "" → 归「(codex)」组）。
    pub(crate) cwd: String,
    mtime_ms: i64,
}

/// 枚举本机 Codex 会话：walk `<codex_root>/sessions` 日期树 `rollout-*.jsonl`，读**首行** session_meta
/// 取 cwd。Codex 未启用（无 `~/.codex/sessions`）→ 空 vec（零回归）。`pub(crate)` 供 usage.rs（F5）复用。
pub(crate) fn enumerate_codex_sessions() -> Vec<CodexSessionInfo> {
    use crate::adapter::AgentKind;
    let Some(root) = crate::adapter::for_kind(AgentKind::Codex).data_root() else {
        return Vec::new();
    };
    let sessions_dir = crate::adapter::records_dir_for(AgentKind::Codex, &root);
    if !sessions_dir.is_dir() {
        return Vec::new();
    }
    let layout = crate::adapter::for_kind(AgentKind::Codex).layout();
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(&sessions_dir)
        .into_iter()
        .filter_map(Result::ok)
    {
        let p = entry.path();
        if !p.is_file() || !crate::adapter::has_record_ext(p) {
            continue;
        }
        // sid 从 `rollout-<ts>-<uuid>` 文件名末 UUID；非此命名 → 跳（非 Codex 会话文件）。
        let Some(sid) = crate::adapter::session_id_from_path_with(layout, p) else {
            continue;
        };
        out.push(CodexSessionInfo {
            sid,
            path: p.to_path_buf(),
            cwd: read_codex_session_cwd(p),
            mtime_ms: file_mtime_ms(p),
        });
    }
    out
}

/// 读 Codex rollout **首行**（session_meta 是首条记录）→ cwd。缺/坏 → ""（归「(codex)」组）。
fn read_codex_session_cwd(p: &Path) -> String {
    let Ok(f) = File::open(p) else {
        return String::new();
    };
    let mut line = String::new();
    use std::io::BufRead;
    if BufReader::new(f).read_line(&mut line).is_err() {
        return String::new();
    }
    serde_json::from_str::<serde_json::Value>(line.trim())
        .ok()
        .and_then(|v| crate::codex_record::session_meta_cwd(&v).map(str::to_string))
        .unwrap_or_default()
}

fn file_mtime_ms(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 合成 Codex 项目（枚举 + 分组）。`project_dir` 键 = `codex:<cwd>`——F1a-3c 的
/// `stream_history_sessions_in_project` 按此前缀识别 Codex 项目 + 还原 cwd。
fn codex_projects() -> Vec<HistoryProject> {
    codex_projects_from(enumerate_codex_sessions())
}

/// 纯：Codex 会话按 cwd 分组 → HistoryProject（供 hermetic 测）。
fn codex_projects_from(sessions: Vec<CodexSessionInfo>) -> Vec<HistoryProject> {
    use std::collections::HashMap;
    let mut groups: HashMap<String, (u32, i64)> = HashMap::new(); // cwd → (count, max_mtime)
    for s in &sessions {
        let e = groups.entry(s.cwd.clone()).or_insert((0, 0));
        e.0 += 1;
        e.1 = e.1.max(s.mtime_ms);
    }
    groups
        .into_iter()
        .map(|(cwd, (count, last))| {
            let project_name = if cwd.is_empty() {
                "(codex)".to_string()
            } else {
                Path::new(&cwd)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(cwd.as_str())
                    .to_string()
            };
            HistoryProject {
                project_path: cwd.clone(),
                project_name,
                project_dir: format!("codex:{cwd}"),
                session_count: count,
                starred_count: 0,
                hidden_count: 0,
                last_activity: last,
                // Codex 无 pidfile 判活 = F4；F1a 先 false（会话仍可读，只是不显示「活着」）。
                has_live: false,
                origin: None,
            }
        })
        .collect()
}

/// F1a-3c：Codex 会话 → `HistorySessionEntry`（点开走已接 read 路）。元数据（starred/title/hidden）
/// 按 sid 查 `HistoryMetadata`（同 Claude）。started/updated 先 mtime 兜底、count 先 0（F1a MVP）。
fn codex_session_entry(s: &CodexSessionInfo, metadata: &HistoryMetadata) -> HistorySessionEntry {
    let meta = metadata.entries.get(&s.sid).cloned().unwrap_or_default();
    let project_name = if s.cwd.is_empty() {
        "(codex)".to_string()
    } else {
        Path::new(&s.cwd)
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or(s.cwd.as_str())
            .to_string()
    };
    HistorySessionEntry {
        session_id: s.sid.clone(),
        project_path: s.cwd.clone(),
        project_name,
        ai_title: None,
        first_user_excerpt: codex_first_user_excerpt(&s.path),
        started_at: s.mtime_ms,
        updated_at: s.mtime_ms,
        jsonl_path: s.path.to_string_lossy().into_owned(),
        is_live: false, // Codex 判活 = F4（无 pidfile）
        message_count_approx: 0,
        is_bg: false,
        starred: meta.starred,
        custom_title: meta.custom_title,
        hidden: meta.hidden,
        forked_from_session_id: None,
        forked_from_message_uuid: None,
        origin: None,
    }
}

/// Codex 会话首条 user message 文本（列表摘要）。读至多 200 行找首个 role=user 的 message；
/// 跳 aterm 坑②：`<environment_context>` 注入上下文是 meta 非真用户输入。截 200 字符。空→""。
fn codex_first_user_excerpt(path: &Path) -> String {
    use std::io::BufRead;
    let Ok(f) = File::open(path) else {
        return String::new();
    };
    for line in BufReader::new(f).lines().map_while(Result::ok).take(200) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        if crate::codex_record::classify(&v) == crate::codex_record::CodexRecordKind::Message
            && crate::codex_record::message_role(&v) == Some("user")
        {
            let text = crate::codex_record::unwrap_envelope(&v)
                .and_then(|(_, p)| p.get("content"))
                .map(crate::codex_record::flatten_text)
                .unwrap_or_default();
            let t = text.trim();
            // Phase G 审计修：列表摘要去噪复用渲染路同一 `is_injected_context`（3 标记：environment_context/
            // recommended_plugins/# AGENTS.md instructions）——此前只跳 environment_context、与渲染去噪漂移，
            // 首条是 plugins/AGENTS.md 注入的会话列表预览会露机器注入文本。
            if !t.is_empty() && !crate::codex_record::is_injected_context(t) {
                return t.chars().take(200).collect();
            }
        }
    }
    String::new()
}

/// issue #12: 流式版（取代已删的非流式 `list_history_sessions_in_project`）。
///
/// 用 Tauri 2 `Channel<HistorySessionEntry>` 边解析边发，前端可逐条增量渲染。
/// 收益：大项目（几十个 session × 几 MB jsonl）首条 < 100ms 出现，不再"等齐"。
///
/// 取消：前端 drop channel 引用时 `on_entry.send()` 返 Err → break loop。
/// 因为本 IPC 在 spawn_blocking 里跑同步 IO，立刻终止下次 send 即可释放资源。
///
/// 返回总计 emit 的 entry 数（前端可拿来对账 / 显示进度终值）。
#[tauri::command]
pub async fn stream_history_sessions_in_project(
    project_dir: String,
    on_entry: tauri::ipc::Channel<HistorySessionEntry>,
    map: tauri::State<'_, Arc<SessionMap>>,
) -> Result<u32, String> {
    let map = map.inner().clone();
    tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        // Phase 2 F1a-3c：Codex 合成项目（键 `codex:<cwd>`）→ 枚举 + 过滤该 cwd 列会话；点开走
        // 已接的 read 路（stream_read_session_jsonl 多 kind）。Claude 项目（非 codex: 前缀）走原路（零回归）。
        if let Some(cwd) = project_dir.strip_prefix("codex:") {
            let metadata = load_metadata().unwrap_or_default();
            let mut count = 0u32;
            for s in enumerate_codex_sessions()
                .into_iter()
                .filter(|s| s.cwd == cwd)
            {
                if on_entry.send(codex_session_entry(&s, &metadata)).is_err() {
                    return Ok(count); // 前端 drop channel → 取消
                }
                count += 1;
            }
            return Ok(count);
        }
        let claude_dir = paths::resolve_claude_dir().ok_or("claude dir not found")?;
        let projects_dir = crate::adapter::records_dir(&claude_dir);
        let target = PathBuf::from(&project_dir);
        if !target.starts_with(&projects_dir) {
            return Err(format!(
                "refuse: {} outside {}",
                target.display(),
                projects_dir.display()
            ));
        }
        if !target.is_dir() {
            return Err(format!("{} not a directory", target.display()));
        }
        let metadata = load_metadata().unwrap_or_default();

        let mut count = 0u32;
        let files = match std::fs::read_dir(&target) {
            Ok(d) => d,
            Err(e) => return Err(format!("read {}: {e}", target.display())),
        };
        for f in files.flatten() {
            let p = f.path();
            if crate::adapter::has_record_ext(&p) {
                if let Some(entry) = analyze_jsonl(&p, &metadata, &map) {
                    if on_entry.send(entry).is_err() {
                        // 前端 drop channel → 取消
                        tracing::info!(
                            "stream_history_sessions_in_project({}): cancelled at {} entries",
                            target.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
                            count
                        );
                        return Ok(count);
                    }
                    count += 1;
                }
            }
        }
        tracing::info!(
            "stream_history_sessions_in_project({}): {} sessions in {}ms",
            target.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
            count,
            started.elapsed().as_millis()
        );
        Ok(count)
    })
    .await
    .map_err(|e| format!("spawn_blocking join: {e}"))?
}

/// issue #12: 流式版（取代已删的非流式 `read_session_jsonl`）。
///
/// 按 100 行一 chunk 边读边发，前端可在 ~500ms 内开始渲染首屏（即使整 jsonl
/// 上千条 / 10MB+）。
///
/// 取消：前端 drop channel 时 send 返 Err → break。
#[tauri::command]
pub async fn stream_read_session_jsonl(
    jsonl_path: String,
    on_chunk: tauri::ipc::Channel<Vec<crate::bridge::JsonlLinePayload>>,
) -> Result<u32, String> {
    const CHUNK_SIZE: usize = 100;
    tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let target = PathBuf::from(&jsonl_path);
        // Phase 2 F1a：按路径判 agent kind（Claude `~/.claude/projects` vs Codex `~/.codex/sessions`）。
        // Claude 路径 kind=ClaudeCode → 根/session_id/解析与原字节一致（零回归）；Codex 走对应根 + 映射。
        let kind = crate::adapter::kind_of_path(&target);
        let root = crate::adapter::for_kind(kind)
            .data_root()
            .map(|dr| crate::adapter::records_dir_for(kind, &dr))
            .ok_or("agent data dir not found")?;
        if !target.starts_with(&root) {
            return Err(format!(
                "refuse: {} outside {}",
                target.display(),
                root.display()
            ));
        }
        if !crate::adapter::has_record_ext(&target) {
            return Err("not a .jsonl file".into());
        }

        let session_id = crate::adapter::session_id_from_path_with(
            crate::adapter::for_kind(kind).layout(),
            &target,
        )
        .unwrap_or_default();
        let file = File::open(&target).map_err(|e| format!("open {}: {e}", target.display()))?;
        let reader = BufReader::new(file);
        let path_str = target.to_string_lossy().into_owned();
        let mut cwd_seen: Option<String> = None;
        let mut buf: Vec<crate::bridge::JsonlLinePayload> = Vec::with_capacity(CHUNK_SIZE);
        let mut total = 0u32;
        // P5.1：history 流式读时同样给每行 seq（per-file 单调）。SessionViewer
        // 用 RecordTimeline 排序时跟实时 tab 走同一套逻辑。
        let mut next_seq: u64 = 0;

        for line in reader.lines().map_while(Result::ok) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let rec = match crate::parser::parse_for_kind(kind, trimmed) {
                Ok(Some(r)) if r.is_displayable() => r,
                _ => continue,
            };
            if let JsonlRecord::User { cwd, .. } = &rec {
                if cwd_seen.is_none() {
                    cwd_seen = cwd.clone();
                }
            }
            let seq = next_seq;
            next_seq += 1;
            buf.push(crate::bridge::JsonlLinePayload {
                session_id: session_id.clone(),
                cwd: cwd_seen.clone(),
                path: path_str.clone(),
                seq,
                // 历史浏览器读本地 jsonl，无远端来源标签。
                origin: None,
                message: rec,
            });
            total += 1;
            if buf.len() >= CHUNK_SIZE {
                let chunk = std::mem::replace(&mut buf, Vec::with_capacity(CHUNK_SIZE));
                if on_chunk.send(chunk).is_err() {
                    tracing::info!(
                        "stream_read_session_jsonl({}): cancelled at {} records",
                        session_id,
                        total
                    );
                    return Ok(total);
                }
            }
        }
        if !buf.is_empty() {
            let _ = on_chunk.send(buf);
        }
        tracing::info!(
            "stream_read_session_jsonl({}): {} records in {}ms",
            session_id,
            total,
            started.elapsed().as_millis()
        );
        Ok(total)
    })
    .await
    .map_err(|e| format!("spawn_blocking join: {e}"))?
}

/// 本地删除的路径守卫（Batch4-F15）：canonicalize 后校验，`..` 与 symlink 穿越都拒。
///
/// 旧实现只做 `PathBuf::starts_with`——那是纯组件前缀比较，不解析 `..` 不解
/// symlink：`<projects>/../../x.jsonl` 能通过校验、由 OS 在 remove_file 时解析。
/// 对照远端版 `sftp.rs::remove_remote_file`（canonicalize 双重守卫），本地反而
/// 更弱。现在两边 canonicalize（Windows 上 canonicalize 产生 `\\?\` 前缀，
/// 单边做必然不匹配），扩展名也在 canonical 路径上查（防 symlink 指向非 jsonl）。
///
/// 返回 canonical 后的删除目标；抽成纯函数以便注入 tempdir 直测。
///
/// 已接受取舍：canonicalize → remove_file 之间存在理论 TOCTOU 窗口（期间目录
/// 组件被换成 symlink）。path-based API 固有限制；威胁模型是"前端传错路径"
/// 而非恶意本地攻击者，与 sftp.rs 远端版（realpath → remove）同级，不做
/// openat/O_NOFOLLOW 级加固。
fn validate_delete_target(jsonl_path: &str, projects_dir: &Path) -> Result<PathBuf, String> {
    let target = PathBuf::from(jsonl_path);
    if !target.exists() {
        return Err(format!("{} does not exist", target.display()));
    }
    let canon_target = target
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {e}", target.display()))?;
    let canon_projects = projects_dir
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {e}", projects_dir.display()))?;
    if !canon_target.starts_with(&canon_projects) {
        return Err(format!(
            "refuse delete: {} is outside {}",
            canon_target.display(),
            canon_projects.display()
        ));
    }
    if !crate::adapter::has_record_ext(&canon_target) {
        return Err("refuse delete: not a .jsonl file".into());
    }
    Ok(canon_target)
}

#[tauri::command]
pub fn delete_history_session(session_id: String, jsonl_path: String) -> Result<(), String> {
    // 安全校验：必须在 claude_dir/projects 之下，避免前端传错路径误删别处文件
    let claude_dir = paths::resolve_claude_dir().ok_or("claude dir not found")?;
    let projects_dir = crate::adapter::records_dir(&claude_dir);
    let target = validate_delete_target(&jsonl_path, &projects_dir)?;

    std::fs::remove_file(&target).map_err(|e| format!("remove {}: {e}", target.display()))?;
    tracing::info!("history: deleted {}", target.display());

    // 同步从 metadata 移除条目
    remove_metadata_entry(&session_id);
    Ok(())
}

// === F62：从历史某一轮创建分支 ===
//
// **§1 只读铁律：不修约、正交（照 F47 先例）**。建分支是「用户显式点某条消息 → 复制
// `[根 … 该消息]` 前缀产出一个**全新** jsonl」——原会话一字节不改、纯新增，与「monitor
// 作为监视器不改坏正在监视的会话文件（尤其防自动/后台写）」这条约正交。防误伤守卫：
// ①源路径白名单（canonicalize + starts_with(projects) + `.jsonl`）；②只写**新生成的
// sid**、目标已存在则拒（绝不覆盖任何现存会话）。
//
// **落盘格式 = Claude 原生 `/branch`**（issue #12 `forkedFrom`，本机 fe4aad07 实证 +
// `claude --resume` 回读实测）：复制沿 parentUuid 从分叉点回溯到根的**线性前缀**，逐条
// 保留原 uuid/parentUuid、`sessionId` 改新 id、加 `forkedFrom{sessionId:源, messageUuid:自身}`。
// 分叉点之后的记录、被 ESC 回退的兄弟子树、sidechain 全部不带过来（前缀只走祖先链）。
//
// === G0（branch-anywhere）：上面那句「= 原生格式」已被扩样本复核，并钉成机检 ===
//
// **样本**：两份**早于本功能合入（07-16）**因而只可能是 CC 自己产的 fork
// —— `0473c3a0`(07-03) 与 `fe4aad07`(07-05)；外加一条三代 fork 链
// （`a40059e8 → 7c2a26d6 → 4f3fba62`）证明每次 `/branch` 都产**独立文件**、父会话仍可 resume。
//
// **决定性指纹**：两份原生 fork 的复制段在源文件里分别跨 1964 / 170 行，却只取了
// 1402 / 118 条 —— **跳过了 562 / 52 条落在区间内的旁支**。若官方是「线性文件切片」，
// 那些记录会被一并带走。⇒ **官方 `/branch` 走的就是祖先回溯，与本实现相同。**
//
// 逐字段亦一致：uuid **原样保留**（不 remap）· timestamp **不改** ·
// `slug`/`sourceToolAssistantUUID`/`agentName` **照样带着** · 复制段只有
// `assistant`/`user`/`attachment`/`system` 四类、不带无 uuid 的旁挂记录
// （`mode`/`permission-mode`/`ai-title`/`last-prompt`/`file-history-snapshot`/`queue-operation`）·
// `logicalParentUuid` 在原生 fork 里就带着指向文件外的目标 ⇒ 官方自己不保证这条边。
//
// **⚠ 别拿 `claude-agent-sdk` 的 `fork_session` 当规范。**
// 它也是官方的，但它 remap 全部 uuid、清 `slug` 等字段、改末条 timestamp、
// 且 `up_to_message_id` 是**线性切片** —— 那些是 SDK 自己的选择，**不是 CC 的落盘规范**。
// 规划 branch-anywhere 时正是照着它列出六条「我们的缺口」，被上面这批语料全部证伪。
// 判据只有一个：**CC 自己落在盘上、`claude --resume` 能读回去的那个格式**。
// `branch_matches_native_fork_shape` 就是这条判据的机检版本，改动本函数前先读它。
//
// **用 `serde_json::Value` 原样搬运**（不走有损的 `JsonlRecord` enum，避免丢 gitBranch/
// version/origin 等 schema 外字段）——除 sessionId/forkedFrom 两处有意改动外逐字段忠实。

/// 建分支的返回体（前端据此提示 / 一键 resume 新分支）。
///
/// **`Deserialize` 是给远端那条路用的**（G6）：daemon 的 `--fork-session` 在 stdout 吐同形 JSON，
/// `remote_branch` 直接反序列化成本类型 —— 两条路一个类型，前端的成功处理才只有一份。
#[derive(Debug, Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
pub struct BranchResult {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "jsonlPath")]
    pub jsonl_path: String,
}

/// 源会话路径守卫（与 `validate_delete_target` 同构，但不改动 delete 那段安全关键代码）。
/// canonicalize 两边解 `..`/symlink → 必须落在 projects 内 → 扩展名 `.jsonl`。
fn validate_branch_source(jsonl_path: &str, projects_dir: &Path) -> Result<PathBuf, String> {
    let target = PathBuf::from(jsonl_path);
    if !target.exists() {
        return Err(format!("{} does not exist", target.display()));
    }
    let canon_target = target
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {e}", target.display()))?;
    let canon_projects = projects_dir
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {e}", projects_dir.display()))?;
    if !canon_target.starts_with(&canon_projects) {
        return Err(format!(
            "refuse branch: {} is outside {}",
            canon_target.display(),
            canon_projects.display()
        ));
    }
    if !crate::adapter::has_record_ext(&canon_target) {
        return Err("refuse branch: not a .jsonl file".into());
    }
    Ok(canon_target)
}

/// 读一个 jsonl 文件为逐行 `serde_json::Value`（剥 BOM、跳空行；解析失败的行**保留原样**
/// 不了了之——建分支只复制祖先链上的记录，坏行若不在链上自然被忽略）。
fn read_jsonl_values(path: &Path) -> Result<Vec<serde_json::Value>, String> {
    let file = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| format!("read {}: {e}", path.display()))?;
        let trimmed = line.trim_start_matches('\u{feff}').trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            out.push(v);
        }
    }
    Ok(out)
}

// G1：**记录变换已提成共享 crate** `branch-core` —— monitor 与远端 daemon 共用同一份。
// 搬走的理由与选型过程见 `.claude/planned-build/branch-anywhere/features/G1-*.md`；
// 落盘格式的实证判据见该 crate 的 `build_branch_records` 头注。
// **本文件只留 IO 与路径守卫**（那两样是 monitor 侧特有的）。
use branch_core::build_branch_records;

/// F62 IPC：从历史会话的某条消息创建分支。前端点消息卡上的 `⑂` 时调，成功返回新 sid。
/// 见本段顶部大注释（§1 正交、原生格式、守卫）。薄壳：resolve_claude_dir → 委托 branch_impl。
#[tauri::command]
pub fn create_branch_session(
    source_jsonl_path: String,
    message_uuid: String,
) -> Result<BranchResult, String> {
    let claude_dir = paths::resolve_claude_dir().ok_or("claude dir not found")?;
    let projects_dir = crate::adapter::records_dir(&claude_dir);
    branch_impl(&source_jsonl_path, &message_uuid, &projects_dir)
}

/// 建分支核心（可注入 projects_dir 直测，绕开 resolve_claude_dir 全局依赖——同 delete 的
/// validate_delete_target 测法）。安全承诺全在这层：源零改动、只写新 sid、绝不覆盖。
fn branch_impl(
    source_jsonl_path: &str,
    message_uuid: &str,
    projects_dir: &Path,
) -> Result<BranchResult, String> {
    let source = validate_branch_source(source_jsonl_path, projects_dir)?;
    let src_sid = source
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("refuse branch: cannot derive source sid from path")?
        .to_string();

    let lines = read_jsonl_values(&source)?;
    let new_sid = uuid::Uuid::new_v4().to_string();
    let records = build_branch_records(&lines, message_uuid, &src_sid, &new_sid)?;

    // 目标写进源会话同目录（projects 内某项目目录），文件名 = 新 sid。
    let parent = source
        .parent()
        .ok_or("refuse branch: source has no parent dir")?;
    let out_path = parent.join(format!("{new_sid}.jsonl"));
    write_branch_file(&out_path, &records)?;
    tracing::info!(
        "history: branched sid={new_sid} from {src_sid}@{message_uuid} ({} records)",
        records.len()
    );

    Ok(BranchResult {
        session_id: new_sid,
        jsonl_path: out_path.to_string_lossy().into_owned(),
    })
}

/// 把记录序列化成 JSONL 原子写入 `out_path`。**`create_new`：目标已存在则直接失败**——
/// 自证「绝不覆盖任何现存会话」契约，消 exists()→write 的 TOCTOU 窗口。抽出便于直测。
fn write_branch_file(out_path: &Path, records: &[serde_json::Value]) -> Result<(), String> {
    use std::io::Write as _;
    let mut body = String::new();
    for rec in records {
        body.push_str(&serde_json::to_string(rec).map_err(|e| format!("serialize: {e}"))?);
        body.push('\n');
    }
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out_path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                format!("refuse branch: {} already exists", out_path.display())
            } else {
                format!("create {}: {e}", out_path.display())
            }
        })?;
    f.write_all(body.as_bytes())
        .map_err(|e| format!("write {}: {e}", out_path.display()))
}

/// 从本地 history-metadata.json 移除某 sid 的条目（best-effort）。本地删除与远端删除
/// （issue F11 `delete_remote_history_session`）共用——元数据是 monitor 本地按 sid 的注解，
/// 无论会话本体在本地还是远端，删除后都该清掉对应注解。
pub(crate) fn remove_metadata_entry(sid: &str) {
    let mut metadata = load_metadata().unwrap_or_default();
    if metadata.entries.remove(sid).is_some() {
        let _ = save_metadata(&metadata);
    }
}

#[tauri::command]
pub fn update_history_metadata(
    session_id: String,
    patch: MetadataPatch,
) -> Result<EntryMetadata, String> {
    let mut metadata = load_metadata().unwrap_or_default();
    let entry = metadata.entries.entry(session_id.clone()).or_default();
    if let Some(s) = patch.starred {
        entry.starred = s;
    }
    if let Some(t) = patch.custom_title {
        // Some(Some(s)) 设置；Some(None) 清空
        entry.custom_title = t.filter(|s| !s.trim().is_empty());
    }
    if let Some(h) = patch.hidden {
        entry.hidden = h;
    }
    if let Some(a) = patch.last_account {
        // Some(Some(name)) 设值；空/空白串 → filter 后 None = 清空（JSON null 走不到这，见 struct 注释）
        entry.last_account = a.filter(|s| !s.trim().is_empty());
    }
    entry.updated_at = now_ms();
    let result = entry.clone();
    save_metadata(&metadata)?;
    Ok(result)
}

/// 纯变换：metadata → sid→lastAccount（只含真有 lastAccount 的条目）。抽出便于单测。
fn last_accounts_of(meta: HistoryMetadata) -> HashMap<String, String> {
    meta.entries
        .into_iter()
        .filter_map(|(sid, e)| e.last_account.map(|a| (sid, a)))
        .collect()
}

/// A4：只读——返回 sid → lastAccount（上次用本工具带账号起该会话时记的）。前端账号徽章
/// 源②（DESIGN §3）：live 探测不到时用它兜底。只含真有 lastAccount 的条目；读失败 → 空表
/// （降级：徽章退回 live/未知，不报错）。**不写 jsonl、不改任何状态。**
#[tauri::command]
pub fn list_last_accounts() -> HashMap<String, String> {
    last_accounts_of(load_metadata().unwrap_or_default())
}

/// 在新终端窗口里 resume 一个历史会话。
///
/// v2.8.1（bug 修复）：改为在 **PowerShell**（系统自带 `powershell.exe`，**加载用户
/// profile**）里跑，命令优先用户的 `cc` wrapper、回退 `claude`。详 `resume_impl`。
/// Windows 上优先 wt.exe，找不到回退独立控制台。其他平台暂不支持。
/// G3b / Phase G：`account` = 用哪个账号起，**三态**见 [`LaunchAccount`]。
///
/// 参数缺席 ⇒ 输出与本参数存在之前**逐字节相同**（既有调用点无需改）。
/// `{"kind":"base"}` ⇒ **显式** `unset CLAUDE_CONFIG_DIR`（不是「什么都不加」——
/// 那会被 shell rc 里的 `export CLAUDE_CONFIG_DIR=<默认账号>` 顶掉 = 静默串号）。
///
/// **订正**：G3b-1 当时把「缺省 / 空 = 账号 0（一个字都不注入，与 IR 的 `--base` 对齐）」
/// 写进了这段注释 —— 那句话是错的：IR 的 `--base` 做的是 `unset`，不是「不注入」，
/// 远端那条路也确实渲染成 `unset CLAUDE_CONFIG_DIR; `。Phase G 审计两个视角各自抓到。
///
/// P3t-Y2：新增 `tmux_name` —— **POSIX 本机**要把会话建进 tmux 时的会话名。
/// 缺席（今天所有调用点都缺席）⇒ 渲染器诚实降级回旧路 ⇒ 与本参数存在之前逐字节相同。
/// 名字必须由前端 `mintTmuxName` 铸（那是全仓唯一带撞名避让的铸造口），所以它只能传进来、
/// 不能在 Rust 里造。Windows 那一侧**不读它**（`C12`）。
#[tauri::command]
pub fn resume_history_session(
    session_id: String,
    cwd: String,
    launcher: Option<String>,
    account: Option<LaunchAccount>,
    tmux_name: Option<String>,
) -> Result<(), String> {
    resume_impl(
        &session_id,
        &cwd,
        launcher.as_deref(),
        account.as_ref(),
        tmux_name.as_deref(),
    )
}

/// F34：用户自定义 resume 启动命令（设置面板「本地 resume 命令」）。
/// 拼进 shell 前必须校验——只允许命令名+简单参数形态（字母数字 `-_.` 与空格），
/// 杜绝 `;`/`|`/`$()` 等注入面。空/纯空白视为未设置。
fn sanitize_launcher(launcher: Option<&str>) -> Result<Option<String>, String> {
    let Some(l) = launcher.map(str::trim).filter(|l| !l.is_empty()) else {
        return Ok(None);
    };
    let valid = l
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '));
    if !valid {
        return Err(format!(
            "refuse resume: 自定义 resume 命令含非法字符（仅允许字母数字、-_.、空格）: {l:?}"
        ));
    }
    Ok(Some(l.to_string()))
}

/// F06（unify-launch）：本地路径的动作枚举——与 TS `LaunchAction` 同构（无 `attach` 变体：
/// 本地会话从无 attach 概念）。
enum LocalPsAction {
    New,
    Resume(String),
}

/// 构造本地 PowerShell 命令体（不含 `-EncodedCommand` 编码）——`build_resume_ps_command`/
/// `build_new_session_ps_command` 曾各自逐字符重复的「F34 自定义命令优先 → cc 别名探测优先 →
/// 回退默认拉起」分支在此收拢成一处（F06：两套 builder 收进同一意图模型）。
///
/// 防注入：resume 场景的 sid 来自前端历史条目，理论上是 UUID，但作为拼进 shell 命令的
/// 不可信输入必须校验——只允许 `[A-Za-z0-9_-]`，否则拒绝（杜绝 `; rm -rf` 之类）。
///
/// 优先 `cc`：检测到用户的 `cc` 函数（PowerShell 集成 wrapper，内部含 `__ccm_bind` +
/// 用户自己的代理 / env 设置）就用它；检测不到才回退默认拉起器。命令在 profile 已加载的
/// PowerShell 里跑（见 resume_impl 不带 -NoProfile），所以即使回退，profile 里的 PATH /
/// 代理 env 仍生效。
///
/// 抽成独立函数是为了单测（不 spawn 进程也能验证防注入 + cc 优先逻辑）。
/// （纯字符串构造，跨平台可编译可测；拉起本身在 launch.rs 按平台门控。）
/// L1：**与平台无关**的本地拉起决策 —— 校验 + 按活跃适配器算出「用哪个命令」。
///
/// 抽出来是因为 L1 给本地加了第二个渲染器（POSIX）。**校验与选择只能有一份**，
/// 否则两个平台迟早各自漂移；而「怎么写这个条件判断」才是平台差异
/// （PowerShell 用 `Get-Command`，POSIX 用 `command -v`）。
///
/// **sid 校验留在这里**：它与前端 `validateLocalLaunch` 是**两道独立防线**，不是重复

/// G3b：账号前缀 —— 把 `CLAUDE_CONFIG_DIR` 注入本地拉起命令。
///
/// # `None` = 账号 0 = **一个字都不注入**
///
/// 与 IR 的 `--base` 语义对齐，也保证「没选账号」这条路的输出**与本功能之前逐字节相同**
/// —— 既有那批钉死输出的测试因此原样全绿，它们就成了「账号 0 不变」的守卫。
///
/// # 校验语义照抄 TS 侧的 `isValidConfigDir`（`src/shell-quote.ts:41`）
///
/// **不重新发明判据**：那边已经因为账号隔离审计 D7（extraEnv key 无校验）收紧过一轮。
/// 拒的东西：非绝对路径 · `/` 本身 · 含 `/../` 或以 `/..` 结尾 · shell 元字符/引号/控制符 ·
/// 可欺骗 Unicode（零宽 / 双向控制 / NBSP / BOM）。
///
/// **非法即 Err，绝不拼进命令** —— 这条路径的产物会进 shell，宽容一格就是注入面。
/// 「这次拉起用哪个账号」。**三态，不是两态** —— Phase G 审计抓出的一条静默串号：
///
/// | 取值 | 含义 | 产出的前缀 |
/// |---|---|---|
/// | 参数缺席（`None`） | **调用方没表态** | 空串（既有调用点逐字节等价旧行为） |
/// | `{"kind":"base"}` | **用户显式选了账号 0** | `unset CLAUDE_CONFIG_DIR; ` |
/// | `{"kind":"named","configDir":"…"}` | 具名账号 | `export CLAUDE_CONFIG_DIR='…'; ` |
///
/// **为什么「账号 0」不能等于「什么都不加」**（`src/shell-quote.ts` 的 Z03 用整段注释写着，
/// 远端那条路也确实渲染成 `unset CLAUDE_CONFIG_DIR; `）：用户的 shell rc 里很可能有一句
/// `export CLAUDE_CONFIG_DIR=<默认账号>`（`cc-acct-iso shellinit` 生成的就是它），
/// 而本地拉起**故意加载 rc**（`launch.rs` 的 `bash -lic` / 不带 `-NoProfile` 的 powershell）
/// ⇒ 「什么都不加」会落到默认账号上 = **静默串号**。弹窗上写着「不注入」，实际起在别的号上，
/// 正是 `fork-launch.ts` 头注要防的那件事。
///
/// **为什么不用一个魔法串**（如 `configDir: "__base__"`）：R05 刚把跨文件比字符串字面量的
/// `"__base__"` 换成判别联合，理由是「拼错一个字符 tsc 抓不到，而行为是基座选项静默变成一个
/// 名叫 `__base__` 的普通账号」。这里不重蹈。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LaunchAccount {
    /// 账号 0：**显式不注入**（产出 `unset`），不是「什么都不做」。
    Base,
    Named {
        #[serde(rename = "configDir")]
        config_dir: String,
    },
}

/// 两种 shell 共用的元字符黑名单。**`\` 不在里面** —— 见 `validate_config_dir_ps`：
/// Windows 的账号目录长成 `C:\Users\z\.claude-accts\z`，把 `\` 一律禁掉等于禁掉整个平台。
/// 它在两种 shell 的**单引号**里都是字面量（POSIX `'…'` 无转义；PowerShell `'…'` 无插值），
/// 所以真正要挡的是能提前闭合引号或另起命令的那几个。
///
/// 历史注记（E75，2026-08-01 已修）：这里当初写成**字符串**而不是 `&[char]` 数组，
/// 是因为 `'\"'`（字符字面量里的双引号）会让 `test-support/strip-comments.ts` 的状态机
/// 以为字符串开始了、从此不再剥注释 ⇒ 本文件后面注释里的 `#[tauri::command]` 字样
/// 被 C04a 守卫当成真属性，报出一个不存在的命令。**那是当时的绕法。**
/// 守卫已经会认 Rust 字符字面量了，所以这条约束**不再成立**；`&str` 形态留着只是因为
/// 配 `.contains(c)` 读起来更顺，不是被逼的。
/// ⚠ **U8c-1 起只剩 Windows / 测试期在用**（POSIX 侧已改调 `backend::control::payload`）。
/// cfg 与它唯一的消费者 [`validate_config_dir_ps`] 对齐 —— 不加就是三条 `never used`
/// 警告，而 `cargo build` 不带 `-D warnings` ⇒ **不会红**（clippy 集合差抓到的）。
// 〔audit-0805 08-06〕**这份副本删了**（E3：一个事实恰好一个权威源）。
// 权威源是 `backend::control::payload::SHELL_META_COMMON`，经 `is_command_unsafe_char` 派生。
// ⚠ 不是理论风险：`payload.rs` 的头注逐字记着，本文件此前那张**不可见字符表**就漂过 ——
// 缺 `U+1680` · `U+2000..200A` · `U+202F` · `U+205F` · `U+2060..2064` · `U+3000`，
// 是一处纵深防御缺口。同一个文件、同一族副本，这次连元字符表一起收掉。

// U8c-3-alt（账本 S18 收口）：这里原本有一张 `SPOOFABLE` 表 —— **U7-3 之前的旧集合**，
// 18 项，缺 `U+1680` · `U+2000..200A` · `U+202F` · `U+205F` · `U+2060..2064` · `U+3000`。
// 它在 U8c-1 之后只剩 Windows 那条路在用；本轮 Windows 也改调 `acct_core::is_deceptive_char`
// （「什么算视觉欺骗」是**平台无关**的判断，与 `is_safe_config_dir` 那条「`\` 与盘符」的
// 平台特化不是一回事 —— `acct-core` 头注对后者的「不合」裁决不适用于这里）。
// ⇒ 那张表**删掉**，不是留着不用：留着就是「旧集合还在仓里等下一个人复制」。

#[cfg(any(windows, test))]
fn has_bad_chars(dir: &str, extra: &str) -> bool {
    // 逐项与权威源等价：`is_command_unsafe_char` = 控制字符 | C1 段 | 元字符 | 视觉欺骗字符；
    // 本函数额外多一个调用点自带的 `extra` 集合（今天唯一调用点传空串）。
    dir.chars()
        .any(|c| crate::backend::control::payload::is_command_unsafe_char(c) || extra.contains(c))
}

/// POSIX 侧校验：必须是**绝对 POSIX 路径**，且不含反斜杠（那边的路径里不该有）。
///
/// **U8c-1：判据本体已搬出本文件** —— P4b 起在 `backend::control::payload::config_dir_command_safe`
/// （U8c-1 时在共享 crate `launch-core`——P4c 起那个 crate 叫 `shell-quote-core` 且只剩 quote）。
/// 本函数只剩「把 bool 变成带上下文的 Err」。
///
/// ⚠ **这次搬家不是纯重构，它把校验变严了**：本文件原先用自己那张 `SPOOFABLE`
/// （18 项，是 **U7-3 之前**的旧集合），而 crate 侧建立在 `acct_core::is_deceptive_char`
/// 的**并集**上 —— 多拒 `U+1680` · `U+2000..200A` · `U+202F` · `U+205F` · `U+2060..2064` ·
/// `U+3000`。U7-3 当时把并集给了两个**读 manifest** 的地方，**拼命令这条路漏了**。
///
/// 诚实定级：那是**纵深防御**缺口，不是当时可利用的洞（configDir 的上游 manifest 读取
/// 已经用并集把过一道）。但「权威也保留本地校验」是本仓自己的纪律（`resolve_query.rs` B2）。
fn validate_config_dir_posix(dir: &str) -> Result<(), String> {
    if crate::backend::control::payload::config_dir_command_safe(dir) {
        Ok(())
    } else {
        Err(format!("拒绝拼入命令：非法 CLAUDE_CONFIG_DIR {dir:?}"))
    }
}

/// PowerShell 侧校验。**与 POSIX 那条的唯一实质差别是「什么算绝对路径」** ——
/// Phase G 审计抓出的一个真 bug：原来两边共用「必须 `/` 开头 + 禁 `\`」，
/// 于是 Windows 上一个真实账号目录（`C:\Users\z\.claude-accts\z`）**必被拒**，
/// 「本机分叉时选一个具名账号」在主平台上 100% 失败。判据照抄 `local_accounts::looks_absolute`
/// （那个函数的头注写明：照搬 `starts_with('/')` 会把每个 Windows 账号判成不安全）。
#[cfg(any(windows, test))]
fn validate_config_dir_ps(dir: &str) -> Result<(), String> {
    let b = dir.as_bytes();
    let drive = b.len() >= 3
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b[2] == b'\\' || b[2] == b'/');
    let absolute = dir.starts_with('/') || drive || dir.starts_with("\\\\");
    // `..` 两种分隔符都要挡（Windows 上 `/` 与 `\` 都是合法分隔符）。
    let dotdot = dir.contains("/../")
        || dir.ends_with("/..")
        || dir.contains("\\..\\")
        || dir.ends_with("\\..");
    if !absolute || dir == "/" || dotdot {
        return Err(format!("拒绝拼入命令：非法 CLAUDE_CONFIG_DIR {dir:?}"));
    }
    if has_bad_chars(dir, "") {
        return Err(format!("拒绝拼入命令：非法 CLAUDE_CONFIG_DIR {dir:?}"));
    }
    Ok(())
}

/// POSIX 侧前缀。三态见 [`LaunchAccount`]；参数缺席 → 空串（逐字节等同旧行为）。
fn config_dir_prefix_posix(account: Option<&LaunchAccount>) -> Result<String, String> {
    match account {
        None => Ok(String::new()),
        // U8c-1：这条串的逐字节形态由内核持有（e2e 探针 `grep -q "unset CLAUDE_CONFIG_DIR;"`）。
        //
        // ⚠ **P4b 改成委托整条臂**（原来是自己 `UNSET_CONFIG_DIR_PREFIX.to_string()`）。
        // 起因是搬家把一处被 crate 边界藏住的事实暴露了出来：clippy 报
        // `Account::Base is never constructed` —— 也就是**这个三态里的 base 那一态，
        // 生产从来没走到内核里**，本文件自己截住了。两处逐字相同 ⇒ 是重复的决定，不是分工。
        // 字节完全一致（两边都是同一个常量），由
        // `posix_account_prefix_is_byte_identical_after_moving_to_the_kernel` 兜。
        // ⚠ 剩下的 `None` 那臂与整个三态 match 仍是镜像 —— 那是**登记在案的重复**，
        // 收它要连 `validate_config_dir_posix`（POSIX 侧多拒一个 `\`）一起重定，不在 P4b 范围。
        Some(LaunchAccount::Base) => crate::backend::control::payload::config_dir_prefix_posix(
            Some(&crate::backend::control::payload::Account::Base),
        ),
        Some(LaunchAccount::Named { config_dir }) => {
            let d = config_dir.trim();
            // 空串**不是**账号 0，是坏数据（空值 ≠ 未设 —— Z01 起整套设计的支点）。
            if d.is_empty() {
                return Err(
                    "refuse resume: 具名账号的 configDir 是空的（账号 0 请用 kind=base）".into(),
                );
            }
            validate_config_dir_posix(d)?;
            // U8c-1：串本身由内核产出（P4b 起在 `backend::control::payload`），本文件不再自己 format。
            crate::backend::control::payload::config_dir_prefix_posix(Some(
                &crate::backend::control::payload::Account::Named { config_dir: d },
            ))
        }
    }
}

/// PowerShell 侧前缀。同上；PS 里用 `$env:` 且单引号是字面量引号（无插值）。
#[cfg(any(windows, test))]
fn config_dir_prefix_ps(account: Option<&LaunchAccount>) -> Result<String, String> {
    match account {
        None => Ok(String::new()),
        // PS 里把环境变量置 `$null` 就是删掉它（等价于 POSIX 的 `unset`）。
        Some(LaunchAccount::Base) => Ok("$env:CLAUDE_CONFIG_DIR=$null; ".to_string()),
        Some(LaunchAccount::Named { config_dir }) => {
            let d = config_dir.trim();
            if d.is_empty() {
                return Err(
                    "refuse resume: 具名账号的 configDir 是空的（账号 0 请用 kind=base）".into(),
                );
            }
            validate_config_dir_ps(d)?;
            Ok(format!("$env:CLAUDE_CONFIG_DIR='{d}'; "))
        }
    }
}

/// ——前端那道拦 UI 传参，这道拦任何绕过前端到达 IPC 的输入。
enum LocalLaunchChoice {
    /// 用户显式指定了命令（F34）⇒ 不做别名探测，直接用。
    Fixed(String),
    /// 有 wrapper 别名（`cc`）：探测得到就用 `preferred`，否则 `fallback`。
    Probe {
        alias: String,
        preferred: String,
        fallback: String,
    },
}

fn local_launch_choice(
    action: &LocalPsAction,
    launcher: Option<&str>,
) -> Result<LocalLaunchChoice, String> {
    if let LocalPsAction::Resume(sid) = action {
        let valid = !sid.is_empty()
            && sid
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !valid {
            return Err(format!("refuse resume: invalid session_id {sid:?}"));
        }
    }
    // F-MA：resume flag / 拉起别名 / 默认拉起都走活跃适配器（CC = --resume / cc / claude）。
    let agent = crate::adapter::active();
    let suffix = |bin: &str| -> String {
        match action {
            LocalPsAction::Resume(sid) => format!("{bin} {} {sid}", agent.resume_flag()),
            LocalPsAction::New => bin.to_string(),
        }
    };
    // F34：设了自定义命令就直接用（不再别名自动检测——用户显式选择优先）
    if let Some(l) = sanitize_launcher(launcher)? {
        return Ok(LocalLaunchChoice::Fixed(suffix(&l)));
    }
    let def = agent.default_launcher();
    Ok(match agent.launcher_alias() {
        Some(alias) => LocalLaunchChoice::Probe {
            alias: alias.to_string(),
            preferred: suffix(alias),
            fallback: suffix(def),
        },
        None => LocalLaunchChoice::Fixed(suffix(def)),
    })
}

/// **平台门控**：生产路径上它只在 Windows 被调用（POSIX 走 `build_local_posix_command`）；
/// 但逐字节钉死它输出的测试要在所有平台跑 ⇒ `any(windows, test)`。
/// 用精确门控而不是 `#[allow(dead_code)]`——后者会把将来真正的死代码一并盖住。
#[cfg(any(windows, test))]
fn build_local_ps_command(
    action: &LocalPsAction,
    launcher: Option<&str>,
    account: Option<&LaunchAccount>,
) -> Result<String, String> {
    let prefix = config_dir_prefix_ps(account)?;
    Ok(prefix + &match local_launch_choice(action, launcher)? {
        LocalLaunchChoice::Fixed(cmd) => cmd,
        // 有 wrapper 别名（cc）：优先它、检测不到回退 default。
        LocalLaunchChoice::Probe {
            alias,
            preferred,
            fallback,
        } => format!(
            "if (Get-Command {alias} -ErrorAction SilentlyContinue) {{ {preferred} }} else {{ {fallback} }}"
        ),
    })
}

/// L1：本地拉起命令的 **POSIX** 渲染 —— 与上面那个是同一个决策的另一种写法。
///
/// `Get-Command` 的 POSIX 等价物是 `command -v`：它同样能找到 shell **函数**与别名
/// （`ccm` 的 `cc` 集成正是一个函数），而命令跑在 `bash -lic` 里、rc 已加载 ⇒ 找得到。
fn build_local_posix_command(
    action: &LocalPsAction,
    launcher: Option<&str>,
    account: Option<&LaunchAccount>,
) -> Result<String, String> {
    let prefix = config_dir_prefix_posix(account)?;
    Ok(prefix
        + &match local_launch_choice(action, launcher)? {
            LocalLaunchChoice::Fixed(cmd) => cmd,
            LocalLaunchChoice::Probe {
                alias,
                preferred,
                fallback,
            } => {
                format!(
                    "if command -v {alias} >/dev/null 2>&1; then {preferred}; else {fallback}; fi"
                )
            }
        })
}

/// P3t-Y2：**POSIX 本机**走 CLI 渲染器那一条 —— 拿不到就带理由回来。
///
/// 与远端那条（`remote-launch-run.ts::renderLaunchCommand`）**同一个形状**：
/// 先探 ccm，再渲染，渲不出来就带 `reason` 降级。差别只有传输 ——
/// 远端探测走 ssh、渲染在 monitor 这侧；本机探测直接 `bash -lic`。
///
/// # `tmux_name` 为 `None` 时**必须**拒
///
/// 会话名不许在 Rust 里铸 —— `remote-launch.ts::mintTmuxName` 是**全仓唯一的铸造口**，
/// 撞名避让全住在那里。F13 记着这个坑的原样：另一处产 `<sid8>-cc` 却不避让，
/// 于是「精心让出 `<sid8>-cc-2`」被直接撞掉。在这里补一个铸造口 = 第三次犯同一个错。
/// ⇒ 名字由前端传下来（P3t-Y2b 接线）；没传 ⇒ 说不出容器 ⇒ 诚实降级回旧路。
#[cfg(not(windows))]
const NO_TMUX_NAME: &str = "没有 tmux 会话名（前端未传）—— 名字只许由 `mintTmuxName` 铸";

#[cfg(not(windows))]
fn render_local_ccm(
    action: &LocalPsAction,
    launcher: Option<&str>,
    account: Option<&LaunchAccount>,
    tmux_name: Option<&str>,
) -> Result<String, String> {
    // ★★ `D6 阻-1`：**说不出容器名就不必先付一次 `bash -lic` 的钱**。
    //    下面那个纯函数半在同一格上也拒（同一个 `NO_TMUX_NAME`，不是两份文案），
    //    所以这不是第二条规则，是把**已经确定的拒**提到探测之前。
    //    ⚠ 它同时是判据能驱动 [`launch_local`] 的前提：不早退的话，一条只想看
    //    「最后交出去的是哪一串」的判据会顺带在跑测试的这台机器上起一次 `bash -lic`
    //    —— 那正是本函数与 `render_local_ccm_with` 当初分家要避开的那件事。
    if tmux_name.is_none_or(str::is_empty) {
        return Err(NO_TMUX_NAME.into());
    }
    // ★ 探测与渲染**分家**（P3t-Y3）：探测是这台机器的事实，渲染是纯函数。
    // 合在一起时，判据的结论会跟着「跑测试的机器装没装 ccm」变 —— 而「本机恰好没装
    // ⇒ 判据静默 return ⇒ 报绿」与「真的测过了」在输出上完全一样，那是「0 passed 不是绿」同族。
    let probe = crate::ccm_probe::probe_local_ccm();
    let caps: std::collections::BTreeSet<String> = probe.capabilities.iter().cloned().collect();
    render_local_ccm_with(action, launcher, account, tmux_name, &caps, probe.installed)
}

/// 上一条的纯函数半 —— 能力集与「装没装」由调用方给，本函数不碰这台机器。
#[cfg(not(windows))]
fn render_local_ccm_with(
    action: &LocalPsAction,
    launcher: Option<&str>,
    account: Option<&LaunchAccount>,
    tmux_name: Option<&str>,
    caps: &std::collections::BTreeSet<String>,
    installed: bool,
) -> Result<String, String> {
    use crate::backend::control::ccm_invocation as ci;

    let Some(name) = tmux_name.filter(|n| !n.is_empty()) else {
        return Err(NO_TMUX_NAME.into());
    };
    let sanitized = sanitize_launcher(launcher)?;
    let agent = crate::adapter::active();
    let default_launcher = agent.default_launcher();
    let (sid_owned, cli_action) = match action {
        LocalPsAction::Resume(sid) => (sid.clone(), None),
        LocalPsAction::New => (String::new(), Some(ci::Action::New)),
    };
    let act = cli_action.unwrap_or(ci::Action::Resume { sid: &sid_owned });

    // ★★ 账号那格是本件真正的边界，把它写清楚（P3t-Y2 摸底）。
    //
    // 本机账号是**三态**，而 CLI 的 `account` 维度**恒真**（F05：沉默 = 意外身份切换）
    // ⇒ 每一态都得说得出话来。逐态对：
    //
    // ① `Some(Base)` —— 旧路发 `unset CLAUDE_CONFIG_DIR;`，CLI 发 `--base`。**同义**，可渲染。
    // ② `Some(Named{config_dir})` —— CLI 只会 `--account <名字>`，而 Rust 这一侧
    //    **只有 configDir、没有名字**（`LaunchAccount::Named` 就一个字段）。
    //    ⇒ 说不出 ⇒ §35 短路 ⇒ 降级回旧路（旧路发 `export CLAUDE_CONFIG_DIR='<dir>'`）。
    // ③ `None` —— 旧路发**空前缀**，语义是「继承环境里现有的 `CLAUDE_CONFIG_DIR`」。
    //    ⚠⚠ **这一态绝不能映射成 `Base`**：`--base` 是「显式不注入」，与「继承」不是一回事。
    //    映过去 = 把用户 shell 里已有的账号悄悄清掉 —— 那正是 **#75「resume 在错数据目录
    //    找不到会话」** 的病灶形状。CLI 语法里**没有「继承」这一态**，所以同样短路。
    //
    // ⇒ 今天只有 ① 渲染得出来。这不是接线没接完，是 **CLI 语法在本机账号上真的窄一格**
    //    （②可补：从 `accounts.json` 反查名字；③是结构性的）。见 ROADMAP `U10`。
    let acct =
        match account {
            Some(LaunchAccount::Base) => ci::CliAccount::Base,
            // ②：有 configDir 没名字 —— 正是 `CliAccount::Named{name:None}` 这一格存在的理由。
            Some(LaunchAccount::Named { .. }) => ci::CliAccount::Named { name: None },
            // ③：`None` 走同一条短路，但**理由不同**（不是「没名字」，是「CLI 说不出继承」）。
            None => return Err(
                "本机未表态账号（继承环境）—— CLI 的 account 维度恒真且无「继承」语法，诚实降级"
                    .into(),
            ),
        };

    let spec = ci::CliSpec {
        is_ssh: false,
        // ★★ 这里**刻意不读** `host_facts` 那个运行期全局量（P3t-Y2 订正 Y1 的形状）。
        //
        // `host_facts` 存在的理由是 `launch_wire` 那条 IPC 路住在 `backend/` 里、不许有平台 cfg
        // （`backend-split` 的 C10）—— 它只能被宿主**告知**。而本文件是宿主自己，
        // 且本函数整个挂在 `#[cfg(not(windows))]` 下 ⇒ 平台事实由**编译器**给，不是运行期给。
        //
        // 差别不是风格：全局量的缺省是 `false`，忘了接线只会**静默失效**。
        // Y2 写判据时当场撞上了这一形态 —— 单元测试进程从不跑 `lib.rs` 的启动段，
        // 于是「具名账号该按 §35 短路」被 `NotSsh` 抢先答了，判据测的根本不是它自称测的东西。
        local_posix: true,
        action: act,
        container: ci::Container::Tmux {
            name,
            send_into: false,
        },
        cwd: None,
        account: acct,
        ccm_sid: match action {
            LocalPsAction::Resume(sid) => Some(sid.as_str()),
            LocalPsAction::New => None,
        },
        model: None,
        launcher: sanitized.as_deref().unwrap_or(default_launcher),
        default_launcher,
        args: &[],
        ccm_path: "ccm",
    };
    ci::render_ccm_invocation(&spec, caps, installed).map_err(|r| r.reason())
}

/// L1：按宿主平台把「本地拉起」送出去。
///
/// 这就是 §40「一条路径，transport 是它唯一的差异」在本地这一侧的落点：
/// 上面两个渲染器共享同一个决策，这里只挑一条送法。
///
/// ★★ **P3t-Y2：POSIX 那半的顺序是硬的 —— 渲染器在前，`build_local_posix_command` 在后。**
/// 后者不再是并列的第二条路，而是「渲染器拒了才走」的回落。理由不是对齐，是它今天就坏：
/// 它产的 `cc --resume <sid>` **不带 `--tmux`** ⇒ ccm 走非容器分支 `exec`，
/// 加上 `launch_local_posix` 的 stdio 全 null ⇒ 一个**无 tty、无 tmux** 的进程，
/// 用户敲进去的字会被脚本吃掉。顺序由 `the_local_launch_tries_the_renderer_before_the_old_path` 钉住。
///
/// # ⚠ 本机 `launch` **不经 daemon**，而本机 `kill` 经〔E 阶段全局审计 08-12，待决 `U13`〕
///
/// `daemon_kill.rs::daemon_kill` 那条本机 kill 走的是 daemon 通道（P3 刀 2）；本函数**没有**。
/// 同一个控制面里两条命令走了两条路，而 `control-parity` 的 `C1` 逐字排除的正是
/// 「本地直接 `Command::new` spawn」这条今天的做法 —— 也就是**本函数下游那条**。
///
/// P3t 做的是把它**修好**（渲染器在前、进 tmux、有 tty），**不是**把它换掉。
/// 这不是漏做，也不是已裁 —— 是**没人裁过**：`launch` 与 `kill`/`send-keys` 可能本来就不同类
///（后两者对**已存在**的会话下达指令，而 launch 是**造**一个，`§1.3` 又把最终那次 exec
/// 钉在用户自己的终端进程里）。⇒ 已开 `U13`，别把这一段读成缺口后顺手「补」上。
///
/// # 🔴 返回值〔`K-P5h` `KP5HD1`〕：**这次拉起的身份 token**
///
/// 上一版回的是 `Result<(), String>`（「成了没有」）。本拍把**铸出来的那个 token**
/// 一路交回给调用方 —— 那是 `K-P5g` 现打的卡点（「写侧把 token 铸完就扔」）唯一的解，
/// 也是 [`new_local_session`] 的调用方能拿到「我刚起的那条是哪个会话」的**唯一**入口。
///
/// ⚠ **它不是 sid**：`K-P5 §3 三` 现打「5 处起会话方没有一处在起新会话时知道 sid」。
/// 拿它反查 sid 是**下一跳**的事（前端 `accounts.ts::sidOfLaunch` 与它旁边那张待回填表），
/// 而那一跳必然要**等进程真的跑起来**才问得到 —— 时序那一格归 `KP5HD3`。
///
/// ⚠ **`Err` 那一支不回 token**：拉起没成功就没有「刚起的那条」可言，
/// 回一个 token 会让调用方去等一条根本不存在的会话。
fn launch_local(
    action: &LocalPsAction,
    launcher: Option<&str>,
    cwd: Option<&str>,
    account: Option<&LaunchAccount>,
    tmux_name: Option<&str>,
) -> Result<String, String> {
    // ★★ `K-H2b`：**这一行就是「那条线」** —— 起会话这一刻把 base URL 指向本机中转。
    //    空串 = 这个号不走中转（`§0e` 裁一：官方号一个字节不进中转）。
    let relay = relay_prefix_for_launch(action, account)?;
    // Windows 那半**逐字不动**（`C12`：「windows不要tmux」）。`tmux_name` 在这一侧
    // 连读都不读 —— 读了就是给「Windows 也进容器」留了个口子。
    #[cfg(windows)]
    let base = {
        let _ = tmux_name;
        build_local_ps_command(action, launcher, account)?
    };
    #[cfg(not(windows))]
    let base = {
        // ★★ `K-H2b`（08-28 第二拍）：**照旧走 ccm 那条容器路，前缀拼在它外面。**
        //
        // # 第一拍为什么绕开它，第二拍为什么不用绕了
        //
        // 第一拍的判断是：ccm 的容器分支把载荷经 `send-keys` 送进**新起的 tmux 会话**，
        // 而 tmux server 的 `update-environment` 默认列表**不含**这个变量
        // ⇒ 在 `ccm` 外侧 export 的东西**在 tmux 边界被吃掉** ⇒ 照旧走 ccm
        // = **静默地没注入**。于是它两害相权选了「注入成功但没有容器」。
        //
        // 那个坑是真的，但**处置选窄了**：`shared/ccm` 里本来就有一段**同形的转发**
        //（R08 那条：把继承来的 `CLAUDE_CONFIG_DIR` 写进载荷**内侧**）。
        // 第二拍照它加了一条 `ANTHROPIC_BASE_URL` 的转发 ⇒ **tmux 边界那一格不再是拦路的那格**。
        //
        // ⚠ **那不违反 `§0e` 裁三**：裁三禁的是「把 ccm 当**收口点**」——
        // 三条生产路结构上绕开它，靠它**注入**会长出一个恒绿的假闸。
        // 而注入仍然发生在 `payload.rs`，ccm 只是**别把已经注入好的变量吃掉**。
        // **「不当收口点」≠「不许碰它」。**
        //
        // 🔴🔴 **订正（`D4 阻-3`）：这里先前逐字写着「于是『走中转』与『有 tmux 容器』
        // 不再互斥」—— 那是假话，今天仍然互斥，只是成因换了。**
        //
        // 成因不再是「变量在 tmux 边界被吃掉」，而是**具名账号根本进不了 ccm**：
        // 能推出中转 id 的只有 `LaunchAccount::Named`（`relay_account_id`：`Base` 与缺席
        // 一律 `None`），而 `render_local_ccm` 对 `Named` **必然** §35 短路
        //（`Named` 只有 configDir、没有名字，CLI 只会 `--account <名字>`）
        // ⇒ **带中转前缀的本机拉起，必然落到下面那条 `build_local_posix_command`，
        // 而那条路没有 tmux 容器；走 ccm 容器路的，`relay` 必然是空串。**
        //
        // ⇒ 这个事实由 `tests::a_launch_that_goes_through_the_relay_still_cannot_get_a_tmux_container`
        // **逐格钉住**（三种形状各喂一次）。消掉它要给 `LaunchAccount::Named` 补名字
        //（改 `LaunchAccount` 与它的前端调用点，都不在本件写区）——**那一天要同一拍改四处**，
        // 清单写在那条判据的头注里，别只改一处。
        //
        // ⚠ **`shared/ccm` 那条转发因此今天在本条路上生产不可达**：那段 shell 真的会转发
        //（`the_ccm_container_path_forwards_the_relay_base_url_across_the_tmux_boundary` 量的是它），
        // 但**没有任何生产输入能同时走到中转与容器** ⇒ 它是**为将来那条路预备的**。
        // 那条判据的头注里也写了这句话，两处别只改一处。
        //
        // ⚠ **这一格没买到的**：「变量真的穿过了一次**真** tmux 边界」要真机 tmux，
        // 本轮没量 ⇒ 归 e2e；而按上面那条，**今天在本机中转这条路上根本走不到**
        // —— 不只是「没量」，是「今天量不到」。
        //
        // ⚠ 写法上刻意让 `render_local_ccm(` 与 `build_local_posix_command(` 在本函数体里
        // **各恰好一处** —— `the_local_launch_tries_the_renderer_before_the_old_path`
        // 用它们的相对位置钉「渲染器在前」，两处就管不住顺序了（第一拍被它逮过一次）。
        match render_local_ccm(action, launcher, account, tmux_name) {
            Ok(rendered) => rendered,
            Err(why) => {
                // 与远端那条降级**同一种说法**：走回落是正常且预期的路径（没装 ccm 的机器
                // 每次拉起都走它）⇒ `debug` 而不是 `warn`。要查「为什么这台机没进 tmux」时，
                // 这一行是唯一线索。
                tracing::debug!("launch: 本机 CLI 渲染器降级 → 旧路：{why}");
                build_local_posix_command(action, launcher, account)?
            }
        }
    };
    // ★★ `D6 阻-1`：**全仓唯一一处**把中转前缀拼到命令前面的地方，两个平台共用。
    //    先前这里是两处（POSIX 一处 · Windows 一处），而守着「两处都拼了」的是一条
    //    数文本的判据 —— `D6` 的刀 `Y1` 把它打穿了（见 `LaunchSink` 头注）。
    //    合成一处之后，这一行在 Linux 上就被
    //    `the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched` 真驱动到。
    //
    // ★★ `K-P5b` `KP5BD3`：**身份那一句拼在中转前缀与命令体之间**，两个平台共用这一行。
    //    位置不是随手挑的：拼在中转前缀**之前**会把
    //    `the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched` 那条
    //    「送出去的那一串逐字节等于『中转前缀 + 基准串』」的相等断言改掉 ——
    //    而那条断言正是 `D6` 刀 `Y1`（算出来没拼上去）今天唯一的牙。⇒ 拼在它后面，
    //    身份那一段落在两趟的**基准串里**，那条断言逐字不动，两件事各自有各自的牙。
    //
    // 🔴🔴〔`K-P5h` `KP5HD1`〕**本拍在这两行上只做了一件事：把铸出来的 token 留下来。**
    //    上一版是 `let cmd = relay + &launch_identity_prefix(action) + &base;` ——
    //    铸法把 token 渲成前缀之后当场丢掉。现在改调 [`launch_identity`]，
    //    **拼进去的仍是同一个 `prefix`（同一份铸法、同一份渲法、同样的顺序）**，
    //    只是 token 那一半没有被扔掉，而是在拉起成功之后交回给调用方。
    //    ⇒ **拼出来的那一串一个字节没变**，这句话由
    //    `the_minted_identity_token_is_handed_back_to_the_caller` 逐字节对拍钉住。
    let identity = launch_identity(action);
    let cmd = relay + &identity.prefix + &base;
    // ★★ 送出去也走缝：判据装一个记账替身，量的是**真正交出去的那一串**，不是源码里的文本。
    (launch_sink().0)(&cmd, cwd)?;
    Ok(identity.token)
}

// ═════════════════════════════════════════════════════════════════════════════
// `K-H2b`：注入侧的三个判断（**账号 id 从哪来 · 表里有没有它 · 中转在不在**）
// ═════════════════════════════════════════════════════════════════════════════

/// 这次拉起的账号在**中转表**里的 id。
///
/// # ⚠ 它是**推出来的**，不是传下来的 —— 这一格必须写清楚
///
/// [`LaunchAccount::Named`] 只有一个字段 `config_dir`，**没有名字**（`render_local_ccm`
/// 头注里那条「②有 configDir 没名字 ⇒ 说不出 ⇒ 降级」记的就是这件事）。
/// 而中转表按**账号 id** 索引 ⇒ 这里只能拿 `config_dir` 的**末段目录名**当 id：
/// `cc-acct-iso` 的布局逐字是 `~/.claude-accts/<名字>`，`local_accounts.rs` 读出来的
/// 账号名**就是那个目录名**。
///
/// **推错了会怎样**：推出一个表里没有的 id ⇒ [`relay_prefix_for`] 回 `None` ⇒
/// **逐字节走旧路**，不是拼一条会 404 的 URL。⇒ 这一格的失效方向是**保守**的。
///
/// - [`LaunchAccount::Base`]（账号 0）⇒ `None`。**说不出 id 就不注入** ——
///   账号 0 是「显式不注入 `CLAUDE_CONFIG_DIR`」那一档，它在 manifest 里没有目录名。
/// - 参数缺席（调用方没表态）⇒ `None`，同上。
fn relay_account_id(account: Option<&LaunchAccount>) -> Option<String> {
    match account {
        Some(LaunchAccount::Named { config_dir }) => relay_account_id_of_dir(config_dir),
        _ => None,
    }
}

/// 上一条的**纯派生半** —— 「一个 configDir 对应中转表里哪个 id」。
///
/// ★ 抽出来的理由是**只许有一份**：界面那一侧（徽章要显「这个号走不走中转」）问的是
/// **同一个问题**，而它手上也只有 configDir。两边各写一个 basename 规则，
/// 漂开的那天症状是「设置里说走中转、起会话时没走」，而两边看起来都没错。
///
/// ⚠⚠ **界面那一侧今天还没有人调它** —— 那条把这个事实端给前端的路（一条只答本机的
/// tauri 命令）**本轮做到一半退回了**：新注册一条命令会让 `parity_ledger.rs` 的
/// `every_tauri_command_is_declared_in_the_ledger` 当场红（实测报文逐字：
/// 「这些命令已注册但**没进平价对账表**：["relay_routing_for"]」），
/// 而那个文件**不在 `K-H2b` 的写区**。⇒ 本函数今天只有起会话那一侧一个调用方；
/// 它被抽出来是为了「接的时候只有一份规则」，**不是**已经接上了。经过住件文件 `§4`。
pub(crate) fn relay_account_id_of_dir(config_dir: &str) -> Option<String> {
    std::path::Path::new(config_dir.trim())
        .file_name()
        .and_then(|s| s.to_str())
        .map(str::to_string)
}

/// `KH2B7` 的**纯派生半**：给一批 configDir 与一张 id 表，答「哪几个走中转」。
///
/// ★ 抽成纯函数的理由与本模块另外两次一样：`relay_rows()` 要读盘、`relay_running()` 要读进程状态，
/// 而**这条规则本身**（怎么从 configDir 推 id、怎么和表比）不该只能对着真实的家目录跑。
///
/// ⚠ **它答的是「表里有没有这一行」，不是「这个 key 能不能用」** —— 后者要到 claude 那边才知道。
/// ⚠ 也不是「这次拉起会不会真的注入」：那还要过 `relay_running` 那一格（`relay_injection_for`）。
pub(crate) fn relay_routed_subset(config_dirs: &[String], rows: &[String]) -> Vec<String> {
    config_dirs
        .iter()
        .filter(|d| relay_account_id_of_dir(d).is_some_and(|id| rows.iter().any(|r| *r == id)))
        .cloned()
        .collect()
}

/// 中转凭据文件里今天有哪几条账号 id。**读不到就是零条**（零条 ⇒ 谁都不走中转）。
///
/// ⚠ 「读不到」与「一条都没配」在这里**故意同一处置**：两者的正确行为都是
/// 「照旧走官方直连」，而把「读文件失败」变成一次起会话失败，是拿一个**能用的**状态
/// 去换一条错误提示。⇒ 只在日志里留一行。
pub(crate) fn relay_rows() -> Vec<String> {
    let Some(p) = crate::creds_store::resolve_path() else {
        return Vec::new();
    };
    relay_rows_at(&p)
}

/// 上一条剥掉「路径从哪来」之后的那一半〔`D1 阻-6`〕。
///
/// ★ 抽出来的理由与 `creds_store::read_status_at` 那次逐字同一条：不抽的话，这段逻辑
/// **只能对着真实家目录下那份文件跑** —— 而判据不许碰用户的真东西，于是它会变成一格
/// **永远没人量过**的代码。`D1` 的刀 C 实测过那个后果：把本函数整个换成 `Vec::new()`，
/// **1221 passed / 0 failed**。
///
/// # ⚠ 它与中转那侧的人群**不完全一致**，差在哪要写清楚
///
/// 中转装表时会把两类行**丢出表**（`relay::table::build`）：① 账号 id 当不了路由段；
/// ② `base_url` 解析不了。本函数**只筛得掉第 ①** 类（`payload::relay_segment_is_safe`
/// 与 `route::segment_is_safe` 是同一条规则，由 `payload.rs` 那边的头注登记着）。
/// **第 ② 类筛不掉** —— 那要一份 `Base::parse`，而它住 daemon 那一侧、monitor 够不着
/// （单向依赖）。
/// ⇒ **残留的症状**：一行 `base_url` 打错的账号，界面会说「经本机中转」而中转那侧 404。
/// **如实登记，不假装两侧人群相等。**〔`D1` 点名的那条同族，处置是「筛掉能筛的、写清剩下的」。〕
pub(crate) fn relay_rows_at(path: &std::path::Path) -> Vec<String> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    match creds_core::store::parse(&raw) {
        Ok(doc) => creds_core::store::read_accounts(&doc)
            .into_iter()
            .map(|e| e.id)
            .filter(|id| crate::backend::control::payload::relay_segment_is_safe(id))
            .collect(),
        Err(e) => {
            tracing::debug!("中转凭据文件读不成表（照旧走官方直连）：{e:?}");
            Vec::new()
        }
    }
}

/// 纯函数半：给定「账号 id / 表里有哪几行 / 中转在不在」，产出要拼上去的前缀。
///
/// 空串 = **不走中转**（逐字节旧路）。`Err` = 该走但走不了（`KH2B2`②，出声不静默）。
fn relay_prefix_for(
    account_id: Option<&str>,
    rows: &[String],
    running: bool,
    sid: Option<&str>,
    windows: bool,
) -> Result<String, String> {
    let agent = crate::adapter::active().id();
    let url = crate::backend::control::payload::relay_injection_for(
        account_id, rows, running, sid, agent,
    )?;
    Ok(match url {
        None => String::new(),
        Some(u) if windows => crate::backend::control::payload::relay_env_prefix_ps(&u),
        Some(u) => crate::backend::control::payload::relay_env_prefix_posix(&u),
    })
}

/// `D5 阻-1`：那两个「本机事实」的**取值口**，收成一条判据能替换的缝。
///
/// # 为什么非有这条缝不可（这是本件病史的第五层，别退回去）
///
/// 先前钉这两个入参的是一条**扫描型**判据：把 [`relay_prefix_for_launch`] 的体切出
/// 700 字节，断言那个窗口里**有没有**那两段文本。`D5` 现打的读数：在同一个窗口里加一行
/// 把两段文本原样留住的死赋值（一个用不到的绑定就够），同时把真入参换成空表 / 常量
/// ⇒ 文本一处不少、锚点命中数一处不少、**全量门禁四个数与干净树逐字相同**，
/// 而「这个号在不在中转表里」「中转在不在跑」两件事**都不再被问**、中转前缀恒空。
///
/// 病史五层，每一层都是**上一层的修法买到的东西被下一层的量法漏掉**：
/// ① 参数位没有账号 → ② 参数位有、值恒空 → ③ 值到得了、判据只量文本 →
/// ④ 判据搬了家（不再把自己算进被测对象）、**仍在量文本** → ⑤ **文本留住、行为摘掉**。
///
/// ⇒ 处置**不是**再写一个更聪明的文本判据（那是第六层），是**不再量文本**：
/// 两个事实一律从本结构取，判据换一份**会记账的替身**进来，断言两件事 ——
/// ㈠ 它**真的被问过**（替身的计数器涨了）；
/// ㈡ 算出来的前缀**真的随替身给的答案变**（表里有这一行 ⇒ 非空；没有 ⇒ 空；中转没跑 ⇒ `Err`）。
/// ★ 第 ㈡ 条正是治第五层的那一格：把答案问完扔掉（`_unused` 那一形），
///   计数器照样涨，而前缀不再随答案变 ⇒ **红**。
///
/// # 🔴🔴 谁在用这条缝 —— **由一道人群闸数着，不是由这段头注数着**〔`D6 阻-4`，08-29〕
///
/// 先前这里逐字写着「这两个取值口的**生产消费方恰好 2**」，并把那个 2 当成了闸。
/// `D6` 的刀 `E5` 打穿它：在 `lib.rs` 加**第三个**消费方、**绕开这条缝**直接调
/// `history::relay_rows()` / `local_daemon::relay_running()` ⇒ **全量门禁四个数与干净树逐字相同**。
/// ⇒ 那句头注买到的是「**这两处**走缝」，**没买到「所有人都得走缝」**。
/// ★ 定性（PM `§8 裁四`）：**治一个「今天数出来的 N」的过程中，长出了一个新的「今天数出来的 N」。**
///
/// **今天数着这件事的是一道闸**，住 `backend/control/payload.rs::
/// `nobody_reaches_the_relay_take_points_without_going_through_the_seam`（**目录扫描**
/// `src-tauri/src`，不是手写名单）。它钉的是**零调用点**：
/// - `relay_rows()` / `relay_running()` 的**调用形**在生产段全树**各恰好 1 处**（就是它们自己的定义行）；
/// - 裸标识符 `relay_rows` / `relay_running` / `platform_is_windows` 各恰好 **2** 处（定义 + 本结构这一处）。
///
/// ⇒ 谁绕开这条缝直接调那三个取值口、或把它们的函数指针复制到第二个地方，**当场红**。
/// 今天的两个生产消费方（起会话侧 [`relay_prefix_for_launch`] · 界面侧 `crate::relay_routing_for`）
/// 各有一条行为判据；**闸不数它们有几个**，闸数的是「有没有人绕过去」。
///
/// # 它买不到什么（如实写，别读宽）
///
/// 本结构只管「**问不问**」与「**答案用不用**」。「那三个取值口自己答得对不对」由它们各自的
/// 判据买（[`relay_rows_at`] 那条读真文件的 · `local_daemon::relay_running_really_reads_the_handle_table`）。
/// 而「生产上这条缝里插的**就是**那三个取值口」由 `the_production_relay_facts_are_those_two_take_points`
/// 按**函数地址**对拍 —— 不是按文本。
///
/// ⚠ **仍然没有判据的那两格**（`D6 阻-4` / PM `§8 裁六` 订正过这两栏，别再照旧读）：
/// ㈠ [`relay_rows`] 自己那三行胶水（`creds_store::resolve_path()` + [`relay_rows_at`]）。
///    `D6` 的刀 `Xa` 把它掏空成 `Vec::new()` ⇒ **全绿、门禁四个数与干净树逐字相同**。
///    🔴 **先前这里写的理由（「要动真实家目录，红线不许 ⇒ 做不到」）是假的，解锁条件（「要动 `paths.rs`」）也是假的**：
///    `paths.rs` 从 `dirs::home_dir()` 拼路径 ⇒ 在 Linux 上它读的就是 `$HOME`，
///    而**本 crate 今天就有这个手法的先例**（`local_daemon::become_host_with_home` 里那行
///    `std::env::set_var("HOME", …)`）⇒ **写得出来，一个字节都不用动 `paths.rs`**。
///    **真代价**是这种判据必须 `--test-threads=1` ⇒ 只能住 `#[ignore]` 的 e2e 那条道
///    ⇒ **进不了 `scripts/gate.sh`**。重新裁定的落点就是这一栏 + 件文件 `§4`。
///    🔴 **裁定（`D8 §4` 第 1 条，PM 08-29 采纳，第九轮照抄进这一栏）：
///    这一格是「买得到」，不是「做不到」。** 买法**不在判据这一侧** ——
///    是给 `scripts/gate.sh` 加一条**单线程道**，把 `#[ignore]` 那一族纳进第五个数。
///    🔴 `scripts/gate.sh` **不在 `K-H2b` 的写区** ⇒ 第九轮**没做**，抬给 PM（上报口有一条）。
///    ⚠ 别再把这一栏读成「做不到」：那正是 `D8 §10 裁一` 判过两次的那一形。
/// ㈡ [`platform_is_windows`] 自己的体（`cfg!(windows)`）。在 Linux 上把它写死成 `false`
///    是一次**恒等变换** ⇒ **任何运行时判据都分不出来**（它只在 Windows 上有区别，而
///    Windows 运行时行为本件本来就在「判不了」里）。**登记，不假装钉住了。**
///    ⚠ **过一遍 PM 08-29 那道闸**（「标平台判不了要给得出 `cfg`」）：**本函数没有 `cfg`，
///    在 Linux 上真编译**（`cargo test -p monitor --lib` 跑得到它）⇒ 它**不**属于
///    「不进编译单元」那一族。它判不了的理由是**另一条**：在 Linux 上 `false` 是它的真值，
///    换上去是**恒等变换**（`D8 §1` 的排除表逐字：恒等变换不算「剥掉」那张脸）。
///    ⇒ 两条理由别混：一条是**构造上看不见**，一条是**看得见但换不出第二张脸**。
///    ⚠ 它与先前那条被删的文本判据的差别在于：**调用点**那一格今天买回来了 ——
///    调用点走 `(facts.windows)()`，谁在那里写死一个常量，
///    `the_launch_side_really_asks_those_two_take_points_and_uses_their_answers` 的
///    「PowerShell 那一格」当场红（`D6` 的刀 `Xb` 打的正是调用点那一格）。
#[derive(Clone, Copy)]
pub(crate) struct RelayFactSources {
    /// 「这个号在不在中转表里」——生产恒指 [`relay_rows`]。
    pub(crate) rows: fn() -> Vec<String>,
    /// 「中转在不在跑」——生产恒指 [`crate::local_daemon::relay_running`]。
    pub(crate) running: fn() -> bool,
    /// 🔴 「这台机是不是 Windows」——生产恒指 [`platform_is_windows`]〔`D6 阻-3`，08-29〕。
    ///
    /// 先前这一格在调用点上逐字写着 `cfg!(windows)`，而**它是一个常量表达式** ——
    /// 唯一守着它的是那条被删掉的文本判据（反空真①「窗口里有 `cfg!(windows)`」）。
    /// `D6` 的刀 `Xb`（`cfg!(windows)` → `false`）⇒ 全绿，而生产后果是
    /// **Windows 上中转前缀渲染成 POSIX 形态**（`export …` 塞进 PowerShell 串）⇒ 注入整个失效。
    /// ⇒ 收进本结构之后它成了**可翻的一维**：判据喂 `|| true` 就该拿到 PowerShell 形态。
    pub(crate) windows: fn() -> bool,
}

/// 「这台机是不是 Windows」的生产取值口。**只有这一处**说得出这句话。
///
/// ⚠ 抽成函数不是为了好看：`cfg!(windows)` 写在调用点上时它是个**常量表达式**，
/// 判据没有任何办法让它变。抽出来 + 进 [`RelayFactSources`] 之后，
/// 「调用点用没用这个答案」变成了可翻的一维（见本结构 `windows` 那一格的头注）。
pub(crate) fn platform_is_windows() -> bool {
    cfg!(windows)
}

/// 生产上这条缝里插的那三个取值口。**只有这一处**，判据按地址对拍它。
pub(crate) const PRODUCTION_RELAY_FACTS: RelayFactSources = RelayFactSources {
    rows: relay_rows,
    running: crate::local_daemon::relay_running,
    windows: platform_is_windows,
};

#[cfg(test)]
thread_local! {
    /// 判据装进来的替身。**线程局部** ⇒ 同进程别的判据不受影响（`cargo test` 是多线程跑的）。
    static RELAY_FACTS_OVERRIDE: std::cell::Cell<Option<RelayFactSources>> =
        const { std::cell::Cell::new(None) };
}

/// 装替身，离开作用域自动还原（`assert!` 炸了也还原）。
#[cfg(test)]
pub(crate) struct RelayFactsGuard(Option<RelayFactSources>);

#[cfg(test)]
impl Drop for RelayFactsGuard {
    fn drop(&mut self) {
        RELAY_FACTS_OVERRIDE.with(|c| c.set(self.0));
    }
}

#[cfg(test)]
pub(crate) fn override_relay_facts(facts: RelayFactSources) -> RelayFactsGuard {
    RelayFactsGuard(RELAY_FACTS_OVERRIDE.with(|c| c.replace(Some(facts))))
}

/// 这一拍要用的两个取值口。生产上恒是 [`PRODUCTION_RELAY_FACTS`]。
pub(crate) fn relay_facts() -> RelayFactSources {
    #[cfg(test)]
    if let Some(f) = RELAY_FACTS_OVERRIDE.with(|c| c.get()) {
        return f;
    }
    PRODUCTION_RELAY_FACTS
}

/// 上一条的**接线半**：这台机器上的两个事实（表里有哪几行 · 中转在不在）在这里读。
///
/// ⚠ 两个事实**只从 [`relay_facts`] 取**（理由见 [`RelayFactSources`] 头注：
/// 直接在这里调那两个函数的写法，只能靠「文本在不在」来钉，而那一形 `D5` 已经打穿了）。
fn relay_prefix_for_launch(
    action: &LocalPsAction,
    account: Option<&LaunchAccount>,
) -> Result<String, String> {
    let id = relay_account_id(account);
    let sid = match action {
        LocalPsAction::Resume(sid) => Some(sid.as_str()),
        LocalPsAction::New => None,
    };
    let facts = relay_facts();
    relay_prefix_for(
        id.as_deref(),
        &(facts.rows)(),
        (facts.running)(),
        sid,
        (facts.windows)(),
    )
}

// ═════════════════════════════════════════════════════════════════════════════
// `K-P5b`：**起会话方把这条会话的身份塞进下一跳进程的环境**（`L1` 这一处）
// ═════════════════════════════════════════════════════════════════════════════

/// 身份落在进程环境里的那个变量名。**全树只有这一处写下这个字面串** ——
/// `launcher_identity_registry` 那张棘轮表数着它（多一处 ⇒ 红）。
///
/// # 它是什么、不是什么
///
/// 它是**起会话方现铸的一个 token**，不是 sid。`K-P5 §3 三` 现打过一条横贯 5 个起会话方的
/// 结构性事实：**没有一处在起「新」会话时知道 sid**（sid 是 claude 自己起来之后才写进 pidfile 的）
/// ⇒ 身份 token 只能是起会话方现铸的 nonce，resume 那一支可以拿 sid 当那个 nonce。
///
/// ⚠ **不许把「有没有 tmux」或「有没有窗口标题」当它能不能落的判据**（`KP5BD1` 逐字）——
/// 本变量与那两样东西**一格关系都没有**：它是一句 `export`，在哪个终端里、有没有 tmux、
/// 窗口标题写了什么，都不改变它落不落。
pub(crate) const LAUNCH_ID_VAR: &str = "CCM_LAUNCH_ID";

/// 这一次拉起的身份 token。**铸法只有一份** ——
/// 直接调 [`crate::backend::control::payload::route_key_for_session`]，本文件不另写一条规则。
///
/// # 为什么是「共用那一份」而不是「两侧各写一份再对拍」〔`KP5BD1`，照 `K-H2c` 买到的形状〕
///
/// 那一件的读数逐字是「漂开这件事在**结构上不可表示**」。两侧各写一份、再用判据焊住，
/// 买到的只是「今天这几条输入两侧同答」；共用一份实现，**漂开根本没有位置可以发生**。
/// ⇒ 这里刻意**不**写 `match action { Resume(sid) => sid.clone(), New => Uuid::new_v4() }`
///    这种「看起来一样」的第二份 —— 它与那一份的差别只在**白名单回落**那一格
///    （sid 过不了 `relay_segment_is_safe` 时那一份回落到 nonce），而那一格恰恰是
///    「本条真的调了那一份铸法吗」唯一能被判据翻出来的一维。
///
/// # ⚠ 它欠的一笔账（如实登记，别读成缺陷也别读成没有）
///
/// **新开**会话时，中转路由键与本 token 是**两个不同的 nonce**（同一份铸法被调了两次）——
/// 中转那一次在 `payload::relay_injection_for` 里面，本文件够不着它算好的值。
/// 今天不构成缺陷：`mint_route_key` 头注现打登记过「route key 对路由完全惰性、tee 今天零消费者」，
/// 而身份 token 与它**不共享任何消费者**。要它们相等得改 `payload.rs`（本拍只许读它）。
fn launch_identity_token(action: &LocalPsAction) -> String {
    let sid = match action {
        LocalPsAction::Resume(sid) => Some(sid.as_str()),
        LocalPsAction::New => None,
    };
    crate::backend::control::payload::route_key_for_session(sid)
}

/// 把 token 渲成「设进下一跳进程环境」的那一句前缀。**纯函数**（平台由调用方给）。
///
/// 形状照 `payload::relay_env_prefix_posix` / `relay_env_prefix_ps` 那一对 ——
/// 两个平台的语法真的不同，这不是「两份实现」，是同一件事的两种**书写法**；
/// 决定用哪一种的那一格只有一处（下面 [`launch_identity`] 里那个 `windows`）。
fn launch_identity_env_prefix(token: &str, windows: bool) -> String {
    if windows {
        format!("$env:{LAUNCH_ID_VAR}='{token}'; ")
    } else {
        format!(
            "export {LAUNCH_ID_VAR}={}; ",
            shell_quote_core::posix_quote(token)
        )
    }
}

/// 一次拉起的身份：**铸出来的那个 token** 与**要拼进命令串的那一句前缀**。
///
/// # 🔴 它为什么存在〔`K-P5h` `KP5HD1`〕
///
/// 这个结构是**本拍唯一的行为增量**，而增量只有一句话：**把铸出来的 token 交给调用方**。
///
/// `K-P5g` 交回时现打过一条卡点，逐字：「用 token 回填新会话的 sid 是这条路上最值钱的
/// 那个消费者，它今天**买不到**，卡点是**写侧把 token 铸完就扔**」——
/// 上一版的 `launch_identity_prefix`（本结构的前身，本拍已改名为 [`launch_identity`]）签名是
/// `fn(&LocalPsAction) -> String`，回的是**拼好的前缀**，token 在函数体里当场丢掉
/// ⇒ 全仓**没有任何调用方手上有那个 token**，而 `K-P5 §3 三` 现打的
/// 「5 处起会话方没有一处在起新会话时知道 sid」**正是这个 token 存在的全部理由**。
///
/// # 🔴 additive 的判据钉在哪（别读成「加个字段而已」）
///
/// **拼出来的命令串必须一个字节没变** —— 那是 additive 的全部含义。
/// 钉住它的是 [`tests::the_minted_identity_token_is_handed_back_to_the_caller`] 那一格：
/// 它拿**真正交出去的那一串**与 `前缀 + 基准串` 逐字节相等对拍
///（两边都由生产函数现算，不抄第二份规则）。
/// ⚠ 另有两条老判据在旁边守着同一件事，本拍一个字节都没动它们：
/// `the_launcher_plants_the_session_identity_into_the_process_environment`（身份那一段的形状）
/// 与 `the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched`（中转前缀那一段）。
struct LaunchIdentity {
    /// 铸出来的那个 token 本身。**交给调用方的就是它。**
    token: String,
    /// 渲好的那一句 `export …; ` / `$env:…; `，原样拼进命令串。
    prefix: String,
}

/// 上面两条的**接线半**：铸一个 token，按这台机器是不是 Windows 渲成一句前缀。
///
/// ⚠ 平台那一格**走 [`relay_facts`] 那条缝取**，不写 `cfg!(windows)`：
/// `D6` 的刀 `Xb` 现打过，写在调用点上的 `cfg!(windows)` 是个**常量表达式**，
/// 判据没有任何办法让它变 ⇒ 「Windows 上渲成 POSIX 形态」这一形全绿。
/// 走缝之后它成了可翻的一维（判据喂 `|| true` 就该拿到 PowerShell 形态）。
/// ⚠ 这里**刻意不提 `platform_is_windows` 这个裸标识符** —— `payload.rs` 那道人群闸
/// 数的正是它在生产段里出现几处（定义 1 + 缝里 1），提一次就多一处。
///
/// 🔴〔`K-P5h` `KP5HD1`〕**本拍只改了返回什么，没改铸什么、也没改怎么拼**：
/// 铸法仍是 [`launch_identity_token`]（那一份共用的 `route_key_for_session`），
/// 渲法仍是 [`launch_identity_env_prefix`]，两者的入参与顺序逐字未动 ⇒
/// `prefix` 这一半与上一版那个 `-> String` 的返回值**逐字节相同**。
fn launch_identity(action: &LocalPsAction) -> LaunchIdentity {
    let token = launch_identity_token(action);
    let prefix = launch_identity_env_prefix(&token, (relay_facts().windows)());
    LaunchIdentity { token, prefix }
}

// ═════════════════════════════════════════════════════════════════════════════
// `D6 阻-1`：**最后送出去的那一串**收成一条缝
// ═════════════════════════════════════════════════════════════════════════════

/// 「本机拉起最后把哪一串交出去」的取值口 —— 收成一条判据能替换的缝〔`D6 阻-1`，08-29〕。
///
/// # 为什么非有这条缝不可（这是本件病史的第七层，别退回去）
///
/// 先前钉「前缀真的拼上去了」的是一条**扫描型**判据
/// （`the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched` 的第一版）：
/// 从 `fn launch_local(` 起切 3600 字节，断言那个窗口里**有没有**
/// `relay_prefix_for_launch(action, account)?` · `relay + &`（恰好 2 处）· `let cmd = relay + &base;`。
/// `D6` 的刀 `Y1` 现打：在拼装那一行加
/// `let relay = if relay.is_empty() { relay } else { String::new() };`
/// ⇒ 三样文本**一处不少**（三个锚点数与干净树相同）⇒ **全量门禁四个数与干净树逐字相同**，
/// 而**前缀算出来了没拼上去** —— 本件的正题在生产上被整个摘掉。
/// ⚠ **那条判据自己的头注逐字写着要防的正是这件事**（「算出来却没拼上去，行为上与本件没做完全一样」）
/// —— 威胁模型写对了，买的东西是文本。
///
/// ⇒ 处置**不是**再写一个更聪明的文本判据（那是下一层），是**不量文本**：
/// 把「送出去」收成本结构这一跳，判据换一个**会记账的替身**进来，断言
/// **真正交出去的那一串**以正确的前缀打头、且前缀随 [`RelayFactSources`] 给的答案与
/// **哪个账号**一起变。
///
/// # 顺带被这条缝按平了的一格
///
/// 收缝的同一拍把 [`launch_local`] 里那**两处**拼接（POSIX 一处 · Windows 一处）
/// 合并成了**一处** —— 平台差异现在只剩「`base` 由谁渲」与「送法是哪一个」两格，
/// 而拼前缀那一步两个平台**共用同一行**。
/// ⇒ 先前那条判据的第 ② 颗牙（「两条平台分支各自真的拼上去」）不再需要一条
/// **只能在 Windows 上验证**的断言来守 —— 那一行在 Linux 上就被驱动到了。
#[derive(Clone, Copy)]
pub(crate) struct LaunchSink(pub(crate) fn(&str, Option<&str>) -> Result<(), String>);

/// 生产上这条缝里插的送法。**只有这一处**，判据按地址对拍它。
#[cfg(not(windows))]
pub(crate) const PRODUCTION_LAUNCH_SINK: LaunchSink = LaunchSink(crate::launch::launch_local_posix);
/// 生产上这条缝里插的送法。**只有这一处**，判据按地址对拍它。
///
/// # 🔴 **订正 `D8` 表里的 `F3`：这一格有判据，不是「零感知」**〔`D8 阻-6` / 阻-5，08-29〕
///
/// `D8` 把这一支标成「❌ 没有（**推的，我没打这一刀**）」，理由是「与 `F1` 同属
/// `#[cfg(windows)]`，Linux 上不进编译单元」。**第九轮把这一刀打了，读数与那个推断相反。**
///
/// - **刀**（`§11.6` 形㈠ 的 Windows 版）：加一个 `#[cfg(windows)]` 的新一跳
///   `fn r9_probe_sink(cmd, cwd)`，体里先 `split_once("; ")` 剥掉中转前缀再委托给
///   `launch_powershell_window`，把本 `const` 指向它。锚点 = 本 `const` 的定义，**命中 1**。
/// - **读数**：`cargo test -p monitor --lib` ⇒ **`1236 passed; 1 failed`**，红的正是
///   `payload::nobody_reaches_the_relay_take_points_without_going_through_the_seam`，
///   报文逐字点名「`launch.rs` 之外还有人直接调那两个送法：history.rs: `launch_powershell_window(` × 1」。
///   （快道红 ⇒ 方向安全，按纪律不升全量门。）
///
/// **成因**：那道人群闸是**量文本**的（`guard_core::production_code` + 目录扫描），
/// 而 `production_code` **只剥 `#[cfg(test)]` 段与整行注释，不剥 `#[cfg(windows)]`**
/// ⇒ Windows-only 的源码**在文本这一层是可见的**。
/// ⇒ 🔴 **「带 `#[cfg(windows)]`」蕴含「运行时判据看不见」，不蕴含「所有判据都看不见」。**
/// `F1`（[`crate::launch::launch_powershell_window`] 的**函数体**）仍然买不到 ——
/// 那一刀不新增任何跨文件调用形，量文本的闸够不着它。**两格别合并读。**
#[cfg(windows)]
pub(crate) const PRODUCTION_LAUNCH_SINK: LaunchSink =
    LaunchSink(crate::launch::launch_powershell_window);

#[cfg(test)]
thread_local! {
    /// 判据装进来的替身。**线程局部** ⇒ 同进程别的判据不受影响（`cargo test` 是多线程跑的）。
    static LAUNCH_SINK_OVERRIDE: std::cell::Cell<Option<LaunchSink>> =
        const { std::cell::Cell::new(None) };
}

/// 装替身，离开作用域自动还原（`assert!` 炸了也还原）。
#[cfg(test)]
pub(crate) struct LaunchSinkGuard(Option<LaunchSink>);

#[cfg(test)]
impl Drop for LaunchSinkGuard {
    fn drop(&mut self) {
        LAUNCH_SINK_OVERRIDE.with(|c| c.set(self.0));
    }
}

#[cfg(test)]
pub(crate) fn override_launch_sink(sink: LaunchSink) -> LaunchSinkGuard {
    LaunchSinkGuard(LAUNCH_SINK_OVERRIDE.with(|c| c.replace(Some(sink))))
}

/// 这一拍要用的送法。生产上恒是 [`PRODUCTION_LAUNCH_SINK`]。
pub(crate) fn launch_sink() -> LaunchSink {
    #[cfg(test)]
    if let Some(s) = LAUNCH_SINK_OVERRIDE.with(|c| c.get()) {
        return s;
    }
    PRODUCTION_LAUNCH_SINK
}

/// 薄委托——保留旧函数名与调用点不变（`resume_impl` 只改内部实现，DoD 要求两个
/// `#[tauri::command]` 的签名/行为/错误文案逐字节不变）。
#[cfg(any(windows, test))]
fn build_resume_ps_command(session_id: &str, launcher: Option<&str>) -> Result<String, String> {
    // G3b：本薄委托保持 2 参签名不变（既有测试逐字节钉住它）——账号 0 走这条。
    // 要带账号的调用方直接用 `build_local_ps_command`。
    build_local_ps_command(
        &LocalPsAction::Resume(session_id.to_string()),
        launcher,
        None,
    )
}

/// Batch14-F41：wt.exe/PowerShell 拉起机械抽到 `launch.rs::launch_powershell_window`
/// （与远端 resume/attach 族共用），本函数只剩「构造本地 resume 命令体 + 委托拉起」。
/// 非 Windows：launch 层统一报错（仅 Windows 支持，错误文案改为中文）。
fn resume_impl(
    session_id: &str,
    cwd: &str,
    launcher: Option<&str>,
    account: Option<&LaunchAccount>,
    tmux_name: Option<&str>,
) -> Result<(), String> {
    // 🔴〔`K-P5h`〕**resume 这一支刻意把 token 丢掉，那不是疏忽。**
    // `K-P5g` 现打过：resume 时 token **就是 sid**（`route_key_for_session(Some(sid))` 在 sid
    // 过白名单时原样返回）⇒ 「拿 token 反查 sid」在这一支上退化成
    // 「答案要么是它自己、要么 `None`」，一个布尔谓词，**买不到本件的正题**。
    // 本件的正主是**新开**那一支（见 [`new_local_session`]）—— 那一支才没有 sid。
    launch_local(
        &LocalPsAction::Resume(session_id.to_string()),
        launcher,
        Some(cwd),
        account,
        tmux_name,
    )?;
    tracing::info!("history: resumed sid={session_id}");
    Ok(())
}

/// F96（#62）：本地「在该目录起**新**会话」的 PowerShell 命令体——薄委托（同上，DoD 要求
/// 行为逐字节不变）。硬约束（用户 2026-07-15）：agent 名 / resume flag 全走活跃适配器，
/// 本函数不出现 agent 字面量。
#[cfg(any(windows, test))]
fn build_new_session_ps_command(launcher: Option<&str>) -> Result<String, String> {
    build_local_ps_command(&LocalPsAction::New, launcher, None)
}

/// F96（#62）：历史页右键「在该目录起新会话」——本地分支。远端分支走前端
/// `runRemoteLauncher`（复用 F53）。在 `cwd` 起一个全新会话（无 sid、无 resume）。
///
/// # 🔴 返回值〔`K-P5h` `KP5HD1`〕：**这次拉起的身份 token**
///
/// 上一版回 `Result<(), String>`。本件把 [`launch_local`] 交出来的那个 token 原样回给前端 ——
/// **这条命令是全仓唯一「起一条新会话」的 tauri 入口**，也就是唯一一处
/// 「起会话方手上有 token、而这条会话还没有 sid」的地方。
///
/// ⚠ **token 不是 sid，也不许被当成 sid 用**。前端拿它去做的事只有一件：
/// 在这条会话真的跑起来之后，用 `accounts.ts::sidOfLaunch` 从 `--session-accounts`
/// 的行里把 sid **反查**出来（`KP5HD2`）。
/// ⚠ **它是个内部 nonce**：不许显示给用户（同 `K-P5g` 那条判据的口径）。
#[tauri::command]
pub fn new_local_session(
    cwd: String,
    launcher: Option<String>,
    account: Option<LaunchAccount>,
) -> Result<String, String> {
    // F96：起新会话**依赖 cwd 定位**（不像 resume 靠 sid）——cwd 非空且不是现存目录（项目被
    // 移动/删除）就明确报错，别静默在默认目录起会话 + 弹假成功 toast。`launch_powershell_window`
    // 只把存在的 cwd 作窗口起始目录、失效则回落默认，对 resume 无害、对 new-session 是错目录。
    if !cwd.is_empty() && !std::path::Path::new(&cwd).is_dir() {
        return Err(format!("目录不存在，无法在此起新会话：{cwd}"));
    }
    // ★★ `K-H2b` `D1 阻-1`：**账号这一格是本轮加的，加它的理由要写清楚。**
    //
    // 原注释逐字：「起**全新**会话不继承任何账号（那是『新开一个』的语义，不是分叉）」。
    // 那句话**今天仍然对**，它说的是「不从某条旧会话继承」。⚠ 但它被读成了「所以这条路
    // 不该有账号参数」，而后果是：**这条主路上一个账号都说不出**，于是
    // ① 起会话落到 shell rc 里那个默认号上（`config_dir_prefix_posix` 头注逐字点名的静默串号），
    // ② 中转那一格**永远拼不出路由键**（没有账号 id ⇒ `relay_account_id` 回 `None`）。
    // ⇒ 现在收**调用方明说的那一个**：前端传的是「用户此刻选中的当前账号」，
    //   **不是**从别的会话继承来的。参数缺席仍然是「没表态」，逐字节旧行为。
    //
    // P3t-Y2：起新会话这条**暂不传名字**（`None` ⇒ 渲染器诚实降级回旧路）。
    // 名字只许由 `mintTmuxName` 铸，在这里补一个默认名就是 F13 那个坑的第三次。
    let launch_id = launch_local(
        &LocalPsAction::New,
        launcher.as_deref(),
        Some(&cwd),
        account.as_ref(),
        None,
    )?;
    // ⚠ **日志里不写 token**：它是身份凭据形态的 nonce，而 tracing 的 ERROR 那一档会被
    //   `bindErrorToast` 刷到界面上 —— 内部 nonce 一个字节都不该往那条路上走。
    tracing::info!("history: new local session in {cwd}");
    Ok(launch_id)
}

// === 内部：项目级 / jsonl 级扫描 ===

/// 项目级元数据 —— 只扫文件 stat + 读单一 jsonl 的第 1 行 cwd，**不读消息内容**。
/// 用于初次打开历史浏览器（"只读几条就好"）。
fn analyze_project_dir(
    dir: &Path,
    metadata: &HistoryMetadata,
    map: &SessionMap,
) -> Option<HistoryProject> {
    let project_dir = dir.to_string_lossy().into_owned();

    let entries = std::fs::read_dir(dir).ok()?;
    let mut jsonls: Vec<(PathBuf, String, i64)> = Vec::new(); // (path, session_id, mtime_ms)
    for e in entries.flatten() {
        let p = e.path();
        // F-MA：记录扩展名走 adapter（此处原不排 subagent，故用 has_record_ext 而非 is_record_file，
        // 保行为零变化）；sid 从路径按 adapter 约定取。
        if crate::adapter::has_record_ext(&p) {
            let sid = match crate::adapter::session_id_from_path(&p) {
                Some(s) => s,
                None => continue,
            };
            let mtime = p
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(systime_to_ms)
                .unwrap_or(0);
            jsonls.push((p, sid, mtime));
        }
    }
    if jsonls.is_empty() {
        return None;
    }

    // 项目 cwd：从任意 jsonl 的首条 user 消息取（按 mtime 最大那个最快有结果）
    jsonls.sort_by(|a, b| b.2.cmp(&a.2));
    let cwd = jsonls
        .iter()
        .find_map(|(p, _, _)| quick_extract_cwd(p))
        .unwrap_or_default();

    let project_name = Path::new(&cwd)
        .file_name()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| {
            // 实在拿不到就用 dir 名兜底（编码后的）
            dir.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("(未知项目)")
                .to_string()
        });

    let session_count = jsonls.len() as u32;
    let last_activity = jsonls.iter().map(|(_, _, m)| *m).max().unwrap_or(0);
    let has_live = jsonls.iter().any(|(_, sid, _)| map.is_session_active(sid));
    let mut starred_count = 0u32;
    let mut hidden_count = 0u32;
    for (_, sid, _) in &jsonls {
        if let Some(em) = metadata.entries.get(sid) {
            if em.starred {
                starred_count += 1;
            }
            if em.hidden {
                hidden_count += 1;
            }
        }
    }

    Some(HistoryProject {
        project_path: cwd,
        project_name,
        project_dir,
        session_count,
        starred_count,
        hidden_count,
        last_activity,
        has_live,
        origin: None, // 本地扫描路径恒为本地
    })
}

/// 只读首条带 cwd 的 user 记录的 cwd 字段，不解析其它行（早返回省 IO）。
/// jsonl 第 1 行通常就是 user（Claude Code 的固定写入顺序），最多扫 30 行兜底。
fn quick_extract_cwd(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    for line in reader.lines().map_while(Result::ok).take(30) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(Some(rec)) = parse_line(trimmed) {
            if let JsonlRecord::User { cwd: Some(c), .. } = rec {
                if !c.is_empty() {
                    return Some(c);
                }
            }
        }
    }
    None
}

fn analyze_jsonl(
    path: &Path,
    metadata: &HistoryMetadata,
    map: &SessionMap,
) -> Option<HistorySessionEntry> {
    let session_id = path.file_stem()?.to_str()?.to_string();
    let file = File::open(path).ok()?;
    let updated_at = file
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .map(systime_to_ms)
        .unwrap_or(0);
    let total_size = file.metadata().ok().map(|m| m.len()).unwrap_or(0);

    let mut cwd: Option<String> = None;
    let mut ai_title: Option<String> = None;
    let mut first_user_excerpt = String::new();
    let mut started_at: i64 = 0;
    let mut message_count: u32 = 0;
    // issue #12: 第一条带 forkedFrom 的 user/assistant 就锁住（典型整 session 共享）
    let mut forked_from_session_id: Option<String> = None;
    let mut forked_from_message_uuid: Option<String> = None;
    // Batch11-F32：CC 后台分身会话探测（记录级 sessionKind:"bg"——官方 resume
    // 选择器同款信号）。JsonlRecord 不透传未知字段，故对原始行做字符串探测
    // （两种空格形态；仅徽标用途，误报面可忽略）。
    let mut is_bg = false;

    let reader = BufReader::new(file);
    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !is_bg
            && (trimmed.contains(r#""sessionKind":"bg""#)
                || trimmed.contains(r#""sessionKind": "bg""#))
        {
            is_bg = true;
        }
        let rec = match parse_line(trimmed) {
            Ok(Some(r)) => r,
            _ => continue,
        };
        match &rec {
            JsonlRecord::User {
                cwd: c,
                timestamp,
                message,
                forked_from,
                ..
            } => {
                if cwd.is_none() {
                    if let Some(v) = c {
                        cwd = Some(v.clone());
                    }
                }
                if started_at == 0 {
                    started_at = iso_to_ms(timestamp);
                }
                if first_user_excerpt.is_empty() {
                    let text = extract_user_text(message);
                    if !text.is_empty() {
                        first_user_excerpt = truncate_chars(&text, 120);
                    }
                }
                if forked_from_session_id.is_none() {
                    if let Some(fk) = forked_from {
                        forked_from_session_id = Some(fk.session_id.clone());
                        forked_from_message_uuid = Some(fk.message_uuid.clone());
                    }
                }
                message_count += 1;
            }
            JsonlRecord::Assistant {
                timestamp,
                forked_from,
                ..
            } => {
                if started_at == 0 {
                    started_at = iso_to_ms(timestamp);
                }
                if forked_from_session_id.is_none() {
                    if let Some(fk) = forked_from {
                        forked_from_session_id = Some(fk.session_id.clone());
                        forked_from_message_uuid = Some(fk.message_uuid.clone());
                    }
                }
                message_count += 1;
            }
            JsonlRecord::AiTitle { ai_title: t, .. } => {
                // 后出现的覆盖（Claude 在会话里可能多次更新 ai-title）
                ai_title = Some(t.clone());
            }
            JsonlRecord::CustomTitle {
                custom_title: t, ..
            } => {
                // Claude Code v2.1.x 起新名字，语义同 ai-title
                ai_title = Some(t.clone());
            }
            _ => {}
        }
    }

    // 无任何可识别记录 → 跳过（异常 / 空文件）
    if started_at == 0 && message_count == 0 && cwd.is_none() {
        // 但仍然给一个最小条目，让用户能看到并删除空文件
        if total_size == 0 {
            return None;
        }
    }

    let project_path = cwd.unwrap_or_default();
    let project_name = Path::new(&project_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&project_path)
        .to_string();

    let entry_meta = metadata
        .entries
        .get(&session_id)
        .cloned()
        .unwrap_or_default();

    Some(HistorySessionEntry {
        session_id: session_id.clone(),
        project_path: project_path.clone(),
        project_name,
        ai_title,
        first_user_excerpt,
        is_bg,
        started_at,
        updated_at,
        jsonl_path: path.to_string_lossy().into_owned(),
        is_live: map.is_session_active(&session_id),
        message_count_approx: message_count,
        starred: entry_meta.starred,
        custom_title: entry_meta.custom_title.clone(),
        hidden: entry_meta.hidden,
        forked_from_session_id,
        forked_from_message_uuid,
        origin: None, // 本地扫描路径恒为本地
    })
}

/// 从 user 消息的 content 抠出纯文本预览。content 可以是 string 或 [Block...]。
fn extract_user_text(message: &ApiMessage) -> String {
    use serde_json::Value;
    match &message.content {
        Value::String(s) => clean_user_text(s),
        Value::Array(arr) => {
            for block in arr {
                if let Some(t) = block.get("type").and_then(|t| t.as_str()) {
                    if t == "text" {
                        if let Some(s) = block.get("text").and_then(|t| t.as_str()) {
                            let cleaned = clean_user_text(s);
                            if !cleaned.is_empty() {
                                return cleaned;
                            }
                        }
                    }
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

/// 去掉 Claude Code CLI 注入的 prompt 包装（<task-notification>/<system-reminder> 等），
/// 与前端 cards/index.ts 的 isInternalUserNoise 同一意图（这里更宽松，只是预览用）。
fn clean_user_text(s: &str) -> String {
    let mut out = s.to_string();
    // 简单去 tag 包裹（不需要完美，预览而已）
    for tag in [
        "task-notification",
        "system-reminder",
        "local-command-caveat",
        "local-command-stdout",
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
    out.trim().to_string()
}

fn truncate_chars(s: &str, n: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= n {
            out.push('…');
            break;
        }
        if ch == '\n' || ch == '\r' {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

// === metadata 持久化 ===

fn metadata_path() -> Option<PathBuf> {
    Some(paths::resolve_monitor_data_dir()?.join("history-metadata.json"))
}

pub(crate) fn load_metadata() -> Result<HistoryMetadata, String> {
    let path = metadata_path().ok_or("no monitor data dir")?;
    if !path.exists() {
        return Ok(HistoryMetadata::default());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str::<HistoryMetadata>(&raw).map_err(|e| {
        tracing::warn!("history-metadata.json parse failed ({e}); using empty");
        e.to_string()
    })
}

fn save_metadata(m: &HistoryMetadata) -> Result<(), String> {
    let path = metadata_path().ok_or("no monitor data dir")?;
    // 走 utils::atomic_write_json：Windows ReplaceFileW + dst-not-exist fallback，
    // 非 Windows 单步 rename，全程原子。比早期"write tmp + remove + rename"三步更
    // 不易丢文件——后者中途 crash 用户的 star/重命名/隐藏全失。
    crate::utils::atomic_write_json(&path, m).map_err(|e| e.to_string())
}

// === 时间换算 → utils 归并（P3）===
// `systime_to_ms` / `parse_iso8601_ms` / `now_ms` 已搬到 crate::utils。
// 本地保留两个适配 helper（带 unwrap_or(0) 兜底）以最小化修改面。

fn iso_to_ms(iso: &str) -> i64 {
    // Claude 写的 timestamp 形如 "2026-05-20T15:11:42.345Z"；失败返 0
    // （前端会显示为 1970，看得见但不崩）。
    crate::utils::parse_iso8601_ms(iso).unwrap_or(0)
}

#[cfg(test)]
mod title_coverage {
    /// ★ **每个带 `*title` 字段的 `JsonlRecord` 变体都必须被标题抽取吃到**〔audit-0805 08-06〕。
    ///
    /// # 它钉的是一个**有历史先例**的缺口
    ///
    /// 标题抽取那段 `match` 只认两种记录，其余走 `_ => {}` —— 静默忽略。
    /// 对绝大多数记录类型这是对的（它们与标题无关），
    /// **但对「又冒出一种承载标题的记录」就不是** ——
    /// 而那件事**真的发生过**：Claude Code v2.1.x 把 `ai-title` 改名成 `custom-title`，
    /// 仓里因此多了 `JsonlRecord::CustomTitle` 这一支。
    /// 那次是**靠人发现的**：改名之后标题会静默消失，没有任何判据会红。
    ///
    /// ⇒ 本条把「谁承载标题」这件事变成可机检的：
    /// 人群 = `messages.rs` 的 `JsonlRecord` 里**带 `*title` 字段**的变体（今天 2 个）；
    /// 性质 = 它必须出现在 `history.rs` 的标题抽取段里。
    /// 加第三种标题记录而忘了接 ⇒ 红。
    ///
    /// ⚠ 人群刻意**不按变体名**取（`*Title` 结尾那种）——名字是可以随便起的，
    /// 而「带一个叫 `xxx_title` 的字段」才是它承载标题的实据。
    #[test]
    fn every_title_bearing_record_is_consumed_by_the_extractor() {
        let msgs = include_str!("messages.rs");
        let b = msgs
            .find("enum JsonlRecord")
            .expect("找不到 `JsonlRecord` 定义 —— 记录类型搬家了，本条要跟着改");
        let e = msgs[b..].find("\nfn ").map_or(msgs.len(), |k| b + k);
        let block = &msgs[b..e];

        let mut cur = String::new();
        let mut bearers: Vec<String> = Vec::new();
        for line in block.lines() {
            let ind = line.len() - line.trim_start().len();
            let s = line.trim_start();
            if ind == 4 && s.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                cur = s
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect::<String>();
            }
            if !cur.is_empty()
                && s.contains("title")
                && s.contains(':')
                && !bearers.contains(&cur)
                && s.split(':')
                    .next()
                    .is_some_and(|f| f.trim().ends_with("title"))
            {
                bearers.push(cur.clone());
            }
        }
        // 抽取器自检：一个都没抠到 ⇒ 下面的对拍会零命中地绿。
        assert!(
            bearers.len() >= 2,
            "只抠到 {} 个带 `*title` 字段的变体（08-06 实测 2：AiTitle / CustomTitle）\
             —— 抽取坏了，本条此刻是空转的：{bearers:?}",
            bearers.len()
        );

        let here = include_str!("history.rs");
        let missing: Vec<&String> = bearers
            .iter()
            .filter(|v| !here.contains(&format!("JsonlRecord::{v}")))
            .collect();
        assert!(
            missing.is_empty(),
            "这些记录类型带 `*title` 字段，却没被 `history.rs` 的标题抽取接住：{missing:?}\n\
             ⚠ 后果不是报错，是**标题静默消失** —— 会话列表上那一行变回默认名，\n\
             而没有任何判据会红。这件事真发生过一次（`ai-title` 改名成 `custom-title`），\n\
             那次是靠人发现的。\n\
             ⇒ 在标题抽取的 `match` 里给它加一条具名臂。"
        );
    }
}

#[cfg(test)]
mod tests {

    /// 〔audit-0805 08-06〕**防命令注入的那道校验，此前一条判据都没有。**
    ///
    /// # 怎么找到的
    ///
    /// 新先验：抽「只被一处调用」的生产函数。`has_bad_chars` 在全仓只出现两次
    /// （定义 + 一处调用），顺着它找到唯一消费者 [`validate_config_dir_ps`] ——
    /// 而它 5 处出现里**没有一处是测试**。
    ///
    /// 它守的是「**拒绝拼入命令**：非法 CLAUDE_CONFIG_DIR」，也就是把一个用户可控的
    /// 目录名塞进 PowerShell 命令串之前的最后一道闸。失效形态是**静默放行**：
    /// 校验松掉不会让任何测试变红，而后果是命令串里多了一个 `;` 或 `$(...)`。
    ///
    /// ⚠ 它 `#[cfg(any(windows, test))]` —— **Linux 的测试构建里是编译的**，
    /// 所以这一族与 `ROADMAP §5` 的 3y（Windows-only 代码本机连编译都不碰）**不同**：
    /// 这里没有平台借口，只是没人写。
    ///
    /// # 用例挑的是「每一条拒绝理由各一发 + 两条不许误拒」
    ///
    /// 不许误拒那两条是有来历的：头注逐字记着 Phase G 审计抓出的真 bug ——
    /// 早先两边共用「必须 `/` 开头 + 禁 `\`」，于是真实的 Windows 账号目录
    /// `C:\Users\z\.claude-accts\z` **必被拒**，「本机分叉时选具名账号」在主平台 100% 失败。
    /// ⇒ 反向用例把那个回归钉住。
    #[test]
    fn the_config_dir_validator_rejects_every_injection_shape() {
        // ★ 先证明夹具走得通：两种平台的合法绝对路径都必须过。
        for ok in [
            "/home/z/.claude",
            "C:\\Users\\z\\.claude-accts\\z",
            "\\\\server\\share\\claude",
        ] {
            assert!(
                validate_config_dir_ps(ok).is_ok(),
                "合法路径被拒了：{ok:?} —— 这正是 Phase G 抓出的那个真 bug 的形状\n\
                 （早先禁 `\\` ⇒ 每个 Windows 账号目录都过不去，主平台 100% 失败）"
            );
        }

        // ① 非绝对 / 根 / `..` 穿越（两种分隔符、中间与结尾各一）
        for bad in [
            "relative/path",
            ".claude",
            "/",
            "/home/../etc",
            "/home/..",
            "C:\\a\\..\\b",
            "C:\\a\\..",
        ] {
            assert!(
                validate_config_dir_ps(bad).is_err(),
                "路径形态没被拒：{bad:?}"
            );
        }

        // ② 控制字符与 C1 段（`\u{85}` 在很多终端里不可见）
        for bad in [
            "/home/z\u{0}/x",
            "/home/z\n/x",
            "/home/z\u{85}/x",
            "/home/z\u{9f}/x",
        ] {
            assert!(
                validate_config_dir_ps(bad).is_err(),
                "控制字符没被拒：{bad:?}"
            );
        }

        // ③ shell 元字符 —— **逐个**过，不是抽一个代表。
        //    ★ 自检：集合非空，否则这个循环是空转的。
        assert!(
            !crate::backend::control::payload::SHELL_META_COMMON.is_empty(),
            "`SHELL_META_COMMON` 空了 —— 下面这轮是空转的"
        );
        for c in crate::backend::control::payload::SHELL_META_COMMON.chars() {
            let bad = format!("/home/z{c}/x");
            assert!(
                validate_config_dir_ps(&bad).is_err(),
                "shell 元字符 {c:?} 没被拒 —— 它会被原样拼进命令串"
            );
        }

        // ④ 同形欺骗字符（走 `acct_core::is_deceptive_char` 那条并集）
        //    先确认这个字符确实被那张表认得，否则用例本身可能选错了字。
        // ★ 这条自检当场救过一次：第一版选的是 `\u{2044}`（FRACTION SLASH，肉眼像 `/`），
        //   它**不在** `acct_core` 那张表里 —— 若没有这条自检，下面那条会因为别的原因红/绿，
        //   而我会以为「欺骗字符这一支验过了」。
        for deceptive in ['\u{200B}', '\u{202E}', '\u{FEFF}', '\u{00A0}'] {
            assert!(
                acct_core::is_deceptive_char(deceptive),
                "样本字符 {deceptive:?} 不在 `acct_core` 的欺骗字符表里 —— \
                 换一个，否则下面那条在测别的东西"
            );
            assert!(
                validate_config_dir_ps(&format!("/home/z{deceptive}etc")).is_err(),
                "同形/不可见字符 {deceptive:?} 没被拒 —— 它在终端里看不见，却会原样进命令串"
            );
        }
    }

    /// ★★ **把「本机 resume 到底跑什么」钉在真构造器上**〔audit-0805 F08 / 报告 B-2〕。
    ///
    /// # 此前那条判据在替代码说好话
    ///
    /// `launch.rs::local_and_remote_share_the_same_payload` 用的是**手写夹具**
    /// `"… && ccm --tmux claude --resume s1"`，而它**从不调用**真正的 payload 构造器。
    /// 那个夹具里有 `--tmux`，生产里没有 —— **判据恰好体现了生产违反的那个假设**。
    ///
    /// # 本条钉的是**现状**，不是理想
    ///
    /// 它断言生产 payload 里**确实没有容器**（既无 `--tmux` 也无 `cct`）。
    /// 这不是在祝福这个行为 —— 是让它**不能再悄悄变、也不能再被一条漂亮的夹具盖住**。
    /// 真要改成进容器，改完这条会红，那时才是带着证据做决定的时刻。
    ///
    /// ⚠ 功能后果（claude 在 `stdin=/dev/null` 下具体怎么表现）**红线内测不了**，
    /// 本条只钉**命令串**这一层可判据的事实。
    #[test]
    fn the_local_resume_payload_has_no_session_container_today() {
        let choice = local_launch_choice(&LocalPsAction::Resume("s1".into()), None)
            .expect("resume s1 应该能构造出来");
        let rendered = match &choice {
            LocalLaunchChoice::Fixed(c) => c.clone(),
            LocalLaunchChoice::Probe {
                preferred,
                fallback,
                ..
            } => format!("{preferred} | {fallback}"),
        };
        assert!(
            rendered.contains("--resume") && rendered.contains("s1"),
            "抽取器自检：构造出来的串里连 `--resume s1` 都没有 —— 切错东西了：{rendered}"
        );
        assert!(
            !rendered.contains("--tmux") && !rendered.split_whitespace().any(|w| w == "cct"),
            "★★ **回落那条路**产出了会话容器（`--tmux` / `cct`）—— 本条不该再绿。\n\
             \n\
             ⚠ **P3t-Y3 翻面**：本条**测什么没变，自陈换了**。它量的从来只是\n\
             `local_launch_choice`，也就是 **P3t 之后的回落路**（渲染器拒了才走的那条）。\n\
             P3t 之前那等价于「本机 resume 没有容器」；**现在不等价了** ——\n\
             本机先过 `render_local_ccm`，渲得出来就带 `--tmux`。\n\
             ⇒ 本条现在钉的是「**回落路仍是无容器的那条**」：它是诚实降级的落点，\n\
             不是第二条并列的路（顺序由 `the_local_launch_tries_the_renderer_before_the_old_path` 钉）。\n\
             真要连回落也进容器，请连同 `launch.rs` 与 `src/fork-start.ts` 那两条头注一起改。\n\
             实得：{rendered}"
        );
    }

    /// ★★ **P3t-Y3 的翻面另一半 —— 不许翻成更弱的一条。**
    ///
    /// 上面那条钉「回落路没有容器」。光有它，**整个 P3t 被回退掉也不会红**
    /// （回落路本来就该没容器，回退之后它还是没容器）。
    /// ⇒ 必须再钉正面事实：**渲染得出来的时候，那条串真的带 `--tmux`**。
    ///
    /// 判据怎么失效（`P2s-Y3` / `P3 刀 0` 各栽过一次）：翻面时只留「旧事实不再成立」，
    /// 丢掉「新事实成立」。所以这里同时钉容器名**就是传进去的那个**
    /// —— 若它被换成 Rust 自己铸的名字，`U11` 那个撞名坑就回来了。
    /// 判据自己给能力集 —— **不问这台机器**。
    ///
    /// = `CLI_REQUIRED_CAPS`（每次调用都要的静态能力）**加上两条 §37 维度能力**
    /// （`account` 恒真维度要 / `model` 条件式维度要）。第一版只给了前者，
    /// 当场报「维度 account 需要远端 ccm 能力 account」—— 那正是 §37 把两类能力
    /// 分开的意义：静态那张表**不是**全集，照它拼会漏。
    ///
    /// 缺能力时该怎样，由 `ccm_invocation` 自己那两条（`MissingCap` / `DimensionNeedsCap`）钉；
    /// 本文件只需要一个「能力齐」的输入。
    #[cfg(not(windows))]
    fn caps_of_a_current_ccm() -> std::collections::BTreeSet<String> {
        crate::backend::control::ccm_invocation::CLI_REQUIRED_CAPS
            .iter()
            .map(|c| (*c).to_string())
            .chain(["account".to_string(), "model".to_string()])
            .collect()
    }

    #[test]
    #[cfg(not(windows))]
    fn the_rendered_local_command_really_carries_the_container() {
        let name = "s1abcdef-cc";
        let cmd = render_local_ccm_with(
            &LocalPsAction::Resume("s1abcdef".into()),
            None,
            Some(&LaunchAccount::Base),
            Some(name),
            &caps_of_a_current_ccm(),
            true,
        )
        .expect("账号 0 + 有名字 + 能力齐 ⇒ 必须渲染得出来");
        assert!(
            cmd.contains("--tmux"),
            "渲出来的本机命令里没有 `--tmux` —— 那就还是**无 tty、无 tmux** 的老样子，\n\
             用户敲进去的字会被脚本吃掉。实得：{cmd}"
        );
        assert!(
            cmd.contains(name),
            "容器名不是传进去的那个（`{name}`）—— 名字只许由前端 `mintTmuxName` 铸，\n\
             在 Rust 里另铸一个就是 F13 修掉的撞名坑（见 ROADMAP `U11`）。实得：{cmd}"
        );
    }

    /// ★★ **P3t-Y2 给上面那条补一句射程**（不改它测什么，只改它自称守什么）。
    ///
    /// 上面那条量的是 `local_launch_choice` —— 也就是**回落那条路**的构造器。
    /// P3t-Y2 之后本机拉起先过 CLI 渲染器（`render_local_ccm`），渲不出来才落到它。
    /// ⇒ 「本机 resume 没有容器」这句话**从此不再等价于**「`local_launch_choice` 没有容器」：
    /// 前者要看渲染器渲不渲得出来，后者只看回落。上面那条**照旧恒绿**，但它守的人群窄了。
    ///
    /// 本条不是重复它，是把「窄了多少」写成可执行的：今天 `render_local_ccm` 的**三格纯逻辑拒绝**
    /// 决定了生产上谁能进容器。三格全拒 ⇒ 生产行为与 P3t 之前逐字节相同（Y2 是零行为改动的接线）。
    /// Y2b 前端接线之后，第一格会开，那时上面那条的自陈就该改了。
    #[test]
    #[cfg(not(windows))]
    fn the_local_renderer_refuses_every_shape_the_front_end_can_send_today() {
        let base = LaunchAccount::Base;
        let named = LaunchAccount::Named {
            config_dir: "/home/u/.claude-accts/z".into(),
        };
        let act = LocalPsAction::Resume("s1".into());

        // ① 没名字 —— 名字只许 `mintTmuxName` 铸，Rust 这侧不许补默认值（F13 那个坑）。
        for no_name in [None, Some(""), Some("   ")].into_iter() {
            let no_name = no_name.filter(|n: &&str| !n.trim().is_empty());
            let r = render_local_ccm_with(
                &act,
                None,
                Some(&base),
                no_name,
                &caps_of_a_current_ccm(),
                true,
            );
            assert!(
                r.as_ref().is_err_and(|e| e.contains("tmux 会话名")),
                "没有会话名时必须拒 —— 在 Rust 里铸一个名字就是 F13 修掉的撞名坑第三次。实得：{r:?}"
            );
        }

        // ② 未表态账号（`None`）—— 旧路发**空前缀**＝继承环境，而 CLI 的 account 维度恒真、
        //    没有「继承」这一态。映成 `--base` 会把用户 shell 里已有的账号悄悄清掉 ＝ #75 病灶。
        let r = render_local_ccm_with(
            &act,
            None,
            None,
            Some("s1abcdef-cc"),
            &caps_of_a_current_ccm(),
            true,
        );
        assert!(
            r.as_ref().is_err_and(|e| e.contains("继承")),
            "未表态账号必须拒且理由是「说不出继承」—— 若它被渲染成 `--base`，\n\
             那就是把「继承环境」偷换成「显式清空」，正是 #75「resume 在错数据目录找不到会话」。实得：{r:?}"
        );

        // ③ 具名账号 —— `LaunchAccount::Named` 只有 configDir、没有名字，而 CLI 只会
        //    `--account <名字>` ⇒ §35 短路。**理由必须是「说不出」，不是别的**：
        //    reason 是生产侧唯一的降级线索，换一个理由就是换一条诊断。
        let r = render_local_ccm_with(
            &act,
            None,
            Some(&named),
            Some("s1abcdef-cc"),
            &caps_of_a_current_ccm(),
            true,
        );
        let reason = r.expect_err(
            "具名账号今天渲染不出来 —— 若它成功了，请先确认 `--account` 的名字是从哪来的",
        );
        assert!(
            reason.contains("account"),
            "具名账号的降级理由该指向 account 维度（§35 短路），实得：{reason}"
        );
    }

    /// ★★★ `D4 阻-3`：**「走中转」与「有 tmux 容器」今天仍然互斥** —— 把这个事实钉住。
    ///
    /// # 它为什么存在：盘上写着「已消掉」，而其实没消掉
    ///
    /// 第一拍报过一条代价「走中转的号拿不到 tmux 容器」（当时的成因：外侧那句 export
    /// 在 tmux 边界被吃掉）。第二拍照 `R08` 在 `shared/ccm` 里加了一条转发，于是件文件
    /// 与 [`launch_local`] 的头注都写上了**「不再互斥」**。
    /// 🔴 `D4` 现打证伪：**代价原样还在，只是成因换了。**
    ///
    /// # 今天的成因（本条逐格量出来，不是推的）
    ///
    /// 分母 = [`LaunchAccount`] 的**全部形状**加上「参数缺席」，共三格：
    ///
    /// | 形状 | [`relay_account_id`] | [`render_local_ccm_with`] |
    /// |---|---|---|
    /// | 缺席（`None`） | `None`（不走中转） | `Err`（CLI 说不出「继承」） |
    /// | `Base`（账号 0） | `None`（不走中转） | `Ok`（**唯一渲得出容器的那一格**） |
    /// | `Named{config_dir}` | `Some(id)`（**唯一走得了中转的那一格**） | `Err`（§35 短路：有 configDir 没名字） |
    ///
    /// ⇒ **能推出中转 id 的那一格，正是 ccm 渲染器拒掉的那一格。**
    /// 凡是带中转前缀的本机拉起，必然落 [`build_local_posix_command`]（那条路没有 tmux 容器）；
    /// 凡是走 ccm 容器路的，中转前缀必然是空串。**两条路今天不相交。**
    ///
    /// # ⚠ 它连带说明了一件别处的事（别让那条判据被读宽）
    ///
    /// `shared/ccm` 那条 `ANTHROPIC_BASE_URL` 转发（连同钉它的
    /// `payload::tests::the_ccm_container_path_forwards_the_relay_base_url_across_the_tmux_boundary`
    /// 与件文件里的 `M12`/`M12b`）量的是一条**在本机中转这条路上今天生产不可达**的路：
    /// 那段 shell 真的会转发，而**没有任何生产输入能同时走到中转与容器**。
    /// 它不是假的，它买不到本件要的那一格。**那条判据的头注里也写了这句话，两处别只改一处。**
    ///
    /// # 🔴 这条前提**本来就该变** —— 变的那天去哪里重新裁定（`testing.md` 三.11 要的那一栏）
    ///
    /// 消掉互斥要给 [`LaunchAccount::Named`] 补上**名字**（要改 `LaunchAccount` 与它的前端
    /// 调用点，都不在 `K-H2b` 的写区）。真做那一天，**同一拍**要做完这四样，缺一样就是又一次
    /// 「盘上写着已解而其实没解」：
    ///   ① 本条会红 —— **在这里重新裁定**（改成「不再互斥」并说清新的人群）；
    ///   ② [`launch_local`] 的头注里那段「互不互斥」跟着改；
    ///   ③ 件计划 `K-H2b §4` 那条登记跟着改；
    ///   ④ **`shared/ccm` 的 `capabilities=` 串要加上那个 token** —— 现打 17 个 token 里
    ///      含 `relay`/`base-url`/`anthropic` 的 **0** 个，而 `ccm_probe` 探的是 PATH 上那个 ccm
    ///      ⇒ 不加的话，装了旧 ccm 的机器会**静默吃掉**这个变量。
    #[test]
    #[cfg(not(windows))]
    fn a_launch_that_goes_through_the_relay_still_cannot_get_a_tmux_container() {
        let act = LocalPsAction::Resume("s1".into());
        let named = LaunchAccount::Named {
            config_dir: "/home/u/.claude-accts/acct-a".into(),
        };
        let base = LaunchAccount::Base;
        // 分母就是这三格 —— `LaunchAccount` 今天只有两个变体，加上「参数缺席」。
        let shapes: [(&str, Option<&LaunchAccount>); 3] = [
            ("缺席", None),
            ("Base", Some(&base)),
            ("Named", Some(&named)),
        ];

        let mut relayed = Vec::new();
        let mut containered = Vec::new();
        for (label, acct) in shapes {
            let relay_id = relay_account_id(acct);
            let renders = render_local_ccm_with(
                &act,
                None,
                acct,
                Some("s1abcdef-cc"),
                &caps_of_a_current_ccm(),
                true,
            )
            .is_ok();
            if relay_id.is_some() {
                relayed.push(label);
            }
            if renders {
                containered.push(label);
            }
        }
        // 反空真：两边**都非空**（都空的话下面那条不相交是空真）。
        assert_eq!(
            relayed,
            ["Named"],
            "能推出中转 id 的形状变了 —— 本条的结论要重新裁定（见头注最后一节）"
        );
        assert_eq!(
            containered,
            ["Base"],
            "能渲染出 ccm 容器的形状变了 —— 本条的结论要重新裁定（见头注最后一节）"
        );
        // 正题：两个集合不相交 ⇒ 今天没有任何一次本机拉起同时拿到中转前缀与 tmux 容器。
        assert!(
            relayed.iter().all(|l| !containered.contains(l)),
            "「走中转」与「有 tmux 容器」不再互斥了 —— 那是**好事**，但盘上有四处话要跟着改：\n\
             ① 本条（重新裁定）② `launch_local` 头注 ③ 件计划 `K-H2b §4` 那条登记\n\
             ④ `shared/ccm` 的 `capabilities=` 串要加 token（否则装了旧 ccm 的机器静默吃掉那个变量）。\n\
             实得：走中转的 {relayed:?} · 有容器的 {containered:?}"
        );
    }

    /// ★★ **P3t-Y2 的顺序判据**：渲染器在前，`build_local_posix_command` 在后。
    ///
    /// 「两条路都在文件里」证明不了任何事 —— 本件的全部内容就是**谁先谁后**：
    /// 旧路必须是「渲染器拒了才走」的回落，不是并列的第二条路。并列意味着
    /// 「本机进不进 tmux」由谁先被写下来决定，那不是一个能守住的性质。
    ///
    /// 形状抄 `local_backend::the_exit_path_really_stops_the_local_backend`：
    /// 锚点当场核唯一性 + 按花括号配平切体 + 切出来的体自检大小（否则会零命中地绿）。
    #[test]
    fn the_local_launch_tries_the_renderer_before_the_old_path() {
        let prod = guard_core::production_code(include_str!("history.rs"));
        // 锚点唯一性：`fn launch_local(` 全树恰好一处（`find_pinned` 自带边界检查）。
        let at = guard_core::find_pinned(&prod, "fn launch_local(").unwrap_or_else(|e| {
            panic!("`fn launch_local(` 不是恰好一处 —— 形状变了，先修锚点：{e}")
        });
        let body = {
            let b = prod.as_bytes();
            let open = (at..b.len())
                .find(|&i| b[i] == b'{')
                .expect("`fn launch_local(` 之后找不到块起点");
            let (mut depth, mut end) = (0i32, b.len());
            for i in open..b.len() {
                if b[i] == b'{' {
                    depth += 1;
                } else if b[i] == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
            }
            &prod[open..end]
        };
        assert!(
            body.len() > 200 && body.len() < 3000,
            "切出来的 `launch_local` 体只有 {} 字节 —— 配平切错了，本条会零命中地绿",
            body.len()
        );

        let r = body.find("render_local_ccm(").expect(
            "`launch_local` 体里找不到 `render_local_ccm(` —— 渲染器没接上，本机还是走旧路",
        );
        let old = body
            .find("build_local_posix_command(")
            .expect("`launch_local` 体里找不到 `build_local_posix_command(` —— 回落没了，渲染器拒了就无路可走");
        assert!(
            r < old,
            "★ 顺序反了：`build_local_posix_command` 出现在 `render_local_ccm` **之前**。\n\
             那样旧路就成了并列的第一条路，渲染器变成够不着的死代码 —— 本件等于没做。"
        );
        // 各恰好一处：两处渲染器调用意味着有一条分支绕过了顺序。
        for (needle, n) in [
            (
                "render_local_ccm(",
                body.matches("render_local_ccm(").count(),
            ),
            (
                "build_local_posix_command(",
                body.matches("build_local_posix_command(").count(),
            ),
        ] {
            assert_eq!(
                n, 1,
                "`launch_local` 体里 `{needle}` 出现 {n} 次 —— 不是恰好一处，顺序就管不住了"
            );
        }
        // 回落必须真的住在 `Err` 那条臂里，而不是顺序碰巧靠后。
        let err_arm = body
            .find("Err(why)")
            .expect("`launch_local` 体里找不到 `Err(why)` —— 回落不在降级臂里了");
        assert!(
            err_arm < old,
            "`build_local_posix_command` 不在 `Err(why)` 臂内 —— 它只是碰巧写在后面，\n\
             那不叫「渲染器拒了才走」。"
        );
    }
    use super::*;

    /// 每个测试独占的临时目录（仓库约定不引 `tempfile`，用 pid + 计数器保唯一）。
    /// **绝不碰用户真实的 `~/.claude`** —— 全部在 `std::env::temp_dir()` 下。
    struct TmpDir(PathBuf);
    static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    impl TmpDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "hist-{}-{}",
                std::process::id(),
                TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            ));
            std::fs::create_dir_all(&p).expect("mkdir");
            TmpDir(p)
        }
        fn write(&self, name: &str, body: &str) -> PathBuf {
            let f = self.0.join(name);
            std::fs::write(&f, body).expect("write");
            f
        }
    }
    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// 一行合法的 user 记录（形状照 `messages.rs` 的黄金样本）。
    fn user_line(cwd: &str) -> String {
        format!(
            r#"{{"type":"user","uuid":"u-1","timestamp":"2026-05-20T01:23:45.678Z","cwd":"{cwd}","message":{{"role":"user","content":"hi"}}}}"#
        )
    }

    /// 〔audit-0805 08-06〕**`read_jsonl_values` 的两个无声决定**：剥 BOM · 静默丢弃坏行。
    ///
    /// 它全仓出现 2 次、所在文件测试段 0 次（先验：只被一处调用的生产函数）。
    /// 两个决定都是**成心的**，也都**没人钉**：
    /// - **剥 BOM**（`trim_start_matches('\u{feff}')`）：Windows 上的文件常带 BOM，
    ///   不剥就是第一行永远 parse 不了 —— 而它的表现是「历史少一条」，不报错。
    /// - **坏行静默丢弃**（`if let Ok(v)`）：一条损坏的行不该让整个历史读不出来。
    ///   这是**刻意的韧性**，但它同时意味着「丢了多少」没人知道 ⇒ 至少要钉住
    ///   「好行一条不少」，否则哪天连好行一起丢也不会红。
    #[test]
    fn read_jsonl_values_strips_bom_and_drops_only_the_broken_lines() {
        let tmp = TmpDir::new();
        let body = format!(
            "\u{feff}{}\n\n   \n{}\n{{ 这行不是 JSON \n{}\n",
            r#"{"a":1}"#, r#"{"b":2}"#, r#"{"c":3}"#
        );
        let f = tmp.write("x.jsonl", &body);
        // ★ 夹具自检：文件里确实有坏行与空行，否则下面在测别的东西。
        assert!(
            body.lines().count() >= 6 && body.contains("这行不是 JSON"),
            "夹具没造出「坏行 + 空行」的场面"
        );

        let got = read_jsonl_values(&f).expect("读不该失败 —— 坏行是丢弃不是报错");
        assert_eq!(
            got.len(),
            3,
            "好行应当一条不少（BOM 那条也算）；实得 {:?}",
            got
        );
        assert_eq!(
            got[0].get("a").and_then(|v| v.as_i64()),
            Some(1),
            "第一行没解析出来 —— BOM 多半没被剥掉"
        );
        assert_eq!(got[2].get("c").and_then(|v| v.as_i64()), Some(3));
    }

    /// 〔audit-0805 08-06〕**同一个问题，本地与远端给两个答案**（E3 + §40）。
    ///
    /// # 实测到的两处分歧
    ///
    /// 「从 jsonl 头部取 cwd」这件事有两处实现：
    /// - monitor：`quick_extract_cwd` —— 窗口 **30** 行，且**只认 `JsonlRecord::User`** 且 cwd 非空；
    /// - daemon：`observe/history_query.rs::extract_cwd_from_head` —— 窗口 **40** 行，
    ///   且认**任何**带非空 `cwd` 字段的记录。
    ///
    /// 后果是具体的：**首个带 cwd 的记录落在第 31–40 行时，远端报得出 cwd、本地报不出**；
    /// 若那条记录不是 `user` 类型，差别还要更大。同一份文件、同一个问题、两个答案。
    ///
    /// ⚠ daemon 那边的头注**已经在做这个对照**了 —— 但它只对照了「流式 vs 整读」，
    /// **没提窗口和记录类型不一样**。⇒ 又一次「订正手头那一处，不等于订正那句话」。
    ///
    /// # 为什么本轮只钉不改
    ///
    /// §40 是〔用 2026-07-29〕拍的方向（「把本地当成不走 ssh 的远端」），照它推**本地该对齐远端**。
    /// 但「窗口取 30 还是 40」「要不要放宽到任意记录类型」是**会改变行为**的设计决定
    /// （放宽后 cwd 可能来自非 user 记录），不该由我顺手定。⇒ 走档①：**登记 + 钉住，不擅自对齐**。
    /// 差异与解锁条件记在 `ROADMAP §5`；本条保证它**不会再悄悄变宽或变窄**。
    #[test]
    fn the_two_cwd_extractors_still_disagree_exactly_as_registered() {
        // 本地那一侧：从自己的生产段里抽，不写死。
        let own = guard_core::production_code(include_str!("history.rs"));
        let local = own
            .lines()
            .find(|l| l.contains("reader.lines().map_while(Result::ok).take("))
            .and_then(|l| l.split(".take(").nth(1))
            .and_then(|s| s.split(')').next())
            .and_then(|s| s.trim().parse::<usize>().ok())
            .expect("抽不到本地那侧的窗口 —— 读法坏了，本条会零命中地绿");

        // 远端那一侧：读 daemon 源码（同 `agent_profile_parity` 的既有做法）。
        let daemon_src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("仓根")
                .join("remote-daemon-proto/src/observe/history_query.rs"),
        )
        .expect("读不到 daemon 的 history_query.rs");
        let remote = guard_core::production_code(&daemon_src)
            .lines()
            .find(|l| l.contains("reader.lines().map_while(Result::ok).take("))
            .and_then(|l| l.split(".take(").nth(1))
            .and_then(|s| s.split(')').next())
            .and_then(|s| s.trim().parse::<usize>().ok())
            .expect("抽不到远端那侧的窗口 —— daemon 那边改形了，读法要跟着改");

        // ★ 登记今天的实况。**这不是「应该这样」，是「今天就是这样」** ——
        //   两边一旦有任何一侧动了，本条就红，逼人做那个被推迟的设计决定。
        assert_eq!(
            (local, remote),
            (30, 40),
            "本地/远端的 cwd 提取窗口变了（实得 本地={local} 远端={remote}）。\n\
             ⚠ 这两个数今天**故意不一致**且已登记（`ROADMAP §5`）：\n\
             首个带 cwd 的记录落在第 31–40 行时，远端报得出、本地报不出。\n\
             §40〔用 2026-07-29〕的方向是「把本地当成不走 ssh 的远端」⇒ 该对齐，\n\
             但选哪个数、要不要同时放宽记录类型，是会改行为的设计决定。\n\
             ⇒ 改之前先把那个决定做掉并更新本条，别让它无声地漂到第三个值。"
        );

        // 记录类型那一半也钉住：本地限定 `User`，远端不限定。
        assert!(
            own.contains("JsonlRecord::User { cwd: Some(c), .. }"),
            "本地那侧不再限定 `JsonlRecord::User` 了 —— 那正是与远端的第二处分歧，\n\
             它变了就说明有人在对齐（好事），请连同上面那条一起更新。"
        );
    }

    /// 〔audit-0805 08-06〕**`quick_extract_cwd` 只看前 30 行 —— 这个上限此前无声也无判据。**
    ///
    /// 它是「列历史项目时快速拿到 cwd」的探针，`take(30)` 是**成本与命中率的折中**：
    /// 超出 30 行就放弃、返回 `None`（调用方另有兜底）。
    /// 问题是这个数**没有任何东西读它** —— 改成 3 或改成 300 都不会红，
    /// 前者让一批会话拿不到 cwd（表现为「项目名不对」，不是报错），后者让列表变慢。
    /// ⇒ 钉住边界本身：**第 30 行还在窗口内、第 31 行不在**。
    #[test]
    fn quick_extract_cwd_stops_after_the_thirtieth_line() {
        let tmp = TmpDir::new();

        // ① 正路：靠前的 user 记录能拿到 cwd。
        let f = tmp.write("early.jsonl", &format!("{}\n", user_line("/w/early")));
        assert_eq!(
            quick_extract_cwd(&f),
            Some("/w/early".to_string()),
            "靠前的记录都拿不到 —— 夹具或解析坏了，下面两条会变成空转"
        );

        // ② 边界：**正好第 30 行**仍在窗口内。
        let at30 = format!("{}{}\n", "\n".repeat(29), user_line("/w/at30"));
        let f30 = tmp.write("at30.jsonl", &at30);
        assert_eq!(
            at30.lines().count(),
            30,
            "夹具没把记录放在第 30 行，边界这条在测别的位置"
        );
        assert_eq!(
            quick_extract_cwd(&f30),
            Some("/w/at30".to_string()),
            "第 30 行被排除了 —— 窗口比 `take(30)` 小"
        );

        // ③ 边界外：第 31 行拿不到（这正是 `take(30)` 的语义）。
        let at31 = format!("{}{}\n", "\n".repeat(30), user_line("/w/at31"));
        let f31 = tmp.write("at31.jsonl", &at31);
        assert_eq!(at31.lines().count(), 31, "夹具没把记录放在第 31 行");
        assert_eq!(
            quick_extract_cwd(&f31),
            None,
            "第 31 行也被读了 —— 窗口比 `take(30)` 大，列历史会变慢而没人知道"
        );

        // ④ 空 cwd 不算命中，要继续往后找。
        let mixed = format!("{}\n{}\n", user_line(""), user_line("/w/real"));
        let fm = tmp.write("mixed.jsonl", &mixed);
        assert_eq!(
            quick_extract_cwd(&fm),
            Some("/w/real".to_string()),
            "空 cwd 被当成了命中 —— 调用方会拿到空串当项目路径"
        );
    }

    /// Phase 2 F1a-3：Codex 会话按 cwd 分组成合成 HistoryProject（count/max-mtime/name/键/has_live）。
    /// 测试用：把一个 configDir 包成具名账号。
    fn named(d: &str) -> LaunchAccount {
        LaunchAccount::Named {
            config_dir: d.to_string(),
        }
    }

    // ── G3b：本地拉起的账号注入（`CLAUDE_CONFIG_DIR`）─────────────────────────
    //
    // 既有那批**逐字节钉死输出**的测试全部传 `None` 后原样通过 ——
    // 它们因此就是「**账号 0 = 一个字都不注入 = 旧行为**」的守卫，不用再写一条。

    #[test]
    fn account_zero_injects_nothing_byte_for_byte() {
        // 三种「没有账号」的表达（None / 空串 / 纯空白）产出必须完全相同。
        // ① 参数缺席 = 调用方没表态 ⇒ 一个字都不注入（既有调用点逐字节等价旧行为）
        let a = build_local_posix_command(&LocalPsAction::New, None, None).unwrap();
        assert!(
            !a.contains("CLAUDE_CONFIG_DIR"),
            "没表态时不该出现这个变量名"
        );

        // ② ★★ 显式账号 0 = **unset**，不是「什么都不加」。Phase G 审计抓出的静默串号：
        //    本地拉起故意加载 rc，而 rc 里很可能有 `export CLAUDE_CONFIG_DIR=<默认账号>`
        //    ⇒ 「什么都不加」会落到别的号上，而弹窗上写着「不注入」。
        let base = build_local_posix_command(&LocalPsAction::New, None, Some(&LaunchAccount::Base))
            .unwrap();
        assert!(
            base.starts_with("unset CLAUDE_CONFIG_DIR; "),
            "账号 0 必须显式 unset：{base}"
        );
        assert!(base.ends_with(&a), "前缀不该改动命令本体");
        let base_ps =
            build_local_ps_command(&LocalPsAction::New, None, Some(&LaunchAccount::Base)).unwrap();
        assert!(
            base_ps.starts_with("$env:CLAUDE_CONFIG_DIR=$null; "),
            "PS 侧的账号 0 同样要显式清掉：{base_ps}"
        );

        // ③ 具名账号但 configDir 是空串 ⇒ **坏数据，报错**（空值 ≠ 未设）
        assert!(build_local_posix_command(&LocalPsAction::New, None, Some(&named(""))).is_err());
        assert!(build_local_posix_command(&LocalPsAction::New, None, Some(&named("   "))).is_err());
    }

    #[test]
    fn account_prefix_is_prepended_posix_and_ps() {
        let dir = "/home/u/.claude-accts/z";
        let px = build_local_posix_command(&LocalPsAction::New, None, Some(&named(dir))).unwrap();
        assert!(
            px.starts_with(&format!("export CLAUDE_CONFIG_DIR='{dir}'; ")),
            "POSIX 前缀必须在最前面（要先于拉起命令生效）：{px}"
        );
        // 前缀之后仍是原来那条命令，逐字节
        let bare = build_local_posix_command(&LocalPsAction::New, None, None).unwrap();
        assert!(px.ends_with(&bare), "前缀不该改动命令本体");

        let ps = build_local_ps_command(&LocalPsAction::New, None, Some(&named(dir))).unwrap();
        assert!(
            ps.starts_with(&format!("$env:CLAUDE_CONFIG_DIR='{dir}'; ")),
            "{ps}"
        );
    }

    /// ★★ Phase G 审计抓出的真 bug：**Windows 上的账号目录必须被接受**。
    ///
    /// 原来 POSIX 与 PS 共用一条「必须 `/` 开头 + 禁 `\`」的校验 ⇒
    /// `C:\Users\z\.claude-accts\z` 恒被拒 ⇒ 「本机分叉时选一个具名账号」在**主平台**上
    /// 100% 失败（`fork-flow.ts` 是全仓唯一给 `resume_history_session` 传 `configDir` 的
    /// 调用点，所以这个洞是分叉专属的、别处测不到）。
    ///
    /// 判据照抄 `local_accounts::looks_absolute` —— 那个函数的头注已经写明这一课。
    /// 而**旧测试全喂 POSIX 路径**（`/home/u/.claude-accts/z`），所以它们测不出来。
    #[test]
    fn windows_account_dirs_are_accepted_by_the_ps_side() {
        for d in [
            "C:\\Users\\z\\.claude-accts\\z",
            "D:/Users/z/.claude-accts/b",
            "\\\\server\\share\\accts\\z",
        ] {
            assert!(
                build_local_ps_command(&LocalPsAction::New, None, Some(&named(d))).is_ok(),
                "Windows 账号目录被拒了：{d:?}"
            );
        }
        // POSIX 那条**仍然**只收 POSIX 路径（各自平台各自判据，别互相放宽）
        assert!(
            build_local_posix_command(&LocalPsAction::New, None, Some(&named("C:\\Users\\z")))
                .is_err(),
            "POSIX 侧不该接受 Windows 路径"
        );
        // 反斜杠形态的 `..` 也要挡住
        for d in ["C:\\Users\\..\\evil", "C:\\Users\\z\\.."] {
            assert!(
                build_local_ps_command(&LocalPsAction::New, None, Some(&named(d))).is_err(),
                "PS 侧漏了反斜杠 `..`：{d:?}"
            );
        }
        // 引号仍然禁（单引号能提前闭合 PS 的字面量串）
        assert!(
            build_local_ps_command(&LocalPsAction::New, None, Some(&named("C:\\a'; rm x; '")))
                .is_err()
        );
    }

    /// ★ 非法 configDir **绝不拼进命令** —— 这条产物会进 shell，宽容一格就是注入面。
    /// 判据照抄 TS 侧 `isValidConfigDir`（`src/shell-quote.ts:41`），不重新发明。
    #[test]
    fn illegal_config_dir_is_refused_not_sanitized() {
        let bad = [
            "relative/path",              // 非绝对
            "/",                          // 根
            "/home/u/../../etc",          // 含 /../
            "/home/u/..",                 // 以 /.. 结尾
            "/home/u'; rm -rf /; echo '", // 单引号闭合 + 注入
            "/home/u`whoami`",            // 反引号
            "/home/u$HOME",               // 变量展开
            "/home/u;id",                 // 分号
            "/home/u|id",                 // 管道
            "/home/u\n/x",                // 控制符（字面反斜杠 n 不算，见下面真控制符）
            "/home/u\u{0000}x",
            "/home/u\u{200b}x", // 零宽
            "/home/u\u{feff}x", // BOM
            "/home/u\u{00a0}x", // NBSP
        ];
        for d in bad {
            assert!(
                build_local_posix_command(&LocalPsAction::New, None, Some(&named(d))).is_err(),
                "非法 configDir 竟被接受：{d:?}"
            );
            assert!(
                build_local_ps_command(&LocalPsAction::New, None, Some(&named(d))).is_err(),
                "非法 configDir 竟被接受（PS）：{d:?}"
            );
        }
        // 反向自检：正常路径必须通过，否则上面全是空转
        assert!(build_local_posix_command(
            &LocalPsAction::New,
            None,
            Some(&named("/home/u/.claude-accts/z"))
        )
        .is_ok());
    }

    /// ★ U8c-1：POSIX 校验改调内核（P4b 起 `backend::control::payload`）之后**多拒**的那六段码位。
    ///
    /// 这不是纯重构 —— 本文件原先用的 `SPOOFABLE` 是 **U7-3 之前**的旧集合，
    /// 而内核建立在 `acct_core::is_deceptive_char` 的并集上。这条测试点名那六段，
    /// 变异（把内核换回旧表）时会逐个报出来。
    ///
    /// ⚠ **PS 那条路刻意还用旧表**（Windows 平台特化，见 `SPOOFABLE` 头注），
    /// 所以这里只断言 POSIX 侧 —— 断言 PS 侧会当场红，那才是假装做完了。
    #[test]
    fn posix_config_dir_now_rejects_the_code_points_u7_3_added() {
        for (name, c) in [
            ("U+1680 Ogham space", '\u{1680}'),
            ("U+2000 en quad", '\u{2000}'),
            ("U+200A hair space", '\u{200a}'),
            ("U+202F narrow NBSP", '\u{202f}'),
            ("U+205F medium math space", '\u{205f}'),
            ("U+2060 word joiner", '\u{2060}'),
            ("U+3000 ideographic space", '\u{3000}'),
        ] {
            let dir = format!("/home/u/.claude-accts/{c}z");
            assert!(
                build_local_posix_command(&LocalPsAction::New, None, Some(&named(&dir))).is_err(),
                "{name} 应被 POSIX 侧拒掉（acct-core 并集里有它，history.rs 旧表没有）"
            );
        }
    }

    /// ★ U8c-1：合法输入的产物**逐字节不变** —— 搬内核不许改一个字节。
    #[test]
    fn posix_account_prefix_is_byte_identical_after_moving_to_the_kernel() {
        assert_eq!(
            config_dir_prefix_posix(None).unwrap(),
            "",
            "参数缺席仍是空串（既有调用点逐字节等价旧行为）"
        );
        assert_eq!(
            config_dir_prefix_posix(Some(&LaunchAccount::Base)).unwrap(),
            "unset CLAUDE_CONFIG_DIR; ",
            "账号 0 的逐字节形态被 e2e 探针 grep 着"
        );
        assert_eq!(
            config_dir_prefix_posix(Some(&named("/home/u/.claude-accts/z"))).unwrap(),
            "export CLAUDE_CONFIG_DIR='/home/u/.claude-accts/z'; "
        );
    }

    #[test]
    fn codex_projects_group_by_cwd() {
        let sessions = vec![
            CodexSessionInfo {
                sid: "s1".into(),
                path: PathBuf::from("/a"),
                cwd: "/home/u/proj".into(),
                mtime_ms: 100,
            },
            CodexSessionInfo {
                sid: "s2".into(),
                path: PathBuf::from("/b"),
                cwd: "/home/u/proj".into(),
                mtime_ms: 300,
            },
            CodexSessionInfo {
                sid: "s3".into(),
                path: PathBuf::from("/c"),
                cwd: "".into(),
                mtime_ms: 50,
            },
        ];
        let projects = codex_projects_from(sessions);
        assert_eq!(projects.len(), 2, "两个 cwd 组");
        let proj = projects
            .iter()
            .find(|p| p.project_path == "/home/u/proj")
            .expect("proj 组");
        assert_eq!(proj.session_count, 2);
        assert_eq!(proj.last_activity, 300, "组内 max mtime");
        assert_eq!(proj.project_name, "proj", "cwd 末段");
        assert_eq!(proj.project_dir, "codex:/home/u/proj", "键带 codex: 前缀");
        assert!(!proj.has_live, "Codex 判活=F4，F1a 先 false");
        let unknown = projects
            .iter()
            .find(|p| p.project_path.is_empty())
            .expect("空 cwd 组");
        assert_eq!(unknown.project_name, "(codex)");
        assert_eq!(unknown.project_dir, "codex:");
    }

    /// F1a-3c + Phase G 审计修：Codex 会话摘要取首个**真** user message，跳 CLI 注入块——
    /// 复用渲染路同一 `is_injected_context`（**3 标记**：environment_context / recommended_plugins /
    /// # AGENTS.md instructions），与渲染去噪一致（此前只跳 environment_context）。
    #[test]
    fn codex_first_user_excerpt_skips_injected_context() {
        let dir = std::env::temp_dir().join(format!("ccm-codex-exc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("rollout.jsonl");
        let user = |t: &str| {
            format!(
                r#"{{"type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":{}}}]}}}}"#,
                serde_json::to_string(t).unwrap()
            )
        };
        // 首 3 条 user = 3 种注入块（全跳）；末 user = 真用户输入（取）。
        let content = [
            r#"{"type":"session_meta","payload":{"cwd":"/p"}}"#.to_string(),
            user("<environment_context>injected</environment_context>"),
            user("<recommended_plugins>\nplugins…"),
            user("# AGENTS.md instructions\n\n<INSTRUCTIONS>\n# AGENTS.md\n本文件…"),
            user("真实问题"),
        ]
        .join("\n");
        std::fs::write(&f, content).unwrap();
        assert_eq!(codex_first_user_excerpt(&f), "真实问题");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// ★★ **建分支入口真的过了围栏吗**〔audit-0805 08-07，Phase G 第 48 件〕。
    ///
    /// 与删除那条**同一族的第二例**。`validate_branch_source` 有两条穿越防护判据
    /// （`..` 穿越 · 软链逃逸），都是实的，但主语同样是**围栏本身**。
    /// 08-07 实测：把 `branch_impl` 里那行换成 `PathBuf::from(source_jsonl_path)`，
    /// **全仓 979 条判据一条不红** —— 而那条路会去**读**调用方给的任意文件，
    /// 再把内容拷进 `projects` 目录（该函数头注自陈「安全承诺全在这层」）。
    ///
    /// ⇒ 一族两例，说明这不是某个人某次疏忽：**「围栏有判据」与「那条路过了围栏」
    /// 是两件事，而写判据的注意力天然落在前者**（后者要跑真路，前者只要调个函数）。
    ///
    /// 本条比删除那条更干净：`branch_impl` 可注入 `projects_dir` ⇒ 临时目录**同时**
    /// 充当「projects」与「界外」，一个字节都不碰用户的目录。
    #[test]
    fn the_branch_entry_point_actually_goes_through_the_fence() {
        let base = std::env::temp_dir().join(format!(
            "ccm-branch-fence-probe-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let projects = base.join("projects");
        std::fs::create_dir_all(&projects).expect("建临时 projects");
        // 源文件放在 projects **之外**：围栏在的话必须拒。
        let outsider = base.join("outsider.jsonl");
        std::fs::write(&outsider, "{\"type\":\"user\"}\n").expect("造界外源文件");

        let r = branch_impl(&outsider.to_string_lossy(), "uuid-x", &projects);
        let _ = std::fs::remove_dir_all(&base);

        let err = r.err().unwrap_or_else(|| {
            panic!(
                "`branch_impl` 接受了一个 **`projects` 之外**的源路径 —— 围栏没接上。\n\
                 那条路会去读调用方给的任意文件，再把内容拷进 projects 目录。"
            )
        });
        // 红要红对成因：必须是**围栏**拒的，不是后面某步偶然失败。
        assert!(
            err.contains("refuse branch"),
            "拒绝了，但不是围栏拒的（错误：{err}）—— \
             本条没真跑到围栏那一步，等于空转。"
        );
    }

    /// 下面那条判据的**子进程哨兵**。
    ///
    /// ⚠ 名字是本判据**专属的假变量**（同 `lib.rs` 里 `env_scrub_tests` 那条纪律）——
    /// 真正的 `CLAUDE_CONFIG_DIR` 只经 `Command::env` 给**子进程**，
    /// 本进程与宿主的环境都没有被动过。
    const FENCE_CHILD: &str = "CCM_TEST_DELETE_FENCE_CHILD";

    /// ★★ **删除入口真的过了围栏吗**〔audit-0805 08-07，Phase G 第 47 件〕。
    ///
    /// 下面五条穿越防护判的都是 `validate_delete_target` **这个函数本身**。它们是实的，
    /// 但它们的主语是**围栏**，不是「那条路真的过了围栏」——
    /// 08-07 实测：把 `delete_history_session` 里那行换成
    /// `let target = PathBuf::from(&jsonl_path);`（整个跳过围栏），
    /// **全仓 978 条判据一条不红**，而那条路是 `fs::remove_file`：
    /// 前端传什么就删什么，用户机器上任意文件。
    ///
    /// ⇒ 与 F27（`history_query` 那两份围栏）同族，也是 F+ 第二问反复报的那个形状：
    /// **纯函数层钉满、接线层为零**。
    ///
    /// # 为什么做成端到端而不是扫源码
    ///
    /// 扫「函数体里有没有 `validate_delete_target(`」只是**代理**（上一件刚记过这条）。
    /// 这里能直接跑真路：造一个**在 `projects` 之外**的真临时文件，要求入口拒绝**且文件还在**。
    /// 围栏一旦被绕过，这条会把那个临时文件真删掉 —— 于是「文件还在」这半当场红。
    /// ⚠ 只碰自己造的临时目录；`~/.claude/` 一个字节都不写（红线）。
    ///
    /// # 🔴 09-10：**本条自己造出它要的前提** —— 而且是在**子进程**里
    ///
    /// 上一版依赖「这台机器上 `~/.claude/projects` 存在」。**那不是它要验的性质，
    /// 是它没建立的前提**：`resolve_claude_dir()` 的第三级回落 `~/.claude`
    /// **不检查存在性**，于是在一台干净机器上入口会在**围栏之前**就 `Err`，
    /// 而「拒了」「文件还在」两格照样绿 —— 09-09 云端首跑红的正是最后那格，
    /// 它报的是「围栏没接上 / 措辞改了」，**两条都是假话**。
    ///
    /// ## 为什么**不**在本进程里 `set_var("CLAUDE_CONFIG_DIR", …)`
    ///
    /// 本仓有一条写下来的纪律，逐字在 `lib.rs` 的 `env_scrub_tests` 里：
    /// 「cargo test 多线程跑，进程级 env 是共享的，**绝不能在测试里 set/remove
    /// 真实的 `CLAUDE_*` 变量**（会干扰并发测试与宿主环境）」。
    /// [`RelayFactSources`] 头注 ㈠ 那一栏记着同族的第二条代价：这种判据
    /// 「必须 `--test-threads=1` ⇒ 只能住 `#[ignore]` 的 e2e 那条道」——
    /// 而那等于本条在 CI 上根本不跑。⇒ 两条路都堵死。
    ///
    /// ## 落法：把那一趟整个搬进子进程
    ///
    /// 父进程造一份**自己的** claude 目录，只经 `Command::env` 交给子进程
    ///（子进程在起来那一刻就带着它，**谁的进程环境都没有被改过**），
    /// 再拿本判据自己的可执行文件、以本判据的名字当过滤器跑一趟。
    /// ⇒ 本进程环境一个字节没动 · 并发判据一格没被干扰 · Linux / Windows 上都跑得动。
    ///
    /// ⚠ **反空真**：过滤器一条都没命中时 libtest 的退出码**也是 0**（「0 passed」）——
    /// 那会是一次干净的假绿。所以父进程除了看退出码，还断子进程真的报了 `1 passed`。
    #[test]
    fn the_delete_entry_point_actually_goes_through_the_fence() {
        // ═══ 父进程那一半：造夹具 · 起子进程 · 把子进程的正文转发出来，然后 `return` ═══
        //
        // ⚠ 两半**刻意写在同一个 `#[test]` 里**〔09-10 第二拍〕。
        //   上一版把子进程那一半拆成了一个单独的函数，被 `structural_scan` 里那条
        //   「测试段里长得像判据、却没有 `#[test]`」的机检判红 ——
        //   **那条红是对的，不是误报**：它认的是「**无参无返回**的 `fn 名()`」这个**形状**
        //   （判别式看的是行首那个 `fn ` 与行尾那个 `() {`，**不看名字**
        //   ⇒ 改名闭不了它的嘴），而那正是判据的形状 ——
        //   读的人无从知道那一大段断言到底跑不跑。
        //
        // 🔴 处置**不是**给它随手加一个用不上的参数（或返回值）把判别式糊过去：
        //   那是钻空子，而且**一个字都没治那个真问题** —— 读者照旧分不出它跑不跑。
        // ⇒ 搬回**唯一那个 `#[test]`** 里。读者看见一个 `#[test]` 与一个 `return`，
        //   就知道下面那一半在哪一趟跑；这个文件的测试段里再没有「长得像判据却不是判据」的东西。
        if std::env::var_os(FENCE_CHILD).is_none() {
            let name = format!("ccm-delete-fence-home-{}", std::process::id());
            let claude_dir = std::env::temp_dir().join(name);
            // 造的是**空的**记录目录 —— 围栏只 `canonicalize` 它，不读里面的东西。
            // ⚠ 目录名走生产那一份 `records_dir`，**不在这里另抄一个 `"projects"`**：
            //   本条要的是「入口会去 canonicalize 的那个目录真的在」，而它叫什么名字
            //   归活跃适配器管 —— 抄一份就会漂。
            let records = crate::adapter::records_dir(&claude_dir);
            std::fs::create_dir_all(&records).expect("造 claude 目录夹具失败");

            let exe = std::env::current_exe().expect("拿不到本判据自己的可执行文件");
            let out = std::process::Command::new(&exe)
                .arg("the_delete_entry_point_actually_goes_through_the_fence")
                .arg("--nocapture")
                .arg("--test-threads=1")
                .env(FENCE_CHILD, "1")
                .env("CLAUDE_CONFIG_DIR", &claude_dir)
                .output()
                .expect("起不来子进程 —— 本条判不了，不许当成绿");
            let so = String::from_utf8_lossy(&out.stdout).into_owned();
            let se = String::from_utf8_lossy(&out.stderr).into_owned();
            let _ = std::fs::remove_dir_all(&claude_dir);

            assert!(
                out.status.success(),
                "子进程里那一趟红了（退出码 {:?}）—— 正文在下面，别只看这一行。\n\
                 ── 子进程 stdout ──\n{so}\n── 子进程 stderr ──\n{se}",
                out.status.code()
            );
            // ★ 反空真：过滤器零命中时 libtest 报 `ok. 0 passed;` 而**退出码也是 0**。
            //   ⚠ 针带上 `ok. ` 与 `;` 两侧边界：裸 `"1 passed"` 会被 `11 passed` 顺带满足。
            assert!(
                so.contains("ok. 1 passed;"),
                "子进程没有恰好跑到本判据那一趟（过滤器命中数不是 1）—— 本条会假绿。\n\
                 ── 子进程 stdout ──\n{so}\n── 子进程 stderr ──\n{se}"
            );
            return;
        }

        // ═══ 子进程那一半：真正那一趟（`CLAUDE_CONFIG_DIR` 已经在环境里）═══
        //
        // 前置条件仍然留着当兜底〔09-09 补的那一格，别删〕：注入万一没生效，
        // 本条要说人话，而不是把「前提没建立」报成「围栏没接上」。
        // 唯一会让它没生效的路：这台机器的 monitor config.json 里写了 `claudeDir`
        // 且那个目录真在 —— 它在 `resolve_claude_dir` 里**优先于**环境变量。
        let Some(claude_dir) = paths::resolve_claude_dir() else {
            panic!("解析不出 claude 目录 —— 本条判不了")
        };
        let projects_dir = crate::adapter::records_dir(&claude_dir);
        let canon_projects = projects_dir.canonicalize().unwrap_or_else(|e| {
            panic!(
                "本条的前置条件不成立：{} 打不开（{e}）——\n\
                 入口会在**围栏之前**就失败，那时本条判的根本不是围栏。\n\
                 ⇒ 父进程已经把 `CLAUDE_CONFIG_DIR` 指向一份自己造好的目录；\
                 拿到别的说明这台机器的 monitor config.json 里写了 `claudeDir`\n\
                 （它在 `resolve_claude_dir` 里优先于环境变量）。\n\
                 🔴 不许把本条改成「读不到就跳过」—— 那是把闸拆了。",
                projects_dir.display()
            )
        });

        let dir = std::env::temp_dir().join(format!(
            "ccm-delete-fence-probe-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let victim = dir.join("victim.jsonl");
        std::fs::write(&victim, "not yours").expect("造临时文件");

        // ★ **反空真**〔09-10 补〕：本条全部的力气都押在「靶子在记录目录**之外**」上。
        //   靶子要是落在里面，围栏**放行**才是对的，而下面那两格会把放行读成缺陷。
        //   先前这一格是**假设**的（「临时目录当然不在 `~/.claude` 里」）——现在现算一次。
        let canon_victim = victim.canonicalize().expect("靶子打不开");
        let outside = !canon_victim.starts_with(&canon_projects);

        let r = delete_history_session("sid".into(), victim.to_string_lossy().into_owned());
        let still_there = victim.exists();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            outside,
            "靶子 {} 落在了记录目录 {} **里面** —— 本条的前提不成立，\
             围栏在这一格**放行**才是对的。",
            canon_victim.display(),
            canon_projects.display()
        );
        let err = r.expect_err(
            "`delete_history_session` 接受了一个 **`projects` 之外**的路径 —— \
             围栏没接上，前端传什么就删什么。",
        );
        assert!(
            still_there,
            "那个临时文件**真被删了**（错误：{err}）—— 围栏被绕过，\
             `fs::remove_file` 直接落在了调用方给的路径上。"
        );
        // ★ 红要红对成因：必须是**围栏**拒的，不能是「claude dir not found」之类前置失败，
        //   否则本条会在一个根本没跑到围栏的环境里假绿。
        // ⚠ 08-07 收紧：原写 `contains("refuse delete") || contains("outside")`。
        // 删除这一侧的 `refuse delete:` 前缀**只有围栏在用**（全文件两处，都在围栏里）
        // ⇒ 本条当时没问题。但**建分支那条同形判据栽在这上面**：那边的
        // `refuse branch:` 前缀下游还有四处，跳过围栏之后下游照样报一条同前缀的错，
        // 判据在它自己要抓的那一刀上是绿的。⇒ 这里一并收紧成围栏**独有**的措辞。
        assert!(
            err.contains("is outside"),
            "拒绝了，但不是**围栏的越界检查**拒的（错误：{err}）—— \
             要么围栏没接上而下游某步偶然报了错（两者长得一样，只有这句话分得开），\
             要么围栏的措辞改了而本条没跟。"
        );
    }

    // === Batch4-F15：validate_delete_target 穿越防护 ===

    /// 独立临时 projects 目录（惯例同 utils.rs / watcher.rs 测试）。
    fn temp_projects(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("ccm-hist-del-{}-{}", tag, std::process::id()))
            .join("projects");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn delete_rejects_dotdot_traversal() {
        let projects = temp_projects("dotdot");
        let root = projects.parent().unwrap();
        // projects 外造一个真实存在的 .jsonl，再用 `..` 从 projects 内指出去
        let outside = root.join("outside.jsonl");
        std::fs::write(&outside, "{}\n").unwrap();
        let sneaky = projects.join("..").join("outside.jsonl");
        let err = validate_delete_target(sneaky.to_str().unwrap(), &projects).unwrap_err();
        assert!(err.contains("refuse delete"), "got: {err}");
        assert!(outside.exists(), "file must survive the refused delete");
        std::fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn delete_rejects_symlink_escaping_projects() {
        let projects = temp_projects("symlink");
        let root = projects.parent().unwrap();
        let outside = root.join("secret.jsonl");
        std::fs::write(&outside, "{}\n").unwrap();
        let link = projects.join("innocent.jsonl");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let err = validate_delete_target(link.to_str().unwrap(), &projects).unwrap_err();
        assert!(err.contains("refuse delete"), "got: {err}");
        assert!(outside.exists());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn delete_accepts_normal_jsonl_inside_projects() {
        let projects = temp_projects("ok");
        let proj = projects.join("some-project");
        std::fs::create_dir_all(&proj).unwrap();
        let f = proj.join("abc-123.jsonl");
        std::fs::write(&f, "{}\n").unwrap();
        let canon = validate_delete_target(f.to_str().unwrap(), &projects).unwrap();
        assert!(canon.ends_with("abc-123.jsonl"));
        // 命令壳用返回的 canonical 路径删——等价验证
        std::fs::remove_file(&canon).unwrap();
        assert!(!f.exists());
        std::fs::remove_dir_all(projects.parent().unwrap()).ok();
    }

    #[test]
    fn delete_rejects_non_jsonl_and_missing() {
        let projects = temp_projects("misc");
        // 不存在
        let missing = projects.join("nope.jsonl");
        let err = validate_delete_target(missing.to_str().unwrap(), &projects).unwrap_err();
        assert!(err.contains("does not exist"), "got: {err}");
        // 存在但非 .jsonl
        let txt = projects.join("note.txt");
        std::fs::write(&txt, "x").unwrap();
        let err2 = validate_delete_target(txt.to_str().unwrap(), &projects).unwrap_err();
        assert!(err2.contains("not a .jsonl"), "got: {err2}");
        std::fs::remove_dir_all(projects.parent().unwrap()).ok();
    }

    // === F62：create_branch_session 守卫 + 原生分支格式 ===

    #[test]
    fn branch_source_guard_rejects_dotdot_traversal() {
        let projects = temp_projects("branch-dotdot");
        let root = projects.parent().unwrap();
        let outside = root.join("outside.jsonl");
        std::fs::write(&outside, "{}\n").unwrap();
        let sneaky = projects.join("..").join("outside.jsonl");
        let err = validate_branch_source(sneaky.to_str().unwrap(), &projects).unwrap_err();
        assert!(err.contains("refuse branch"), "got: {err}");
        assert!(outside.exists());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn branch_result_camel_case_contract() {
        let r = BranchResult {
            session_id: "new-sid".into(),
            jsonl_path: "/p/new-sid.jsonl".into(),
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"sessionId\""), "缺 sessionId: {j}");
        assert!(j.contains("\"jsonlPath\""), "缺 jsonlPath: {j}");
    }

    #[test]
    fn write_branch_file_refuses_existing_target() {
        let dir = temp_projects("branch-write");
        // create_new：目标已存在 → Err，且既存内容零改动（自证「绝不覆盖」）
        let f = dir.join("x.jsonl");
        std::fs::write(&f, "PRE").unwrap();
        let err = write_branch_file(&f, &[serde_json::json!({"a":1})]).unwrap_err();
        assert!(err.contains("already exists"), "got: {err}");
        assert_eq!(
            std::fs::read_to_string(&f).unwrap(),
            "PRE",
            "既存文件被覆盖了"
        );
        // 正常写新文件
        let f2 = dir.join("y.jsonl");
        write_branch_file(&f2, &[serde_json::json!({"a":1})]).unwrap();
        assert_eq!(std::fs::read_to_string(&f2).unwrap(), "{\"a\":1}\n");
        std::fs::remove_dir_all(dir.parent().unwrap()).ok();
    }

    #[cfg(unix)]
    #[test]
    fn branch_source_guard_rejects_symlink_escape() {
        let projects = temp_projects("branch-symlink");
        let root = projects.parent().unwrap();
        let outside = root.join("secret.jsonl");
        std::fs::write(&outside, "{}\n").unwrap();
        let link = projects.join("innocent.jsonl");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let err = validate_branch_source(link.to_str().unwrap(), &projects).unwrap_err();
        assert!(err.contains("refuse branch"), "got: {err}");
        assert!(outside.exists());
        std::fs::remove_dir_all(root).ok();
    }

    /// G1：这条 IO 壳测试要的只是「一段能分叉的会话」。
    /// 纯变换的夹具已随函数搬去 `branch-core`（那里有真正区分算法的
    /// `native_shape_session`）；本地留一份**最小**的，免得为了一个 IO 测试
    /// 把测试夹具也做成跨 crate 的公开面。
    fn io_sample_session() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({"type":"user","uuid":"u1","parentUuid":null,"timestamp":"t1","sessionId":"SRC","message":{"role":"user","content":"q1"}}),
            serde_json::json!({"type":"assistant","uuid":"u2","parentUuid":"u1","timestamp":"t2","sessionId":"SRC","message":{"role":"assistant","content":"a1"}}),
            serde_json::json!({"type":"system","uuid":"u3","parentUuid":"u2","timestamp":"t3","sessionId":"SRC"}),
            serde_json::json!({"type":"user","uuid":"u4","parentUuid":"u3","timestamp":"t4","sessionId":"SRC","message":{"role":"user","content":"q2"}}),
            serde_json::json!({"type":"assistant","uuid":"u5","parentUuid":"u4","timestamp":"t5","sessionId":"SRC","message":{"role":"assistant","content":"a2"}}),
        ]
    }

    /// 重要（D 审计）：安全关键的写盘壳直测——源零改动 + 新文件原生格式正确。
    /// 注入 tempdir projects 绕开 resolve_claude_dir（同 delete 测法）。
    #[test]
    fn branch_impl_leaves_source_untouched_and_writes_native_branch() {
        let projects = temp_projects("branch-impl");
        let proj = projects.join("proj-x");
        std::fs::create_dir_all(&proj).unwrap();
        let src = proj.join("srcsid.jsonl");
        let mut body = String::new();
        for r in &io_sample_session() {
            body.push_str(&serde_json::to_string(r).unwrap());
            body.push('\n');
        }
        std::fs::write(&src, &body).unwrap();
        let before = std::fs::read(&src).unwrap();

        let res = branch_impl(src.to_str().unwrap(), "u4", &projects).unwrap();

        // 源一字节不改
        assert_eq!(std::fs::read(&src).unwrap(), before, "源文件被改动了");
        // 新文件在源同目录、文件名=新 sid
        let out = PathBuf::from(&res.jsonl_path);
        // branch_impl 经 validate_branch_source canonicalize 源路径（安全守卫）——
        // Windows 上会解 8.3 短名(RUNNER~1→runneradmin)并加 `\\?\` 前缀,故 out.parent()
        // 已是规范形,而 proj 来自 temp_dir() 原样路径。两边都 canonicalize 再比,消除
        // 平台差异(否则 Windows CI 上 `\\?\…runneradmin…` != `…RUNNER~1…` 恒红)。
        assert_eq!(
            std::fs::canonicalize(out.parent().unwrap()).unwrap(),
            std::fs::canonicalize(&proj).unwrap(),
        );
        assert_eq!(out.file_stem().unwrap().to_str().unwrap(), res.session_id);
        // 内容 = 原生分支格式（祖先链 + 新 sid + forkedFrom{srcsid@自身}）
        let out_rows: Vec<serde_json::Value> = std::fs::read_to_string(&out)
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let uuids: Vec<&str> = out_rows
            .iter()
            .map(|r| r.get("uuid").unwrap().as_str().unwrap())
            .collect();
        assert_eq!(uuids, vec!["u1", "u2", "u3", "u4"]);
        for r in &out_rows {
            assert_eq!(
                r.get("sessionId").unwrap().as_str().unwrap(),
                res.session_id
            );
            assert_eq!(
                r.get("forkedFrom").unwrap().get("sessionId").unwrap(),
                "srcsid"
            );
        }
        std::fs::remove_dir_all(projects.parent().unwrap()).ok();
    }

    /// P1.2 contract test：守护后端 wire 跟前端 TS interface 字段名一致。
    /// 改字段名必须同步改前端 views/history.ts 的 HistoryProject / HistorySessionEntry interface。
    /// 若本测试失败 = 后端 wire 漂移；若 tsc 编译错 = 前端 access 漂移。两边都受保护。
    #[test]
    fn history_project_camel_case_contract() {
        let p = HistoryProject {
            project_path: "/x/y".into(),
            project_name: "y".into(),
            project_dir: "/y-encoded".into(),
            session_count: 1,
            starred_count: 2,
            hidden_count: 3,
            last_activity: 1700_000_000_000,
            has_live: true,
            origin: Some("pi-host".into()), // issue #16：远端来源也走同一 wire 契约
        };
        let j = serde_json::to_string(&p).unwrap();
        for camel_key in [
            "\"projectPath\"",
            "\"projectName\"",
            "\"projectDir\"",
            "\"sessionCount\"",
            "\"starredCount\"",
            "\"hiddenCount\"",
            "\"lastActivity\"",
            "\"hasLive\"",
        ] {
            assert!(
                j.contains(camel_key),
                "HistoryProject wire 缺 {camel_key}: {j}"
            );
        }
        // 反例守护：不应出现任何 snake_case 字段
        for snake_key in [
            "\"project_path\"",
            "\"project_name\"",
            "\"project_dir\"",
            "\"session_count\"",
            "\"starred_count\"",
            "\"hidden_count\"",
            "\"last_activity\"",
            "\"has_live\"",
        ] {
            assert!(
                !j.contains(snake_key),
                "HistoryProject 漏改 {snake_key}: {j}"
            );
        }
    }

    #[test]
    fn history_session_entry_camel_case_contract() {
        let e = HistorySessionEntry {
            session_id: "s-1".into(),
            project_path: "/x".into(),
            project_name: "x".into(),
            ai_title: Some("t".into()),
            first_user_excerpt: "hi".into(),
            started_at: 1,
            updated_at: 2,
            jsonl_path: "/a.jsonl".into(),
            is_live: true,
            message_count_approx: 5,
            is_bg: true,
            starred: false,
            custom_title: None,
            hidden: false,
            forked_from_session_id: Some("p-1".into()),
            forked_from_message_uuid: Some("u-1".into()),
            origin: Some("pi-host".into()),
        };
        let j = serde_json::to_string(&e).unwrap();
        for camel_key in [
            "\"sessionId\"",
            "\"isBg\"",
            "\"projectPath\"",
            "\"projectName\"",
            "\"aiTitle\"",
            "\"firstUserExcerpt\"",
            "\"startedAt\"",
            "\"updatedAt\"",
            "\"jsonlPath\"",
            "\"isLive\"",
            "\"messageCountApprox\"",
            "\"customTitle\"",
            "\"forkedFromSessionId\"",
            "\"forkedFromMessageUuid\"",
        ] {
            assert!(
                j.contains(camel_key),
                "HistorySessionEntry wire 缺 {camel_key}: {j}"
            );
        }
        for snake_key in [
            "\"session_id\"",
            "\"project_path\"",
            "\"first_user_excerpt\"",
            "\"is_live\"",
            "\"forked_from_session_id\"",
        ] {
            assert!(
                !j.contains(snake_key),
                "HistorySessionEntry 漏改 {snake_key}: {j}"
            );
        }
    }

    /// A4：EntryMetadata / MetadataPatch 的 lastAccount serde 契约 + 向后兼容 + 三态 patch。
    #[test]
    fn last_account_serde_and_patch_semantics() {
        // 1) 向后兼容：旧文件无 lastAccount 字段 → None，不报错。
        let old: EntryMetadata =
            serde_json::from_str(r#"{"starred":true,"hidden":false,"updatedAt":9}"#).unwrap();
        assert_eq!(old.last_account, None);

        // 2) camelCase wire：Some(name) 序列化含 "lastAccount"、不含 snake。
        let e = EntryMetadata {
            last_account: Some("z".into()),
            ..Default::default()
        };
        let j = serde_json::to_string(&e).unwrap();
        assert!(j.contains("\"lastAccount\""), "wire 缺 lastAccount: {j}");
        assert!(!j.contains("last_account"), "wire 不该含 snake: {j}");

        // 2b) 旧 snake alias 仍可读入（迁移容错）。
        let via_alias: EntryMetadata = serde_json::from_str(r#"{"last_account":"b"}"#).unwrap();
        assert_eq!(via_alias.last_account, Some("b".into()));

        // 3) MetadataPatch：缺键 / null 都折叠为 None(不改)——与既有 customTitle 同(plain
        //    serde default，非 double_option)；清空经"空串 → filter"实现(见 4))，不靠 null。
        let none: MetadataPatch = serde_json::from_str("{}").unwrap();
        assert_eq!(none.last_account, None);
        let via_null: MetadataPatch = serde_json::from_str(r#"{"lastAccount":null}"#).unwrap();
        assert_eq!(via_null.last_account, None);
        let set: MetadataPatch = serde_json::from_str(r#"{"lastAccount":"z"}"#).unwrap();
        assert_eq!(set.last_account, Some(Some("z".into())));

        // 4) apply 语义（镜像 update_history_metadata 分支）：空白账号名按清空处理。
        fn apply(mut e: EntryMetadata, json: &str) -> EntryMetadata {
            let p: MetadataPatch = serde_json::from_str(json).unwrap();
            if let Some(a) = p.last_account {
                e.last_account = a.filter(|s| !s.trim().is_empty());
            }
            e
        }
        let base = EntryMetadata {
            last_account: Some("z".into()),
            ..Default::default()
        };
        assert_eq!(
            apply(EntryMetadata::default(), r#"{"lastAccount":"z"}"#).last_account,
            Some("z".into())
        );
        assert_eq!(
            apply(base.clone(), r#"{"lastAccount":""}"#).last_account,
            None
        ); // 空串=清空
        assert_eq!(
            apply(base.clone(), r#"{"lastAccount":null}"#).last_account,
            Some("z".into()) // null 折叠为"不改"（同 customTitle）
        );
        assert_eq!(
            apply(base.clone(), r#"{"starred":true}"#).last_account,
            Some("z".into()) // 未提 lastAccount → 不改
        );
        assert_eq!(
            apply(EntryMetadata::default(), r#"{"lastAccount":"   "}"#).last_account,
            None // 纯空白 = 清空
        );
    }

    /// A4：list_last_accounts 的纯变换——只含有 lastAccount 的条目，None 的剔除。
    #[test]
    fn last_accounts_of_filters_none() {
        let mut entries = HashMap::new();
        entries.insert(
            "s-has".to_string(),
            EntryMetadata {
                last_account: Some("z".into()),
                ..Default::default()
            },
        );
        entries.insert("s-none".to_string(), EntryMetadata::default()); // 无 lastAccount
        let out = last_accounts_of(HistoryMetadata {
            version: 1,
            entries,
        });
        assert_eq!(out.get("s-has"), Some(&"z".to_string()));
        assert!(!out.contains_key("s-none"));
        assert_eq!(out.len(), 1);
    }

    // P3 归并：iso_parse_* 测试已搬到 utils::tests（函数本身搬到 utils）。

    #[test]
    fn truncate_chars_unicode() {
        let s = truncate_chars("你好世界abc", 3);
        assert_eq!(s, "你好世…");
    }

    #[test]
    fn truncate_chars_short() {
        let s = truncate_chars("hi", 10);
        assert_eq!(s, "hi");
    }

    #[test]
    fn truncate_chars_newline_replaced() {
        let s = truncate_chars("a\nb\nc", 10);
        assert_eq!(s, "a b c");
    }

    #[test]
    fn resume_cmd_prefers_cc_with_claude_fallback() {
        let sid = "01998f2a-1234-7abc-9def-0123456789ab";
        let cmd = build_resume_ps_command(sid, None).unwrap();
        // 优先 cc、回退 claude，两者都带正确 sid
        assert!(cmd.contains("Get-Command cc"));
        assert!(cmd.contains(&format!("cc --resume {sid}")));
        assert!(cmd.contains(&format!("claude --resume {sid}")));
    }

    #[test]
    fn resume_cmd_rejects_injection() {
        // 含 shell 元字符的 session_id 必须被拒（防命令注入）
        for bad in [
            "a; rm -rf /",
            "a && calc",
            "a`whoami`",
            "a$(id)",
            "a b",
            "a\"b",
            "",
            "a/../b",
        ] {
            assert!(
                build_resume_ps_command(bad, None).is_err(),
                "应拒绝危险 session_id: {bad:?}"
            );
        }
    }

    /// F34：自定义 launcher——合法形态放行、注入面拒绝、空视为未设置。
    #[test]
    fn sanitize_launcher_allows_simple_reject_injection() {
        assert_eq!(sanitize_launcher(None).unwrap(), None);
        assert_eq!(sanitize_launcher(Some("")).unwrap(), None);
        assert_eq!(sanitize_launcher(Some("   ")).unwrap(), None);
        assert_eq!(
            sanitize_launcher(Some("cct")).unwrap().as_deref(),
            Some("cct")
        );
        assert_eq!(
            sanitize_launcher(Some(" cc -p 8 ")).unwrap().as_deref(),
            Some("cc -p 8")
        );
        for bad in ["cc; calc", "cc|id", "cc$(id)", "cc`id`", "cc&&x", "cc\"x"] {
            assert!(sanitize_launcher(Some(bad)).is_err(), "应拒绝: {bad:?}");
        }
    }

    #[test]
    fn resume_cmd_custom_launcher_used_verbatim() {
        let sid = "abc-123";
        let cmd = build_resume_ps_command(sid, Some("cct")).unwrap();
        assert_eq!(cmd, "cct --resume abc-123");
        // 设了自定义命令就不再出现 cc 自动检测
        assert!(!cmd.contains("Get-Command"));
    }

    /// F96：本地起新会话命令——同 cc 优先/回退逻辑，但**不带 resume flag / sid**。
    #[test]
    fn new_session_cmd_prefers_cc_no_resume_flag() {
        let cmd = build_new_session_ps_command(None).unwrap();
        assert!(cmd.contains("Get-Command cc"));
        assert!(cmd.contains("{ cc }"), "cc 分支: {cmd}");
        assert!(cmd.contains("{ claude }"), "回退分支: {cmd}");
        // 起新会话不是 resume：绝不带 --resume / sid
        assert!(
            !cmd.contains("--resume"),
            "起新会话不应带 resume flag: {cmd}"
        );
    }

    #[test]
    fn new_session_cmd_custom_launcher_verbatim() {
        let cmd = build_new_session_ps_command(Some("cct")).unwrap();
        assert_eq!(cmd, "cct");
        assert!(!cmd.contains("Get-Command"));
        assert!(!cmd.contains("--resume"));
    }

    #[test]
    fn new_session_cmd_rejects_injection_launcher() {
        for bad in ["cc; calc", "cc|id", "cc$(id)", "cc`id`", "cc&&x"] {
            assert!(
                build_new_session_ps_command(Some(bad)).is_err(),
                "应拒绝注入 launcher: {bad:?}"
            );
        }
    }

    /// ★ L1：POSIX 渲染器的形状 —— 与 PowerShell 那条是**同一个决策**的另一种写法。
    ///
    /// `Get-Command` 的等价物是 `command -v`（它同样找得到 shell **函数**，
    /// 而 `ccm` 的 `cc` 集成正是一个函数；命令跑在 `bash -lic` 里、rc 已加载）。
    #[test]
    fn posix_renderer_mirrors_the_powershell_one() {
        let sid = "01998f2a-1234-7abc-9def-0123456789ab";
        assert_eq!(
            build_local_posix_command(&LocalPsAction::Resume(sid.to_string()), None, None).unwrap(),
            format!(
                "if command -v cc >/dev/null 2>&1; then cc --resume {sid}; \
                 else claude --resume {sid}; fi"
            )
        );
        assert_eq!(
            build_local_posix_command(&LocalPsAction::New, None, None).unwrap(),
            "if command -v cc >/dev/null 2>&1; then cc; else claude; fi"
        );
        // F34 自定义命令：两边都不做别名探测，**逐字节相同**（这一支没有平台差异）。
        for action in [LocalPsAction::New, LocalPsAction::Resume(sid.to_string())] {
            assert_eq!(
                build_local_posix_command(&action, Some("cct"), None).unwrap(),
                build_local_ps_command(&action, Some("cct"), None).unwrap(),
                "显式指定命令时两个渲染器不该有任何差异"
            );
        }
    }

    /// ★★ **P3t-Y0：Windows 那条路本件一个字不动 —— 而钉的是「该活下来的性质」，不是字节。**
    ///
    /// `C12` 逐字「windows不要tmux」。用内容哈希钉「一个字没改」看着更严，其实更坏：
    /// 将来任何一次正当的 Windows 改动都会让它假红，而假红久了就会被人加豁免 ——
    /// 那时它连性质都不守了。⇒ 钉性质：**Windows 本机的渲染器自己永远不产会话容器**。
    ///
    /// ⚠ `launcher` 必须传 `None`。用户显式指定 `cct`（F34）时输出里当然会有 `cct`，
    /// 那是**用户自己要的**，不是本工具替他加的 —— `posix_renderer_mirrors_the_powershell_one`
    /// 正是拿 `Some("cct")` 在对拍。人群划错这一格，本条会变成「禁止用户用 cct」。
    ///
    /// # 射程（`reach`）
    ///
    /// 够得到：Windows 渲染器的**输出里没有容器**。
    /// **够不到**：Windows 那条路今天还跑不跑得起来 —— 没有 Windows 机器，
    /// 「没改」证明不了「还能跑」。那一格归 `auto-e2e`（本件 §4 已登记）。
    #[test]
    fn the_windows_local_path_never_grows_a_session_container() {
        let sid = "01998f2a-1234-7abc-9def-0123456789ab";
        let named = LaunchAccount::Named {
            config_dir: "C:\\Users\\z\\.claude-accts\\z".into(),
        };
        let accounts: [Option<&LaunchAccount>; 3] =
            [None, Some(&LaunchAccount::Base), Some(&named)];
        let mut checked = 0usize;
        for action in [LocalPsAction::New, LocalPsAction::Resume(sid.to_string())] {
            for acct in accounts {
                let cmd =
                    build_local_ps_command(&action, None, acct).expect("这几组形状都该渲染得出来");
                checked += 1;
                assert!(
                    !cmd.contains("--tmux"),
                    "Windows 渲染器吐了 `--tmux` —— `C12` 逐字「windows不要tmux」。实得：{cmd}"
                );
                assert!(
                    !cmd.split_whitespace().any(|w| w == "cct"),
                    "Windows 渲染器自己挑了带 tmux 的别名 `cct`（用户没指定）。实得：{cmd}"
                );
            }
        }
        // 完备性自检：人群空掉时「全过」与「没测」长得一模一样。
        assert_eq!(checked, 6, "只渲了 {checked} 组，人群跑偏了");

        // ★ 结构半：`launch_local` 的 Windows 那支**不许调渲染器**。
        // 光有上面的行为半不够 —— 渲染器可以在 `launch_local` 里被调、把容器加在
        // `build_local_ps_command` **之外**，那样上面六组照样全绿。
        let prod = guard_core::production_code(include_str!("history.rs"));
        // ⚠ 锚点从裸 `#[cfg(windows)]` 扩到「它 + 它门着的那一行」〔`D6` 回修，08-29〕：
        //   本轮 `PRODUCTION_LAUNCH_SINK` 也按平台分了两支 ⇒ 裸锚点从 1 处变成 2 处，
        //   本条当场红（报文逐字「断言指不明是哪一处」）。**它逮到的是真的**：
        //   锚点不唯一时下面切出来的臂可能是别人的。⇒ 按 `F19` 那条纪律
        //   「把 needle 扩到能唯一确定那个事实的大小」，而**不是**把断言放宽。
        let at = guard_core::find_pinned(&prod, "#[cfg(windows)]\n    let base = {")
            .unwrap_or_else(|e| {
                panic!("`launch_local` 的 Windows 臂锚点不是恰好一处，先修锚点：{e}")
            });
        let arm = {
            let b = prod.as_bytes();
            let open = (at..b.len()).find(|&i| b[i] == b'{').expect("找不到块起点");
            let (mut depth, mut end) = (0i32, b.len());
            for i in open..b.len() {
                if b[i] == b'{' {
                    depth += 1;
                } else if b[i] == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
            }
            &prod[open..end]
        };
        assert!(
            arm.len() > 60 && arm.len() < 1500,
            "切出来的 Windows 臂只有 {} 字节 —— 配平切错了，本条会零命中地绿",
            arm.len()
        );
        assert!(
            !arm.contains("render_local_ccm"),
            "`launch_local` 的 Windows 臂调了 CLI 渲染器 —— 那条路会带 `--tmux`。实得：{arm}"
        );
        assert!(
            arm.contains("build_local_ps_command"),
            "`launch_local` 的 Windows 臂不再走 `build_local_ps_command` —— 换路了。实得：{arm}"
        );
    }

    /// ★★ **P3t-Y5 的出口**：把生产渲染器的**真输出**吐给 e2e。
    ///
    /// e2e 要证「这条命令在真 tmux 上干了什么」。若脚本里手抄一份命令串，证的就是手抄那份 ——
    /// 渲染器改了、脚本没改，实测照样绿。⇒ 串必须从**这里**出去。
    ///
    /// `#[ignore]` 是因为它不是判据（不断言任何事），只是个数据出口；
    /// 跑法：`cargo test --lib emit_local_launch_command_for_e2e -- --ignored --nocapture`。
    #[test]
    #[ignore]
    #[cfg(not(windows))]
    fn emit_local_launch_command_for_e2e() {
        let sid = std::env::var("P3T_E2E_SID").unwrap_or_else(|_| "s1abcdef".into());
        let name = std::env::var("P3T_E2E_TMUX").unwrap_or_else(|_| "s1abcdef-cc".into());
        // ★★ launcher 由 e2e 指定，而且必须是个**独一无二的名字**（实测逼出来的）。
        //
        // 第一版让 e2e 拿 PATH shim 顶掉 `claude`。**那在生产送法下不成立**：
        // `launch_local_posix` 用的是 `bash -lic`，**登录 shell 会重跑 profile 并把
        // `~/.local/bin` 重排到 PATH 最前** ⇒ shim 被顶掉、解析到的是用户**真实的 claude**
        // （C7d 逐字禁的那件事，实测真起了两次）。
        // 用一个只在隔离目录里存在的名字，PATH 谁在前都盖不住它 —— 这是结构保证，不是纪律。
        let launcher = std::env::var("P3T_E2E_LAUNCHER").ok();
        let cmd = render_local_ccm_with(
            &LocalPsAction::Resume(sid),
            launcher.as_deref(),
            Some(&LaunchAccount::Base),
            Some(&name),
            &caps_of_a_current_ccm(),
            true,
        )
        .expect("渲染不出来 —— e2e 无对象可跑");
        println!("P3T_CMD<<<{cmd}>>>");
    }

    /// ★ L1：**sid 校验与注入防线在 POSIX 那条路上同样生效**。
    ///
    /// 主计划点名这道校验「要保留——那是一道独立防线，不是重复」。
    /// L1 把它抽进了共享决策 `local_launch_choice`，本测试钉住抽完之后两条路都还有。
    #[test]
    fn posix_renderer_keeps_sid_and_launcher_defenses() {
        for bad in ["", "../etc", "a b", "x;id", "sid$(id)"] {
            assert!(
                build_local_posix_command(&LocalPsAction::Resume(bad.to_string()), None, None)
                    .is_err(),
                "POSIX 渲染器应拒绝非法 sid: {bad:?}"
            );
        }
        for bad in ["cc; calc", "cc|id", "cc$(id)", "cc`id`", "cc&&x"] {
            assert!(
                build_local_posix_command(&LocalPsAction::New, Some(bad), None).is_err(),
                "POSIX 渲染器应拒绝注入 launcher: {bad:?}"
            );
        }
    }

    /// F06：`build_resume_ps_command`/`build_new_session_ps_command` 收拢成
    /// `build_local_ps_command` 后必须逐字节保持——把重构前两个函数曾经产出的具体字符串
    /// 内联成期望值（而非依赖上面 6 条测试的"包含子串"断言，那些不足以证明完全同构）。
    #[test]
    fn unified_builder_byte_identical_to_pre_f06_resume_output() {
        let sid = "01998f2a-1234-7abc-9def-0123456789ab";
        let expected = "if (Get-Command cc -ErrorAction SilentlyContinue) \
             { cc --resume 01998f2a-1234-7abc-9def-0123456789ab } \
             else { claude --resume 01998f2a-1234-7abc-9def-0123456789ab }";
        assert_eq!(build_resume_ps_command(sid, None).unwrap(), expected);
        assert_eq!(
            build_local_ps_command(&LocalPsAction::Resume(sid.to_string()), None, None).unwrap(),
            expected
        );
    }

    #[test]
    fn unified_builder_byte_identical_to_pre_f06_new_session_output() {
        let expected = "if (Get-Command cc -ErrorAction SilentlyContinue) { cc } else { claude }";
        assert_eq!(build_new_session_ps_command(None).unwrap(), expected);
        assert_eq!(
            build_local_ps_command(&LocalPsAction::New, None, None).unwrap(),
            expected
        );
    }

    // ═════════════════════════════════════════════════════════════════════
    // `K-H2b`：接上注入点 —— 注入侧那三个判断
    // ═════════════════════════════════════════════════════════════════════

    /// ★★★ `KH2B5`（`§0e` 裁一）**在这一层的对照** —— 同一条起会话路径、同一个函数，
    /// 只有「这个号在不在中转表里」不同：
    /// api-key 号（表里有行）的命令**带**那个 env，官方号的命令里**一个字节都没有**。
    #[test]
    fn only_an_account_that_has_a_row_in_the_relay_table_gets_the_base_url_prefix() {
        let rows = vec!["acct-a".to_string()];
        let named = |d: &str| LaunchAccount::Named {
            config_dir: d.to_string(),
        };
        // ① 表里有行 ⇒ 前缀在（非空对照：证明这把尺子不是恒空串）。
        let id = relay_account_id(Some(&named("/home/u/.claude-accts/acct-a")));
        assert_eq!(id.as_deref(), Some("acct-a"), "账号 id 是从末段目录名推的");
        let p = relay_prefix_for(id.as_deref(), &rows, true, Some("sid-1"), false).unwrap();
        assert_eq!(
            p, "export ANTHROPIC_BASE_URL='http://127.0.0.1:8788/s/claude-code/acct-a/sid-1'; ",
            "api-key 号的命令没带上中转 base URL —— 那条线还是没接"
        );
        // ② 表里没有这一行（订阅号）⇒ **空串**，命令逐字节与本件之前相同。
        let other = relay_account_id(Some(&named("/home/u/.claude-accts/acct-b")));
        assert_eq!(
            relay_prefix_for(other.as_deref(), &rows, true, Some("sid-1"), false).unwrap(),
            "",
            "没配第三方 key 的号被接进了中转 —— `§0e` 裁一逐字禁这一形"
        );
        // ③ 账号 0 / 没表态 ⇒ 说不出 id ⇒ 空串。
        assert_eq!(relay_account_id(Some(&LaunchAccount::Base)), None);
        assert_eq!(relay_account_id(None), None);
        assert_eq!(
            relay_prefix_for(None, &rows, true, None, false).unwrap(),
            ""
        );
        // ④ Windows 那一侧渲的是 PowerShell 形态（**只到「编得过」**，运行时没量过）。
        let ps = relay_prefix_for(id.as_deref(), &rows, true, Some("sid-1"), true).unwrap();
        assert_eq!(
            ps,
            "$env:ANTHROPIC_BASE_URL='http://127.0.0.1:8788/s/claude-code/acct-a/sid-1'; "
        );
    }

    // ★★★ `D5 阻-1`：**那条扫描型判据（`the_two_inputs_at_the_call_site_are_still_the_two_take_points`）
    //    整条删了**，换成下面**三条**判据（两条行为 + 一条按函数地址对拍）。
    //    删它的理由是一个实测读数，不是风格：
    //
    // 它先前住 `local_daemon.rs`（`D4` 搬过去的，为的是「判据与被扫的代码不同文件」），
    // 而它量的仍然是「`relay_prefix_for_launch` 的体切出 700 字节，窗口里**有没有**那两段文本」。
    // `D5` 现打：在同一个窗口里加一行把那两段文本原样留住的死赋值，同时把真入参换成空表 / 常量
    // ⇒ 文本一处不少、**全量门禁四个数与干净树逐字相同**，而中转注入在生产上被整个摘掉。
    // ⇒ 按铁律 13「删之前先证明它恒绿」——`D5` 那一刀就是那份证明。
    //
    // ★ 本件病史五层，每层都是**上一层的修法买到的东西被下一层的量法漏掉**：
    //   ① 参数位没有账号 → ② 值恒空 → ③ 只量文本 → ④ 判据搬了家、仍只量文本 → ⑤ 文本留住、行为摘掉。
    //   ⇒ **第六层的出路不是更聪明的文本判据，是不量文本。**见 `RelayFactSources` 头注。

    /// ★★★ `D5 阻-1` + `D6 阻-2` + `D6 阻-3`：**那次拉起真的问了那三件事，而且真的用了答案。**
    ///
    /// # 它怎么挡住第五层那一刀
    ///
    /// 替身把「被问了几次」记下来，但**光有计数不够** —— 第五层那一刀（问完把答案扔掉）
    /// 会让计数照涨。⇒ 承重的是第二半：**算出来的前缀必须与「拿替身那几个答案直接喂纯函数」
    /// 逐字节相同**，并且几种答案组合各自落到不同的脸上（非空 / 空 / `Err` / PowerShell 形态）。
    /// 把任何一个入参换成常量，这几格里至少一格当场不同。
    ///
    /// # 🔴🔴 `D6 阻-2`：**「哪个号」也是一维，而它先前的输入域是 1**
    ///
    /// 第一版只喂**一个**账号（`acct-a`）⇒ `D6` 的刀 `E6` 把 [`relay_account_id`] 的答案
    /// `.map(|_| "acct-a")` 写死（那段文本一字不动）⇒ **全绿、门禁四个数与干净树逐字相同**。
    /// 生产后果是**路由键的 `<account>` 段恒是一个号** ⇒ 中转按它取 key ⇒
    /// **acct-b 的会话拿着 acct-a 的那把 key 发请求，两边都显示成功** ——
    /// 正是整个多账号工作要防的最坏那一形。
    /// ⇒ 本条**至少喂两个不同的号**，并断言前缀里的 `<account>` 段跟着变。
    ///
    /// # 🔴 `D6 阻-3`：**平台开关也收进了这条缝**
    ///
    /// `cfg!(windows)` 写在调用点上时是个常量表达式，判据翻不动它 ——
    /// `D6` 的刀 `Xb`（把它写死成 `false`）全绿，而生产后果是 Windows 上渲成 POSIX 形态。
    /// 收进 [`RelayFactSources`] 之后，本条第 ④ 格喂 `|| true` 就该拿到 PowerShell 形态。
    /// ⚠ **它守的是调用点那一格**；[`platform_is_windows`] 自己的体在 Linux 上判不了
    ///（登记在 [`RelayFactSources`] 的**结构头注** ㈡ 那一栏里 —— 不在 [`platform_is_windows`]
    /// 自己的头注里，`D7` 逐字订正过这一处指偏）。
    ///
    /// # 🔴🔴 `D7 阻-4`：**「哪一次拉起」（`action` / `sid`）也是一维，先前它的取值域是 1**
    ///
    /// 第 ①–④ 格把 `rows` / `running` / `windows` / `account` 四维都打开了，
    /// **而 `action` 从头到尾只喂过 `Resume("sid-1")`**。`D7` 两刀实打：
    /// - 刀 `S2`（`LocalPsAction::New => return Ok(String::new())`）⇒ **`1229` 全绿**，
    ///   生产后果是**新开会话那条路上中转注入恒空**，而 `KH2B6` 逐字写着那条路今天生产可达；
    /// - 刀 `S1`（`Resume(_sid) => Some("sid-1")`）⇒ 也全绿，那一维什么都没买。
    /// ⇒ 第 ⑤ 格喂**两个不同的 sid**、第 ⑥ 格喂 **`New`**，两支都断。
    ///
    /// # 它买不到什么
    ///
    /// 它不管那几个取值口**自己答得对不对**（那是 [`relay_rows_at`] 那条读真文件的判据、
    /// 与 `local_daemon::relay_running_really_reads_the_handle_table` 的活），
    /// 也不管**生产上插进那条缝的是不是它们**（那是下一条判据按函数地址对拍的活）。
    /// **三条合起来才等于「这条线真的在问那几件事」。**
    #[test]
    fn the_launch_side_really_asks_those_two_take_points_and_uses_their_answers() {
        use std::cell::{Cell, RefCell};
        thread_local! {
            static ROWS_CALLS: Cell<u32> = const { Cell::new(0) };
            static RUNNING_CALLS: Cell<u32> = const { Cell::new(0) };
            static ROWS_ANSWER: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
            static RUNNING_ANSWER: Cell<bool> = const { Cell::new(false) };
            static WINDOWS_ANSWER: Cell<bool> = const { Cell::new(false) };
        }
        fn spy_rows() -> Vec<String> {
            ROWS_CALLS.with(|c| c.set(c.get() + 1));
            ROWS_ANSWER.with(|v| v.borrow().clone())
        }
        fn spy_running() -> bool {
            RUNNING_CALLS.with(|c| c.set(c.get() + 1));
            RUNNING_ANSWER.with(Cell::get)
        }
        fn spy_windows() -> bool {
            WINDOWS_ANSWER.with(Cell::get)
        }
        fn answer(rows: &[&str], running: bool, windows: bool) {
            ROWS_ANSWER.with(|v| *v.borrow_mut() = rows.iter().map(|s| s.to_string()).collect());
            RUNNING_ANSWER.with(|c| c.set(running));
            WINDOWS_ANSWER.with(|c| c.set(windows));
        }

        let acct_a = LaunchAccount::Named {
            config_dir: "/h/.claude-accts/acct-a".to_string(),
        };
        // 🔴 `D6 阻-2`：**第二个号**。「哪个号」这一维的输入域从 1 变成 2。
        let acct_b = LaunchAccount::Named {
            config_dir: "/h/.claude-accts/acct-b".to_string(),
        };
        let action = LocalPsAction::Resume("sid-1".to_string());
        let _guard = override_relay_facts(RelayFactSources {
            rows: spy_rows,
            running: spy_running,
            windows: spy_windows,
        });

        // ① 表里有这一行 + 中转在跑 ⇒ 前缀 = 纯函数在**替身给的那几个答案**上算出来的那一份。
        answer(&["acct-a", "acct-b"], true, false);
        let want = relay_prefix_for(
            Some("acct-a"),
            &["acct-a".to_string(), "acct-b".to_string()],
            true,
            Some("sid-1"),
            false,
        )
        .expect("纯函数在这组输入上不该报错");
        // 反空真：这组输入下期望值本来就该是非空的，否则下面那条 `assert_eq!` 是「空 == 空」。
        assert!(
            !want.is_empty(),
            "期望值是空串 —— 那下面那条相等断言就是空真，本条按红处理"
        );
        let got = relay_prefix_for_launch(&action, Some(&acct_a)).expect("这一档不该报错");
        assert!(
            ROWS_CALLS.with(Cell::get) >= 1,
            "这次拉起**没问**「这个号在不在中转表里」—— 那一格成了常量"
        );
        assert!(
            RUNNING_CALLS.with(Cell::get) >= 1,
            "这次拉起**没问**「中转在不在跑」—— 那一格成了常量"
        );
        assert_eq!(
            got, want,
            "\n问是问了，**答案没被用上** —— 这正是 `D5` 那一刀的形状：\n\
             把两个事实算出来扔掉（一个用不到的绑定），入参换成空表 / 常量，\n\
             两段文本原地留着 ⇒ 上一版那条「窗口里有没有这段文本」的判据照绿，\n\
             而中转前缀恒空、本件的正题被整个摘掉。"
        );

        // ①b 🔴 `D6 阻-2`：**同一张表、只换一个号** ⇒ 路由键的 `<account>` 段必须跟着变。
        let got_b = relay_prefix_for_launch(&action, Some(&acct_b)).expect("这一档不该报错");
        assert_ne!(
            got, got_b,
            "\n换一个号，拼出来的前缀一个字节都没变 —— 「这次拉起是哪个号」这一维成了常量。\n\
             生产后果：路由键的 `<account>` 段恒指一个号 ⇒ 中转按它取 key ⇒\n\
             **acct-b 的会话拿着 acct-a 的那把 key 发请求，而两边都显示成功。**"
        );
        assert!(
            got.contains("/acct-a/") && got_b.contains("/acct-b/"),
            "路由键里的账号段不是这次拉起的那个号：acct-a ⇒ {got:?} · acct-b ⇒ {got_b:?}"
        );

        // ② **只**把「表里有没有这一行」翻过来 ⇒ 前缀空（不注入，逐字节旧路）。
        answer(&["someone-else"], true, false);
        assert_eq!(
            relay_prefix_for_launch(&action, Some(&acct_a)).expect("不该报错"),
            "",
            "表里没有这个号，却仍然拼出了前缀 —— 「在不在表里」这个答案没被用上"
        );

        // ③ **只**把「中转在不在跑」翻过来 ⇒ `Err`（`KH2B2`②：不许静默）。
        answer(&["acct-a"], false, false);
        assert!(
            relay_prefix_for_launch(&action, Some(&acct_a)).is_err(),
            "中转没在跑却照旧起出去了 —— 「在不在跑」这个答案没被用上，\n\
             症状会长成「claude 连不上 API」，与网络故障同形，而原因在我们这一侧"
        );

        // ④ 🔴 `D6 阻-3`：**只**把「这台机是不是 Windows」翻过来 ⇒ 渲成 PowerShell 形态。
        //    先前这一格是调用点上的 `cfg!(windows)`（常量表达式），刀 `Xb` 把它写死成 `false`
        //    ⇒ 全绿。收进缝之后，写死常量就意味着**这一格的答案没被用上**。
        answer(&["acct-a"], true, true);
        let ps = relay_prefix_for_launch(&action, Some(&acct_a)).expect("这一档不该报错");
        assert_eq!(
            ps, "$env:ANTHROPIC_BASE_URL='http://127.0.0.1:8788/s/claude-code/acct-a/sid-1'; ",
            "\n「这台机是不是 Windows」这个答案没被用上 —— 调用点把那一格写死了。\n\
             生产后果：Windows 上中转前缀渲染成 POSIX 形态（`export …` 塞进 PowerShell 串）\n\
             ⇒ 中转注入在 Windows 上整个失效，而 Windows 运行时行为本件在「判不了」里\n\
             ⇒ **判据是那一格唯一的守卫**。"
        );
        // 阴性对照：同一组输入只翻这一格，答案必须真的不同（否则上面那条是「两张脸长一样」）。
        answer(&["acct-a"], true, false);
        assert_ne!(
            relay_prefix_for_launch(&action, Some(&acct_a)).expect("不该报错"),
            ps,
            "两个平台渲出来的前缀一模一样 —— 这把尺子分不出 PowerShell 与 POSIX"
        );

        // ═══════════════════════════════════════════════════════════════════
        // ⑤⑥ 🔴 `D7 阻-4`：**「哪一次拉起」（`action` / `sid`）也是一维，它的取值域一直是 1**
        // ═══════════════════════════════════════════════════════════════════
        //
        // 上一轮这条判据在 `rows` / `running` / `windows` / `account` 四维上各喂了 ≥2 个值，
        // **而 `action` 只喂过 `LocalPsAction::Resume("sid-1")` 一个**。`D7` 打了两刀：
        // - 刀 `S2`：`LocalPsAction::New => return Ok(String::new())` ⇒ **`1229` 全绿**。
        //   生产后果：**新开会话那条路上中转注入恒空** —— 一个 api-key 号新开一个会话，
        //   claude 直连官方端点、第三方 key 用不上，而门禁四个数一格不动。
        //   ⚠ 而件文件 `§3a` 的 `KH2B6` 逐字写着「新开会话这条路**现在生产可达**」。
        // - 刀 `S1`：`Resume(_sid) => Some("sid-1")`（把 `<key>` 段写死）⇒ 也全绿。
        //   后果轻（`mint_route_key` 头注登记着这一段对路由惰性），**但那一维什么都没买**。
        //
        // ⇒ 与 `term` 那条链同一条纪律：**分叉点上的两支都要喂**，不许只买一支。
        let key_seg = |prefix: &str| -> String {
            let url = prefix
                .split('\'')
                .nth(1)
                .unwrap_or_else(|| panic!("前缀里没有被单引号包住的 URL：{prefix:?}"));
            url.rsplit('/')
                .next()
                .expect("URL 一个路径段都没有")
                .to_string()
        };

        answer(&["acct-a"], true, false);
        // ⑤ **`Resume` 那一支**：`<key>` 段必须是**这一次**的 sid，不是一个写死的串。
        let resumed_1 = relay_prefix_for_launch(&action, Some(&acct_a)).expect("不该报错");
        let resumed_2 =
            relay_prefix_for_launch(&LocalPsAction::Resume("sid-9".to_string()), Some(&acct_a))
                .expect("不该报错");
        assert_eq!(
            key_seg(&resumed_1),
            "sid-1",
            "路由键的 `<key>` 段不是这次的 sid"
        );
        assert_eq!(
            key_seg(&resumed_2),
            "sid-9",
            "\n换一个会话 id，路由键的 `<key>` 段没跟着变 —— 刀 `S1` 的形状：\n\
             `Resume(_sid) => Some(\"sid-1\")` 把这一段写死，那一维什么都没买。\n\
             实得 = {:?}",
            key_seg(&resumed_2)
        );

        // ⑥ 🔴 **`New` 那一支**：新开会话**也真的走中转**，而它的 `<key>` 段是一次性 nonce。
        let new_1 = relay_prefix_for_launch(&LocalPsAction::New, Some(&acct_a))
            .expect("新开会话这一档不该报错");
        assert!(
            !new_1.is_empty() && new_1.contains("ANTHROPIC_BASE_URL"),
            "\n★★ **新开会话那条路上中转前缀是空的** —— 这正是刀 `S2` 的形状：\n\
             `LocalPsAction::New => return Ok(String::new())`，而件文件 `§3a` 的 `KH2B6`\n\
             逐字写着这条路**现在生产可达**。\n\
             生产后果：一个 api-key 号新开一个会话 ⇒ **claude 直连官方端点、第三方 key 用不上**，\n\
             而门禁四个数一格不动。实得 = {new_1:?}"
        );
        assert!(
            new_1.contains("/acct-a/"),
            "新开会话拼出来的路由键里没有这次的账号段：{new_1:?}"
        );
        let new_key = key_seg(&new_1);
        assert_ne!(
            new_key,
            key_seg(&resumed_1),
            "新开会话拿到了 resume 那一支的 `<key>` 段 —— 这一维被抹平了"
        );
        // 反空真：nonce 那一支**每次都不同**（`route_key_for_session(None)` → `mint_route_key`）。
        // 恒定的 `<key>` 意味着这一支被换成了一个常量，而上面那条 `assert_ne!` 分不出来。
        let new_2 = relay_prefix_for_launch(&LocalPsAction::New, Some(&acct_a)).expect("不该报错");
        assert_ne!(
            new_key,
            key_seg(&new_2),
            "两次新开会话拿到同一个 `<key>` 段 —— 那一段不是 nonce，是一个常量"
        );
    }

    /// ★★★ `D5 阻-1` 的**同职第二处**：界面那一侧（`KH2B7` 的 `relay_routing_for`）
    /// **也真的问了那两件事，而且真的用了答案。**
    ///
    /// # 为什么它非有不可（分母在这里）
    ///
    /// 那两个取值口的生产消费方**恰好 2**：起会话那一侧（上一条）与本条这一侧。
    /// 上一条只买了第一处 —— 而「只覆盖了那条病的一个动词」正是本区最近四次打回的形状。
    /// 本条把第二处也钉住：把 `routed` 写死成空表 / 把 `running` 写死成常量，界面就会
    /// **说反**（「这个号走中转」和「中转在跑」两句都是用户唯一看得见的说法），而没有别的判据会红。
    #[test]
    fn the_ui_status_side_asks_those_two_take_points_and_uses_their_answers() {
        use std::cell::{Cell, RefCell};
        thread_local! {
            static ROWS_CALLS: Cell<u32> = const { Cell::new(0) };
            static RUNNING_CALLS: Cell<u32> = const { Cell::new(0) };
            static ROWS_ANSWER: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
            static RUNNING_ANSWER: Cell<bool> = const { Cell::new(false) };
        }
        fn spy_rows() -> Vec<String> {
            ROWS_CALLS.with(|c| c.set(c.get() + 1));
            ROWS_ANSWER.with(|v| v.borrow().clone())
        }
        fn spy_running() -> bool {
            RUNNING_CALLS.with(|c| c.set(c.get() + 1));
            RUNNING_ANSWER.with(Cell::get)
        }
        fn answer(rows: &[&str], running: bool) {
            ROWS_ANSWER.with(|v| *v.borrow_mut() = rows.iter().map(|s| s.to_string()).collect());
            RUNNING_ANSWER.with(|c| c.set(running));
        }

        let dirs = vec![
            "/h/.claude-accts/acct-a".to_string(),
            "/h/.claude-accts/acct-b".to_string(),
        ];
        let _guard = override_relay_facts(RelayFactSources {
            rows: spy_rows,
            running: spy_running,
            // 界面那一侧不看平台（徽章文案两个平台同一份）⇒ 这一格照生产那个取值口，不装替身。
            windows: platform_is_windows,
        });

        // ① 表里只有 `acct-a` + 中转在跑 ⇒ 只有那一个 configDir 被判「走中转」，`running` 为真。
        answer(&["acct-a"], true);
        let got = crate::relay_routing_for(dirs.clone());
        assert!(
            ROWS_CALLS.with(Cell::get) >= 1 && RUNNING_CALLS.with(Cell::get) >= 1,
            "界面这一侧**没问**那两件事（rows={} running={}）—— 那两格成了常量",
            ROWS_CALLS.with(Cell::get),
            RUNNING_CALLS.with(Cell::get)
        );
        assert_eq!(
            got.routed,
            vec!["/h/.claude-accts/acct-a".to_string()],
            "问是问了，**答案没被用上** —— 界面会把「走不走中转」说反"
        );
        assert!(got.running, "「中转在不在跑」的答案没被用上");

        // ② **只**把表翻过来 ⇒ 一个都不走（不是「随便回一份」）。
        answer(&[], true);
        assert!(
            crate::relay_routing_for(dirs.clone()).routed.is_empty(),
            "表空了界面还说有号走中转"
        );
        // ③ **只**把「在不在跑」翻过来 ⇒ `running` 跟着变（且 `routed` 不受它影响，两格分开）。
        answer(&["acct-b"], false);
        let flipped = crate::relay_routing_for(dirs);
        assert!(!flipped.running, "中转没跑，界面还说在跑");
        assert_eq!(
            flipped.routed,
            vec!["/h/.claude-accts/acct-b".to_string()],
            "两格串了 —— `routed` 不该跟着「在不在跑」变"
        );
    }

    /// ★★★ `D5 阻-1` 的第三格：**生产上插进那条缝的，就是那两个真取值口。**
    ///
    /// 上一条把替身换进去量行为 ⇒ 它量不到「生产那一份指的是谁」。
    /// 这一条按**函数地址**对拍（不是按文本）：把 [`PRODUCTION_RELAY_FACTS`] 里任何一格
    /// 换成一个返回常量的闭包 / 别的函数，本条当场红。
    ///
    /// # 🔴🔴 第二半是**我自己找第六层时找出来的**，别删
    ///
    /// 只对拍那个 `const` **不够**：`relay_facts()` 才是生产真正取值的那一跳。
    /// 有人把 `relay_facts()` 改成「不装替身时也回一份写死的」而**一个字节不动那个 `const`**
    /// ⇒ 地址对拍照绿（它读的是 `const`）、行为判据也照绿（它们装了替身、走的是另一支）
    /// ⇒ **又是一次「文本/形状留住、行为摘掉」，全绿。**
    /// ⇒ 所以下面**先在没装替身的状态下调一次 `relay_facts()`**，按地址断言它交出来的就是那两个真取值口。
    #[test]
    fn the_production_relay_facts_are_those_two_take_points() {
        // 反空真排最前：这把尺子**分得出**「不是那个函数」，否则下面两条是恒真。
        fn not_it() -> bool {
            true
        }
        assert!(
            !std::ptr::fn_addr_eq(PRODUCTION_RELAY_FACTS.running, not_it as fn() -> bool),
            "这把尺子对任何同型函数都说「是」—— 它恒真，本条按红处理"
        );
        // ★★ 第二半：**没装替身**的那一跳（= 生产那一跳）交出来的必须就是那两个真取值口。
        //    ⚠ 本条**刻意不装替身**；替身住 thread-local ⇒ 别的判据装的那份影响不到这里。
        let live = relay_facts();
        assert!(
            std::ptr::fn_addr_eq(live.rows, relay_rows as fn() -> Vec<String>),
            "没装替身时 `relay_facts()` 交出来的「表从哪来」不是 `relay_rows` ——\n\
             生产那一跳被换掉了，而只对拍那个 `const` 的判据看不见（第六层的形状）"
        );
        assert!(
            std::ptr::fn_addr_eq(
                live.running,
                crate::local_daemon::relay_running as fn() -> bool
            ),
            "没装替身时 `relay_facts()` 交出来的「中转在不在跑」不是 `local_daemon::relay_running`"
        );
        assert!(
            std::ptr::fn_addr_eq(
                PRODUCTION_RELAY_FACTS.rows,
                relay_rows as fn() -> Vec<String>
            ),
            "生产上「这个号在不在中转表里」不再由 `relay_rows` 答 ——\n\
             换成一个恒空的东西，谁都不走中转，而行为判据（喂替身的那条）照绿"
        );
        assert!(
            std::ptr::fn_addr_eq(
                PRODUCTION_RELAY_FACTS.running,
                crate::local_daemon::relay_running as fn() -> bool
            ),
            "生产上「中转在不在跑」不再由 `local_daemon::relay_running` 答 ——\n\
             换成恒真，`KH2B2`② 那道「起不来就当场拒」的闸整个失效，而行为判据照绿"
        );
        // 🔴 `D6 阻-3`：第三格（平台开关）同样按地址对拍，两跳都拍。
        assert!(
            std::ptr::fn_addr_eq(live.windows, platform_is_windows as fn() -> bool)
                && std::ptr::fn_addr_eq(
                    PRODUCTION_RELAY_FACTS.windows,
                    platform_is_windows as fn() -> bool
                ),
            "生产上「这台机是不是 Windows」不再由 `platform_is_windows` 答 ——\n\
             换成一个恒假的东西，Windows 上前缀渲成 POSIX 形态、注入整个失效，\n\
             而 Windows 运行时在本件的「判不了」里 ⇒ 这一格只有判据这一个守卫"
        );

        // ★★ `D6 阻-1`：**送出去**那条缝同样按地址对拍（同样两跳：`const` 与没装替身的那一跳）。
        //    只对拍 `const` 不够的理由与上面第二半逐字同一条。
        let sink = launch_sink();
        #[cfg(not(windows))]
        let production_sink =
            crate::launch::launch_local_posix as fn(&str, Option<&str>) -> Result<(), String>;
        #[cfg(windows)]
        let production_sink =
            crate::launch::launch_powershell_window as fn(&str, Option<&str>) -> Result<(), String>;
        assert!(
            std::ptr::fn_addr_eq(sink.0, production_sink)
                && std::ptr::fn_addr_eq(PRODUCTION_LAUNCH_SINK.0, production_sink),
            "没装替身时最后送出去的那一步不是 `launch::launch_local_posix` / \
             `launch::launch_powershell_window` ——\n\
             生产那一跳被换掉了，而驱动 `launch_local` 的那条行为判据装了替身、看不见这件事"
        );
    }

    /// ★★★ `D1 阻-6` 刀 C 的反面：**`relay_rows` 真的去读那份文件、真的解析出行。**
    ///
    /// `D1` 实测过：把它整个换成 `Vec::new()`，**1221 passed / 0 failed** ——
    /// 也就是说「这个号在不在中转表里」这个**取值口**当时一条判据都没有，
    /// 而它一旦恒空，整件事的表现就是「谁都不走中转」，**而且全绿**。
    #[test]
    fn the_rows_really_come_from_that_file_not_from_a_constant() {
        let dir = std::env::temp_dir().join(format!(
            "ccm-rows-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let f = dir.join("relay-credentials.json");

        // ① 文件不在 ⇒ 零条（**不是**报错：读不到与一条没配的正确行为都是「照旧直连」）。
        assert!(relay_rows_at(&f).is_empty(), "文件不在却读出了行");
        // ② 真写一份（裸 `fs::write` = 人拿编辑器写的那一份）⇒ 逐条读出来。
        std::fs::write(
            &f,
            b"{\n  \"accounts\": {\n    \"acct-a\": { \"api_key\": \"K1\" },\n    \"acct-b\": {}\n  }\n}\n",
        )
        .expect("写夹具");
        let mut got = relay_rows_at(&f);
        got.sort();
        assert_eq!(
            got,
            vec!["acct-a".to_string(), "acct-b".to_string()],
            "没把那份文件里的行读出来 —— 这个取值口恒空的话，谁都不会走中转，而且全绿"
        );
        // ③ 当不了路由段的 id **筛掉**（与中转装表那一侧同一条规则）。
        std::fs::write(
            &f,
            b"{\n  \"accounts\": {\n    \"ok-1\": {},\n    \"has.dot\": {},\n    \"has/slash\": {}\n  }\n}\n",
        )
        .expect("写夹具");
        assert_eq!(
            relay_rows_at(&f),
            vec!["ok-1".to_string()],
            "界面这一侧收下了中转装表时会丢掉的行 —— 那会让界面说「经本机中转」而中转 404"
        );
        // ④ 文件坏了 ⇒ 零条 + 不 panic（人手编打错一个逗号是常态）。
        std::fs::write(&f, b"{ not json").expect("写夹具");
        assert!(relay_rows_at(&f).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ★★ `KH2B7` 的产出方：**界面问的那个「有没有行」，与起会话那一侧问的是同一个规则。**
    ///
    /// 两处各写一个 basename 规则，漂开的那天症状是「设置里说走中转、起会话时没走」，
    /// 而两边看起来都没错。⇒ 本条把它钉成**同一个函数的两个调用方**。
    #[test]
    fn the_ui_and_the_launch_side_derive_the_account_id_from_the_same_rule() {
        let rows = vec!["acct-a".to_string()];
        let dirs = vec![
            "/home/u/.claude-accts/acct-a".to_string(),
            "/home/u/.claude-accts/acct-b".to_string(),
            // 末段带尾斜杠 / 带空白的写法也要落到同一个 id 上。
            "  /home/u/.claude-accts/acct-a/  ".to_string(),
        ];
        // 非空对照排最前：先证明这把尺子不是恒空。
        assert_eq!(
            relay_routed_subset(&dirs, &rows),
            vec![
                "/home/u/.claude-accts/acct-a".to_string(),
                "  /home/u/.claude-accts/acct-a/  ".to_string()
            ],
            "界面那一侧筛出来的不是「表里有行」的那几个"
        );
        // ★ 与起会话那一侧**同一个规则**：同一个目录，两条路推出同一个 id。
        let named = LaunchAccount::Named {
            config_dir: "/home/u/.claude-accts/acct-a".to_string(),
        };
        assert_eq!(
            relay_account_id(Some(&named)),
            relay_account_id_of_dir("/home/u/.claude-accts/acct-a"),
            "两个调用方推出来的账号 id 不一样 —— 那正是「设置里说走中转、起会话时没走」的形状"
        );
        // 表里没有的行一个都不许混进来（`KL7` 第 2 条的界面侧倒影）。
        assert!(relay_routed_subset(&dirs, &[]).is_empty(), "空表却筛出了行");
        // 账号 0 / 空 configDir 推不出 id ⇒ 不在结果里（说不出就不表态）。
        assert!(relay_routed_subset(&["".to_string()], &rows).is_empty());
    }

    /// ★★ `KH2B2`②在这一层：中转没在跑 ⇒ **起会话这一侧当场说话**，
    /// 不许渲染成一条指向没人听的口的 URL（那会长成「claude 连不上 API」）。
    #[test]
    fn a_launch_that_needs_the_relay_is_refused_when_the_relay_is_not_running() {
        let rows = vec!["acct-a".to_string()];
        let e = relay_prefix_for(Some("acct-a"), &rows, false, Some("sid-1"), false)
            .expect_err("中转没起来却照旧渲染 —— 症状会与网络故障同形");
        assert!(e.contains("中转没在跑"), "错误得说出真正的原因：{e}");
        // 非空对照：只把「中转在跑」翻过来，同一条路径就不再报错。
        assert!(relay_prefix_for(Some("acct-a"), &rows, true, Some("sid-1"), false).is_ok());
    }

    /// ★★★ **接线判据**：`launch_local` **真正交出去的那一串**以中转前缀打头。
    ///
    /// # 🔴🔴🔴 它先前是一条文本判据，而 `D6` 的刀 `Y1` 把它打穿了（第七层）
    ///
    /// 第一版量的是「从 `fn launch_local(` 起切 3600 字节，窗口里**有没有**这三样文本」：
    /// ① `relay_prefix_for_launch(action, account)?` ② `relay + &` 恰好 2 处
    /// ③ `let cmd = relay + &base;` 与另外两处的先后序。
    /// 刀 `Y1` 在拼装那一行加了一句
    /// `let relay = if relay.is_empty() { relay } else { String::new() };`
    /// ⇒ 三样文本**一处不少**（三个锚点数与干净树逐字相同）
    /// ⇒ **`1227 passed; 0 failed`、`GATE: OK`、四个数与干净树逐字相同**，
    /// **而前缀算出来了没拼上去 —— 本件的正题整个被摘掉。**
    /// ⚠ 而**上一版这段头注里逐字写着它要防的正是这件事**（「算出来却没拼上去，行为上与本件
    /// 没做完全一样」）—— 威胁模型写对了，买的东西是文本。
    /// ⚠ 它还带着 `D4 阻-1` 那一形：判据住 `history.rs::tests`，而它 `include_str!("history.rs")`
    /// 扫的就是本文件 ⇒ `relay + &` 全仓 4 = 生产 2 + 本条的针 1 + 报文 1，按本仓变异纪律
    /// 「锚点全改」会把针一起带走。
    ///
    /// # ⇒ 换成量**真正送出去的那一串**
    ///
    /// [`LaunchSink`] 那条缝让判据能装一个记账替身，于是本条量的不再是源码，是
    /// **`launch_local` 最后交给送法的那个字符串**。三格，每格都能被一刀翻掉：
    ///
    /// | 格 | 断的是什么 | 翻掉它的形状 |
    /// |---|---|---|
    /// | ① | 表里没这个号 ⇒ 那一串里**一个 `ANTHROPIC_BASE_URL` 都没有** | 无条件注入 |
    /// | ② | 表里有 ⇒ 那一串**逐字节等于**「前缀 + ① 那一串」 | 刀 `Y1`（算了没拼）· 掏空注入点 |
    /// | ③ | 换一个号 ⇒ 前缀里的 `<account>` 段跟着变 | 刀 `E6`（账号段写死成常量） |
    /// | ⑤ | **新开会话**那一支也带前缀 | 刀 `S2`（`New => Ok(String::new())`） |
    /// | ⑥ | 换一个 sid ⇒ `<key>` 段跟着变 | 刀 `S1`（`Resume(_sid) => Some("sid-1")`） |
    ///
    /// 🔴 ⑤⑥ 是 `D7 阻-4` 补的：先前 ①–④ **全部走 `Resume("sid-1")`**
    /// ⇒ `action` 这一维的输入域是 1，而**新开会话是 `KH2B6` 逐字写着「现在生产可达」的一条路**。
    ///
    /// # ⚠ 它买不到什么（如实写）
    ///
    /// - **走 ccm 容器那一支**：本条喂 `tmux_name = None` ⇒ 走的是回落那条路。
    ///   而按 `a_launch_that_goes_through_the_relay_still_cannot_get_a_tmux_container`，
    ///   **带中转前缀的拉起今天必然落到回落路** ⇒ 本条驱动的正是那条生产可达的路。
    ///   ccm 那一支上「前缀有没有拼」由同一行代码管（合流之后**只有一处**拼接）。
    /// - **送法自己拿到串之后干了什么**：那是 `launch::launch_local_posix` 自己的判据面；
    ///   「生产上插进这条缝的就是它」由 `the_production_relay_facts_are_those_two_take_points`
    ///   末尾那一格按**函数地址**对拍。
    /// - **谁绕开这条缝直接调送法**：由 `payload.rs` 那道人群闸数着（零调用点）。
    #[test]
    fn the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched() {
        use std::cell::{Cell, RefCell};
        thread_local! {
            static SENT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
            static ROWS_ANSWER: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
            static RUNNING_ANSWER: Cell<bool> = const { Cell::new(false) };
        }
        fn recorder(cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
            SENT.with(|v| v.borrow_mut().push(cmd.to_string()));
            Ok(())
        }
        fn spy_rows() -> Vec<String> {
            ROWS_ANSWER.with(|v| v.borrow().clone())
        }
        fn spy_running() -> bool {
            RUNNING_ANSWER.with(Cell::get)
        }
        fn answer(rows: &[&str], running: bool) {
            ROWS_ANSWER.with(|v| *v.borrow_mut() = rows.iter().map(|s| s.to_string()).collect());
            RUNNING_ANSWER.with(|c| c.set(running));
        }
        fn last_sent() -> String {
            SENT.with(|v| v.borrow().last().cloned().expect("这一趟什么都没送出去"))
        }

        let _sink = override_launch_sink(LaunchSink(recorder));
        let _facts = override_relay_facts(RelayFactSources {
            rows: spy_rows,
            running: spy_running,
            // 平台那一格照生产那个取值口（本条不翻它 —— 翻它的是上面那条判据的第 ④ 格）。
            windows: platform_is_windows,
        });

        let action = LocalPsAction::Resume("sid-1".to_string());
        let accounts = [
            ("acct-a", "/h/.claude-accts/acct-a"),
            // 🔴 `D6 阻-2`：**两个号**，不是一个 —— 「这次拉起是哪个号」也要能翻。
            ("acct-b", "/h/.claude-accts/acct-b"),
        ];

        let mut with_relay = Vec::new();
        for (id, dir) in accounts {
            let account = LaunchAccount::Named {
                config_dir: dir.to_string(),
            };
            // ① 表里没有这个号 ⇒ 逐字节旧路。这一趟同时是下面那条相等断言的**基准串**。
            answer(&[], true);
            launch_local(&action, None, None, Some(&account), None)
                .expect("不走中转这一趟不该失败");
            let bare = last_sent();
            assert!(
                !bare.is_empty() && bare.contains(dir),
                "基准串不像一条本机拉起命令（连这个号的 configDir 都没有）：{bare:?}"
            );
            assert!(
                !bare.contains("ANTHROPIC_BASE_URL"),
                "表里没有这个号，送出去的那一串却带着中转注入 —— \
                 `KH2B5`「没配第三方 key 的号一个字节都不受影响」当场破了：{bare:?}"
            );

            // ② 表里有这个号 ⇒ 送出去的那一串**逐字节等于**「前缀 + 基准串」。
            answer(&["acct-a", "acct-b"], true);
            let prefix = relay_prefix_for(
                Some(id),
                &["acct-a".to_string(), "acct-b".to_string()],
                true,
                Some("sid-1"),
                cfg!(windows),
            )
            .expect("纯函数在这组输入上不该报错");
            // 反空真：期望的前缀本来就该是非空的，否则下面那条相等断言是「x == x」。
            assert!(!prefix.is_empty(), "期望前缀是空串 —— 本条按红处理");
            launch_local(&action, None, None, Some(&account), None).expect("走中转这一趟不该失败");
            let routed = last_sent();
            assert_eq!(
                routed,
                format!("{prefix}{bare}"),
                "\n★★ **算出来了没拼上去** —— 这正是 `D6` 刀 `Y1` 的形状：\n\
                 `relay_prefix_for_launch` 照样被调、照样答对，而拼装那一行把它扔了\n\
                 ⇒ 起会话的命令串里没有 `ANTHROPIC_BASE_URL`，本件的正题整个被摘掉，\n\
                 **而上一版那条数三样文本的判据照绿。**\n\
                 号 = {id} · 实得 = {routed:?} · 期望 = {:?}",
                format!("{prefix}{bare}")
            );
            with_relay.push((id, routed));
        }

        // ③ 🔴 `D6 阻-2`：**两个号送出去的前缀必须不一样**，而且各自带自己的账号段。
        let (id_a, cmd_a) = &with_relay[0];
        let (id_b, cmd_b) = &with_relay[1];
        assert!(
            cmd_a.contains(&format!("/{id_a}/")) && cmd_b.contains(&format!("/{id_b}/")),
            "\n路由键里的账号段不是这次拉起的那个号 —— 刀 `E6` 的形状：\n\
             把 `relay_account_id` 的答案 `.map(|_| \"acct-a\")` 写死，\n\
             生产后果是 **acct-b 的会话拿着 acct-a 的那把 key 发请求，两边都显示成功**。\n\
             实得：{cmd_a:?} · {cmd_b:?}"
        );
        assert_ne!(
            cmd_a.split("; ").next(),
            cmd_b.split("; ").next(),
            "两个号送出去的第一段（中转前缀）逐字节相同 —— 「哪个号」这一维成了常量"
        );

        // ④ 中转没在跑 ⇒ **当场拒，而且一个字节都没送出去**（`KH2B2`②：不许静默）。
        let before = SENT.with(|v| v.borrow().len());
        answer(&["acct-a"], false);
        let account = LaunchAccount::Named {
            config_dir: "/h/.claude-accts/acct-a".to_string(),
        };
        assert!(
            launch_local(&action, None, None, Some(&account), None).is_err(),
            "中转没在跑却照旧起出去了"
        );
        assert_eq!(
            SENT.with(|v| v.borrow().len()),
            before,
            "拒了却还是往外送了一条命令 —— 那条「当场拒」只拒在返回值上"
        );

        // ═══════════════════════════════════════════════════════════════════
        // ⑤⑥ 🔴 `D7 阻-4`：**「哪一次拉起」这一维，在送出去的那一串上也要翻得动**
        // ═══════════════════════════════════════════════════════════════════
        //
        // 上面 ①–④ 全部走 `Resume("sid-1")` ⇒ `action` 这一维的输入域 = 1。
        // 刀 `S2`（`New => return Ok(String::new())`）在**这条判据上也全绿**，
        // 而**新开会话是 `KH2B6` 逐字写着「现在生产可达」的一条路**。
        answer(&["acct-a"], true);
        let acct = LaunchAccount::Named {
            config_dir: "/h/.claude-accts/acct-a".to_string(),
        };

        // ⑤ **`New` 那一支**：送出去的那一串必须也带中转注入，且账号段是这次的号。
        launch_local(&LocalPsAction::New, None, None, Some(&acct), None)
            .expect("新开会话走中转这一趟不该失败");
        let new_sent = last_sent();
        assert!(
            new_sent.contains("ANTHROPIC_BASE_URL") && new_sent.contains("/acct-a/"),
            "\n★★ **新开会话送出去的那一串里没有中转注入** —— 刀 `S2` 的形状：\n\
             `LocalPsAction::New => return Ok(String::new())`。\n\
             生产后果：一个 api-key 号**新开**一个会话 ⇒ claude 直连官方端点、\n\
             第三方 key 用不上，而全量门禁四个数一格不动。实得 = {new_sent:?}"
        );

        // ⑥ **`Resume` 那一支的 sid 不是写死的**：换一个 sid，送出去的那一串跟着变。
        launch_local(
            &LocalPsAction::Resume("sid-9".to_string()),
            None,
            None,
            Some(&acct),
            None,
        )
        .expect("这一趟不该失败");
        let resumed_9 = last_sent();
        assert!(
            resumed_9.contains("/sid-9'"),
            "\n换一个会话 id，送出去的那一串里的 `<key>` 段没跟着变 —— 刀 `S1` 的形状：\n\
             `Resume(_sid) => Some(\"sid-1\")`。实得 = {resumed_9:?}"
        );
        // 反空真：这两趟本来就该是两条不同的串（否则上面两条里有一条在数同一份东西）。
        assert_ne!(
            new_sent, resumed_9,
            "新开与 resume 送出去的是同一串 —— 「哪一次拉起」这一维成了常量"
        );
    }

    // ═════════════════════════════════════════════════════════════════════════
    // 🔴 `K-P5b` `KP5BD1`：**起会话方把身份塞进了下一跳的进程环境**
    // ═════════════════════════════════════════════════════════════════════════

    /// 从送出去的那一串里，把身份那一段（到第一个 `; ` 为止）抠出来。
    ///
    /// ⚠ 用**变量名**定位，不用位置定位：位置是会变的（今天身份那一段前面还有中转前缀），
    /// 而「哪一段是身份」这件事只有变量名说得准。
    fn identity_segment(cmd: &str) -> Option<&str> {
        let i = cmd.find(LAUNCH_ID_VAR)?;
        let seg = &cmd[i..];
        Some(&seg[..seg.find("; ").unwrap_or(seg.len())])
    }

    /// ★★★ `KP5BD1`：`launch_local` **真正交出去的那一串**里，身份被塞进了进程环境。
    ///
    /// # 它量的是行为，不是文本（这条纪律是 `D6` 刀 `Y1` 花一整轮买回来的）
    ///
    /// 量文本那一形已经在本文件里被打穿过一次：`relay_prefix_for_launch` 照样被调、
    /// 答案照样对，而拼装那一行把它扔了 ⇒ 三个文本锚点一处不少、全量门禁四个数与干净树逐字相同。
    /// ⇒ 本条一个字节的源码都不扫，只看 [`LaunchSink`] 那条缝上**真正交出去的那个字符串**。
    ///
    /// # 五格，每格能被哪一刀翻掉
    ///
    /// | 格 | 断的是什么 | 翻掉它的形状 |
    /// |---|---|---|
    /// | ① | 送出去的那一串里**有** `CCM_LAUNCH_ID=<sid>` 这一句 | 把 `+ &identity.prefix` 从拼装那一行删掉（刀 `Y1` 同形） |
    /// | ② | 换一个 sid ⇒ 那一段跟着变 | `Resume(_) => Some("sid-1")`（身份写死） |
    /// | ③ | **新开**那一支也有身份，且两趟 token 不同 | `New => String::new()`（只给 resume 落身份 —— 而 `K-P5 §3 三` 现打的正是「新开那一支没有 sid」，它才是本件的正主） |
    /// | ④ | **铸法是共用那一份**：喂一个过不了白名单的 sid ⇒ token **不是**那个 sid | 在本文件里另写一份 `match { Resume(s) => s.clone(), … }`（第二份铸法，白名单回落那一格丢了） |
    /// | ⑤ | 平台那一维翻得动：`windows = true` ⇒ 渲成 `$env:` 形态 | 把 `windows` 那一格写死（`D6` 刀 `Xb` 同形，生产后果是 Windows 上塞出一句 POSIX `export`） |
    ///
    /// # ⚠ 它买不到什么（如实写，别读宽）
    ///
    /// - **走 ccm 容器那一支身份到不到得了 agent 进程**：到不了。本条喂 `tmux_name = None`
    ///   ⇒ 走的是回落那条路（渲染器早退，见 [`NO_TMUX_NAME`]）。容器那一支上外侧这句 `export`
    ///   会在 tmux 边界被吃掉（与 `K-H2b` 给 `ANTHROPIC_BASE_URL` 踩过的**同一个坑**，
    ///   那一次的修法是在 `shared/ccm` 的容器载荷内侧补一句转发）——
    ///   `shared/ccm` **本拍是红线文件**，那一句没补 ⇒ 这一格**今天是个洞**，
    ///   登记在 `launcher_identity_registry` 的 `L1` 那一行里，别读成「已经全覆盖」。
    /// - **读的那一侧**：daemon 从 `/proc/<pid>/environ` 读回来、经 wire 帧发出去 —— 本拍**没有做**
    ///   （面在 `remote-daemon-proto/`，不在本拍写区）。⇒ 今天这个变量**有人写、没人读**。
    /// - **Windows 上的运行时行为**：一行都没量（这台机器是 Linux）。⑤ 买到的只是
    ///   「平台那一格翻得动、渲出来的形态跟着变」，不是「PowerShell 里真的设上了」。
    #[test]
    fn the_launcher_plants_the_session_identity_into_the_process_environment() {
        use std::cell::{Cell, RefCell};
        thread_local! {
            static SENT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
            static WINDOWS_ANSWER: Cell<bool> = const { Cell::new(false) };
        }
        fn recorder(cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
            SENT.with(|v| v.borrow_mut().push(cmd.to_string()));
            Ok(())
        }
        fn no_rows() -> Vec<String> {
            Vec::new()
        }
        fn relay_up() -> bool {
            true
        }
        fn spy_windows() -> bool {
            WINDOWS_ANSWER.with(Cell::get)
        }
        fn last_sent() -> String {
            SENT.with(|v| v.borrow().last().cloned().expect("这一趟什么都没送出去"))
        }

        let _sink = override_launch_sink(LaunchSink(recorder));
        // 表里一行都没有 ⇒ 中转前缀恒空 ⇒ 本条量到的只有身份那一段（两件事分开量）。
        let _facts = override_relay_facts(RelayFactSources {
            rows: no_rows,
            running: relay_up,
            windows: spy_windows,
        });
        let account = LaunchAccount::Named {
            config_dir: "/h/.claude-accts/acct-a".to_string(),
        };

        // ① resume：身份就是这条会话的 sid，逐字节。
        let sid = "0198f0d2-1111-4222-8333-444455556666";
        launch_local(
            &LocalPsAction::Resume(sid.to_string()),
            None,
            None,
            Some(&account),
            None,
        )
        .expect("这一趟不该失败");
        let first = last_sent();
        assert!(
            !first.contains("ANTHROPIC_BASE_URL"),
            "表里一行都没有，却混进了中转前缀 —— 本条的替身没装上，下面几格量的不是身份：{first:?}"
        );
        let want_first = format!("{LAUNCH_ID_VAR}='{sid}'");
        assert_eq!(
            identity_segment(&first),
            Some(want_first.as_str()),
            "\n★★ **起会话方没把身份塞进进程环境** —— 刀的形状是把\n\
             `+ &identity.prefix` 从拼装那一行删掉（`D6` 刀 `Y1` 同形：\n\
             token 照样铸得出来，只是没拼上去）。\n\
             生产后果：起出来的那条会话**在环境里说不出自己是谁**，\n\
             读的那一侧只能退回去扫窗口标题 —— 那正是本件要消灭的东西。\n\
             实得整串 = {first:?}"
        );

        // ② 换一个 sid ⇒ 身份那一段跟着变（不是常量）。
        let sid9 = "0198f0d2-9999-4222-8333-444455556666";
        launch_local(
            &LocalPsAction::Resume(sid9.to_string()),
            None,
            None,
            Some(&account),
            None,
        )
        .expect("这一趟不该失败");
        let second = last_sent();
        let want_second = format!("{LAUNCH_ID_VAR}='{sid9}'");
        assert_eq!(
            identity_segment(&second),
            Some(want_second.as_str()),
            "换一条会话，环境里的身份没跟着变 —— 「这条会话是谁」成了常量：{second:?}"
        );

        // ③ **新开**那一支也落身份，而且两趟拿到的是两个不同的 nonce。
        //   `K-P5 §3 三` 现打：5 个起会话方**没有一处**在起新会话时知道 sid
        //   ⇒ 新开这一支才是本件的正主，它落不落身份不能靠 resume 那一支代言。
        launch_local(&LocalPsAction::New, None, None, Some(&account), None)
            .expect("新开这一趟不该失败");
        let new_a = identity_segment(&last_sent())
            .expect("新开那一支送出去的串里没有身份 —— `New => String::new()` 那一刀的形状")
            .to_string();
        launch_local(&LocalPsAction::New, None, None, Some(&account), None)
            .expect("新开这一趟不该失败");
        let new_b = identity_segment(&last_sent()).expect("同上").to_string();
        assert_ne!(
            new_a, new_b,
            "两次新开拿到同一个身份 —— nonce 成了常量，两条会话在环境里说自己是同一个人"
        );
        assert!(
            !new_a.contains(sid) && !new_a.contains(sid9),
            "新开那一支把上一条 resume 的 sid 当成了自己的身份：{new_a:?}"
        );

        // ④ **铸法是共用那一份**：喂一个过不了 `relay_segment_is_safe` 白名单的 sid，
        //   共用那份铸法会回落到 nonce；本文件里另写的第二份不会。
        //   ⇒ 这一格是「有没有真的调那一份」唯一翻得出来的一维。
        //
        //   ⚠ 夹具**必须同时满足两件事**，第一版选错了（现打修的）：
        //   ① 过得了 `local_launch_choice` 那道 sid 校验（字母数字 + `-` + `_`，**无长度上限**）——
        //      带 `/` 的串在那一关就被拒了，整趟 `launch_local` 回 `Err`，
        //      本条量到的是「拉起失败」而不是「身份铸法」；
        //   ② 过不了 `relay_segment_is_safe`（同一套字符集，但**多一条 ≤128 字节**）。
        //   ⇒ 两者的差集今天恰好只有**长度**这一维 ⇒ 用一个 129 字节的纯字母 sid。
        let bad = "a".repeat(129);
        let bad = bad.as_str();
        assert!(
            !crate::backend::control::payload::relay_segment_is_safe(bad),
            "夹具选错了：这个 sid 过得了白名单 ⇒ 下面那条断言是空真"
        );
        launch_local(
            &LocalPsAction::Resume(bad.to_string()),
            None,
            None,
            Some(&account),
            None,
        )
        .expect("这一趟不该失败");
        let dirty = identity_segment(&last_sent())
            .expect("这一趟没有身份")
            .to_string();
        assert!(
            !dirty.contains(bad),
            "\n★★ **本文件自己又铸了一份身份** —— 一个过不了白名单的 sid 被原样当成了身份。\n\
             共用的那份铸法（`payload::route_key_for_session`）在这一格会回落到 nonce；\n\
             会这样答的只有第二份实现。⇒ `KP5BD1`「铸法只有一份」当场破。实得 = {dirty:?}"
        );

        // ⑤ 平台那一维翻得动：`windows = true` ⇒ 渲成 PowerShell 形态。
        //   写死那一格的生产后果是 Windows 上往 PowerShell 串里塞一句 POSIX `export`
        //   ⇒ 身份注入整个失效（`D6` 刀 `Xb` 在中转那一格上的同一形）。
        WINDOWS_ANSWER.with(|c| c.set(true));
        launch_local(
            &LocalPsAction::Resume(sid.to_string()),
            None,
            None,
            Some(&account),
            None,
        )
        .expect("这一趟不该失败");
        let ps = last_sent();
        assert!(
            ps.contains(&format!("$env:{LAUNCH_ID_VAR}='{sid}'; ")),
            "\n把「这台机是不是 Windows」翻成 true，身份那一句还是 POSIX 形态 ——\n\
             那一格是个常量，Windows 上会往 PowerShell 串里塞一句 `export`。实得 = {ps:?}"
        );
        // 反空真：POSIX 那一趟本来就该拿不到这个形状（否则上面那条恒真）。
        //
        // ⚠ **09-09 订正（云端 windows-latest 首跑逮到）**：这里原写
        //   `!first.contains("$env:")` —— 断的是**整串**。而 `launch_local` 里 `base`
        //   那一格是 `#[cfg(windows)]` 选的（它**不走**这条缝），Windows 上它渲出
        //   `$env:CLAUDE_CONFIG_DIR='…'; ` ⇒ 原断言在 Windows 上**恒假**，
        //   量的根本不是身份那一段。
        //   ⇒ 收窄到**只盯身份那一段**，并补一条**正**的：POSIX 那一趟必须真的渲出
        //   `export <变量名>=`。两条合起来比原来那一条**更严** ——
        //   原写法在「身份那一段整个不见了」时是绿的。
        assert!(
            first.contains(&format!("export {LAUNCH_ID_VAR}=")),
            "POSIX 那一趟没渲出 `export {LAUNCH_ID_VAR}=` —— 上面那条断言是恒真的：{first:?}"
        );
        assert!(
            !first.contains(&format!("$env:{LAUNCH_ID_VAR}=")),
            "POSIX 那一趟把身份渲成了 `$env:` —— 上面那条断言是恒真的：{first:?}"
        );
    }

    // ═════════════════════════════════════════════════════════════════════════
    // 🔴🔴 `K-P5h` `KP5HD1`：**铸出来的那个 token 真的交到了调用方手上**
    // ═════════════════════════════════════════════════════════════════════════

    /// ★★★ `KP5HD1`：[`launch_local`] / [`new_local_session`] 回的那个串，
    /// **就是塞进那次拉起进程环境里的同一个 token**；而拼出来的命令串**一个字节没变**。
    ///
    /// # 它与旁边那条老判据的分工（两条都要，别合并）
    ///
    /// [`the_launcher_plants_the_session_identity_into_the_process_environment`] 买的是
    /// 「**塞进去了**」；本条买的是「**交出来了**」。`K-P5g` 交回时现打的卡点逐字是
    /// 「写侧把 token 铸完就扔」—— 那一天上面那条老判据**全绿**，
    /// 因为塞进去这件事一直是对的，缺的是**没有任何调用方手上有那个 token**。
    /// ⇒ 两件事各自要有自己的牙。
    ///
    /// # 五格，每格能被哪一刀翻掉
    ///
    /// | 格 | 断的是什么 | 翻掉它的形状 |
    /// |---|---|---|
    /// | ① | 回的那个串**逐字节**是命令里 `CCM_LAUNCH_ID=` 后面那个值 | `Ok("x".into())`（回一个常量）· `Ok(identity.prefix)`（回错那一半） |
    /// | ② | 两趟**新开**回的是两个不同的 token | 同上那个常量刀（`KP5HD1` 的死值验逐字点名的就是它） |
    /// | ③ | resume 那一支回的是 sid 本身 | 把 `New`/`Resume` 两支的返回值接反 |
    /// | ④ | **additive**：交出来这件事没改动命令串 —— 送出去的那一串逐字节等于 `前缀 + 基准串` | 在拼装那一行顺手动一下（多拼 / 少拼 / 换序，`D6` 刀 `Y1` 那一族） |
    /// | ⑤ | 拉起**失败**时不回 token | 把 `?` 换成忽略错误（那会让调用方去等一条不存在的会话） |
    ///
    /// # ⚠ 它买不到什么（如实写）
    ///
    /// - **拿这个 token 真能反查出 sid**：那要一条真的跑起来的会话 + 一个真 daemon。
    ///   本条只买到「token 到了调用方手上」，反查那一跳的判据在前端
    ///   （`src/accounts.vitest.ts` 的 `K-P5h` 那一组，`KP5HD2`）。
    /// - **走 ccm 容器那一支**：与老判据同一个洞（喂 `tmux_name = None` ⇒ 走回落那条路），
    ///   登记在 `launcher_identity_registry` 的 `L1` 那一行里。
    /// - **Windows 上的运行时行为**：④ 那一格按平台各自取基准串，但这台机器是 Linux，
    ///   PowerShell 那一侧一行都没真跑过。
    #[test]
    fn the_minted_identity_token_is_handed_back_to_the_caller() {
        use std::cell::RefCell;
        thread_local! {
            static SENT2: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        }
        fn recorder(cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
            SENT2.with(|v| v.borrow_mut().push(cmd.to_string()));
            Ok(())
        }
        fn boom(_cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
            Err("拉起失败（判据夹具）".to_string())
        }
        fn no_rows() -> Vec<String> {
            Vec::new()
        }
        fn relay_up() -> bool {
            true
        }
        fn not_windows() -> bool {
            false
        }
        fn last_sent() -> String {
            SENT2.with(|v| v.borrow().last().cloned().expect("这一趟什么都没送出去"))
        }

        let _sink = override_launch_sink(LaunchSink(recorder));
        // 表里一行都没有 ⇒ 中转前缀恒空 ⇒ ④ 那一格量的是「身份 + 基准串」这两段，
        // 中转那一段由它自己那条判据管（两件事分开量）。
        let _facts = override_relay_facts(RelayFactSources {
            rows: no_rows,
            running: relay_up,
            windows: not_windows,
        });
        let account = LaunchAccount::Named {
            config_dir: "/h/.claude-accts/acct-a".to_string(),
        };

        // ① **新开**那一支：回的那个串就是命令里那个值。
        //    ⚠ 这里刻意**不**拿 `identity_segment` 的整段去比 —— 那样只要回的是
        //    「`CCM_LAUNCH_ID=…` 这一整句」就绿了，而本条要的是**值本身**。
        let token_a = launch_local(&LocalPsAction::New, None, None, Some(&account), None)
            .expect("新开这一趟不该失败");
        assert!(
            !token_a.trim().is_empty(),
            "新开那一趟回了个空串 —— 「交出来」这一步等于没做"
        );
        let sent_a = last_sent();
        let want_a = format!("{LAUNCH_ID_VAR}='{token_a}'");
        assert_eq!(
            identity_segment(&sent_a),
            Some(want_a.as_str()),
            "\n★★ **交回来的 token 不是塞进环境里的那一个。**\n\
             刀的形状：`Ok(\"x\".into())`（回一个常量）或 `Ok(identity.prefix)`（回错那一半）。\n\
             生产后果：起会话方拿着一个**谁也不认识**的串去反查 sid ⇒ 永远查不到，\n\
             而「查不到就不猜」会让整条回填静默失效 —— 与本件没做完全一样。\n\
             交回来的 = {token_a:?}，送出去的整串 = {sent_a:?}"
        );

        // ② 两趟新开 ⇒ 两个**不同**的 token（常量刀在这里也红一次，两格互为纵深）。
        let token_b = launch_local(&LocalPsAction::New, None, None, Some(&account), None)
            .expect("新开这一趟不该失败");
        assert_ne!(
            token_a, token_b,
            "两趟新开交回来的是同一个 token —— 「交出来」那一步回的是常量，\n\
             而 `KP5HD1` 的死值验逐字点名的就是这一刀"
        );

        // ③ resume 那一支：token 就是 sid 本身（`route_key_for_session(Some(sid))` 原样返回）。
        //    ⚠ 这一格**不是**本件的正主（`K-P5g` 现打过它会退化成布尔谓词），
        //    写在这里只为钉住「两支没接反」。
        let sid = "0198f0d2-1111-4222-8333-444455556666";
        let token_r = launch_local(
            &LocalPsAction::Resume(sid.to_string()),
            None,
            None,
            Some(&account),
            None,
        )
        .expect("这一趟不该失败");
        assert_eq!(
            token_r, sid,
            "resume 那一支交回来的不是 sid —— 两支的返回值接反了"
        );

        // ④ ★★ **additive**：送出去的那一串逐字节 = 「身份那一句 + 基准串」。
        //    两边都由**生产函数现算**，本条不抄第二份拼装规则 ——
        //    抄一份的话，改了生产那一行、判据跟着抄错，两边一起错还全绿。
        {
            let act = LocalPsAction::Resume(sid.to_string());
            #[cfg(windows)]
            let base = build_local_ps_command(&act, None, Some(&account))
                .expect("基准串算不出来，④ 这一格是空真");
            #[cfg(not(windows))]
            let base = build_local_posix_command(&act, None, Some(&account))
                .expect("基准串算不出来，④ 这一格是空真");
            let want = format!("{}{base}", launch_identity_env_prefix(&token_r, false));
            let got = last_sent();
            // 反空真：基准串不是空的（空的话下面那条就退化成「送出去的等于身份那一句」）。
            assert!(
                base.len() > 10,
                "基准串只有 {} 字节 —— ④ 这一格在拿一个空壳对拍",
                base.len()
            );
            assert_eq!(
                got, want,
                "\n★★ **「把 token 交出来」这一拍改动了拼出来的命令串** —— \
                 而 `KP5HD1` 逐字要求「拼出来的命令串一个字节没变」。\n\
                 additive 的全部含义就是这一行；两边都是生产函数现算的，\n\
                 对不上说明拼装那一行被动过（多拼 / 少拼 / 换序，`D6` 刀 `Y1` 那一族）。"
            );
        }

        // ⑤ 拉起**失败**时不回 token —— 回了会让调用方去等一条根本不存在的会话。
        let _boom = override_launch_sink(LaunchSink(boom));
        let failed = launch_local(&LocalPsAction::New, None, None, Some(&account), None);
        assert!(
            failed.is_err(),
            "拉起失败了却回了 `Ok` —— 失败被吞掉，调用方会去等一条不存在的会话：{failed:?}"
        );
    }

    // ═════════════════════════════════════════════════════════════════════════
    // 🔴🔴 `D8 阻-1`：**账号传递链那三跳** —— 生产主路，第九轮之前一格判据都没有
    // ═════════════════════════════════════════════════════════════════════════
    //
    // # 病是怎么长出来的（`D8 §4` 第 2 条，别只读结论）
    //
    // 本件所有承重的行为判据（[`the_relay_prefix_is_really_prepended_to_the_command_that_gets_launched`]
    // / [`the_launch_side_really_asks_those_two_take_points_and_uses_their_answers`]）的
    // **驱动入口都是 [`launch_local`] 或更下游**，而**生产入口在它上面两跳**：
    //
    // ```text
    // #[tauri::command] resume_history_session  →  resume_impl  →  launch_local
    // #[tauri::command] new_local_session       ─────────────────→  launch_local
    // ```
    //
    // ⇒ **判据的射程上界正好卡在 `launch_local`，而九轮买的那些牙全长在它的下游。**
    // `D8` 三刀实打（三处调用点各写一次 `account.filter(|_| false)`）：
    // **全量门禁四个数一格不动（`1328 / 一致 / 493 / 1512`）、`GATE: OK`，而中转前缀恒空。**
    //
    // 🔴 **而看起来在守它的那把尺子，作用域对不上事实**：`src/ipc/commands.vitest.ts:402`
    //「每一处起本机会话的调用都带 `account`」守的是 **TS 那一侧**（`D8` 的 `E4` 实测：
    // 在前端调用点上下同一形状的刀，`npm` 那道门当场红）。
    // **同一根链的 Rust 这一侧三跳，一格都没守。** —— 「两条路只修了一条」。
    //
    // # 为什么是**三条**判据而不是一条（`K22` 的口径）
    //
    // **N 支信号要 N 个只由这一支挡住的探针。** 三跳各自能独立答错，所以三刀的**红名单
    // 必须两两不同**（本轮实打的三张红名单在件文件 `§12` 的变异表里逐行给了）：
    //
    // | 刀 | 落在哪一处 | 红名单 |
    // |---|---|---|
    // | ① | `resume_impl` 里那一处 `account` | 探针 ① **与** ② 一起红 |
    // | ② | `resume_history_session` 里那一处 `account.as_ref()` | **只有**探针 ② 红 |
    // | ③ | `new_local_session` 里那一处 `account.as_ref()` | **只有**探针 ③ 红 |
    //
    // ⚠ 探针 ② 在刀 ① 上也红，是**链的包含关系**（②的驱动路径经过①），不是重复计数：
    // 三张红名单两两不同 ⇒ 三刀可区分 ⇒ **三格**。反过来只写一条探针 ② 的话，
    // ①②两刀的红名单会相同 ⇒ 那才是「N 支只买了一格」。
    //
    // # ⚠ 它们**买不到**什么（如实写）
    //
    // - **前端到底传没传 `account`**：那是 `commands.vitest.ts:402` 那把尺子的面，本族只管
    //   「传进来之后 Rust 这一侧有没有原样送到拼前缀那一行」。**两把尺子各守一侧，别只改一处。**
    // - **`launch_local` 以下的任何一格**：那是上面那条 `…_is_really_prepended_…` 的面。
    //   本族刻意**不**重复买它 —— 三条探针的反空真只断「不走中转那一趟长得像一条真拉起」。
    // - **Windows 那条腿**：`PRODUCTION_LAUNCH_SINK` 在 Windows 上是另一个送法，
    //   而本族喂的是记账替身 ⇒ **送法**那一格三条探针一格都驱动不到（登记，不假装）。
    //   〔09-09 订正：这里原写「而门禁跑在 Linux ⇒ 本族与本文件其余判据同样只驱动
    //    POSIX 那一支」—— **那半句今天是假话**。云端 `Rust lint + test` 跑在
    //    windows-latest，而 `launch_local` 里 `base` 那一格是 `#[cfg(windows)]` 选的
    //    ⇒ 本族在 CI 上驱动的是 **PowerShell** 那一支，在开发机上才是 POSIX 那一支。〕

    thread_local! {
        /// `D8 阻-1` 三支探针共用的记账台：这一趟真正交出去的 `(命令串, cwd)`。
        /// **线程局部** ⇒ 三条判据并行跑互不干扰（`cargo test` 一测一线程）。
        static ENTRY_SENT: std::cell::RefCell<Vec<(String, Option<String>)>> =
            const { std::cell::RefCell::new(Vec::new()) };
        /// 这一拍中转表里有哪几行（探针自己写）。
        static ENTRY_ROWS: std::cell::RefCell<Vec<String>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }

    fn entry_recorder(cmd: &str, cwd: Option<&str>) -> Result<(), String> {
        ENTRY_SENT.with(|v| {
            v.borrow_mut()
                .push((cmd.to_string(), cwd.map(str::to_string)))
        });
        Ok(())
    }
    fn entry_rows() -> Vec<String> {
        ENTRY_ROWS.with(|v| v.borrow().clone())
    }
    fn entry_running() -> bool {
        true
    }
    /// 装台：一条会记账的送法 + 一组答案由探针写死的取值口。两个守卫掉出作用域自动还原。
    fn entry_stage() -> (LaunchSinkGuard, RelayFactsGuard) {
        ENTRY_SENT.with(|v| v.borrow_mut().clear());
        ENTRY_ROWS.with(|v| v.borrow_mut().clear());
        (
            override_launch_sink(LaunchSink(entry_recorder)),
            override_relay_facts(RelayFactSources {
                rows: entry_rows,
                running: entry_running,
                // 平台那一格照生产那个取值口（翻它的是别处那条判据的第 ④ 格）。
                windows: platform_is_windows,
            }),
        )
    }
    fn entry_answer(rows: &[&str]) {
        ENTRY_ROWS.with(|v| *v.borrow_mut() = rows.iter().map(|s| s.to_string()).collect());
    }
    fn entry_last() -> (String, Option<String>) {
        ENTRY_SENT.with(|v| v.borrow().last().cloned().expect("这一趟什么都没送出去"))
    }

    /// ★★ **探针 ①**〔`D8 阻-1`，`KH2B1`〕：`resume_impl` 这一跳把 `account` / `session_id` / `cwd`
    /// **原样**交给 [`launch_local`]。
    ///
    /// 刀 `D8P32`（`resume_impl` 里那一行 `account,` 写成 `account.filter(|_| false)`）
    /// 在本条落地之前是**全量门禁四个数一格不动**的。
    /// ⚠ 行号带尖号（`D8 阻-5` 的纪律）：`c02d954` 上是 `:1826`，本尖上是 `:1856`；
    /// **锚点用文本别用行号** —— `^        account,$` 在本文件全文恰好 **1** 处。
    #[test]
    fn the_resume_hop_above_launch_local_carries_the_account_and_the_sid_through() {
        let (_sink, _facts) = entry_stage();
        let dir = "/h/.claude-accts/acct-r1";
        let account = LaunchAccount::Named {
            config_dir: dir.to_string(),
        };
        // 中性名（`brief` 12：断言用的子串不许取自夹具名字里带含义的那半）。
        let cwd = "/p/one";

        // ① 反空真 —— 表里没有这个号 ⇒ 这条入口本来就该送出一条**不带中转注入**的命令，
        //    而且它得像一条真的本机拉起（这个号的 configDir 在里面）。
        //    没有这一格，下面那条「有前缀」的断言在「整条链恒空」时会读成假红/假绿。
        entry_answer(&[]);
        resume_impl("sid-r1", cwd, None, Some(&account), None).expect("不走中转这一趟不该失败");
        let (bare, bare_cwd) = entry_last();
        assert!(
            bare.contains(dir),
            "基准串里连这个号的 configDir 都没有 —— 这条入口根本没把账号送下去：{bare:?}"
        );
        assert!(
            !bare.contains("ANTHROPIC_BASE_URL"),
            "表里没有这个号，送出去的那一串却带着中转注入：{bare:?}"
        );
        assert_eq!(
            bare_cwd.as_deref(),
            Some(cwd),
            "这一跳把 `cwd` 弄丢了 —— 会话会起在默认目录上，而 toast 照报成功"
        );

        // ② 表里有这个号 ⇒ 送出去的那一串带中转注入，账号段与 sid 段都是**这一发**的。
        entry_answer(&["acct-r1"]);
        resume_impl("sid-r1", cwd, None, Some(&account), None).expect("走中转这一趟不该失败");
        let (routed, _) = entry_last();
        assert!(
            routed.contains("ANTHROPIC_BASE_URL"),
            "\n★★ **`resume_impl` 这一跳把账号扔了** —— 刀 `D8P32` 的形状：\n\
             `launch_local(…, account.filter(|_| false), …)`。\n\
             生产后果：历史页 resume 一个 api-key 号 ⇒ claude 直连官方端点、第三方 key 用不上，\n\
             而在 `D8` 实测里**全量门禁四个数一格不动**。实得 = {routed:?}"
        );
        assert!(
            routed.contains(&format!("/{}/", "acct-r1")),
            "路由键里的账号段不是这一发的号：{routed:?}"
        );
        assert!(
            routed.contains("/sid-r1'"),
            "路由键里的 `<key>` 段不是这一发的 sid —— `session_id` 在这一跳被换掉了：{routed:?}"
        );
        // 逐字节：前缀 + 基准串。剥掉第一段之后剩下的**必须**逐字节等于 ① 那趟的基准串
        // —— 「多注入一个前缀」与「顺手把命令体也换了」在只断 `contains` 的判据上同形。
        let (head, tail) = routed.split_once("; ").expect("走中转那一趟没有前缀段");
        // ⚠ **09-09 订正（云端 windows-latest 首跑逮到）**：`entry_stage` 的平台那一格
        //   照**生产取值口**（真答案），所以在 Windows 上中转前缀**真的**渲成 PS 形态
        //   `$env:ANTHROPIC_BASE_URL='…'`，而这里原来把 POSIX 那一种写死了。
        //   ⇒ 按**这台机器**算出该有的那一种，各断各的。
        //   🔴 **不是「两种都放行」** —— 那会把「渲错了平台形态」这一刀松掉
        //   （`D6` 刀 `Xb` 的正主）。现在两个平台各自只放行一种：
        //   Linux 上渲成 `$env:` 照样红，Windows 上渲成 `export` 也照样红。
        let want_head = if platform_is_windows() {
            "$env:ANTHROPIC_BASE_URL="
        } else {
            "export ANTHROPIC_BASE_URL="
        };
        assert!(
            head.starts_with(want_head),
            "第一段不是中转注入（这台机器该渲成 {want_head:?}）：{head:?}"
        );
        assert_eq!(
            tail, bare,
            "剥掉中转前缀之后的命令体与不走中转那一趟不一样 —— 这一跳除了账号还动了别的"
        );
    }

    /// ★★ **探针 ②**〔`D8 阻-1`，`KH2B1`〕：**前端真正调的那条命令**
    /// [`resume_history_session`]（`#[tauri::command]`）把五个入参原样交给 [`resume_impl`]。
    ///
    /// 刀 `D8P32b`（[`resume_history_session`] 里那一行 `account.as_ref(),` 写成
    /// `account.as_ref().filter(|_| false)`）在本条落地之前是**全量门禁四个数一格不动**的。
    /// ⚠ 行号带尖号（`D8 阻-5`）：`c02d954` 与本尖上都是 `:892`（本轮加的行都在它下面）。
    ///
    /// ⚠ 第 ② 格是**对拍**（同一组输入喂两条入口，两串必须逐字节相同）——
    /// 它买的是「这一跳一个入参都没被换掉」，比只断「有前缀」宽一格：
    /// 换掉 `launcher` / `tmux_name` / `cwd` 中任何一个，这一格也红。
    #[test]
    fn the_resume_command_the_frontend_calls_hands_all_five_arguments_down_unchanged() {
        let (_sink, _facts) = entry_stage();
        let dir = "/h/.claude-accts/acct-r2";
        let cwd = "/p/two";
        entry_answer(&["acct-r2"]);

        // ① 从**那条 `#[tauri::command]`** 进去。
        resume_history_session(
            "sid-r2".to_string(),
            cwd.to_string(),
            Some("cc".to_string()),
            Some(LaunchAccount::Named {
                config_dir: dir.to_string(),
            }),
            None,
        )
        .expect("这一趟不该失败");
        let (from_cmd, cmd_cwd) = entry_last();
        assert!(
            from_cmd.contains("ANTHROPIC_BASE_URL") && from_cmd.contains("/acct-r2/"),
            "\n★★ **那条 `#[tauri::command]` 把账号扔了** —— 刀 `D8P32b` 的形状：\n\
             `resume_impl(…, account.as_ref().filter(|_| false), …)`。\n\
             这一跳就是**历史页 resume 那个按钮真正调的那条命令**，\n\
             而在 `D8` 实测里一刀下去**全量门禁四个数一格不动**。实得 = {from_cmd:?}"
        );
        assert!(
            from_cmd.contains("/sid-r2'"),
            "路由键里的 `<key>` 段不是这一发的 sid：{from_cmd:?}"
        );

        // ② 对拍：同一组输入直接喂下一跳，两串必须**逐字节相同**。
        //    ⇒ 这一跳换掉五个入参里的**任何一个**（不只是 `account`），本格都红。
        let account = LaunchAccount::Named {
            config_dir: dir.to_string(),
        };
        resume_impl("sid-r2", cwd, Some("cc"), Some(&account), None).expect("这一趟不该失败");
        let (from_impl, impl_cwd) = entry_last();
        assert_eq!(
            from_cmd, from_impl,
            "\n那条 `#[tauri::command]` 与它下一跳送出去的不是同一串 —— \
             这一跳换掉了某个入参（不一定是 `account`）"
        );
        assert_eq!(cmd_cwd, impl_cwd, "`cwd` 在这一跳被换掉了");
    }

    /// ★★ **探针 ③**〔`D8 阻-1`，`KH2B1`〕：**「在该目录起新会话」那条命令**
    /// [`new_local_session`]（`#[tauri::command]`）把 `account` / `cwd` 原样交给 [`launch_local`]。
    ///
    /// 刀 `D8P33`（[`new_local_session`] 里那一行 `account.as_ref(),` 写成
    /// `account.as_ref().filter(|_| false)`）在本条落地之前是**全量门禁四个数一格不动**的。
    /// ⚠ 行号带尖号（`D8 阻-5`）：`c02d954` 上是 `:1871`，本尖上是 `:1901`。
    ///
    /// ⚠ 这条路**没有 sid**（`<key>` 段走 nonce，见 `payload::relay_key_for` 的表），
    /// 所以本条只钉账号段与 `cwd`；`<key>` 那一维归 `payload` 那一侧的判据。
    #[test]
    fn the_new_session_command_the_frontend_calls_carries_the_account_and_the_cwd_through() {
        let (_sink, _facts) = entry_stage();
        let tmp = TmpDir::new(); // `new_local_session` 会先核 `cwd` 是不是现存目录
        let cwd = tmp.0.to_string_lossy().into_owned();
        let dir = "/h/.claude-accts/acct-r3";

        // ① 反空真：表里没有这个号 ⇒ 不带中转注入，但 configDir 与 cwd 都得走到。
        entry_answer(&[]);
        new_local_session(
            cwd.clone(),
            None,
            Some(LaunchAccount::Named {
                config_dir: dir.to_string(),
            }),
        )
        .expect("不走中转这一趟不该失败");
        let (bare, bare_cwd) = entry_last();
        assert!(
            bare.contains(dir),
            "基准串里连这个号的 configDir 都没有：{bare:?}"
        );
        assert!(
            !bare.contains("ANTHROPIC_BASE_URL"),
            "表里没有这个号却带着中转注入：{bare:?}"
        );
        assert_eq!(
            bare_cwd.as_deref(),
            Some(cwd.as_str()),
            "这一跳把 `cwd` 弄丢了 —— 「在该目录起新会话」会起到别的目录去"
        );

        // ② 表里有这个号 ⇒ 带中转注入，且账号段是这一发的号。
        entry_answer(&["acct-r3"]);
        new_local_session(
            cwd.clone(),
            None,
            Some(LaunchAccount::Named {
                config_dir: dir.to_string(),
            }),
        )
        .expect("走中转这一趟不该失败");
        let (routed, routed_cwd) = entry_last();
        assert!(
            routed.contains("ANTHROPIC_BASE_URL") && routed.contains("/acct-r3/"),
            "\n★★ **「在该目录起新会话」那条命令把账号扔了** —— 刀 `D8P33` 的形状：\n\
             `launch_local(…, account.as_ref().filter(|_| false), …)`。\n\
             生产后果：一个 api-key 号**新开**会话 ⇒ 直连官方端点，\n\
             而在 `D8` 实测里一刀下去**全量门禁四个数一格不动**。实得 = {routed:?}"
        );
        assert_eq!(
            routed_cwd.as_deref(),
            Some(cwd.as_str()),
            "走中转这一趟把 `cwd` 弄丢了"
        );
        assert_ne!(
            bare, routed,
            "两趟送出去的是同一串 —— 「表里有没有这一行」这一维在这条入口上成了常量"
        );
    }
}
