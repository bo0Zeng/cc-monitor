//! **只读查询的帧面宿主** —— 层 1 那一搬。
//!
//! # 它补的是哪一格
//!
//! 「帧面 16 条，账号与 8 条只读查询都不在 ⇒ 每点一下拨一次 SSH、
//! 每 10 秒对每台机器握一次手」。那 8 条今天全是**一次性子命令**：monitor 每问一次就
//! 新开一条 TCP+SSH+鉴权，exec 一次本二进制，读完 stdout 就断。
//! 而同一台机器上**已经有一条**长连接（流模式那条），入方向早就能一问一答
//! （`stream/inbound/`）—— 缺的只是这 8 条没登记上去。
//!
//! # 为什么住顶层，而不是 `observe/`
//!
//! 与 `files/` 同一个理由：`stream/inbound/` 不许出现 `observe::`
//! （`inbound_structure_guards::inbound_never_reaches_into_the_observe_layer`），
//! 而查询的本体就住 `observe/`。⇒ 本文件是那道线**之外**的一层薄宿主：
//! 解帧面的 `args`、把输出装进应答、给错误一个 code。**查询本身一行不在这里** ——
//! 每条都调 CLI 那一臂**同一个函数**（只是把 `out` 从 stdout 换成内存），
//! 「一份代码两种宿主」在这一族上的样子。
//!
//! # 应答的两种形状
//!
//! - **按行**（六条）：`{"lines": [...]}` —— 与 CLI 那条 stdout 逐行、trim 过、剔空行后一致；
//!   整份输出超过 [`LINES_CAP_BYTES`] ⇒ `too_large`（不截断：截断的清单会被当成完整的用）。
//! - **按字节分页**（`history-read`）：一份会话可以是几百 MB，一帧装不下
//!   ⇒ 调用方给 `[offset, until)`，回一页（切在行尾）＋ 续点，调用方循环到 `eof`。
//!   `history-tail` 只回那张「尾段在哪」的图，正文仍走 `history-read`。
//!
//! # 它**不**做什么
//!
//! 不拨号、不起进程、不写盘 —— 全是既有读函数的换壳。`readonly_guard` 那条写盘禁令照旧管它。

use crate::stream::inbound::spec::wire;
use copy_core::copy_text;
use serde_json::{json, Value};

/// `history-read` 一页的目标字节数。
///
/// 取 1 MiB：一页就是一帧应答，它在后端写出方向上与实时 `line` 帧**排同一条队** ——
/// 页越大，实时行被挡住的时间越长；页越小，往返越多。1 MiB 在 1 Mbps 的弱网上约 8 秒一页。
pub(crate) const READ_PAGE_BYTES: usize = 1 << 20;

/// 单行比一页还长时，最多续读到多长（超过 ⇒ `oversized_line`）。
///
/// monitor 那头单帧上限 64 MiB（`stream_source::BACKEND_FRAME_LINE_CAP`），而字节进 JSON 串
/// 要转义（引号、反斜杠）—— 留一半余量。
pub(crate) const LINE_CAP_BYTES: usize = 32 << 20;

/// 按行那六条整份输出的上限（同上，留一半余量）。
pub(crate) const LINES_CAP_BYTES: usize = 32 << 20;

/// `backend-log` 一次最多回多少字节（尾部）。
pub(crate) const LOG_TAIL_BYTES: u64 = 256 << 10;

/// 本进程的 stderr 此刻落在哪份文件（`main.rs` 装上之后交进来；没装 ⇒ 空）。
static BACKEND_LOG: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// `main.rs` 装上 stderr 诊断文件之后调一次（诊断文件的门在那里，本模块只读它）。
pub fn note_backend_log(path: std::path::PathBuf) {
    let _ = BACKEND_LOG.set(path);
}

/// 那份文件的尾部 ⇒ `{path, size, text, truncated}`；没装 ⇒ `path: null`。截断时从截点后第一个换行起（不给半行）。
pub(crate) fn log_tail(path: Option<&std::path::Path>, max: u64) -> Answer {
    use std::io::{Read, Seek, SeekFrom};
    let Some(path) = path else {
        return Ok(json!({ "path": null, "size": 0, "text": "", "truncated": false }));
    };
    let mut f = std::fs::File::open(path).map_err(|e| ("failed", e.to_string()))?;
    let size = f.metadata().map_err(|e| ("failed", e.to_string()))?.len();
    let from = size.saturating_sub(max);
    let mut buf = Vec::new();
    f.seek(SeekFrom::Start(from))
        .and_then(|_| f.take(max).read_to_end(&mut buf))
        .map_err(|e| ("failed", e.to_string()))?;
    let body = if from > 0 {
        buf.iter()
            .position(|&b| b == b'\n')
            .map_or(&buf[..0], |k| &buf[k + 1..])
    } else {
        &buf[..]
    };
    Ok(json!({
        "path": path.display().to_string(),
        "size": size,
        "text": String::from_utf8_lossy(body),
        "truncated": from > 0,
    }))
}

/// 本族的应答：`data` 或 `(code, message)`。
pub(crate) type Answer = Result<Value, (&'static str, String)>;

/// 帧面入口：命令名从 `r.cmd` 来（与 `files::answer_wire` 同一形 —— 「登记的名字」与
/// 「真正跑的那条」在类型上是同一个值）。
pub(crate) fn answer(cmd: &str, args: &Value, tz: &crate::Tz) -> Answer {
    answer_at(&crate::observe::history_query::agent_home(), cmd, args, tz)
}

/// [`answer`] 的本体，家目录是参数（判据拿夹具喂它，不去动进程级环境变量）。
/// `tz` ＝ 看的那一台的时区（请求信封带来的）：成品里的钟面 · 命中时刻 · 行尾时刻都按它写。
pub(crate) fn answer_at(home: &std::path::Path, cmd: &str, args: &Value, tz: &crate::Tz) -> Answer {
    use crate::observe::{accounts_query, history_query, search_query};
    match cmd {
        // 这台后端自己的 stderr 诊断文件（尾部）—— 机器页「日志」经这台后端的只读面取回来看。
        "backend-log" => {
            let max = u64_arg(args, "maxBytes")?.map_or(LOG_TAIL_BYTES, |n| n.min(LOG_TAIL_BYTES));
            log_tail(BACKEND_LOG.get().map(|p| p.as_path()), max)
        }
        // 一个子运行的记录（按运行读，不认任何一家的目录 / 字段）：父记录 ＋（子运行 ‖ 派出它的那次工具调用）⇒ 那份记录从 `from` 起的一页成品。
        //   `end` ＝ 读到哪了（下次从这里续）；`more` ＝ 这一页没读完。
        "history-run" => {
            let parent = str_arg(args, "parent")?;
            let run = opt_str_arg(args, "run")?;
            let tool = opt_str_arg(args, "tool")?;
            let from = u64_arg(args, "from")?.unwrap_or(0);
            let (source, run) = history_query::run_source(home, parent, run, tool)?;
            let source_str = source.to_string_lossy().into_owned();
            let (target, face) = record_face(home, &source_str)?;
            let page = history_query::read_page(
                home,
                &source_str,
                from,
                None,
                READ_PAGE_BYTES,
                LINE_CAP_BYTES,
            )?;
            let rows = crate::observe::record_page::run_rows(
                &mut reader(&face, &target, from, tz),
                from,
                &page.bytes,
            );
            capped(json!({
                "run": run,
                "path": source_str,
                "rows": rows,
                "end": page.next,
                "more": !page.eof,
            }))
        }
        // 这台后端的漂移账（看不懂的记录类型）出成品：`{faces: [...]}`（各家的面并在一起，注册序）。
        "drift-report" => {
            let faces: Vec<Value> = crate::agents::drift_reports()
                .into_iter()
                .flat_map(|r| r["faces"].as_array().cloned().unwrap_or_default())
                .collect();
            Ok(json!({ "faces": faces }))
        }
        // 各台 `history-search` 的会话行合一份（纯计算，不看家目录）。
        "history-search-merge" => search_query::answer_merge(args, tz),
        // 读不动几份（`unreadable`）· 内容搜索不覆盖、这台上又有会话的那几家（`skipped`，对用户的叫法）一并回给界面说。
        "history-search" => {
            let query = str_arg(args, "query")?;
            let rest = search_rest(args)?;
            // 只比标题与第一句：帧面才有的一格（CLI 面没有这个选项）。
            let titles = args.get("titles").and_then(Value::as_bool) == Some(true);
            let mut unreadable = 0usize;
            let lines = rows(|out| {
                unreadable = search_query::search_into(home, query, &rest, titles, out)
                    .map_err(|e| ("failed", e))?;
                Ok(())
            })?;
            wire(&Searched {
                lines,
                unreadable,
                skipped: crate::agents::content_search_skips(),
            })
        }
        // 停 / 重启 / 更新 / 卸载这台的 cc-monitor 之前会打断什么（`observe/accounts_query.rs::machine_product`）：
        //   这台的活会话经不经本机中转 · 活着的几个 · 这台账上通往 `machine` 的转发。只读。
        "machine-interrupts" => {
            let machine = opt_str_arg(args, "machine")?;
            let lines = accounts_query::session_lines_for_frame(home);
            if args.get("appExit").and_then(Value::as_bool) == Some(true) {
                let kill = crate::control::exit_policy::read_now().kill_on_exit();
                return Ok(accounts_query::app_exit_product(
                    kill,
                    &lines,
                    crate::dial::forwards::running_all(),
                ));
            }
            let forwards = machine.map_or(0, crate::dial::forwards::running_to_machine);
            Ok(accounts_query::machine_product(&lines, forwards))
        }
        "accounts-sessions" => {
            let rows = accounts_query::session_lines_for_frame(home);
            let size: usize = rows.iter().map(|l| l.len() + 1).sum();
            if size > LINES_CAP_BYTES {
                return Err(too_large(size));
            }
            Ok(json!({ "lines": rows }))
        }
        // 账号清单**出成品**（账号域读自己那台的 apikey 表、agent 随请求带）：
        //   monitor 那一份行解析 / 降级说明 / 本机并表（`local_accounts::with_apikey_table`）删了〔散文墓碑〕，界面经通道直接问。
        //   并的是**这台机器自己**那份表（与 `apikey-read` · 中转里的上游选择同一个出处）—— 远端从此第一次并上它自己的表。
        "accounts-list" => {
            let agent = crate::accounts::upstream_select::endpoint::agent_arg(args)?;
            let rows = crate::accounts::upstream_select::file_face::machine_rows();
            let v = accounts_query::list_product(
                &rows,
                agent,
                crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT,
            );
            let size = v.to_string().len();
            if size > LINES_CAP_BYTES {
                return Err(too_large(size));
            }
            Ok(v)
        }
        // 换号前的信任预检上帧面（替掉最后两条仍逐次拨号的子命令）。
        //   `configDir` 缺席 / `null` ⇒ 账号 0；拒绝码原样（CLI 那一臂同一个函数）。
        "accounts-trust" => {
            let cwd = str_arg(args, "cwd")?;
            let config_dir = match args.get("configDir") {
                None | Some(Value::Null) => None,
                Some(Value::String(s)) => Some(s.as_str()),
                Some(_) => {
                    return Err((
                        "bad_args",
                        crate::common::contract::malformed(
                            "`configDir` must be a string or null (null = account 0)",
                        ),
                    ))
                }
            };
            accounts_query::trust_product(config_dir, cwd).map_err(|(c, m)| (trust_code(&c), m))
        }
        // 按字节分页读，出**行摘要**（monitor 旁路快照那一页）：`{rows: [{end, hash, record?, cwd?}], next, eof}`，
        //   每个可计行一条（`observe/record_page.rs::rows_of`）。原先回 `text`、由 monitor 切行解析。
        "history-read" => {
            let path = str_arg(args, "path")?;
            let offset = u64_arg(args, "offset")?.unwrap_or(0);
            let until = u64_arg(args, "until")?;
            let (target, face) = record_face(home, path)?;
            let page = history_query::read_page(
                home,
                path,
                offset,
                until,
                READ_PAGE_BYTES,
                LINE_CAP_BYTES,
            )?;
            Ok(json!({
                "rows": crate::observe::record_page::rows_of(
                    &mut reader(&face, &target, offset, tz),
                    offset,
                    &page.bytes,
                ),
                "next": page.next,
                "eof": page.eof,
            }))
        }
        // 按字节分页读，出**记录行**（界面直接问：查看器整份读 · 骨架按偏移取一段）：
        //   `{lines, next, nextSeq, eof}`；`seq` ＝ `offset` 那一行的行号（缺 ＝ 0）、`nextSeq` 原样交回下一问。
        "history-page" => {
            let path = str_arg(args, "path")?;
            let offset = u64_arg(args, "offset")?.unwrap_or(0);
            let until = u64_arg(args, "until")?;
            let seq = u64_arg(args, "seq")?.unwrap_or(0);
            let (target, face) = record_face(home, path)?;
            let page = history_query::read_page(
                home,
                path,
                offset,
                until,
                READ_PAGE_BYTES,
                LINE_CAP_BYTES,
            )?;
            let (lines, next_seq) = crate::observe::record_page::record_lines_of_page(
                &mut reader(&face, &target, offset, tz),
                &target,
                seq,
                offset,
                &page.bytes,
            );
            Ok(json!({
                "lines": lines,
                "next": page.next,
                "nextSeq": next_seq,
                "eof": page.eof,
            }))
        }
        // 按**行号**取回（不依赖骨架索引的那条取回路，`history_query::read_lines` 头注）。
        // 出**记录行**（原先回可计行原文、由 monitor 解析）：`{from, next, eof, lines}`。
        "history-lines" => {
            let path = str_arg(args, "path")?;
            let from = u64_arg(args, "from")?.unwrap_or(0);
            let until = u64_arg(args, "until")?;
            let (target, face) = record_face(home, path)?;
            let page = history_query::read_lines(
                home,
                path,
                from,
                until,
                READ_PAGE_BYTES,
                LINE_CAP_BYTES,
            )?;
            let first = page.starts.first().copied().unwrap_or(0);
            let (lines, _) = crate::observe::record_page::record_lines(
                &mut reader(&face, &target, first, tz),
                &target,
                page.from,
                page.lines
                    .iter()
                    .map(|l| l.as_bytes())
                    .zip(page.starts.iter().copied()),
            );
            Ok(json!({
                "from": page.from,
                "next": page.next,
                "eof": page.eof,
                "lines": lines,
            }))
        }
        // `frame_query::STILL_DIALED` 缩到只剩真该拨号的那两条：骨架索引与大纲清单
        // 是**每开一个大会话就要一次**的查询，不是「点一次才发一次」。与 CLI 那一臂同一个扫描。
        // 这三条**出成品**（不再是按行 `{"lines":[头, …, 尾]}`）：monitor 那一份「核头尾、剥行」
        //   删了，界面经通道直接问、按形状收。头尾三段只属于 CLI 那一臂（stdout 要分帧才认得出截断）；
        //   一帧应答是原子的 —— 装不下就是 `too_large` 明拒，没有「有头没尾」这一形。
        "history-index" => {
            let path = str_arg(args, "path")?;
            let offset = u64_arg(args, "offset")?.unwrap_or(0);
            let until = u64_arg(args, "until")?;
            let mut rows = CappedRows::default();
            let end = if offset == 0 && until.is_none() {
                let map = history_query::cold_scan(home, path).map_err(|e| ("failed", e))?;
                let pushed = rows.push_all(&map.index);
                rows.finish(pushed)?;
                map.end
            } else {
                let r = history_query::open_session_at(home, path, offset)
                    .map_err(|e| ("failed", e))?;
                let scanned =
                    history_query::scan_session_index(r, offset, until, |row| rows.push(row));
                rows.finish(scanned)?.1
            };
            Ok(json!({ "from": offset, "end": end, "rows": rows.rows }))
        }
        "history-user-inputs" => {
            let path = str_arg(args, "path")?;
            let from = u64_arg(args, "from")?.unwrap_or(0);
            let mut rows = CappedRows::default();
            let end = if from == 0 {
                let map = history_query::cold_scan(home, path).map_err(|e| ("failed", e))?;
                let pushed = rows.push_all(&map.inputs);
                rows.finish(pushed)?;
                map.end
            } else {
                let r = history_query::open_user_inputs_at(home, path, from)
                    .map_err(|e| ("failed", e))?;
                let scanned =
                    crate::observe::user_inputs::scan_user_inputs(r, from, |row| rows.push(row));
                rows.finish(scanned)?.1
            };
            Ok(json!({ "from": from, "end": end, "entries": rows.rows }))
        }
        // 一轮的摘要（B4）：`from` 是某一轮的 `at`（或 0）；还在跑的最后一轮下次从它的 `at` 再取。
        "history-turns" => {
            let path = str_arg(args, "path")?;
            let from = u64_arg(args, "from")?.unwrap_or(0);
            let (mut turns, end) = if from == 0 {
                let map = history_query::cold_scan(home, path).map_err(|e| ("failed", e))?;
                (map.turns.clone(), map.end)
            } else {
                let r = history_query::open_user_inputs_at(home, path, from)
                    .map_err(|e| ("failed", e))?;
                let mut out = Vec::new();
                let (_, end) = crate::observe::turns::scan_turns(r, from, |row| {
                    out.push(row.clone());
                    Ok(())
                })
                .map_err(|e| ("failed", format!("stream failed: {e}")))?;
                (out, end)
            };
            // 钟面按看的那一台的时区写（先于下面那一步：还在跑的那一轮的右端按钟面改写）。
            turns.iter_mut().for_each(|t| t.stamp(tz));
            // 还没收尾的最后一轮：「现在在做哪一步 / 在等你什么」按这台此刻的会话事实拼进过程行（判定同 `history-facts`）。
            if let Some(last) = turns.last_mut().filter(|t| !t.done) {
                let sid = std::path::Path::new(path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                let needs = crate::observe::facts_query::needs_of(
                    &last.pending,
                    accounts_query::session_wait(home, sid).as_ref(),
                );
                let live = !accounts_query::session_writers(home, sid).is_empty();
                crate::observe::turns::dress_live(last, needs.as_ref(), live);
            }
            let mut rows = CappedRows::default();
            let pushed = rows.push_all(&turns);
            rows.finish(pushed)?;
            Ok(json!({ "from": from, "end": end, "turns": rows.rows }))
        }
        // 会话内查找（Ctrl+F，SE2 的 `--find-in-session`）随骨架索引与大纲一起上帧面。
        // `limit` 超封顶按封顶算、缺席取缺省 —— 与 CLI 那一臂的 `parse_find_args` 同一对常量；`skip` ＝ 跳过前几条（续下一页）。
        "history-find" => {
            use crate::observe::search_query::{FindPage, FIND_DEFAULT_LIMIT, FIND_MAX_LIMIT};
            let path = str_arg(args, "path")?;
            let query = str_arg(args, "query")?;
            let include_tools = args
                .get("include_tools")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let limit = u64_arg(args, "limit")?
                .map_or(FIND_DEFAULT_LIMIT, |n| (n as usize).min(FIND_MAX_LIMIT));
            let skip = u64_arg(args, "skip")?.unwrap_or(0) as usize;
            let page = FindPage { skip, limit };
            let target = history_query::session_path_at(home, path).map_err(|e| ("failed", e))?;
            let mut hits = CappedRows::default();
            // 先走 SX1 常驻索引（不再每次从头扫）；不归它管 ⇒ 现扫。
            let scanned =
                match search_query::find_indexed(home, &target, query, include_tools, page, |h| {
                    hits.push(&search_query::with_hit_text(h, tz))
                }) {
                    Some(r) => r,
                    None => {
                        let r = history_query::open_session_at(home, path, 0)
                            .map_err(|e| ("failed", e))?;
                        search_query::scan_session_find(r, query, include_tools, page, |h| {
                            hits.push(&search_query::with_hit_text(h, tz))
                        })
                    }
                };
            let (_, total) = hits.finish(scanned)?;
            Ok(json!({ "total": total, "hits": hits.rows }))
        }
        // 会话事实出成品（分叉血缘 · 改动文件集 · 最新 usage 与上下文上限 · 项目目录 · 此刻在写它的进程 · 没结果的调用 · 最后一句 · 需手动）。
        //   `limits` = 设置里的上限表（模型名子串 → 上限），可缺；上限每次按它重判。
        //   `prior` = 调用方上一次拿到的应答**原样**（续传令牌，后端零状态）：缺席 / `null` ⇒ 从字节 0 扫；
        //   给了 ⇒ 形状必须恰好是本命令出的那一形（`facts_query::prior_from`），从它的 `end` 接着扫、累加在它上面。
        //   续点越过文件尾 / 不在行边界 ⇒ `failed`（调用方从 0 重要一份，同大纲清单）。
        "history-facts" => {
            use crate::observe::facts_query;
            let path = str_arg(args, "path")?;
            let prior = match args.get("prior") {
                None | Some(Value::Null) => None,
                Some(v) => Some(
                    facts_query::prior_from(v)
                        .map_err(|e| ("bad_args", crate::common::contract::malformed(&e)))?,
                ),
            };
            let limits = facts_query::limits_from(args.get("limits"))
                .map_err(|e| ("bad_args", crate::common::contract::malformed(&e)))?;
            // 中转看见过这个会话的请求 ⇒ 它带没带扩展上下文那一项（会话 id ＝ 记录文件名，中转的流标签就是它）。
            let sid = std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let relay = crate::agents::context_marks()
                .into_iter()
                .find_map(|m| crate::observe::relay_marks::seen(sid, m));
            let mut facts = match prior {
                // 冷开（没有续点）⇒ 共用那张扫描图；有续点 ⇒ 从它的 `end` 接着扫、累加在它上面。
                None => history_query::cold_scan(home, path)
                    .map_err(|e| ("failed", e))?
                    .facts(&limits, relay),
                Some(prior) => {
                    let r = history_query::open_facts_at(home, path, prior.end)
                        .map_err(|e| ("failed", e))?;
                    facts_query::scan_facts(r, prior, &limits, relay)
                        .map_err(|e| ("failed", format!("stream failed: {e}")))?
                }
            };
            if facts.project_dir.is_none() {
                facts.project_dir = history_query::facts_project_dir(home, path);
            }
            if facts.agent.is_none() {
                facts.agent = history_query::facts_agent(home, path);
            }
            facts.writers = accounts_query::session_writers(home, sid);
            // 需手动：那台 pidfile 此刻说在等 ⇒ 配上记录里没结果的那一步判种类（不累加，`prior` 里那一份不用）。
            facts.needs = facts_query::needs_of(
                &facts.pending,
                accounts_query::session_wait(home, sid).as_ref(),
            );
            // 后台任务运行中：那台 pidfile 此刻说是这一态 ⇒ 配上记录里还没收场的后台命令写那一句（不累加，`prior` 里那一份不用）。
            facts.background = accounts_query::session_background(home, sid).map(|born| {
                let now = crate::common::time::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
                facts_query::background_of(&facts.bg_tasks, born, now)
            });
            // 每一步还没结果时的样子：在等你 · 在跑 · 状态不明（界面只读这一格）。这一家不留 pidfile ⇒ 判不了活。
            let tracked = facts
                .agent
                .as_deref()
                .and_then(|k| crate::agents::launch_face_among(crate::agents::REGISTRY, k))
                .is_some_and(|f| f.has_pidfiles);
            facts_query::settle_pending(&mut facts, tracked);
            let v = serde_json::to_value(&facts).map_err(|e| ("failed", e.to_string()))?;
            let size = v.to_string().len();
            if size > LINES_CAP_BYTES {
                return Err(too_large(size));
            }
            Ok(v)
        }
        // 这台上需手动的会话清单（一次问一台）：活着、那台说在等人的每一个，带它在等什么（同 `history-facts.needs`，不另判）。
        "sessions-needs" => crate::stream::inbound::spec::wire(&NeedsList {
            waiting: accounts_query::live_needs(home),
        }),
        // 主线外清单（回退掉的那几条）：冷读一次（实时那一路是帧 `session_branch`）。
        "history-branch" => {
            let path = str_arg(args, "path")?;
            let map = history_query::cold_scan(home, path).map_err(|e| ("failed", e))?;
            capped(wire(&Branch {
                off: map.off.clone(),
                end: map.end,
            })?)
        }
        "history-tail" => {
            let path = str_arg(args, "path")?;
            let n = u64_arg(args, "n")?.ok_or_else(|| {
                (
                    "bad_args",
                    crate::common::contract::malformed("missing `n`"),
                )
            })?;
            let plan =
                history_query::tail_now(home, path, n as usize).map_err(|e| ("failed", e))?;
            wire(&plan)
        }
        // 这条会话的记录还在不在（resume 之前问；本体住 `history_query::record_in`）。
        // 可选 `configDir`：这次 resume 要用的那个账号根（缺席 / `null` ⇒ 这台的家目录）。
        "history-record" => {
            let sid = str_arg(args, "sid")?;
            let config_dir = match args.get("configDir") {
                None | Some(Value::Null) => None,
                Some(Value::String(s)) => Some(s.as_str()),
                Some(_) => {
                    return Err((
                        "bad_args",
                        crate::common::contract::malformed(
                            "`configDir` must be a string or null (null = this machine's home)",
                        ),
                    ))
                }
            };
            let probe =
                history_query::record_for(home, config_dir, sid).map_err(|e| ("bad_args", e))?;
            wire(&probe)
        }
        other => Err((
            "bad_args",
            crate::common::contract::malformed(&format!("this face has no command `{other}`")),
        )),
    }
}

/// 过围栏 ⇒ 这份记录是哪一家的（注册表按它落在谁的根下认），解释交那一家。
fn record_face(
    home: &std::path::Path,
    path: &str,
) -> Result<(std::path::PathBuf, crate::agents::RecordFace), (&'static str, String)> {
    let target =
        crate::observe::history_query::session_path_at(home, path).map_err(|e| ("refused", e))?;
    let face = crate::agents::record_face_of(&target).ok_or_else(|| {
        (
            "failed",
            crate::common::contract::malformed("no adapter reads this record"),
        )
    })?;
    Ok((target, face))
}

/// 从 `offset` 起读那一份记录的读法：起点之前那一段一起喂进去配排队消息的打字时刻（`record_page::lead_of`）。
fn reader<'a>(
    face: &'a crate::agents::RecordFace,
    target: &std::path::Path,
    offset: u64,
    tz: &crate::Tz,
) -> crate::observe::record_page::Reader<'a> {
    let (lead_at, lead) = crate::observe::record_page::lead_of(target, offset);
    crate::observe::record_page::Reader::new(face, lead_at, &lead, tz.clone())
}

/// 整份成品过 [`LINES_CAP_BYTES`] ⇒ `too_large`（不截断）。
fn capped(v: Value) -> Answer {
    let size = v.to_string().len();
    if size > LINES_CAP_BYTES {
        return Err(too_large(size));
    }
    Ok(v)
}

/// 跑一个「往 `out` 里逐行写」的查询，收成非空的那几行。
fn rows(
    f: impl FnOnce(&mut CappedBuf) -> Result<(), (&'static str, String)>,
) -> Result<Vec<String>, (&'static str, String)> {
    let mut out = CappedBuf::default();
    let res = f(&mut out);
    if out.over {
        return Err(too_large(out.seen));
    }
    res?;
    Ok(String::from_utf8_lossy(&out.buf)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

/// `history-search` 的应答。
#[derive(Debug, serde::Serialize)]
pub(crate) struct Searched {
    /// 每命中会话一行 `SessionHits`（JSON 串），形状与行序同 `--search`。
    pub(crate) lines: Vec<String>,
    /// 这一趟有几份会话记录读不动、没搜到。
    pub(crate) unreadable: usize,
    /// 内容搜索不覆盖、这台上又有它的会话记录的那几家。
    pub(crate) skipped: Vec<&'static str>,
}

/// `sessions-needs` 的应答：这台上需手动的会话，先答的在前。
#[derive(Debug, serde::Serialize)]
pub(crate) struct NeedsList {
    pub(crate) waiting: Vec<crate::observe::accounts_query::NeedsRow>,
}

/// `history-branch` 的应答。
#[derive(Debug, serde::Serialize)]
pub(crate) struct Branch {
    /// 回退掉的那几条记录的 `id`（文件序）。
    pub(crate) off: Vec<String>,
    /// 最后一个完整行的末字节。
    pub(crate) end: u64,
}

/// `accounts-trust` 的拒绝码（CLI 那一臂给的是 `String`）→ 帧面的 `&'static str`。**闭集，与 `stream/inbound/registry/accounts.rs`
/// 那一块的 `codes` 同一张**；认不出的一律 `failed`（不把一个没登记的码漏上线）。
fn trust_code(c: &str) -> &'static str {
    // ⚠ agent 那一层自己的码（读 / 解析那家的配置文件失败）**不在表里**：通用层不认任何一家 agent 的名字
    //   （`agent_boundary_guard`）⇒ 它们落 `failed`，原因那句原样带着。
    const KNOWN: &[&str] = &[
        "unsafe_config_dir",
        "unknown_config_dir",
        "manifest_unavailable",
        "no_home",
    ];
    KNOWN.iter().find(|k| **k == c).copied().unwrap_or("failed")
}

fn too_large(size: usize) -> (&'static str, String) {
    (
        "too_large",
        copy_text(
            "beReadFace.tooLarge.say",
            &[
                ("size", &size.to_string()),
                ("cap", &LINES_CAP_BYTES.to_string()),
            ],
        ),
    )
}

/// 出成品那三条的收集器：逐条收成 JSON 值，整份（按序列化字节计）过 [`LINES_CAP_BYTES`]
/// ⇒ 当场停下扫描、回 `too_large`（不截断：截断的清单会被当成完整的用）—— 与 [`CappedBuf`] 同一条纪律。
#[derive(Default)]
struct CappedRows {
    rows: Vec<Value>,
    seen: usize,
    over: bool,
}

impl CappedRows {
    fn push<T: serde::Serialize>(&mut self, row: &T) -> std::io::Result<()> {
        let v = serde_json::to_value(row).map_err(std::io::Error::other)?;
        self.seen += v.to_string().len() + 1;
        if self.seen > LINES_CAP_BYTES {
            self.over = true;
            return Err(std::io::Error::other("reply cap reached"));
        }
        self.rows.push(v);
        Ok(())
    }

    /// 一整张表逐条推（冷开那几问从扫描图出）；超了上限就停。
    fn push_all<T: serde::Serialize>(&mut self, rows: &[T]) -> std::io::Result<()> {
        rows.iter().try_for_each(|r| self.push(r))
    }

    /// 扫描的结局 ⇒ 帧面的结局。超了上限的那一次停下，原因是「超了」，不是扫描函数报的那句 I/O 错。
    fn finish<T>(&self, res: std::io::Result<T>) -> Result<T, (&'static str, String)> {
        if self.over {
            return Err(too_large(self.seen));
        }
        res.map_err(|e| ("failed", format!("stream failed: {e}")))
    }
}

/// 有上限的内存出口。超了就报错（让查询函数停下），并记下「超了」—— 调用方据此回 `too_large`，
/// 而不是把查询函数报的那句「write failed」当成真原因。
#[derive(Default)]
struct CappedBuf {
    buf: Vec<u8>,
    seen: usize,
    over: bool,
}

impl std::io::Write for CappedBuf {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.seen += data.len();
        if self.buf.len() + data.len() > LINES_CAP_BYTES {
            self.over = true;
            return Err(std::io::Error::other("reply cap reached"));
        }
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, (&'static str, String)> {
    args.get(key).and_then(Value::as_str).ok_or_else(|| {
        (
            "bad_args",
            crate::common::contract::malformed(&format!("missing `{key}` (a string)")),
        )
    })
}

/// 可缺的串参数：缺 / `null` ⇒ `None`；在而不是串 ⇒ `bad_args`。
fn opt_str_arg<'a>(args: &'a Value, key: &str) -> Result<Option<&'a str>, (&'static str, String)> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(v)) => Ok(Some(v)),
        Some(_) => Err((
            "bad_args",
            crate::common::contract::malformed(&format!("{key} must be a string")),
        )),
    }
}

fn u64_arg(args: &Value, key: &str) -> Result<Option<u64>, (&'static str, String)> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_u64().map(Some).ok_or_else(|| {
            (
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "`{key}` must be a non-negative integer"
                )),
            )
        }),
    }
}

/// `history-search` 的选项 → `--search <query>` 之后那一截 argv。
///
/// 🔴 **不在这里另写一份选项语义**：解析走 CLI 那一臂同一个 `parse_opts`，
/// 本函数只把 JSON 摊回它认的那几个 token（`include_tools` / `scope` / `after_ms` / `limit`）。
fn search_rest(args: &Value) -> Result<Vec<String>, (&'static str, String)> {
    let mut rest = Vec::new();
    if args.get("include_tools").and_then(Value::as_bool) == Some(true) {
        rest.push("--include-tools".to_string());
    }
    if let Some(s) = args.get("scope").and_then(Value::as_str) {
        rest.push("--scope".to_string());
        rest.push(s.to_string());
    }
    if let Some(n) = u64_arg(args, "after_ms")? {
        rest.push("--after-ms".to_string());
        rest.push(n.to_string());
    }
    if let Some(n) = u64_arg(args, "limit")? {
        rest.push("--limit".to_string());
        rest.push(n.to_string());
    }
    Ok(rest)
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/read_face_tests.rs"]
mod tests;
