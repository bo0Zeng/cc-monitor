//! issue #16 P1a：一次性历史查询模式。
//!
//! cc-monitor 通过**独立 SSH 连接**一次性 exec 本二进制并带参数：
//!
//! - `--list-projects`                → 列举 `<claude_dir>/projects/` 下各项目
//! - `--list-sessions <project_dir>`  → 列举某项目目录下的历史会话（带元数据）
//! - `--read-session <jsonl_path>`    → 原样透传该 jsonl 文件内容（monitor 侧解析）
//!
//! 输出协议：`--list-*` 每行一个 JSON 对象（**不是** wire::Frame——查询模式与流式
//! 协议互不混用，旧 daemon 不认参数会照常进流模式发 hello，monitor 以"首行是
//! hello 帧"识别旧版并优雅降级）；`--read-session` 输出原始文件字节。
//! 错误：stderr 写原因 + 退出码 2。成功退出码 0。
//!
//! 安全：所有路径参数严格限制在 `<claude_dir>/projects/` 之内（canonicalize 后
//! 前缀校验，防 `../` 穿越）；project_dir 参数不允许含路径分隔符。
//! 只读铁律（cc-monitor 不写远端）在此同样成立：本模块只 read_dir / read。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// 查询模式入口。返回进程退出码。
pub fn run(claude_dir: &Path, args: &[String]) -> i32 {
    let result = match args.first().map(String::as_str) {
        Some("--list-projects") => list_projects(claude_dir),
        Some("--list-sessions") => match args.get(1) {
            Some(dir) => list_sessions(claude_dir, dir),
            None => Err("--list-sessions requires <project_dir> argument".into()),
        },
        Some("--read-session-tail") => match (args.get(1), args.get(2)) {
            (Some(p), Some(n)) => match n.parse::<usize>() {
                Ok(n) => read_session_tail(claude_dir, p, n),
                Err(_) => Err("--read-session-tail <jsonl_path> <N>: N must be a number".into()),
            },
            _ => Err("--read-session-tail requires <jsonl_path> <N> arguments".into()),
        },
        Some("--read-session") => match args.get(1) {
            Some(p) => read_session(claude_dir, p),
            None => Err("--read-session requires <jsonl_path> argument".into()),
        },
        Some(other) => Err(format!("unknown argument: {other}")),
        None => Err("no query argument".into()),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("cc-monitor-remote query error: {e}");
            2
        }
    }
}

fn projects_root(claude_dir: &Path) -> PathBuf {
    claude_dir.join("projects")
}

/// `--list-projects`：每个项目目录一行 JSON：
/// `{"dirName","projectPath","sessionCount","lastActivityMs"}`
/// projectPath 从该项目**最新** jsonl 的头部记录提取 cwd（对齐本地口径：真实工作
/// 目录，而非编码过的目录名）；提取不到则空字符串，monitor 侧回退显示 dirName。
fn list_projects(claude_dir: &Path) -> Result<(), String> {
    let root = projects_root(claude_dir);
    let entries =
        std::fs::read_dir(&root).map_err(|e| format!("read_dir {} failed: {e}", root.display()))?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let mut session_count = 0u32;
        let mut last_activity_ms = 0i64;
        let mut newest_jsonl: Option<(i64, PathBuf)> = None;
        if let Ok(files) = std::fs::read_dir(&dir) {
            for f in files.flatten() {
                let p = f.path();
                if !p.is_file() || p.extension().is_none_or(|e| e != "jsonl") {
                    continue;
                }
                session_count += 1;
                let mtime = mtime_ms(&p);
                if mtime > last_activity_ms {
                    last_activity_ms = mtime;
                }
                if newest_jsonl.as_ref().is_none_or(|(m, _)| mtime > *m) {
                    newest_jsonl = Some((mtime, p));
                }
            }
        }
        if session_count == 0 {
            continue; // 空目录（全删过/只剩 sidecar）不展示
        }
        let project_path = newest_jsonl
            .and_then(|(_, p)| extract_cwd_from_head(&p))
            .unwrap_or_default();
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        let line = serde_json::json!({
            "dirName": dir_name,
            "projectPath": project_path,
            "sessionCount": session_count,
            "lastActivityMs": last_activity_ms,
        });
        writeln!(out, "{line}").map_err(|e| format!("stdout write failed: {e}"))?;
    }
    Ok(())
}

/// `--list-sessions <project_dir>`：该项目每个 jsonl 一行 JSON：
/// `{"sessionId","jsonlPath","startedAtMs","updatedAtMs","messageCountApprox",
///   "firstUserExcerpt","aiTitle","cwd"}`
/// 元数据在远端 CPU 上扫整个文件提取（对齐本地 analyze 口径的精简版）。
fn list_sessions(claude_dir: &Path, project_dir: &str) -> Result<(), String> {
    // project_dir 是目录名而非路径：拒绝任何分隔符 / 上跳
    if project_dir.contains('/') || project_dir.contains('\\') || project_dir.contains("..") {
        return Err(format!("invalid project dir name: {project_dir}"));
    }
    // 与 read_session 对齐（也兑现本文件头部"canonicalize 后前缀校验"的承诺）：名字
    // 合法但 projects/ 下若有指向外部的 symlink 目录，read_dir 会跟随逃逸出 projects/
    // ——canonicalize 解析 symlink 后做前缀校验挡住。
    let root = projects_root(claude_dir)
        .canonicalize()
        .map_err(|e| format!("projects root unavailable: {e}"))?;
    let dir = root
        .join(project_dir)
        .canonicalize()
        .map_err(|e| format!("project dir unavailable: {e}"))?;
    if !dir.starts_with(&root) {
        return Err(format!(
            "refusing to list outside projects dir: {}",
            dir.display()
        ));
    }
    let entries =
        std::fs::read_dir(&dir).map_err(|e| format!("read_dir {} failed: {e}", dir.display()))?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() || p.extension().is_none_or(|e| e != "jsonl") {
            continue;
        }
        let meta = analyze_session(&p);
        writeln!(out, "{meta}").map_err(|e| format!("stdout write failed: {e}"))?;
    }
    Ok(())
}

/// `--read-session <jsonl_path>`：路径校验后原样透传文件内容。
/// 透传而非逐行解析：monitor 侧本就有完整的 parse_line 管线，daemon 不重复造。
fn validate_session_path(
    claude_dir: &Path,
    jsonl_path: &str,
) -> Result<std::path::PathBuf, String> {
    let root = projects_root(claude_dir)
        .canonicalize()
        .map_err(|e| format!("projects root unavailable: {e}"))?;
    let target = Path::new(jsonl_path)
        .canonicalize()
        .map_err(|e| format!("session path unavailable: {e}"))?;
    if !target.starts_with(&root) {
        return Err(format!(
            "refusing to read outside projects dir: {}",
            target.display()
        ));
    }
    if target.extension().is_none_or(|e| e != "jsonl") {
        return Err("refusing to read non-jsonl file".into());
    }
    Ok(target)
}

fn read_session(claude_dir: &Path, jsonl_path: &str) -> Result<(), String> {
    let target = validate_session_path(claude_dir, jsonl_path)?;
    let mut f = std::fs::File::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    std::io::copy(&mut f, &mut out).map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// Batch9-F30：`--read-session-tail <path> <N>`——尾部优先输出：
/// 首行 meta `{"kind":"snapshot_meta","total":T,"tail_from":F}`（T/F 均按
/// **可计行**口径：完整（`\n` 收尾）且非 BOM/全空白——与 watcher/monitor 的
/// 行号空间一字一致），随后原样输出可计行 [F,T)（最新 N 行）、再输出 [0,F)。
/// monitor 据 meta 编 seq：前 T-F 行 = F+i，其余 = i。空文件 → 仅 meta。
fn read_session_tail(claude_dir: &Path, jsonl_path: &str, n: usize) -> Result<(), String> {
    let target = validate_session_path(claude_dir, jsonl_path)?;
    // 审计 D：整文件 std::fs::read 在 Pi 级设备上对数百 MB 会话有 OOM 风险
    // （旧 --read-session 是 io::copy 流式）——改单遍流式扫描（环形缓冲只存
    // 最近 N 个可计行的字节偏移，O(N) 内存）+ 两次 seek 范围拷贝。
    use std::io::{BufRead, Read, Seek, SeekFrom, Write};
    let f = std::fs::File::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let mut reader = std::io::BufReader::new(f);
    let mut recent: std::collections::VecDeque<u64> = std::collections::VecDeque::new();
    let keep = n.max(1);
    let mut total: u64 = 0;
    let mut pos: u64 = 0;
    let mut complete_end: u64 = 0;
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        let read = reader
            .read_until(b'\n', &mut buf)
            .map_err(|e| format!("scan failed: {e}"))?;
        if read == 0 {
            break;
        }
        let line_start = pos;
        pos += read as u64;
        if *buf.last().unwrap() != b'\n' {
            break; // torn 残尾不计（F14 口径）
        }
        complete_end = pos;
        let text = String::from_utf8_lossy(&buf[..buf.len() - 1]);
        if text.trim_start_matches('\u{feff}').trim().is_empty() {
            continue; // 空行不计（与 watcher/monitor 口径一致）
        }
        total += 1;
        recent.push_back(line_start);
        if recent.len() > keep {
            recent.pop_front();
        }
    }
    let tail_from = total - recent.len() as u64;
    let split_at = recent.front().copied().unwrap_or(complete_end);
    let meta =
        format!("{{\"kind\":\"snapshot_meta\",\"total\":{total},\"tail_from\":{tail_from}}}\n");
    let mut f = reader.into_inner();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    out.write_all(meta.as_bytes())
        .map_err(|e| format!("stream failed: {e}"))?;
    // 尾段 [split_at, complete_end)
    f.seek(SeekFrom::Start(split_at))
        .map_err(|e| format!("seek failed: {e}"))?;
    std::io::copy(&mut (&mut f).take(complete_end - split_at), &mut out)
        .map_err(|e| format!("stream failed: {e}"))?;
    // 头段 [0, split_at)
    f.seek(SeekFrom::Start(0))
        .map_err(|e| format!("seek failed: {e}"))?;
    std::io::copy(&mut (&mut f).take(split_at), &mut out)
        .map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// 纯函数：把文件字节按"最新 N 可计行优先"切成 (meta 行, 尾段, 头段)。
/// 只处理到最后一个 `\n`（torn 残尾不进任何段——F14 口径）。
/// 生产路径已流式化（read_session_tail，审计 D 内存修订）；本函数保留为
/// 口径锚点（tail_tests 锚定语义），流式版与它的等价性由本机行为验证对账
/// （真实 18MB 会话：meta/字节输出逐段一致，见 Batch9 feature 30 §6 留档）。
fn split_tail(bytes: &[u8], n: usize) -> (String, &[u8], &[u8]) {
    let complete_end = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let complete = &bytes[..complete_end];
    // 收集每个可计行的起始字节偏移（口径 = watcher::read_new_lines：BOM/全空白跳过）
    let mut starts: Vec<usize> = Vec::new();
    let mut pos = 0usize;
    for line in complete.split_inclusive(|&b| b == b'\n') {
        let text = String::from_utf8_lossy(&line[..line.len() - 1]);
        if !text.trim_start_matches('\u{feff}').trim().is_empty() {
            starts.push(pos);
        }
        pos += line.len();
    }
    let total = starts.len();
    let tail_from = total.saturating_sub(n.max(1));
    let meta =
        format!("{{\"kind\":\"snapshot_meta\",\"total\":{total},\"tail_from\":{tail_from}}}\n");
    let split_at = starts.get(tail_from).copied().unwrap_or(complete_end);
    (meta, &complete[split_at..], &complete[..split_at])
}

fn mtime_ms(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn created_ms_or_mtime(p: &Path) -> i64 {
    let meta = match std::fs::metadata(p) {
        Ok(m) => m,
        Err(_) => return 0,
    };
    let t = meta.created().or_else(|_| meta.modified());
    t.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 从 jsonl 头部（前 40 行）提取首个带 cwd 的记录的 cwd。
fn extract_cwd_from_head(p: &Path) -> Option<String> {
    let content = std::fs::read_to_string(p).ok()?;
    for line in content.lines().take(40) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(cwd) = v.get("cwd").and_then(|c| c.as_str()) {
                if !cwd.is_empty() {
                    return Some(cwd.to_string());
                }
            }
        }
    }
    None
}

/// 单个会话的元数据提取（整文件扫描，跑在远端 CPU 上）：
/// - messageCountApprox = 非空行数
/// - firstUserExcerpt = 首条"真用户输入"的前 120 字符（跳过 isMeta / 工具结果 /
///   interrupt 标记，对齐本地口径的精简版）
/// - aiTitle = 最后一条 ai-title 记录（Claude 会多次更新，取最新）
/// - cwd = 首个带 cwd 的记录
fn analyze_session(p: &Path) -> serde_json::Value {
    let session_id = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut count = 0u32;
    let mut excerpt = String::new();
    let mut ai_title: Option<String> = None;
    let mut cwd: Option<String> = None;
    if let Ok(content) = std::fs::read_to_string(p) {
        for line in content.lines() {
            let trimmed = line.trim_start_matches('\u{feff}').trim();
            if trimmed.is_empty() {
                continue;
            }
            count += 1;
            let v: serde_json::Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if cwd.is_none() {
                if let Some(c) = v.get("cwd").and_then(|c| c.as_str()) {
                    if !c.is_empty() {
                        cwd = Some(c.to_string());
                    }
                }
            }
            match v.get("type").and_then(|t| t.as_str()) {
                Some("ai-title") => {
                    if let Some(t) = v.get("aiTitle").and_then(|t| t.as_str()) {
                        ai_title = Some(t.to_string()); // 取最新（持续覆盖）
                    }
                }
                Some("user")
                    if excerpt.is_empty()
                        && v.get("isMeta").and_then(|m| m.as_bool()) != Some(true) =>
                {
                    if let Some(text) = user_text(&v) {
                        if !text.starts_with("[Request interrupted") {
                            excerpt = truncate_chars(&text, 120);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    serde_json::json!({
        "sessionId": session_id,
        "jsonlPath": p.to_string_lossy(),
        "startedAtMs": created_ms_or_mtime(p),
        "updatedAtMs": mtime_ms(p),
        "messageCountApprox": count,
        "firstUserExcerpt": excerpt,
        "aiTitle": ai_title,
        "cwd": cwd,
    })
}

/// user 记录的纯文本内容：message.content 为字符串直接用；为数组取首个 text 块。
/// 工具结果（tool_result 块）返回 None——它不是用户敲的。
fn user_text(v: &serde_json::Value) -> Option<String> {
    let content = v.get("message")?.get("content")?;
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    if let Some(arr) = content.as_array() {
        for block in arr {
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(s) = block.get("text").and_then(|t| t.as_str()) {
                    return Some(s.to_string());
                }
            }
        }
    }
    None
}

/// 按字符截断（不劈 UTF-8 码点；中文场景 byte 截断会 panic/乱码）。
fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_project(root: &Path, dir_name: &str) -> PathBuf {
        let dir = root.join("projects").join(dir_name);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_jsonl(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, lines.join("\n")).unwrap();
        p
    }

    #[test]
    fn analyze_extracts_excerpt_title_cwd_count() {
        let tmp = std::env::temp_dir().join(format!("ccm-hq-test-{}", std::process::id()));
        let dir = fixture_project(&tmp, "proj-a");
        let p = write_jsonl(
            &dir,
            "s1.jsonl",
            &[
                r#"{"type":"user","cwd":"/home/pi/proj","isMeta":true,"message":{"role":"user","content":"skill 注入不算"}}"#,
                r#"{"type":"user","message":{"role":"user","content":"真正的首条用户输入，应该成为摘要"}}"#,
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"回复"}]}}"#,
                r#"{"type":"ai-title","aiTitle":"旧标题"}"#,
                r#"{"type":"ai-title","aiTitle":"最新标题"}"#,
            ],
        );
        let v = analyze_session(&p);
        assert_eq!(v["sessionId"], "s1");
        assert_eq!(v["messageCountApprox"], 5);
        assert_eq!(v["cwd"], "/home/pi/proj");
        assert_eq!(v["aiTitle"], "最新标题"); // 取最新
        assert!(v["firstUserExcerpt"]
            .as_str()
            .unwrap()
            .starts_with("真正的首条用户输入")); // isMeta 跳过
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn truncate_is_char_safe() {
        assert_eq!(truncate_chars("中文字符串", 3), "中文字");
        assert_eq!(truncate_chars("ab", 120), "ab");
    }

    #[test]
    fn list_sessions_rejects_path_traversal() {
        let tmp = std::env::temp_dir();
        assert!(list_sessions(&tmp, "../escape").is_err());
        assert!(list_sessions(&tmp, "a/b").is_err());
        assert!(list_sessions(&tmp, r"a\b").is_err());
    }

    #[test]
    fn read_session_rejects_outside_projects() {
        let tmp = std::env::temp_dir().join(format!("ccm-hq-ro-{}", std::process::id()));
        let dir = fixture_project(&tmp, "proj-b");
        write_jsonl(&dir, "ok.jsonl", &[r#"{"type":"user"}"#]);
        // projects 外的真实文件 → 拒绝
        let outside = tmp.join("secret.jsonl");
        std::fs::write(&outside, "nope").unwrap();
        assert!(read_session(&tmp, &outside.to_string_lossy()).is_err());
        // 非 jsonl → 拒绝
        let txt = dir.join("note.txt");
        std::fs::write(&txt, "x").unwrap();
        assert!(read_session(&tmp, &txt.to_string_lossy()).is_err());
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    #[cfg(unix)]
    fn list_sessions_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;
        let tmp = std::env::temp_dir().join(format!("ccm-hq-sym-{}", std::process::id()));
        let projects = tmp.join("projects");
        std::fs::create_dir_all(&projects).unwrap();
        // projects/ 外的目录，放一个 jsonl
        let outside = tmp.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("leak.jsonl"), r#"{"type":"user"}"#).unwrap();
        // projects/sneaky -> ../outside（名字合法、无分隔符、无 ..，旧 string 校验放行）
        symlink(&outside, projects.join("sneaky")).unwrap();
        // canonicalize 前缀校验解析 symlink 后落在 projects/ 外 → 拒绝
        assert!(list_sessions(&tmp, "sneaky").is_err());
        std::fs::remove_dir_all(&tmp).ok();
    }
}

#[cfg(test)]
mod tail_tests {
    use super::split_tail;

    fn meta_of(m: &str) -> (u64, u64) {
        let v: serde_json::Value = serde_json::from_str(m.trim()).unwrap();
        (
            v["total"].as_u64().unwrap(),
            v["tail_from"].as_u64().unwrap(),
        )
    }

    #[test]
    fn tail_splits_and_numbers() {
        let data = b"{\"a\":0}\n{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n{\"a\":4}\n";
        let (meta, tail, head) = split_tail(data, 2);
        assert_eq!(meta_of(&meta), (5, 3));
        assert_eq!(tail, b"{\"a\":3}\n{\"a\":4}\n");
        assert_eq!(head, b"{\"a\":0}\n{\"a\":1}\n{\"a\":2}\n");
    }

    #[test]
    fn n_bigger_than_total_is_all_tail() {
        let data = b"{\"a\":0}\n{\"a\":1}\n";
        let (meta, tail, head) = split_tail(data, 500);
        assert_eq!(meta_of(&meta), (2, 0));
        assert_eq!(tail, data.as_slice());
        assert!(head.is_empty());
    }

    #[test]
    fn empty_and_torn_only() {
        let (meta, tail, head) = split_tail(b"", 500);
        assert_eq!(meta_of(&meta), (0, 0));
        assert!(tail.is_empty() && head.is_empty());
        let (meta, tail, head) = split_tail(b"{\"torn", 500);
        assert_eq!(meta_of(&meta), (0, 0));
        assert!(tail.is_empty() && head.is_empty());
    }

    #[test]
    fn blank_lines_not_counted_but_bytes_preserved() {
        let data = b"{\"a\":0}\n\n{\"a\":1}\n{\"a\":2}\n";
        let (meta, tail, head) = split_tail(data, 1);
        assert_eq!(meta_of(&meta), (3, 2));
        assert_eq!(tail, b"{\"a\":2}\n");
        assert_eq!(head, b"{\"a\":0}\n\n{\"a\":1}\n");
    }
}
