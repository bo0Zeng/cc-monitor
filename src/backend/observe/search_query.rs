//! issue #28：远端全文搜索（一次性查询子命令 `--search`）。
//!
//! cc-monitor 通过**独立 SSH 连接**一次性 exec `<daemon> --search <query> [opts]`，
//! daemon 在远端 CPU 上扫 `<claude_dir>/projects/**/*.jsonl`、做服务端搜索（避免拉
//! 整库回本地），输出**每命中会话一行** camelCase JSON（与 monitor `search::SessionHits`
//! 形状严格一致，可直接反序列化）：
//! `{sessionId,projectPath,projectName,jsonlPath,title,updatedAt,hitCount,hits:[{uuid,tsMs,kind,before,matched,after}]}`
//!
//! 🔴 语义与本地 `../../bridge/src/search.rs` **不是「对齐」，是同一份**〔`K-R100` 09-13〕：
//! 抽取 / 匹配 / snippet 的 12 个助手、4 个口径常量、snippet 预算与预算顺序全部住
//! `../src/bridge/crates/search-core`，两侧都调它。
//! 收口前本文件各写了一遍那 12 个（`K-R85` 实测逐字相同），而 monitor 的
//! `cross_half_edge_registry::CROSS_EDGES` 17 条跨轨边里 **search 零命中** ⇒
//! **没有任何判据在拦着它们漂开**。判据现在有了，住
//! `../../bridge/src/search_kou_jing_guard.rs::the_search_kou_jing_has_exactly_one_home`。
//! daemon 无 `parse_line`，故仍直接在 `serde_json::Value` 上抽取 —— 那是**取数**的差别，
//! 不是**口径**的差别。
//!
//! 安全：路径严格限 `<claude_dir>/projects/`（canonicalize 前缀校验，复刻 history_query）；
//! 只读铁律（cc-monitor 不写远端）成立——本模块只 read_dir / read。

// U2/U3：这两个原来在本文件里各有一份逐字相同的副本。去向**不同**：
// `projects_root` 跨 observe/control 两层 ⇒ `common/`；`mtime_ms` 两个调用点同属 observe
// ⇒ U3 按 `common/` 自己的「≥2 层」门槛搬回 `observe/`。
use crate::agents::claudecode::paths::projects_root;
use crate::observe::fs::mtime_ms;
use search_core::{SnippetBudget, SnippetVerdict, MAIN_CAP, TOOL_CAP};
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// 🔴 口径常量**一个都不在这里**（`K-R100`）——它们就是口径本身，本文件再写一个同样的
// 字面量 = 又开了第二份。`MAIN_CAP` / `TOOL_CAP` / `SNIPPET_CTX` / `PER_SESSION_CAP` /
// `DEFAULT_LIMIT` 住 `search_core`，monitor 用的是同一份。

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
pub fn run(agent_home: &Path, args: &[String]) -> i32 {
    // args[0] == "--search"
    let query = match args.get(1) {
        Some(q) => q.as_str(),
        None => {
            eprintln!("cc-monitor-remote query error: --search requires <query> argument");
            return 2;
        }
    };
    let opts = parse_opts(&args[2.min(args.len())..]);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match search(agent_home, query, &opts, &mut out) {
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
        limit: search_core::DEFAULT_LIMIT,
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
                    opts.limit = search_core::clamp_limit(
                        v.parse::<usize>().unwrap_or(search_core::DEFAULT_LIMIT),
                    );
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
///
/// ⚠ `out` 是参数而不是直接 `stdout()`：**输出的行序就是预算顺序**，
/// 而「预算按最近优先花」正是 `KR100D2` 要判的性质 —— 判据得看得见那个序
/// （`the_snippet_budget_goes_to_the_most_recent_sessions`）。直接写 stdout 就判不了。
fn search(
    agent_home: &Path,
    query: &str,
    opts: &SearchOpts,
    out: &mut impl Write,
) -> Result<(), String> {
    let q = query.trim().to_lowercase();
    let root = projects_root(agent_home);
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
        .filter(|e| {
            e.file_type().is_file() && crate::agents::claudecode::records::is_session_file(e.path())
        })
        .map(|e| e.into_path())
        .collect();

    // 🔴 `K-R100`：**snippet 预算按最近优先花**，与 monitor 同一份排序
    // （`search_core::sort_by_recency`）。收口前这里没有任何排序、按 `WalkDir`
    // （= `readdir`）先走到的顺序花 ⇒ 与 monitor 的 `updated_at desc` 几乎正交
    // （实测走序前 3 与最近序前 3 **重合 0/3**），而**展示顺序两边都是最近优先**
    // ⇒ 缺 snippet 的正好是列表最上面那几张卡。理由与读数逐条在
    // `search_core::sort_by_recency` 的文档注释里。
    let mut files: Vec<(PathBuf, i64)> = files
        .into_iter()
        .map(|p| {
            let m = mtime_ms(&p);
            (p, m)
        })
        .collect();
    search_core::sort_by_recency(&mut files, |(_, m)| *m);

    let mut budget = SnippetBudget::new(opts.limit);
    for (path, updated_at) in files {
        // 防 symlink 逃逸：canonicalize 后仍须在 projects/ 下。
        let Ok(canon) = path.canonicalize() else {
            continue;
        };
        if !canon.starts_with(&canon_root) {
            continue;
        }
        if let Some(session) = build_session_hits(&path, &q, opts, &mut budget, updated_at) {
            writeln!(out, "{session}").map_err(|e| format!("stdout write failed: {e}"))?;
        }
    }
    Ok(())
}

/// 扫一个 jsonl，返回该会话的命中 JSON（无命中 → None）。`budget` 跨会话累计已构造
/// snippet 数，达到 `opts.limit` 后只计数不再构造 snippet（贵活封顶）——
/// 🔴 判定在 `search_core::SnippetBudget`，与 monitor 同一份，且它**分得清**
/// 「全局预算用完」与「单会话满 `PER_SESSION_CAP` 条」（收口前这两件事挤在一个
/// `if` 里，下游只看得到 `hitCount > hits.len()` 这一个信号）。
/// `updated_at` 由调用方传入（排序时已 stat 过一次，别再 stat 第二次）。
fn build_session_hits(
    path: &Path,
    q_lc: &str,
    opts: &SearchOpts,
    budget: &mut SnippetBudget,
    updated_at: i64,
) -> Option<Value> {
    let session_id = path.file_stem()?.to_str()?.to_string();
    let content = std::fs::read_to_string(path).ok()?;

    let mut hits: Vec<Value> = Vec::new();
    let mut hit_count: u32 = 0;
    // 本会话有命中因**全局预算用完**而拿不到 snippet（≠ 单会话超 PER_SESSION_CAP）。
    let mut session_starved = false;
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
                let raw_main = content_v
                    .map(search_core::extract_text_blocks)
                    .unwrap_or_default();
                let main = if is_assistant {
                    search_core::truncate_plain(&raw_main, MAIN_CAP)
                } else {
                    search_core::truncate_plain(&search_core::clean_user_text(&raw_main), MAIN_CAP)
                };
                if !is_assistant && first_user_excerpt.is_empty() && !main.is_empty() {
                    first_user_excerpt = search_core::truncate_excerpt(&main, 120);
                }
                let tool = if opts.include_tools {
                    content_v
                        .map(|c| {
                            search_core::truncate_plain(
                                &search_core::extract_tool_text(c, is_assistant),
                                TOOL_CAP,
                            )
                        })
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
                match budget.take(hits.len()) {
                    SnippetVerdict::Give => {
                        let (hkind, text) = if in_main {
                            (if is_assistant { "assistant" } else { "user" }, &main)
                        } else {
                            ("tool", &tool)
                        };
                        let (before, matched, after) = search_core::make_snippet(text, q_lc);
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
                    }
                    SnippetVerdict::BudgetExhausted => session_starved = true,
                    SnippetVerdict::SessionCapped => {}
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
    let title = search_core::session_title(ai_title.as_deref(), &first_user_excerpt, &session_id);
    Some(serde_json::json!({
        "sessionId": session_id,
        "projectPath": project_path,
        "projectName": project_name,
        "jsonlPath": path.to_string_lossy(),
        "title": title,
        "updatedAt": updated_at,
        "hitCount": hit_count,
        "hits": hits,
        // 🔴 `K-R100`：**远端截断从此说得出话。** 收口前这一行不存在 ⇒ 一份
        // `hitCount: 12, hits: []` 与「这个会话没什么可看的」在 monitor 与前端眼里同形，
        // 而 `merge_search_results` 逐字 `truncated: local.truncated` 把远端那一半整个丢掉。
        "hitsTruncated": session_starved,
    }))
}

// === 文本抽取 / snippet / 截断：**一份都不在这里**（`K-R100`） ===
//
// 🔴 那 12 个助手（`extract_text_blocks` · `extract_tool_text` · `stringify_json` ·
// `clean_user_text` · `make_snippet` · `find_ci` · `tail_chars` · `head_chars` ·
// `collapse_ws` · `collapse_ws_keep_ellipsis` · `truncate_plain` · `truncate_excerpt`）
// 与它们的单元测试全部住 `../src/bridge/crates/search-core`。monitor 调它，本文件也调它
// —— **同一份**。别在这里「顺手再写一个小的」：那就是收口前的形状（两份、逐字同、零判据）。

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

    // ⚠ `extract_*` / `clean_user_text` / `find_ci` / `make_snippet` 那 4 条单元测试
    // **已随实现搬进 `../src/bridge/crates/search-core`**（`K-R100`）。
    // 在这里再抄一份 = 又在本文件养出一个「口径的家」，正是本件要治的形状。

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
        let mut budget = SnippetBudget::new(opts.limit);
        let hit = build_session_hits(&jsonl, "docker", &opts, &mut budget, 1_700_000_000_000)
            .expect("must hit");
        assert_eq!(hit["sessionId"], "s1");
        assert_eq!(
            hit["updatedAt"], 1_700_000_000_000i64,
            "updatedAt 用调用方传进来的那份"
        );
        assert_eq!(hit["projectPath"], "/home/pi/proj");
        assert_eq!(
            hit["hitCount"], 2,
            "user + assistant both match 'docker' ci"
        );
        assert!(hit["hits"].as_array().unwrap().len() == 2);
        assert_eq!(
            hit["hitsTruncated"], false,
            "预算充足、两条都给了 snippet ⇒ 没被截断"
        );

        // scope=user → 只 user 命中
        let opts_u = SearchOpts {
            include_tools: false,
            scope: Some("user".into()),
            after_ms: 0,
            limit: 300,
        };
        let mut budget2 = SnippetBudget::new(opts_u.limit);
        let hu = build_session_hits(&jsonl, "docker", &opts_u, &mut budget2, 1).expect("user hits");
        assert_eq!(hu["hitCount"], 1);

        std::fs::remove_dir_all(&tmp).ok();
    }

    // ── `K-R100` 的三条行为判据 ──────────────────────────────────────────
    // 它们**不判源码文本**（那是判写法，且今天两侧本来就一样，会恒绿）。
    // 判的是「本侧真跑出来的东西跟不跟 `search_core` 走」。

    /// 建一棵 `<home>/projects/<proj>/<sid>.jsonl` 语料，`mtimes` 按给定毫秒设。
    fn corpus(tag: &str, sessions: &[(&str, i64, usize)]) -> std::path::PathBuf {
        let tmp = std::env::temp_dir().join(format!("ccm-kr100-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        for (sid, mtime_ms_val, n_hits) in sessions {
            let proj = tmp.join("projects").join(format!("p-{sid}"));
            std::fs::create_dir_all(&proj).unwrap();
            let jsonl = proj.join(format!("{sid}.jsonl"));
            let lines: Vec<String> = (0..*n_hits)
                .map(|i| {
                    format!(
                        r#"{{"type":"user","uuid":"{sid}-{i}","timestamp":"2026-01-01T00:00:0{}Z","cwd":"/w","message":{{"role":"user","content":"命中 docker 第 {i} 条"}}}}"#,
                        i % 10
                    )
                })
                .collect();
            std::fs::write(&jsonl, lines.join("\n")).unwrap();
            let t = filetime_from_ms(*mtime_ms_val);
            set_mtime(&jsonl, t);
        }
        tmp
    }
    fn filetime_from_ms(ms: i64) -> std::time::SystemTime {
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms as u64)
    }
    /// 只用 std 设 mtime（本 crate 不引 `filetime`）。
    ///
    /// 🔴 `K-R122`（09-14）：**这一处换成了 `std::fs::File::set_times`，加的不是 `cfg`。**
    /// 上一版走 `unsafe { libc::utimensat(libc::AT_FDCWD, …) }` —— 那两个名字在
    /// `x86_64-pc-windows-msvc` 上**不存在**（`libc` 的 Windows 侧没有它们），
    /// 于是 daemon 那条「Windows 编得过」的跨 target check 在 **test 档**上红了 2 个错。
    ///
    /// ⚠ **为什么这一处与 `sidecars/codepicture/acquire.rs` 那三条的处置相反**：
    /// 那三条断的是**只在 unix 上成立的语义**（可执行位 · `chmod` 造出来的 `EACCES`），
    /// 换个平台连前提都不成立 ⇒ 加 `cfg`；而**「把一份文件的 mtime 设成某个值」在
    /// Windows 上照样成立**，缺的只是一条跨平台的写法 —— `std` 从 1.75 起就有
    /// （[`std::fs::FileTimes`]）。⇒ 这一处该换 API，不该加 `cfg`：加了 `cfg`
    /// 就等于把「预算按最近优先花」那一族判据在 Windows 上整族关掉，而它们本来跑得了。
    ///
    /// ⚠ 语义逐字对齐旧版：旧版给 `times[0]`（atime）与 `times[1]`（mtime）**同一个值**，
    /// 这里同样两个都设。
    fn set_mtime(p: &Path, t: std::time::SystemTime) {
        let f = std::fs::File::options()
            .write(true)
            .open(p)
            .expect("打开要改 mtime 的那份文件失败，本条判据的前提没建起来");
        f.set_times(std::fs::FileTimes::new().set_accessed(t).set_modified(t))
            .expect("set_times 失败，本条判据的前提没建起来");
    }

    fn run_search(home: &Path, q: &str, limit: usize) -> Vec<Value> {
        let opts = SearchOpts {
            include_tools: false,
            scope: None,
            after_ms: 0,
            limit,
        };
        let mut buf: Vec<u8> = Vec::new();
        search(home, q, &opts, &mut buf).expect("search ok");
        String::from_utf8(buf)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    /// `KR100D2`：**预算按最近优先花** —— 输出的行序就是预算顺序。
    ///
    /// 死值验①（恢复成「按文件系统先走到的顺序」）当场红：语料刻意让
    /// **创建序 / 名字序 与 mtime 序相反**，所以只要把 `sort_by_recency` 拿掉，
    /// 第一行就不再是最新那个会话。
    #[test]
    fn the_snippet_budget_goes_to_the_most_recent_sessions() {
        // 创建序 old → mid → new；mtime 序 new(3000) > mid(2000) > old(1000)
        let home = corpus(
            "order",
            &[("old", 1_000, 3), ("mid", 2_000, 3), ("new", 3_000, 3)],
        );
        let rows = run_search(&home, "docker", 300);
        let ids: Vec<String> = rows
            .iter()
            .map(|r| r["sessionId"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            ids,
            vec!["new", "mid", "old"],
            "预算/输出顺序必须是最近优先（与 monitor 的 `updated_at desc` 同一份 \
             `search_core::sort_by_recency`）。收口前这里没有排序、按 readdir 走 —— \
             而两侧的**展示**顺序都是最近优先 ⇒ 缺 snippet 的正好是列表最上面那几张卡。"
        );

        // 预算只够 2 条 ⇒ 两条都必须花在**最新**那个会话上。
        let rows = run_search(&home, "docker", 2);
        let newest = rows.iter().find(|r| r["sessionId"] == "new").unwrap();
        assert_eq!(
            newest["hits"].as_array().unwrap().len(),
            2,
            "预算先给最新的"
        );
        for r in rows.iter().filter(|r| r["sessionId"] != "new") {
            assert_eq!(r["hits"].as_array().unwrap().len(), 0);
        }
        std::fs::remove_dir_all(&home).ok();
    }

    /// `KR100D3`：**截断说得出话**，而且与「本会话就这么点命中」分得开。
    #[test]
    fn truncation_is_stated_not_left_to_an_empty_array() {
        let home = corpus("trunc", &[("a", 3_000, 2), ("b", 2_000, 5)]);
        // 预算 2 ⇒ 全给 a，b 一条 snippet 都没有。
        let rows = run_search(&home, "docker", 2);
        let a = rows.iter().find(|r| r["sessionId"] == "a").unwrap();
        let b = rows.iter().find(|r| r["sessionId"] == "b").unwrap();
        assert_eq!(a["hitsTruncated"], false, "a 全给到了 ⇒ 没被砍");
        assert_eq!(b["hits"].as_array().unwrap().len(), 0);
        assert_eq!(b["hitCount"], 5);
        assert_eq!(
            b["hitsTruncated"], true,
            "🔴 `hitCount>0` 而 `hits: []` 必须自己说出「我被预算砍了」——\
             收口前这一格不存在，下游只能拿 `hitCount > hits.len()` 反推，\
             而那个式子对「预算砍的」与「本会话超 30 条」给出同一个答案。"
        );
        // 预算充足 ⇒ 两个都 false（反空真：这条判据不是恒 true）
        let rows = run_search(&home, "docker", 300);
        for r in &rows {
            assert_eq!(r["hitsTruncated"], false, "预算充足时不许乱报截断");
        }
        std::fs::remove_dir_all(&home).ok();
    }

    /// `KR100D1` 第 ③ 刀（本侧那一半）：**改 `search_core` 一处，本侧真跑出来的东西跟着变**。
    ///
    /// 期望值**从 `search_core::SNIPPET_CTX` 取**，实际值从本文件的生产管线
    /// （`search` → `build_session_hits` → `search_core::make_snippet`）来。
    /// · 改 core 的 `SNIPPET_CTX` ⇒ 实际与期望**一起动**，本条仍绿（＝行为跟着变了）；
    /// · 本侧哪天自己写回一个 `const SNIPPET_CTX = 48` ⇒ 实际不动、期望动 ⇒ **当场红**。
    /// monitor 侧有一条同形的（`search_tests.rs::the_snippet_window_comes_from_core`）。
    #[test]
    fn the_snippet_window_comes_from_core() {
        let ctx = search_core::SNIPPET_CTX;
        let filler = "x".repeat(ctx * 4);
        let tmp = std::env::temp_dir().join(format!("ccm-kr100-ctx-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        let proj = tmp.join("projects").join("p");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(
            proj.join("s.jsonl"),
            format!(
                r#"{{"type":"user","uuid":"u","timestamp":"2026-01-01T00:00:00Z","cwd":"/w","message":{{"role":"user","content":"{filler}docker{filler}"}}}}"#
            ),
        )
        .unwrap();
        let rows = run_search(&tmp, "docker", 300);
        let hit = &rows[0]["hits"][0];
        // 两侧都截断了 ⇒ before = `…` + ctx 字符、after = ctx 字符 + `…`
        assert_eq!(
            hit["before"].as_str().unwrap().chars().count(),
            ctx + 1,
            "snippet 前窗必须等于 `search_core::SNIPPET_CTX`（={ctx}）+ 省略号"
        );
        assert_eq!(hit["after"].as_str().unwrap().chars().count(), ctx + 1);
        std::fs::remove_dir_all(&tmp).ok();
    }
}
