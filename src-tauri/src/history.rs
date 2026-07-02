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
    })
    .await
    .map_err(|e| format!("spawn_blocking join: {e}"))?
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

        let mut count = 0u32;
        let files = match std::fs::read_dir(&target) {
            Ok(d) => d,
            Err(e) => return Err(format!("read {}: {e}", target.display())),
        };
        for f in files.flatten() {
            let p = f.path();
            if p.extension().is_some_and(|e| e == "jsonl") {
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
        if target.extension().is_none_or(|e| e != "jsonl") {
            return Err("not a .jsonl file".into());
        }

        let session_id = target
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
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
            let rec = match parse_line(trimmed) {
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
    if canon_target.extension().map_or(true, |e| e != "jsonl") {
        return Err("refuse delete: not a .jsonl file".into());
    }
    Ok(canon_target)
}

#[tauri::command]
pub fn delete_history_session(session_id: String, jsonl_path: String) -> Result<(), String> {
    // 安全校验：必须在 claude_dir/projects 之下，避免前端传错路径误删别处文件
    let claude_dir = paths::resolve_claude_dir().ok_or("claude dir not found")?;
    let projects_dir = claude_dir.join("projects");
    let target = validate_delete_target(&jsonl_path, &projects_dir)?;

    std::fs::remove_file(&target).map_err(|e| format!("remove {}: {e}", target.display()))?;
    tracing::info!("history: deleted {}", target.display());

    // 同步从 metadata 移除条目
    remove_metadata_entry(&session_id);
    Ok(())
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
    entry.updated_at = now_ms();
    let result = entry.clone();
    save_metadata(&metadata)?;
    Ok(result)
}

/// 在新终端窗口里 resume 一个历史会话。
///
/// v2.8.1（bug 修复）：改为在 **PowerShell**（系统自带 `powershell.exe`，**加载用户
/// profile**）里跑，命令优先用户的 `cc` wrapper、回退 `claude`。详 `resume_impl`。
/// Windows 上优先 wt.exe，找不到回退独立控制台。其他平台暂不支持。
#[tauri::command]
pub fn resume_history_session(session_id: String, cwd: String) -> Result<(), String> {
    resume_impl(&session_id, &cwd)
}

/// 构造 resume 用的 PowerShell 命令体（不含 `-EncodedCommand` 编码）。
///
/// 防注入：`session_id` 来自前端历史条目，理论上是 UUID，但作为拼进 shell 命令的
/// 不可信输入必须校验——只允许 `[A-Za-z0-9_-]`，否则拒绝（杜绝 `; rm -rf` 之类）。
///
/// 优先 `cc`：检测到用户的 `cc` 函数（PowerShell 集成 wrapper，内部含 `__ccm_bind` +
/// 用户自己的代理 / env 设置）就用 `cc --resume`；检测不到才回退 `claude --resume`。
/// 命令在 profile 已加载的 PowerShell 里跑（见 resume_impl 不带 -NoProfile），所以即使
/// 回退 claude，profile 里的 PATH / 代理 env 仍生效。
///
/// 抽成独立函数是为了单测（不 spawn 进程也能验证防注入 + cc 优先逻辑）。
#[cfg(windows)]
fn build_resume_ps_command(session_id: &str) -> Result<String, String> {
    let valid = !session_id.is_empty()
        && session_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid {
        return Err(format!("refuse resume: invalid session_id {session_id:?}"));
    }
    Ok(format!(
        "if (Get-Command cc -ErrorAction SilentlyContinue) {{ cc --resume {sid} }} \
         else {{ claude --resume {sid} }}",
        sid = session_id
    ))
}

#[cfg(windows)]
fn resume_impl(session_id: &str, cwd: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    // CREATE_NEW_CONSOLE：让 Tauri GUI 父进程能创建独立控制台窗口给 powershell。
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

    let cwd_valid = Path::new(cwd).is_dir();

    let ps_command = build_resume_ps_command(session_id)?;
    // -EncodedCommand（base64 of UTF-16LE）：命令含空格 / 括号 / `;`，直接当字符串穿
    // wt.exe（用 `;` 分隔多 tab）会被切碎。编码成 base64 token（只含 [A-Za-z0-9+/=]）后
    // 任何一层 shell 都不会误解析。详 utils::powershell_encoded_command。
    let encoded = crate::utils::powershell_encoded_command(&ps_command);
    // 关键：**不带 `-NoProfile`** —— 必须加载用户 PowerShell profile，cc / __ccm_bind /
    // 代理 env 才会生效（这正是旧版用 `cmd /K claude` 时两个 bug 的根因：cmd 不是
    // PowerShell、更没加载 profile）。-NoExit：claude 退出后窗口保留，且 cc 已定义可继续敲。
    // 用系统自带 powershell.exe（PowerShell 5.1），**不是** pwsh.exe（PowerShell 7 需独立装）。
    let ps_args = ["-NoExit", "-EncodedCommand", encoded.as_str()];

    // Plan A：wt.exe（Windows Terminal）新标签里跑 powershell。
    let mut wt_args: Vec<String> = Vec::new();
    if cwd_valid {
        wt_args.push("-d".into());
        wt_args.push(cwd.into());
    }
    wt_args.push("powershell.exe".into());
    for a in ps_args {
        wt_args.push(a.into());
    }

    if Command::new("wt.exe").args(&wt_args).spawn().is_ok() {
        tracing::info!("history: resumed via wt.exe powershell sid={session_id}");
        return Ok(());
    }

    // Plan B：直接 powershell.exe + CREATE_NEW_CONSOLE 让系统给个新控制台窗口。
    // 不依赖 wt.exe，conhost 兜底。
    let mut builder = Command::new("powershell.exe");
    builder.args(ps_args);
    builder.creation_flags(CREATE_NEW_CONSOLE);
    if cwd_valid {
        builder.current_dir(cwd);
    }
    builder
        .spawn()
        .map_err(|e| format!("spawn powershell failed: {e}"))?;
    tracing::info!("history: resumed via powershell fallback sid={session_id}");
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

    #[cfg(windows)]
    #[test]
    fn resume_cmd_prefers_cc_with_claude_fallback() {
        let sid = "01998f2a-1234-7abc-9def-0123456789ab";
        let cmd = build_resume_ps_command(sid).unwrap();
        // 优先 cc、回退 claude，两者都带正确 sid
        assert!(cmd.contains("Get-Command cc"));
        assert!(cmd.contains(&format!("cc --resume {sid}")));
        assert!(cmd.contains(&format!("claude --resume {sid}")));
    }

    #[cfg(windows)]
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
                build_resume_ps_command(bad).is_err(),
                "应拒绝危险 session_id: {bad:?}"
            );
        }
    }
}
