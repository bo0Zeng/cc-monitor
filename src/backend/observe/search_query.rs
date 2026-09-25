//! issue #28：远端全文搜索（一次性查询子命令 `--search`）。
//!
//! cc-monitor 通过**独立 SSH 连接**一次性 exec `<backend> --search <query> [opts]`，
//! backend 在远端 CPU 上扫 `<claude_dir>/projects/**/*.jsonl`、做服务端搜索（避免拉
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
//! backend 无 `parse_line`，故仍直接在 `serde_json::Value` 上抽取 —— 那是**取数**的差别，
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
            eprintln!("cc-monitor-backend query error: --search requires <query> argument");
            return 2;
        }
    };
    let opts = parse_opts(&args[2.min(args.len())..]);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match search(agent_home, query, &opts, &mut out) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("cc-monitor-backend search error: {e}");
            2
        }
    }
}

/// 帧面那条（`history-search`）的入口：`rest` 是 `--search <query>` **之后**那一截，
/// 解析走**同一个** [`parse_opts`] —— 选项的口径只有一份，不在帧面另写一套 JSON 解析。
pub(crate) fn search_into(
    agent_home: &Path,
    query: &str,
    rest: &[String],
    out: &mut impl Write,
) -> Result<(), String> {
    let opts = parse_opts(rest);
    search(agent_home, query, &opts, out)
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
                // 〔SE2〕「这条记录拿哪两段文本去搜、命中算哪一种」只住 [`record_text`] / [`record_hit`]：
                // 会话内查找（`--find-in-session`）调的是同一对函数。
                let Some(rt) = record_text(&v, opts.include_tools) else {
                    continue;
                };
                if !rt.is_assistant && first_user_excerpt.is_empty() && !rt.main.is_empty() {
                    first_user_excerpt = search_core::truncate_excerpt(&rt.main, 120);
                }
                // scope 过滤：想要 user 却是 assistant（或反之）→ 跳过。
                if let Some(s) = opts.scope.as_deref() {
                    let want_user = s == "user";
                    if want_user == rt.is_assistant {
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
                let Some((hkind, text)) = record_hit(&rt, q_lc) else {
                    continue;
                };
                hit_count += 1;
                match budget.take(hits.len()) {
                    SnippetVerdict::Give => {
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
        // 而 `merge_search_results`〔散文墓碑〕逐字 `truncated: local.truncated` 把远端那一半整个丢掉。
        "hitsTruncated": session_starved,
    }))
}

/// 一条 user / assistant 记录拿去搜的两段文本（〔SE2〕从 `build_session_hits` 里拆出来）。
pub(crate) struct RecordText {
    pub(crate) is_assistant: bool,
    /// 正文：文本块；user 那侧先剥 CLI 注入的包装（`clean_user_text`），再按 `MAIN_CAP` 截断。
    pub(crate) main: String,
    /// 工具内容（tool_use 入参 / tool_result 输出 / thinking），按 `TOOL_CAP` 截断；不搜工具时空串。
    pub(crate) tool: String,
}

/// 一条已解析的记录 → 拿去搜的文本；不是 user / assistant ⇒ `None`。
///
/// 🔴 **全局搜索（`--search`）与会话内查找（`--find-in-session`）的口径只有这一个住址**；
/// 抽取 / 剥注入 / 截断的助手本身住 `search-core`（与 monitor 同一份）。
pub(crate) fn record_text(v: &Value, include_tools: bool) -> Option<RecordText> {
    let is_assistant = match v.get("type").and_then(Value::as_str) {
        Some("assistant") => true,
        Some("user") => false,
        _ => return None,
    };
    let content_v = v.get("message").and_then(|m| m.get("content"));
    let raw_main = content_v
        .map(search_core::extract_text_blocks)
        .unwrap_or_default();
    let main = if is_assistant {
        search_core::truncate_plain(&raw_main, MAIN_CAP)
    } else {
        search_core::truncate_plain(&search_core::clean_user_text(&raw_main), MAIN_CAP)
    };
    let tool = if include_tools {
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
    Some(RecordText {
        is_assistant,
        main,
        tool,
    })
}

/// 命中判定：先看正文、再看工具内容（大小写不敏感子串）。命中 ⇒ `(种类, 命中的那段文本)`，
/// 种类是 `"user"` / `"assistant"` / `"tool"`（与 `Hit.kind` 同一套词）。`q_lc` 已小写、已 trim。
pub(crate) fn record_hit<'a>(rt: &'a RecordText, q_lc: &str) -> Option<(&'static str, &'a str)> {
    if rt.main.to_lowercase().contains(q_lc) {
        return Some((if rt.is_assistant { "assistant" } else { "user" }, &rt.main));
    }
    if !rt.tool.is_empty() && rt.tool.to_lowercase().contains(q_lc) {
        return Some(("tool", &rt.tool));
    }
    None
}

/// 会话内查找一次最多列多少条（缺省）。尾行照报**全量**命中数。
pub(crate) const FIND_DEFAULT_LIMIT: usize = 500;
/// 会话内查找的上限封顶（调用方要得再多也只列这么多）。
pub(crate) const FIND_MAX_LIMIT: usize = 2000;

/// 〔SE2 · `设计/10 §6 步 6`〕**会话内查找**的内核：读 `r`（一份会话，从头）逐行找 `query`，
/// 出三段（形状登记 `IPC-PROTOCOL.md §10.5`）：
/// 1. 头 `{"kind":"session_find","v":1}`；
/// 2. 每条命中一行 `{"uuid","kind","before","matched","after"}`，**按文件序**（= 对话序），最多 `limit` 条；
/// 3. 尾 `{"kind":"session_find_end","count":N,"total":T}` —— `T` = 全量命中数（≥ N）。**没有尾行 ⇒ 截断**。
///
/// 与 `--search` 的差别只在「扫哪些文件、给多少条」：口径（[`record_text`] / [`record_hit`] ＋ `search-core`
/// 的片段）同一份。没有 uuid 的记录不算（跳不过去 —— 列出来就是一条点了没反应的项）。
/// 只看**完整行**（torn 残尾下一次再看）。空查询 ⇒ 零条。返回 `(count, total)`。
pub(crate) fn write_session_find<R: std::io::BufRead, W: std::io::Write>(
    r: R,
    query: &str,
    include_tools: bool,
    limit: usize,
    out: &mut W,
) -> std::io::Result<(u64, u64)> {
    writeln!(out, "{{\"kind\":\"session_find\",\"v\":1}}")?;
    let (count, total) = scan_session_find(r, query, include_tools, limit, |hit| {
        serde_json::to_writer(&mut *out, hit)?;
        out.write_all(b"\n")
    })?;
    writeln!(
        out,
        "{{\"kind\":\"session_find_end\",\"count\":{count},\"total\":{total}}}"
    )?;
    Ok((count, total))
}

/// 〔C4b · 第四波 4B〕[`write_session_find`] 的中段：**逐条命中交给 `on_hit`**（按文件序、最多 `limit` 条），
/// 回 `(count, total)`。判定一行都不在这一层之外 —— CLI 那一臂（上面，写头尾三段：stdout 要分帧）与帧面那一臂
/// （`read_face.rs` 的 `history-find`，把同一串命中装成成品 `{total, hits}`）跑的是**同一个**扫描。
/// `on_hit` 回错 ⇒ 扫描当场停、错原样上抛（帧面那一臂靠它在整份超上限时停下）。
pub(crate) fn scan_session_find<R: std::io::BufRead>(
    mut r: R,
    query: &str,
    include_tools: bool,
    limit: usize,
    mut on_hit: impl FnMut(&Value) -> std::io::Result<()>,
) -> std::io::Result<(u64, u64)> {
    let q = query.trim().to_lowercase();
    let mut count: u64 = 0;
    let mut total: u64 = 0;
    let mut buf: Vec<u8> = Vec::new();
    while !q.is_empty() {
        buf.clear();
        let read = r.read_until(b'\n', &mut buf)?;
        if read == 0 || buf.last() != Some(&b'\n') {
            break;
        }
        let text = String::from_utf8_lossy(&buf[..buf.len() - 1]);
        let Ok(v) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}').trim())
        else {
            continue;
        };
        let Some(uuid) = v
            .get("uuid")
            .and_then(Value::as_str)
            .filter(|u| !u.is_empty())
        else {
            continue;
        };
        let Some(rt) = record_text(&v, include_tools) else {
            continue;
        };
        let Some((kind, hit)) = record_hit(&rt, &q) else {
            continue;
        };
        total += 1;
        if (count as usize) < limit {
            let (before, matched, after) = search_core::make_snippet(hit, &q);
            on_hit(&serde_json::json!({
                "uuid": uuid,
                "kind": kind,
                "before": before,
                "matched": matched,
                "after": after,
            }))?;
            count += 1;
        }
    }
    Ok((count, total))
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
/// 〔C4d〕开成 `pub(crate)`：历史会话清单那一行的开始时刻（`history_query::analyze_session`）用同一份。
pub(crate) fn parse_iso8601_ms(s: &str) -> Option<i64> {
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
#[path = "../../../tests/backend/observe/search_query_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/search_query_find_tests.rs"]
mod find_tests;
