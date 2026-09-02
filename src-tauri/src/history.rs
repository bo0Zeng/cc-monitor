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
fn render_local_ccm(
    action: &LocalPsAction,
    launcher: Option<&str>,
    account: Option<&LaunchAccount>,
    tmux_name: Option<&str>,
) -> Result<String, String> {
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
        return Err("没有 tmux 会话名（前端未传）—— 名字只许由 `mintTmuxName` 铸".into());
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
fn launch_local(
    action: &LocalPsAction,
    launcher: Option<&str>,
    cwd: Option<&str>,
    account: Option<&LaunchAccount>,
    tmux_name: Option<&str>,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        // Windows 那半**逐字不动**（`C12`：「windows不要tmux」）。`tmux_name` 在这一侧
        // 连读都不读 —— 读了就是给「Windows 也进容器」留了个口子。
        let _ = tmux_name;
        let ps = build_local_ps_command(action, launcher, account)?;
        crate::launch::launch_powershell_window(&ps, cwd)
    }
    #[cfg(not(windows))]
    {
        let cmd = match render_local_ccm(action, launcher, account, tmux_name) {
            Ok(rendered) => rendered,
            Err(why) => {
                // 与远端那条降级**同一种说法**：走回落是正常且预期的路径（没装 ccm 的机器
                // 每次拉起都走它）⇒ `debug` 而不是 `warn`。要查「为什么这台机没进 tmux」时，
                // 这一行是唯一线索。
                tracing::debug!("launch: 本机 CLI 渲染器降级 → 旧路：{why}");
                build_local_posix_command(action, launcher, account)?
            }
        };
        crate::launch::launch_local_posix(&cmd, cwd)
    }
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
#[tauri::command]
pub fn new_local_session(cwd: String, launcher: Option<String>) -> Result<(), String> {
    // F96：起新会话**依赖 cwd 定位**（不像 resume 靠 sid）——cwd 非空且不是现存目录（项目被
    // 移动/删除）就明确报错，别静默在默认目录起会话 + 弹假成功 toast。`launch_powershell_window`
    // 只把存在的 cwd 作窗口起始目录、失效则回落默认，对 resume 无害、对 new-session 是错目录。
    if !cwd.is_empty() && !std::path::Path::new(&cwd).is_dir() {
        return Err(format!("目录不存在，无法在此起新会话：{cwd}"));
    }
    // G3b：起**全新**会话不继承任何账号（那是「新开一个」的语义，不是分叉）。
    // P3t-Y2：起新会话这条**暂不传名字**（`None` ⇒ 渲染器诚实降级回旧路）。
    // 名字只许由 `mintTmuxName` 铸，而这条命令今天的两个前端调用点都还没传 ——
    // 在这里补一个默认名就是 F13 那个坑的第三次。接线归 P3t-Y2b。
    launch_local(
        &LocalPsAction::New,
        launcher.as_deref(),
        Some(&cwd),
        None,
        None,
    )?;
    tracing::info!("history: new local session in {cwd}");
    Ok(())
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
    #[test]
    fn the_delete_entry_point_actually_goes_through_the_fence() {
        let dir = std::env::temp_dir().join(format!(
            "ccm-delete-fence-probe-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let victim = dir.join("victim.jsonl");
        std::fs::write(&victim, "not yours").expect("造临时文件");

        let r = delete_history_session("sid".into(), victim.to_string_lossy().into_owned());
        let still_there = victim.exists();
        let _ = std::fs::remove_dir_all(&dir);

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
        let at = guard_core::find_pinned(&prod, "#[cfg(windows)]")
            .unwrap_or_else(|e| panic!("`#[cfg(windows)]` 不是恰好一处，先修锚点：{e}"));
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
}
