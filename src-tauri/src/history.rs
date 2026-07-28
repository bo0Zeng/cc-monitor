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
    pub last_activity: i64,
    /// 该项目下是否有 session 当前 PID 还活着
    pub has_live: bool,
    /// issue #16：数据来源。None=本地；Some(host)=远端（前端组头显示 [host] 徽标，
    /// 展开时改调 stream_remote_history_sessions）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HistorySessionEntry {
    pub session_id: String,
    pub project_path: String,
    pub project_name: String,
    pub ai_title: Option<String>,
    pub first_user_excerpt: String,
    pub started_at: i64,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forked_from_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forked_from_message_uuid: Option<String>,
    /// issue #16：数据来源。None=本地；Some(host)=远端（前端据此禁用 resume/delete、
    /// 查看走 stream_read_remote_session）。
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
pub struct EntryMetadata {
    #[serde(default)]
    pub starred: bool,
    #[serde(default, rename = "customTitle", alias = "custom_title")]
    pub custom_title: Option<String>,
    #[serde(default)]
    pub hidden: bool,
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
// **用 `serde_json::Value` 原样搬运**（不走有损的 `JsonlRecord` enum，避免丢 gitBranch/
// version/origin 等 schema 外字段）——除 sessionId/forkedFrom 两处有意改动外逐字段忠实。

/// 建分支的返回体（前端据此提示 / 一键 resume 新分支）。
#[derive(Debug, Serialize)]
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

/// **纯函数**（可注入直测）：从解析好的记录里，取分叉点 `message_uuid` 沿 parentUuid 回溯
/// 到根的线性前缀，逐条改写成原生分支格式。返回按「根 → 分叉点」顺序的输出记录。
///
/// - `message_uuid` 不在记录集中 → Err（前端传了不存在的 uuid）。
/// - 环防御：parentUuid 指回已访问节点即停（append-only jsonl 理论无环，防御性）。
fn build_branch_records(
    lines: &[serde_json::Value],
    message_uuid: &str,
    src_sid: &str,
    new_sid: &str,
) -> Result<Vec<serde_json::Value>, String> {
    use std::collections::{HashMap, HashSet};
    let mut by_uuid: HashMap<&str, &serde_json::Value> = HashMap::new();
    for v in lines {
        if let Some(u) = v.get("uuid").and_then(|x| x.as_str()) {
            by_uuid.entry(u).or_insert(v); // 保首见（幂等，容 at-least-once 重复行）
        }
    }
    if !by_uuid.contains_key(message_uuid) {
        return Err(format!(
            "refuse branch: message_uuid {message_uuid:?} not found in source session"
        ));
    }
    // 沿 parentUuid 回溯到根
    let mut chain: Vec<&str> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut cur = Some(message_uuid);
    while let Some(u) = cur {
        if !by_uuid.contains_key(u) || seen.contains(u) {
            break;
        }
        seen.insert(u);
        chain.push(u);
        cur = by_uuid[u]
            .get("parentUuid")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty());
    }
    chain.reverse(); // 根 → 分叉点

    let mut out = Vec::with_capacity(chain.len());
    for u in chain {
        let mut rec = by_uuid[u].clone();
        let obj = rec
            .as_object_mut()
            .ok_or("refuse branch: record is not a JSON object")?;
        obj.insert(
            "sessionId".into(),
            serde_json::Value::String(new_sid.to_string()),
        );
        obj.insert(
            "forkedFrom".into(),
            serde_json::json!({ "sessionId": src_sid, "messageUuid": u }),
        );
        out.push(rec);
    }
    // 原生 root 恒 parentUuid=null。回溯若因链断（parentUuid 指向集合外 / 坏行被跳）
    // 而止，新 root(out[0]) 会残留悬空 parentUuid → 置 null，产出干净自洽的根。
    if let Some(first) = out.first_mut() {
        if let Some(obj) = first.as_object_mut() {
            if obj.get("parentUuid").is_some_and(|p| !p.is_null()) {
                obj.insert("parentUuid".into(), serde_json::Value::Null);
            }
        }
    }
    Ok(out)
}

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
#[tauri::command]
pub fn resume_history_session(
    session_id: String,
    cwd: String,
    launcher: Option<String>,
) -> Result<(), String> {
    resume_impl(&session_id, &cwd, launcher.as_deref())
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
fn build_local_ps_command(
    action: &LocalPsAction,
    launcher: Option<&str>,
) -> Result<String, String> {
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
        return Ok(suffix(&l));
    }
    let def = agent.default_launcher();
    Ok(match agent.launcher_alias() {
        // 有 wrapper 别名（cc）：优先它、检测不到回退 default。
        Some(alias) => format!(
            "if (Get-Command {alias} -ErrorAction SilentlyContinue) {{ {} }} else {{ {} }}",
            suffix(alias),
            suffix(def),
        ),
        None => suffix(def),
    })
}

/// 薄委托——保留旧函数名与调用点不变（`resume_impl` 只改内部实现，DoD 要求两个
/// `#[tauri::command]` 的签名/行为/错误文案逐字节不变）。
fn build_resume_ps_command(session_id: &str, launcher: Option<&str>) -> Result<String, String> {
    build_local_ps_command(&LocalPsAction::Resume(session_id.to_string()), launcher)
}

/// Batch14-F41：wt.exe/PowerShell 拉起机械抽到 `launch.rs::launch_powershell_window`
/// （与远端 resume/attach 族共用），本函数只剩「构造本地 resume 命令体 + 委托拉起」。
/// 非 Windows：launch 层统一报错（仅 Windows 支持，错误文案改为中文）。
fn resume_impl(session_id: &str, cwd: &str, launcher: Option<&str>) -> Result<(), String> {
    let ps_command = build_resume_ps_command(session_id, launcher)?;
    crate::launch::launch_powershell_window(&ps_command, Some(cwd))?;
    tracing::info!("history: resumed sid={session_id}");
    Ok(())
}

/// F96（#62）：本地「在该目录起**新**会话」的 PowerShell 命令体——薄委托（同上，DoD 要求
/// 行为逐字节不变）。硬约束（用户 2026-07-15）：agent 名 / resume flag 全走活跃适配器，
/// 本函数不出现 agent 字面量。
fn build_new_session_ps_command(launcher: Option<&str>) -> Result<String, String> {
    build_local_ps_command(&LocalPsAction::New, launcher)
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
    let ps_command = build_new_session_ps_command(launcher.as_deref())?;
    crate::launch::launch_powershell_window(&ps_command, Some(&cwd))?;
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
mod tests {
    use super::*;

    /// Phase 2 F1a-3：Codex 会话按 cwd 分组成合成 HistoryProject（count/max-mtime/name/键/has_live）。
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

    /// 一棵含「废弃 ESC 兄弟 + 分叉点后续」的小会话树：
    /// u1(user) → u2(asst) → u3(system) → u4(user 分叉点) → u5(asst 分叉后)
    ///                                  └→ u6(user 废弃 ESC 兄弟，parent 同 u3)
    fn sample_session() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({"type":"user","uuid":"u1","parentUuid":null,"timestamp":"t1","sessionId":"SRC","gitBranch":"main","message":{"role":"user","content":"q1"}}),
            serde_json::json!({"type":"assistant","uuid":"u2","parentUuid":"u1","timestamp":"t2","sessionId":"SRC","message":{"role":"assistant","content":"a1"}}),
            serde_json::json!({"type":"system","uuid":"u3","parentUuid":"u2","timestamp":"t3","sessionId":"SRC"}),
            serde_json::json!({"type":"user","uuid":"u4","parentUuid":"u3","timestamp":"t4","sessionId":"SRC","message":{"role":"user","content":"q2"}}),
            serde_json::json!({"type":"assistant","uuid":"u5","parentUuid":"u4","timestamp":"t5","sessionId":"SRC","message":{"role":"assistant","content":"a2"}}),
            serde_json::json!({"type":"user","uuid":"u6","parentUuid":"u3","timestamp":"t6","sessionId":"SRC","message":{"role":"user","content":"q2-alt"}}),
        ]
    }

    #[test]
    fn branch_copies_ancestor_prefix_in_native_format() {
        let lines = sample_session();
        // 从分叉点 u4 建分支
        let out = build_branch_records(&lines, "u4", "SRC", "NEWSID").unwrap();
        // 只保留祖先链 u1→u4（顺序、根→分叉点），排除分叉后 u5 与废弃兄弟 u6
        let uuids: Vec<&str> = out
            .iter()
            .map(|r| r.get("uuid").unwrap().as_str().unwrap())
            .collect();
        assert_eq!(uuids, vec!["u1", "u2", "u3", "u4"], "祖先链顺序不对或漏/多");
        for r in &out {
            let u = r.get("uuid").unwrap().as_str().unwrap();
            // sessionId 改新 id
            assert_eq!(r.get("sessionId").unwrap(), "NEWSID");
            // forkedFrom{sessionId:源, messageUuid:自身 uuid}
            let ff = r.get("forkedFrom").unwrap();
            assert_eq!(ff.get("sessionId").unwrap(), "SRC");
            assert_eq!(ff.get("messageUuid").unwrap().as_str().unwrap(), u);
            // parentUuid 原样保留（链完整）
        }
        // 逐字段忠实：schema 外字段 gitBranch 不丢
        assert_eq!(out[0].get("gitBranch").unwrap(), "main");
        // 分叉点 u4 的 parentUuid 仍指 u3
        assert_eq!(out[3].get("parentUuid").unwrap(), "u3");
    }

    #[test]
    fn branch_at_leaf_includes_whole_active_path() {
        let lines = sample_session();
        // 从活跃叶 u5 建分支 → 整条主干 u1..u5，废弃兄弟 u6 仍排除
        let out = build_branch_records(&lines, "u5", "SRC", "N2").unwrap();
        let uuids: Vec<&str> = out
            .iter()
            .map(|r| r.get("uuid").unwrap().as_str().unwrap())
            .collect();
        assert_eq!(uuids, vec!["u1", "u2", "u3", "u4", "u5"]);
    }

    #[test]
    fn branch_rejects_unknown_message_uuid() {
        let lines = sample_session();
        let err = build_branch_records(&lines, "does-not-exist", "SRC", "N3").unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

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
    fn branch_at_root_yields_single_clean_root() {
        let out = build_branch_records(&sample_session(), "u1", "SRC", "N").unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].get("uuid").unwrap(), "u1");
        assert!(
            out[0].get("parentUuid").unwrap().is_null(),
            "root parentUuid 应为 null"
        );
        assert_eq!(
            out[0]
                .get("forkedFrom")
                .unwrap()
                .get("messageUuid")
                .unwrap(),
            "u1"
        );
    }

    #[test]
    fn branch_nulls_dangling_root_parent() {
        // 模拟链断：把 u3 的 parentUuid 改成集合外 ghost；从 u4 分叉 → 链 u4→u3→(止)，
        // 新 root=u3 的悬空 parentUuid 应被置 null（原生 root 恒 null）。
        let mut lines = sample_session();
        lines[2]
            .as_object_mut()
            .unwrap()
            .insert("parentUuid".into(), serde_json::json!("ghost-not-here"));
        let out = build_branch_records(&lines, "u4", "SRC", "N").unwrap();
        let uuids: Vec<&str> = out
            .iter()
            .map(|r| r.get("uuid").unwrap().as_str().unwrap())
            .collect();
        assert_eq!(uuids, vec!["u3", "u4"]);
        assert!(
            out[0].get("parentUuid").unwrap().is_null(),
            "链断的新 root parentUuid 应置 null"
        );
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

    /// 重要（D 审计）：安全关键的写盘壳直测——源零改动 + 新文件原生格式正确。
    /// 注入 tempdir projects 绕开 resolve_claude_dir（同 delete 测法）。
    #[test]
    fn branch_impl_leaves_source_untouched_and_writes_native_branch() {
        let projects = temp_projects("branch-impl");
        let proj = projects.join("proj-x");
        std::fs::create_dir_all(&proj).unwrap();
        let src = proj.join("srcsid.jsonl");
        let mut body = String::new();
        for r in &sample_session() {
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
            build_local_ps_command(&LocalPsAction::Resume(sid.to_string()), None).unwrap(),
            expected
        );
    }

    #[test]
    fn unified_builder_byte_identical_to_pre_f06_new_session_output() {
        let expected = "if (Get-Command cc -ErrorAction SilentlyContinue) { cc } else { claude }";
        assert_eq!(build_new_session_ps_command(None).unwrap(), expected);
        assert_eq!(
            build_local_ps_command(&LocalPsAction::New, None).unwrap(),
            expected
        );
    }
}
