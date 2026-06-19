//! issue #16 P1a：远端历史浏览的 monitor 侧。
//!
//! 每条查询走**独立 SSH 连接**一次性 exec `<daemon_path> --list-projects` 等
//! （方案权衡见 issue #16 计划评论：历史浏览用户驱动低频，握手开销可接受，
//! 完全不碰稳定的流式路径；连接建立复用 `ssh_source::connect_session` 全套
//! 指纹校验/鉴权）。
//!
//! 旧 daemon 兼容：不认参数的旧版会照常进流模式、首行发 hello 帧——这里检测
//! `"kind":"hello"` 即返回明确的"daemon 版本过旧"错误（优雅降级，前端 toast）。
//!
//! 只读铁律（INVARIANT § 1）：本模块只读远端；resume/delete 对远端在前端禁用。
//! INVARIANTS § 25：本路径是一次性读取（非 at-least-once 行流），SessionViewer
//! 每次 load 全新实例，无重投幂等义务。

use crate::history::{HistoryProject, HistorySessionEntry};
use crate::messages::JsonlRecord;
use crate::parser::parse_line;
use crate::ssh_source::{self, RemoteConfig};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

/// 查询超时：列举类命令整体限时（远端扫盘 + 传输）。
const LIST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// 读单会话：不设整体超时（会话可能大、流式合法耗时），但 (a) 每次 read_line 加
/// 单次超时，防"连接活着却永不来数据"卡死；(b) 总字节上限兜底，防无 EOF / 无换行
/// 的巨型损坏文件吃爆内存。正常会话毫秒级、远小于上限。
const READ_LINE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
const MAX_SESSION_BYTES: u64 = 256 * 1024 * 1024;

fn require_cfg_by_label(label: &str) -> Result<RemoteConfig, String> {
    crate::load_remote_config_by_label(label)
        .ok_or_else(|| format!("远端 '{label}' 未配置或未启用"))
}

/// 旧 daemon 检测：查询命令的输出行不可能含 wire 的 `"kind":"hello"`（查询模式
/// 输出裸 JSON 对象 / 裸 jsonl 行）；出现即说明远端 daemon 不认参数、进了流模式。
fn is_old_daemon_hello(line: &str) -> bool {
    line.contains(r#""kind":"hello""#) || line.contains(r#""kind": "hello""#)
}

const OLD_DAEMON_MSG: &str =
    "远端 daemon 版本过旧（不支持历史查询）——请按 doc/REMOTE-PHASE0-DEPLOY.md 重新构建部署";

/// 跑一条列举类查询，收集全部输出行（带整体超时 + 旧版检测）。
async fn run_list_query(cfg: &RemoteConfig, args: &str) -> Result<Vec<String>, String> {
    let cmd = format!("{} {}", ssh_source::shell_quote(&cfg.daemon_path), args);
    let collect = async {
        let stream = ssh_source::connect_and_exec_cmd(cfg, &cmd).await?;
        let mut reader = BufReader::new(stream);
        let mut lines = Vec::new();
        let mut buf = String::new();
        loop {
            buf.clear();
            let n = reader
                .read_line(&mut buf)
                .await
                .map_err(|e| format!("读取远端输出失败: {e}"))?;
            if n == 0 {
                break; // EOF = 命令结束
            }
            let line = buf.trim();
            if line.is_empty() {
                continue;
            }
            if lines.is_empty() && is_old_daemon_hello(line) {
                return Err(OLD_DAEMON_MSG.to_string());
            }
            lines.push(line.to_string());
        }
        Ok(lines)
    };
    tokio::time::timeout(LIST_TIMEOUT, collect)
        .await
        .map_err(|_| format!("远端查询超时（{}s）: {args}", LIST_TIMEOUT.as_secs()))?
}

/// 远端项目列表（多机 #30：fan-out 所有已配置远端）。无远端 → 空列表（前端无感合并）；
/// 单台查询失败 → warn + 跳过该台（不拖垮其余台）。各 project 带 `origin = 该台 label`。
#[tauri::command]
pub async fn list_remote_history_projects() -> Result<Vec<HistoryProject>, String> {
    let cfgs = crate::load_remote_configs();
    if cfgs.is_empty() {
        return Ok(Vec::new());
    }
    let mut projects = Vec::new();
    let mut any_ok = false;
    let mut last_err = String::new();
    for cfg in &cfgs {
        let lines = match run_list_query(cfg, "--list-projects").await {
            Ok(l) => {
                any_ok = true;
                l
            }
            Err(e) => {
                // 逐台失败不拖垮整体：该台 warn + 跳过，其余台照常返回。
                tracing::warn!(
                    "远端 [{}] --list-projects 失败（跳过该台）: {e}",
                    cfg.origin_label()
                );
                last_err = e;
                continue;
            }
        };
        for line in lines {
            let v: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("remote --list-projects 行解析失败（跳过）: {e}: {line}");
                    continue;
                }
            };
            let dir_name = v["dirName"].as_str().unwrap_or_default().to_string();
            if dir_name.is_empty() {
                continue;
            }
            let project_path = v["projectPath"].as_str().unwrap_or_default().to_string();
            // 对齐本地口径：projectName = cwd 最后一段；提取不到 cwd 时回退编码目录名
            let project_name = if project_path.is_empty() {
                dir_name.clone()
            } else {
                project_path
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(&dir_name)
                    .to_string()
            };
            projects.push(HistoryProject {
                project_path,
                project_name,
                // 远端的"懒加载 key"= 远端编码目录名（前端原样传回 stream_remote_history_sessions）
                project_dir: dir_name,
                session_count: v["sessionCount"].as_u64().unwrap_or(0) as u32,
                // P1a：远端不合并本地元数据计数（列表级开销不值得），条目级照常合并
                starred_count: 0,
                hidden_count: 0,
                last_activity: v["lastActivityMs"].as_i64().unwrap_or(0),
                // 活跃远端会话已有 [host] live Tab，历史组不重复标 live
                has_live: false,
                origin: Some(cfg.origin_label()),
            });
        }
    }
    // 配了远端但**全部**台查询都失败 → 返回 Err（前端可 toast），避免与"无远端配置"的
    // 空列表（cfgs.is_empty 早返）混淆，让用户能区分"没配"和"配了但连不上"。
    if !any_ok {
        return Err(format!(
            "所有远端历史查询失败（{} 台），最后一个错误: {last_err}",
            cfgs.len()
        ));
    }
    tracing::info!(
        "list_remote_history_projects: {} projects from {} host(s)",
        projects.len(),
        cfgs.len()
    );
    Ok(projects)
}

/// 远端某项目的历史会话列表（流式 Channel，对齐本地 stream_history_sessions_in_project）。
/// `project_dir` = 远端编码目录名（list_remote_history_projects 给出的 projectDir）。
#[tauri::command]
pub async fn stream_remote_history_sessions(
    project_dir: String,
    origin: String,
    on_entry: tauri::ipc::Channel<HistorySessionEntry>,
) -> Result<u32, String> {
    let cfg = require_cfg_by_label(&origin)?;
    // 防穿越：目录名不允许含分隔符（daemon 侧同样校验，双层防御）
    if project_dir.contains('/') || project_dir.contains('\\') || project_dir.contains("..") {
        return Err(format!("非法项目目录名: {project_dir}"));
    }
    let args = format!("--list-sessions {}", ssh_source::shell_quote(&project_dir));
    let lines = run_list_query(&cfg, &args).await?;
    // 条目级元数据（star/rename/hide）按 session_id 存本地，远端会话同样适用
    let metadata = crate::history::load_metadata().unwrap_or_default();
    let mut total = 0u32;
    for line in lines {
        let v: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("remote --list-sessions 行解析失败（跳过）: {e}");
                continue;
            }
        };
        let session_id = v["sessionId"].as_str().unwrap_or_default().to_string();
        if session_id.is_empty() {
            continue;
        }
        let cwd = v["cwd"].as_str().unwrap_or_default().to_string();
        let project_name = cwd
            .rsplit(['/', '\\'])
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(&project_dir)
            .to_string();
        let meta = metadata
            .entries
            .get(&session_id)
            .cloned()
            .unwrap_or_default();
        let entry = HistorySessionEntry {
            session_id,
            project_path: cwd,
            project_name,
            ai_title: v["aiTitle"].as_str().map(String::from),
            first_user_excerpt: v["firstUserExcerpt"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            started_at: v["startedAtMs"].as_i64().unwrap_or(0),
            updated_at: v["updatedAtMs"].as_i64().unwrap_or(0),
            jsonl_path: v["jsonlPath"].as_str().unwrap_or_default().to_string(),
            is_live: false,
            message_count_approx: v["messageCountApprox"].as_u64().unwrap_or(0) as u32,
            starred: meta.starred,
            custom_title: meta.custom_title,
            hidden: meta.hidden,
            // P1a：daemon 不提取 fork 关系，远端会话在 fork 树上呈平铺
            forked_from_session_id: None,
            forked_from_message_uuid: None,
            origin: Some(cfg.origin_label()),
        };
        if on_entry.send(entry).is_err() {
            tracing::info!("stream_remote_history_sessions: 前端取消");
            return Ok(total);
        }
        total += 1;
    }
    tracing::info!("stream_remote_history_sessions({project_dir}): {total} sessions");
    Ok(total)
}

/// 流式读取远端单个会话（对齐本地 stream_read_session_jsonl 的 chunk 口径：
/// 每 100 条一发，payload 带 origin=Some(host)，SessionViewer 零改动复用）。
#[tauri::command]
pub async fn stream_read_remote_session(
    jsonl_path: String,
    origin: String,
    on_chunk: tauri::ipc::Channel<Vec<crate::bridge::JsonlLinePayload>>,
) -> Result<u32, String> {
    const CHUNK_SIZE: usize = 100;
    let cfg = require_cfg_by_label(&origin)?;
    let started = std::time::Instant::now();
    // 与本地 history.rs 的 file_stem 口径一致：剥**一个** ".jsonl" 后缀（strip_suffix
    // 是字面后缀，不是 trim_end_matches 的字符集语义）。
    let file_name = jsonl_path.rsplit(['/', '\\']).next().unwrap_or("");
    let session_id = file_name
        .strip_suffix(".jsonl")
        .unwrap_or(file_name)
        .to_string();
    let args = format!("--read-session {}", ssh_source::shell_quote(&jsonl_path));
    let cmd = format!("{} {}", ssh_source::shell_quote(&cfg.daemon_path), args);
    let stream = ssh_source::connect_and_exec_cmd(&cfg, &cmd).await?;
    let mut reader = BufReader::new(stream.take(MAX_SESSION_BYTES));
    let mut buf = String::new();
    let mut cwd_seen: Option<String> = None;
    let mut chunk: Vec<crate::bridge::JsonlLinePayload> = Vec::with_capacity(CHUNK_SIZE);
    let mut total = 0u32;
    let mut next_seq: u64 = 0;
    let mut first_line = true;
    loop {
        buf.clear();
        let n = tokio::time::timeout(READ_LINE_TIMEOUT, reader.read_line(&mut buf))
            .await
            .map_err(|_| "读取远端会话超时（单次读取卡住）".to_string())?
            .map_err(|e| format!("读取远端会话失败: {e}"))?;
        if n == 0 {
            break;
        }
        let trimmed = buf.trim();
        if trimmed.is_empty() {
            continue;
        }
        if first_line {
            first_line = false;
            if is_old_daemon_hello(trimmed) {
                return Err(OLD_DAEMON_MSG.to_string());
            }
        }
        // 与本地 stream_read_session_jsonl 同口径：parse + displayable 过滤 + per-file seq
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
        chunk.push(crate::bridge::JsonlLinePayload {
            session_id: session_id.clone(),
            cwd: cwd_seen.clone(),
            path: jsonl_path.clone(),
            seq,
            origin: Some(cfg.origin_label()),
            message: rec,
        });
        total += 1;
        if chunk.len() >= CHUNK_SIZE {
            let full = std::mem::replace(&mut chunk, Vec::with_capacity(CHUNK_SIZE));
            if on_chunk.send(full).is_err() {
                tracing::info!("stream_read_remote_session({session_id}): 前端取消于 {total} 条");
                return Ok(total);
            }
        }
    }
    if !chunk.is_empty() {
        let _ = on_chunk.send(chunk);
    }
    tracing::info!(
        "stream_read_remote_session({session_id}): {total} records in {}ms",
        started.elapsed().as_millis()
    );
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_daemon_hello_detected() {
        assert!(is_old_daemon_hello(
            r#"{"kind":"hello","v":1,"build_id":"phase0-proto","host_arch":"aarch64","claude_dir":"/home/pi/.claude"}"#
        ));
        // 查询模式的正常输出不含 kind
        assert!(!is_old_daemon_hello(
            r#"{"dirName":"-home-pi-proj","projectPath":"/home/pi/proj","sessionCount":3,"lastActivityMs":1}"#
        ));
        // jsonl 正文里聊到 hello 不该误判（必须是 kind 字段形态）
        assert!(!is_old_daemon_hello(
            r#"{"type":"user","message":{"content":"say hello"}}"#
        ));
    }

    #[test]
    fn shell_quote_via_ssh_source() {
        assert_eq!(
            crate::ssh_source::shell_quote("/a/b c.jsonl"),
            "'/a/b c.jsonl'"
        );
        assert_eq!(crate::ssh_source::shell_quote("a'b"), r"'a'\''b'");
    }
}
