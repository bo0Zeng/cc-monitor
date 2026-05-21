//! 历史会话浏览器后端：扫描 `<claude_dir>/projects/**/*.jsonl`，提供
//! list / delete / metadata 增删改 / resume IPC 命令。
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
//!  2. `list_history_sessions_in_project` —— 用户展开某项目时才调，全量读那个项目下
//!     的所有 jsonl 解析 ai_title / first_user_excerpt 等。前端缓存结果
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
use crate::utils::days_from_civil;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

// === 数据结构 ===

/// 项目级元数据 —— 首次 list 时返回，**不含**任何 session 内容。
#[derive(Debug, Serialize, Clone)]
pub struct HistoryProject {
    /// 真实工作目录路径（从某个 jsonl 的首条 user 消息的 cwd 取）
    pub project_path: String,
    /// 项目名 = cwd 最后一段
    pub project_name: String,
    /// 编码后的目录名（位于 `<claude_dir>/projects/` 之下），前端调用
    /// `list_history_sessions_in_project` 时传回来作 key
    pub project_dir: String,
    pub session_count: u32,
    pub starred_count: u32,
    pub hidden_count: u32,
    /// 该项目下任意 jsonl 文件的最大 mtime（ms）
    pub last_activity: i64,
    /// 该项目下是否有 session 当前 PID 还活着
    pub has_live: bool,
}

#[derive(Debug, Serialize, Clone)]
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
    // 用户元数据合并进来，前端一次拿全
    pub starred: bool,
    pub custom_title: Option<String>,
    pub hidden: bool,
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
}

#[derive(Debug, Deserialize)]
pub struct MetadataPatch {
    #[serde(default)]
    pub starred: Option<bool>,
    #[serde(default, rename = "customTitle", alias = "custom_title")]
    pub custom_title: Option<Option<String>>,
    #[serde(default)]
    pub hidden: Option<bool>,
}

// === IPC 命令 ===

/// 项目级元数据列表 —— **不读 jsonl 内容**。首次打开历史浏览器时调。
/// 每个项目仅 1 个 1-line read（拿 cwd） + N 个文件 stat（拿 mtime / count）。
#[tauri::command]
pub fn list_history_projects(
    map: tauri::State<'_, Arc<SessionMap>>,
) -> Result<Vec<HistoryProject>, String> {
    let started = std::time::Instant::now();
    let claude_dir = paths::resolve_claude_dir().ok_or("claude dir not found")?;
    let projects_dir = claude_dir.join("projects");
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
}

/// 单个项目内的全部会话详情（全量读 jsonl）。用户展开某项目组时才调。
#[tauri::command]
pub fn list_history_sessions_in_project(
    project_dir: String,
    map: tauri::State<'_, Arc<SessionMap>>,
) -> Result<Vec<HistorySessionEntry>, String> {
    let started = std::time::Instant::now();
    let claude_dir = paths::resolve_claude_dir().ok_or("claude dir not found")?;
    let projects_dir = claude_dir.join("projects");
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

    let mut out: Vec<HistorySessionEntry> = Vec::new();
    let files = match std::fs::read_dir(&target) {
        Ok(d) => d,
        Err(e) => return Err(format!("read {}: {e}", target.display())),
    };
    for f in files.flatten() {
        let p = f.path();
        if p.extension().map_or(false, |e| e == "jsonl") {
            if let Some(entry) = analyze_jsonl(&p, &metadata, &map) {
                out.push(entry);
            }
        }
    }
    out.sort_by(|a, b| {
        b.starred
            .cmp(&a.starred)
            .then(b.updated_at.cmp(&a.updated_at))
    });
    tracing::info!(
        "list_history_sessions_in_project({}): {} sessions in {}ms",
        target.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
        out.len(),
        started.elapsed().as_millis()
    );
    Ok(out)
}

/// 把一个 jsonl 文件全量读出为 `JsonlLinePayload` 列表，用于历史查看器渲染。
/// 复用前端 `renderMessage` 那一套，无需为只读视图另开渲染管线。
#[tauri::command]
pub fn read_session_jsonl(
    jsonl_path: String,
) -> Result<Vec<crate::bridge::JsonlLinePayload>, String> {
    let started = std::time::Instant::now();
    let claude_dir = paths::resolve_claude_dir().ok_or("claude dir not found")?;
    let projects_dir = claude_dir.join("projects");
    let target = PathBuf::from(&jsonl_path);
    if !target.starts_with(&projects_dir) {
        return Err(format!(
            "refuse: {} outside {}",
            target.display(),
            projects_dir.display()
        ));
    }
    if target.extension().map_or(true, |e| e != "jsonl") {
        return Err("not a .jsonl file".into());
    }

    let session_id = target
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let file = File::open(&target).map_err(|e| format!("open {}: {e}", target.display()))?;
    let reader = BufReader::new(file);
    let mut out: Vec<crate::bridge::JsonlLinePayload> = Vec::new();
    let mut cwd_seen: Option<String> = None;
    let path_str = target.to_string_lossy().into_owned();

    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let rec = match parse_line(trimmed) {
            Ok(Some(r)) if r.is_displayable() => r,
            _ => continue,
        };
        // 第一条 user 的 cwd 锁住，后续记录沿用，与 lib.rs 的实时流一致
        if let JsonlRecord::User { cwd, .. } = &rec {
            if cwd_seen.is_none() {
                cwd_seen = cwd.clone();
            }
        }
        out.push(crate::bridge::JsonlLinePayload {
            session_id: session_id.clone(),
            cwd: cwd_seen.clone(),
            path: path_str.clone(),
            message: rec,
        });
    }
    tracing::info!(
        "read_session_jsonl({}): {} records in {}ms",
        session_id,
        out.len(),
        started.elapsed().as_millis()
    );
    Ok(out)
}

#[tauri::command]
pub fn delete_history_session(
    session_id: String,
    jsonl_path: String,
) -> Result<(), String> {
    // 安全校验：必须在 claude_dir/projects 之下，避免前端传错路径误删别处文件
    let claude_dir = paths::resolve_claude_dir().ok_or("claude dir not found")?;
    let projects_dir = claude_dir.join("projects");
    let target = PathBuf::from(&jsonl_path);
    if !target.starts_with(&projects_dir) {
        return Err(format!(
            "refuse delete: {} is outside {}",
            target.display(),
            projects_dir.display()
        ));
    }
    if !target.exists() {
        return Err(format!("{} does not exist", target.display()));
    }
    if target.extension().map_or(true, |e| e != "jsonl") {
        return Err("refuse delete: not a .jsonl file".into());
    }

    std::fs::remove_file(&target)
        .map_err(|e| format!("remove {}: {e}", target.display()))?;
    tracing::info!("history: deleted {}", target.display());

    // 同步从 metadata 移除条目
    let mut metadata = load_metadata().unwrap_or_default();
    if metadata.entries.remove(&session_id).is_some() {
        let _ = save_metadata(&metadata);
    }
    Ok(())
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
    entry.updated_at = now_ms();
    let result = entry.clone();
    save_metadata(&metadata)?;
    Ok(result)
}

/// 在新终端窗口里跑 `claude --resume <session_id>`。
/// Windows 上优先 wt.exe，找不到回退 powershell。其他平台暂不支持。
#[tauri::command]
pub fn resume_history_session(
    session_id: String,
    cwd: String,
) -> Result<(), String> {
    resume_impl(&session_id, &cwd)
}

#[cfg(windows)]
fn resume_impl(session_id: &str, cwd: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    // CREATE_NEW_CONSOLE：让 Tauri GUI 父进程能创建独立控制台窗口给 cmd。
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

    let cwd_valid = Path::new(cwd).is_dir();
    let claude_cmd = format!("claude --resume {}", session_id);

    // Plan A：wt.exe（Windows Terminal）+ `cmd /K "claude --resume <id>"`。
    //   * `cmd /K`：命令完后保持窗口，用户能看 claude 输出 + 继续打字
    //   * 用 cmd.exe **不要用 pwsh** —— pwsh.exe 不是 Windows 自带（PowerShell Core
    //     需独立安装），早先用 pwsh 时报错 0x80070002（ERROR_FILE_NOT_FOUND）。
    //     cmd.exe 永远在系统目录可执行
    let mut wt_args: Vec<String> = Vec::new();
    if cwd_valid {
        wt_args.push("-d".into());
        wt_args.push(cwd.into());
    }
    wt_args.push("cmd".into());
    wt_args.push("/K".into());
    wt_args.push(claude_cmd.clone());

    if Command::new("wt.exe").args(&wt_args).spawn().is_ok() {
        tracing::info!("history: resumed via wt.exe sid={session_id}");
        return Ok(());
    }

    // Plan B：直接 cmd /K，CREATE_NEW_CONSOLE 让系统给个新控制台窗口。
    // 不依赖 wt.exe，conhost 兜底，比 Win10 default 还稳。
    let mut builder = Command::new("cmd");
    builder.args(["/K", &claude_cmd]);
    builder.creation_flags(CREATE_NEW_CONSOLE);
    if cwd_valid {
        builder.current_dir(cwd);
    }
    builder
        .spawn()
        .map_err(|e| format!("spawn cmd failed: {e}"))?;
    tracing::info!("history: resumed via cmd /K fallback sid={session_id}");
    Ok(())
}

#[cfg(not(windows))]
fn resume_impl(_session_id: &str, _cwd: &str) -> Result<(), String> {
    Err("resume only supported on Windows (v1)".into())
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
        if p.extension().map_or(false, |x| x == "jsonl") {
            let sid = match p.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
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

    let reader = BufReader::new(file);
    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
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
                message_count += 1;
            }
            JsonlRecord::Assistant { timestamp, .. } => {
                if started_at == 0 {
                    started_at = iso_to_ms(timestamp);
                }
                message_count += 1;
            }
            JsonlRecord::AiTitle { ai_title: t, .. } => {
                // 后出现的覆盖（Claude 在会话里可能多次更新 ai-title）
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

    let entry_meta = metadata.entries.get(&session_id).cloned().unwrap_or_default();

    Some(HistorySessionEntry {
        session_id: session_id.clone(),
        project_path: project_path.clone(),
        project_name,
        ai_title,
        first_user_excerpt,
        started_at,
        updated_at,
        jsonl_path: path.to_string_lossy().into_owned(),
        is_live: map.is_session_active(&session_id),
        message_count_approx: message_count,
        starred: entry_meta.starred,
        custom_title: entry_meta.custom_title.clone(),
        hidden: entry_meta.hidden,
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
    for tag in ["task-notification", "system-reminder", "local-command-caveat"] {
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

fn load_metadata() -> Result<HistoryMetadata, String> {
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
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let pretty = serde_json::to_string_pretty(m).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, pretty).map_err(|e| e.to_string())?;
    // 复用 config.rs 的原子替换逻辑：直接走 std::fs::rename 在 Linux/macOS 没问题，
    // Windows 用 MoveFileExW 才能原子覆盖；这里走简单 rename + 容忍失败重试一次。
    // history-metadata 损坏代价很小（只丢用户的 star 信息，不影响 jsonl），不值得引入额外 unsafe。
    if let Err(first_err) = std::fs::rename(&tmp, &path) {
        tracing::warn!(
            "history-metadata rename failed (will retry after delete): {first_err}"
        );
        let _ = std::fs::remove_file(&path);
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// === 时间换算 ===

fn systime_to_ms(t: SystemTime) -> i64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn iso_to_ms(iso: &str) -> i64 {
    // Claude 写的 timestamp 形如 "2026-05-20T15:11:42.345Z" —— 不引 chrono 依赖，
    // 直接 parse 各字段。失败返回 0（前端会显示为 1970，看得见但不崩）。
    parse_iso8601_ms(iso).unwrap_or(0)
}

fn parse_iso8601_ms(s: &str) -> Option<i64> {
    // YYYY-MM-DDTHH:MM:SS[.fff][Z|+HH:MM]
    let bytes = s.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: u32 = s.get(5..7)?.parse().ok()?;
    let day: u32 = s.get(8..10)?.parse().ok()?;
    let hour: u32 = s.get(11..13)?.parse().ok()?;
    let min: u32 = s.get(14..16)?.parse().ok()?;
    let sec: u32 = s.get(17..19)?.parse().ok()?;
    // fractional ms
    let mut ms: i64 = 0;
    if s.len() > 19 && bytes[19] == b'.' {
        let mut end = 20;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        let frac = s.get(20..end)?;
        let frac_num: i64 = frac.parse().ok()?;
        // 归一到毫秒
        ms = match frac.len() {
            0 => 0,
            1 => frac_num * 100,
            2 => frac_num * 10,
            3 => frac_num,
            n if n > 3 => frac_num / 10_i64.pow((n - 3) as u32),
            _ => 0,
        };
    }
    // 简化：忽略时区，统统按 UTC 计算（精度差几小时也能用于排序）
    let days = days_from_civil(year, month as i64, day as i64);
    let total = days * 86_400 + hour as i64 * 3600 + min as i64 * 60 + sec as i64;
    Some(total * 1000 + ms)
}

fn now_ms() -> i64 {
    systime_to_ms(SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_parse_simple() {
        let ms = parse_iso8601_ms("2026-05-20T15:11:42.345Z").unwrap();
        // sanity: 2026-05-20 是 2025 之后
        assert!(ms > 1_700_000_000_000);
        assert!(ms < 2_000_000_000_000);
    }

    #[test]
    fn iso_parse_no_fraction() {
        let ms = parse_iso8601_ms("2026-05-20T15:11:42Z").unwrap();
        assert!(ms > 1_700_000_000_000);
    }

    #[test]
    fn iso_parse_bad_returns_none() {
        assert!(parse_iso8601_ms("not-a-date").is_none());
    }

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
}
