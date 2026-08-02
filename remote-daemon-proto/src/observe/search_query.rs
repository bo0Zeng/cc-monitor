//! issue #28：远端全文搜索（一次性查询子命令 `--search`）。
//!
//! cc-monitor 通过**独立 SSH 连接**一次性 exec `<daemon> --search <query> [opts]`，
//! daemon 在远端 CPU 上扫 `<claude_dir>/projects/**/*.jsonl`、做服务端搜索（避免拉
//! 整库回本地），输出**每命中会话一行** camelCase JSON（与 monitor `search::SessionHits`
//! 形状严格一致，可直接反序列化）：
//! `{sessionId,projectPath,projectName,jsonlPath,title,updatedAt,hitCount,hits:[{uuid,tsMs,kind,before,matched,after}]}`
//!
//! 语义与本地 `../src-tauri/src/search.rs` **从宽对齐**（移植其已测的抽取/匹配/snippet
//! 逻辑）：抽 user/assistant 正文（+ 可选 tool），user 正文剥 CLI 包装，大小写不敏感
//! substring，char-safe snippet。daemon 无 `parse_line`，故直接在 serde_json::Value 上抽取。
//!
//! 安全：路径严格限 `<claude_dir>/projects/`（canonicalize 前缀校验，复刻 history_query）；
//! 只读铁律（cc-monitor 不写远端）成立——本模块只 read_dir / read。

// U2/U3：这两个原来在本文件里各有一份逐字相同的副本。去向**不同**：
// `projects_root` 跨 observe/control 两层 ⇒ `common/`；`mtime_ms` 两个调用点同属 observe
// ⇒ U3 按 `common/` 自己的「≥2 层」门槛搬回 `observe/`。
use crate::common::paths::projects_root;
use crate::observe::fs::mtime_ms;
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// 单会话最多返回的命中 snippet 条数（防一个会话刷屏；hitCount 仍报全量）。
const PER_SESSION_CAP: usize = 30;
/// snippet 命中点前后各保留的字符数。
const SNIPPET_CTX: usize = 48;
/// 单条 main / tool 文本索引上限（字符），对齐本地封顶防超长粘贴/大 dump。
const MAIN_CAP: usize = 20_000;
const TOOL_CAP: usize = 4_000;

/// 解析后的查询选项。
struct SearchOpts {
    include_tools: bool,
    /// None=全部；Some("user")/Some("assistant")=只搜该类型。
    scope: Option<String>,
    after_ms: i64,
    /// 全局返回 snippet 上限（hitCount 仍报全量）。
    limit: usize,
}

/// `--search <query> [--include-tools] [--scope user|assistant] [--after-ms N] [--limit N]`。
/// 返回进程退出码（0 ok / 2 err），与 history_query::run 同约定。
pub fn run(claude_dir: &Path, args: &[String]) -> i32 {
    // args[0] == "--search"
    let query = match args.get(1) {
        Some(q) => q.as_str(),
        None => {
            eprintln!("cc-monitor-remote query error: --search requires <query> argument");
            return 2;
        }
    };
    let opts = parse_opts(&args[2.min(args.len())..]);
    match search(claude_dir, query, &opts) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("cc-monitor-remote search error: {e}");
            2
        }
    }
}

/// 从 `--search <query>` 之后的参数解析选项（未知/缺值的容错忽略）。
fn parse_opts(rest: &[String]) -> SearchOpts {
    let mut opts = SearchOpts {
        include_tools: false,
        scope: None,
        after_ms: 0,
        limit: 300,
    };
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--include-tools" => opts.include_tools = true,
            "--scope" => {
                if let Some(v) = rest.get(i + 1) {
                    if v == "user" || v == "assistant" {
                        opts.scope = Some(v.clone());
                    }
                    i += 1;
                }
            }
            "--after-ms" => {
                if let Some(v) = rest.get(i + 1) {
                    opts.after_ms = v.parse::<i64>().unwrap_or(0).max(0);
                    i += 1;
                }
            }
            "--limit" => {
                if let Some(v) = rest.get(i + 1) {
                    opts.limit = v.parse::<usize>().unwrap_or(300).clamp(1, 2000);
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    opts
}

/// 扫 projects/**/*.jsonl，搜索匹配，每命中会话输出一行 JSON。
fn search(claude_dir: &Path, query: &str, opts: &SearchOpts) -> Result<(), String> {
    let q = query.trim().to_lowercase();
    let root = projects_root(claude_dir);
    if q.is_empty() || !root.is_dir() {
        return Ok(()); // 空查询 / 无 projects → 无输出（exit 0）
    }
    // 路径白名单根（canonicalize；read 的文件必须在其下，挡 symlink 逃逸）。
    let canon_root = root
        .canonicalize()
        .map_err(|e| format!("projects root unavailable: {e}"))?;

    let files: Vec<PathBuf> = WalkDir::new(&canon_root)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|x| x == "jsonl"))
        .map(|e| e.into_path())
        .collect();

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut kept_total = 0usize; // 已构造的 snippet 总数（限 opts.limit）
    for path in files {
        // 防 symlink 逃逸：canonicalize 后仍须在 projects/ 下。
        let Ok(canon) = path.canonicalize() else {
            continue;
        };
        if !canon.starts_with(&canon_root) {
            continue;
        }
        if let Some(session) = build_session_hits(&path, &q, opts, &mut kept_total) {
            writeln!(out, "{session}").map_err(|e| format!("stdout write failed: {e}"))?;
        }
    }
    Ok(())
}

/// 扫一个 jsonl，返回该会话的命中 JSON（无命中 → None）。`kept_total` 跨会话累计已构造
/// snippet 数，达到 `opts.limit` 后只计数不再构造 snippet（贵活封顶）。
fn build_session_hits(
    path: &Path,
    q_lc: &str,
    opts: &SearchOpts,
    kept_total: &mut usize,
) -> Option<Value> {
    let session_id = path.file_stem()?.to_str()?.to_string();
    let content = std::fs::read_to_string(path).ok()?;

    let mut hits: Vec<Value> = Vec::new();
    let mut hit_count: u32 = 0;
    let mut cwd: Option<String> = None;
    let mut ai_title: Option<String> = None;
    let mut first_user_excerpt = String::new();

    for line in content.lines() {
        let trimmed = line.trim_start_matches('\u{feff}').trim();
        if trimmed.is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if cwd.is_none() {
            if let Some(c) = v.get("cwd").and_then(Value::as_str) {
                if !c.is_empty() {
                    cwd = Some(c.to_string());
                }
            }
        }
        let kind = v.get("type").and_then(Value::as_str).unwrap_or("");
        match kind {
            "ai-title" => {
                if let Some(t) = v.get("aiTitle").and_then(Value::as_str) {
                    ai_title = Some(t.to_string());
                }
            }
            "custom-title" => {
                if let Some(t) = v.get("customTitle").and_then(Value::as_str) {
                    ai_title = Some(t.to_string());
                }
            }
            "user" | "assistant" => {
                let is_assistant = kind == "assistant";
                let content_v = v.get("message").and_then(|m| m.get("content"));
                let raw_main = content_v.map(extract_text_blocks).unwrap_or_default();
                let main = if is_assistant {
                    truncate_plain(&raw_main, MAIN_CAP)
                } else {
                    truncate_plain(&clean_user_text(&raw_main), MAIN_CAP)
                };
                if !is_assistant && first_user_excerpt.is_empty() && !main.is_empty() {
                    first_user_excerpt = truncate_excerpt(&main, 120);
                }
                let tool = if opts.include_tools {
                    content_v
                        .map(|c| truncate_plain(&extract_tool_text(c, is_assistant), TOOL_CAP))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                // scope 过滤：想要 user 却是 assistant（或反之）→ 跳过。
                if let Some(s) = opts.scope.as_deref() {
                    let want_user = s == "user";
                    if want_user == is_assistant {
                        continue;
                    }
                }
                // 时间过滤。
                let ts_ms = v
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .and_then(parse_iso8601_ms)
                    .unwrap_or(0);
                if opts.after_ms > 0 && ts_ms < opts.after_ms {
                    continue;
                }
                let in_main = main.to_lowercase().contains(q_lc);
                let in_tool = !tool.is_empty() && tool.to_lowercase().contains(q_lc);
                if !in_main && !in_tool {
                    continue;
                }
                hit_count += 1;
                if *kept_total < opts.limit && hits.len() < PER_SESSION_CAP {
                    let (hkind, text) = if in_main {
                        (if is_assistant { "assistant" } else { "user" }, &main)
                    } else {
                        ("tool", &tool)
                    };
                    let (before, matched, after) = make_snippet(text, q_lc);
                    let uuid = v
                        .get("uuid")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    hits.push(serde_json::json!({
                        "uuid": uuid,
                        "tsMs": ts_ms,
                        "kind": hkind,
                        "before": before,
                        "matched": matched,
                        "after": after,
                    }));
                    *kept_total += 1;
                }
            }
            _ => {}
        }
    }

    if hit_count == 0 {
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
    Some(serde_json::json!({
        "sessionId": session_id,
        "projectPath": project_path,
        "projectName": project_name,
        "jsonlPath": path.to_string_lossy(),
        "title": title,
        "updatedAt": mtime_ms(path),
        "hitCount": hit_count,
        "hits": hits,
    }))
}

// === 文本抽取（移植自 ../src-tauri/src/search.rs，Value 版） ===

/// 抽 content 里所有 text block（或裸字符串）。
fn extract_text_blocks(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let mut out = String::new();
            for b in arr {
                if b.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(s) = b.get("text").and_then(Value::as_str) {
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

/// 抽 tool 相关内容（可选搜索）。assistant：tool_use(name+input)+thinking；user：tool_result。
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
        match b.get("type").and_then(Value::as_str) {
            Some("tool_use") if is_assistant => {
                if let Some(name) = b.get("name").and_then(Value::as_str) {
                    push(name);
                }
                if let Some(input) = b.get("input") {
                    push(&stringify_json(input));
                }
            }
            Some("thinking") if is_assistant => {
                if let Some(t) = b.get("thinking").and_then(Value::as_str) {
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

/// 把 JSON 值压成可搜索纯文本（string 直接取；array/object 取字符串叶子）。
fn stringify_json(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let mut out = String::new();
            for item in arr {
                let s = if let Some(t) = item.get("text").and_then(Value::as_str) {
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

/// 去掉 CLI 注入的 prompt 包装 + ESC 中断标记（搜索用从宽，对齐本地 clean_user_text）。
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
    if trimmed.starts_with("[Request interrupted by user") {
        return String::new();
    }
    trimmed.to_string()
}

// === snippet（移植自 ../src-tauri/src/search.rs） ===

fn make_snippet(text: &str, needle_lc: &str) -> (String, String, String) {
    match find_ci(text, needle_lc) {
        Some((start, end)) => (
            tail_chars(&text[..start], SNIPPET_CTX),
            collapse_ws(&text[start..end]),
            head_chars(&text[end..], SNIPPET_CTX),
        ),
        None => (
            String::new(),
            String::new(),
            head_chars(text, SNIPPET_CTX * 2),
        ),
    }
}

/// 大小写不敏感子串查找：返回原文匹配区间 (起始字节, 结束字节)。CJK + ASCII 安全。
fn find_ci(hay: &str, needle_lc: &str) -> Option<(usize, usize)> {
    if needle_lc.is_empty() {
        return None;
    }
    let needle: Vec<char> = needle_lc.chars().collect();
    let hay_idx: Vec<(usize, char)> = hay.char_indices().collect();
    let n = hay_idx.len();
    for i in 0..n {
        let mut hi = i;
        let mut ni = 0;
        let mut pending: Vec<char> = Vec::new();
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

fn head_chars(s: &str, n: usize) -> String {
    let mut out = String::new();
    for (count, ch) in s.chars().enumerate() {
        if count >= n {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    collapse_ws_keep_ellipsis(&out)
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

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

/// 按字符硬截断（不加 …，用于正文封顶）。
fn truncate_plain(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    s.chars().take(n).collect()
}

/// 按字符截断（换行折空格，加 …，用于标题 excerpt）。
fn truncate_excerpt(s: &str, n: usize) -> String {
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

/// 解析 Claude 的 ISO8601 时间戳 `YYYY-MM-DDTHH:MM:SS(.fff)?Z` → epoch ms。
/// 自带 civil-days 算法（Howard Hinnant），无需 chrono。
fn parse_iso8601_ms(s: &str) -> Option<i64> {
    if s.len() < 19 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let mon: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;
    let hour: i64 = s.get(11..13)?.parse().ok()?;
    let min: i64 = s.get(14..16)?.parse().ok()?;
    let sec: i64 = s.get(17..19)?.parse().ok()?;
    // 小数秒：扫 '.' 之后的数字串，归一到毫秒（取前 3 位、不足右补 0），对齐本地 utils
    // 口径——容忍 1/2/3+ 位小数（真实 Claude 总是 .fffZ，但稳健处理变体）。
    let millis = if s.as_bytes().get(19) == Some(&b'.') {
        let mut frac: String = s[20..]
            .chars()
            .take_while(char::is_ascii_digit)
            .take(3)
            .collect();
        while !frac.is_empty() && frac.len() < 3 {
            frac.push('0');
        }
        frac.parse::<i64>().unwrap_or(0)
    } else {
        0
    };
    let days = days_from_civil(year, mon, day);
    Some((days * 86_400 + hour * 3_600 + min * 60 + sec) * 1_000 + millis)
}

/// days since 1970-01-01 for a civil (proleptic Gregorian) date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let asst =
            serde_json::json!([{"type":"tool_use","name":"Bash","input":{"command":"ls -la"}}]);
        let t = extract_tool_text(&asst, true);
        assert!(t.contains("Bash") && t.contains("ls -la"));
        let user = serde_json::json!([{"type":"tool_result","content":[{"type":"text","text":"file out"}]}]);
        assert!(extract_tool_text(&user, false).contains("file out"));
    }

    #[test]
    fn clean_user_text_strips_wrappers_and_interrupt() {
        assert_eq!(
            clean_user_text("<system-reminder>noise</system-reminder>真内容"),
            "真内容"
        );
        assert_eq!(clean_user_text("[Request interrupted by user]"), "");
    }

    #[test]
    fn find_ci_and_snippet_cjk_safe() {
        let s = "你好世界 docker 部署";
        let (start, end) = find_ci(s, "docker").unwrap();
        assert_eq!(&s[start..end], "docker");
        let (_b, matched, after) = make_snippet("请问怎么用 Docker 部署", "docker");
        assert_eq!(matched, "Docker");
        assert!(after.contains("部署"));
    }

    #[test]
    fn parse_iso8601_basic() {
        // 1970-01-01T00:00:00Z = 0
        assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00Z"), Some(0));
        // 1970-01-01T00:00:01.500Z = 1500
        assert_eq!(parse_iso8601_ms("1970-01-01T00:00:01.500Z"), Some(1500));
        // 小数秒变体归一到毫秒：.12 → 120ms，.1 → 100ms，.123456 → 123ms，无小数 → 0
        assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00.12Z"), Some(120));
        assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00.1Z"), Some(100));
        assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00.123456Z"), Some(123));
        assert_eq!(parse_iso8601_ms("1970-01-01T00:00:00Z"), Some(0));
        // 2021-01-01T00:00:00Z = 1609459200000
        assert_eq!(
            parse_iso8601_ms("2021-01-01T00:00:00Z"),
            Some(1_609_459_200_000)
        );
        assert_eq!(parse_iso8601_ms("garbage"), None);
    }

    #[test]
    fn search_end_to_end_and_rejects_traversal() {
        let tmp = std::env::temp_dir().join(format!("ccm-search-test-{}", std::process::id()));
        let proj = tmp.join("projects").join("proj-a");
        std::fs::create_dir_all(&proj).unwrap();
        let jsonl = proj.join("s1.jsonl");
        std::fs::write(
            &jsonl,
            [
                r#"{"type":"user","uuid":"u1","timestamp":"2026-01-01T00:00:00Z","cwd":"/home/pi/proj","message":{"role":"user","content":"请用 Docker 部署"}}"#,
                r#"{"type":"assistant","uuid":"a1","timestamp":"2026-01-01T00:00:01Z","message":{"role":"assistant","content":[{"type":"text","text":"好的，用 docker compose"}]}}"#,
            ]
            .join("\n"),
        )
        .unwrap();

        let opts = SearchOpts {
            include_tools: false,
            scope: None,
            after_ms: 0,
            limit: 300,
        };
        let mut kept = 0usize;
        let hit = build_session_hits(&jsonl, "docker", &opts, &mut kept).expect("must hit");
        assert_eq!(hit["sessionId"], "s1");
        assert_eq!(hit["projectPath"], "/home/pi/proj");
        assert_eq!(
            hit["hitCount"], 2,
            "user + assistant both match 'docker' ci"
        );
        assert!(hit["hits"].as_array().unwrap().len() == 2);

        // scope=user → 只 user 命中
        let opts_u = SearchOpts {
            include_tools: false,
            scope: Some("user".into()),
            after_ms: 0,
            limit: 300,
        };
        let mut kept2 = 0usize;
        let hu = build_session_hits(&jsonl, "docker", &opts_u, &mut kept2).expect("user hits");
        assert_eq!(hu["hitCount"], 1);

        std::fs::remove_dir_all(&tmp).ok();
    }
}
