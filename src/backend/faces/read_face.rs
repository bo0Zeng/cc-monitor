//! 〔`C1` · 2026-09-24〕**只读查询的帧面宿主** —— `设计/15 §3.2` 层 1 那一搬。
//!
//! # 它补的是哪一格
//!
//! `设计/99 §4.19.2 ⑥` 逐字：「帧面 16 条，账号与 8 条只读查询都不在 ⇒ 每点一下拨一次 SSH、
//! 每 10 秒对每台机器握一次手」。那 8 条今天全是**一次性子命令**：monitor 每问一次就
//! 新开一条 TCP+SSH+鉴权，exec 一次本二进制，读完 stdout 就断。
//! 而同一台机器上**已经有一条**长连接（流模式那条），入方向早就能一问一答
//! （`inbound.rs`）—— 缺的只是这 8 条没登记上去。
//!
//! # 为什么住顶层，而不是 `observe/`
//!
//! 与 `files/` 同一个理由：`inbound.rs` 不许出现 `observe::`
//! （`inbound_structure_guards::inbound_never_reaches_into_the_observe_layer`），
//! 而查询的本体就住 `observe/`。⇒ 本文件是那道线**之外**的一层薄宿主：
//! 解帧面的 `args`、把输出装进应答、给错误一个 code。**查询本身一行不在这里** ——
//! 每条都调 CLI 那一臂**同一个函数**（只是把 `out` 从 stdout 换成内存），
//! 「一份代码两种宿主」（`设计/60 §8.1`）在这一族上的样子。
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

use copy_core::copy_text;
use serde_json::{json, Value};

/// `history-read` 一页的目标字节数。
///
/// 取 1 MiB：一页就是一帧应答，它在后端写出方向上与实时 `line` 帧**排同一条队** ——
/// 页越大，实时行被挡住的时间越长；页越小，往返越多。1 MiB 在 1 Mbps 的弱网上约 8 秒一页。
pub(crate) const READ_PAGE_BYTES: usize = 1 << 20;

/// 单行比一页还长时，最多续读到多长（超过 ⇒ `oversized_line`）。
///
/// monitor 那头单帧上限 64 MiB（`ssh_source::BACKEND_FRAME_LINE_CAP`），而字节进 JSON 串
/// 要转义（引号、反斜杠）—— 留一半余量。
pub(crate) const LINE_CAP_BYTES: usize = 32 << 20;

/// 〔MOD〕「整份读进查看器」那一件读到多少字节就明拒（原 monitor `history·rs` 那个 `MAX_SESSION_BYTES`〔散文墓碑〕，F06）。
///
/// ⚠〔audit-0805 F06〕实测本机最大会话 **270,103,105 字节**，已经越过这条线 ⇒ 上限会被真实数据打到，
/// 打到之后不能是静默：`history-page` 带 `whole` 时读过它就回 `too_large`，那句话说清读到了哪。
pub(crate) const WHOLE_SESSION_MAX_BYTES: u64 = 256 * 1024 * 1024;

/// 按行那六条整份输出的上限（同上，留一半余量）。
pub(crate) const LINES_CAP_BYTES: usize = 32 << 20;

/// 〔GAP1〕`backend-log` 一次最多回多少字节（尾部）。
pub(crate) const LOG_TAIL_BYTES: u64 = 256 << 10;

/// 〔GAP1 · `设计/15 §4.7 S1`〕本进程的 stderr 此刻落在哪份文件（`main.rs` 装上之后交进来；没装 ⇒ 空）。
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
pub(crate) fn answer(cmd: &str, args: &Value) -> Answer {
    answer_at(&crate::observe::history_query::agent_home(), cmd, args)
}

/// [`answer`] 的本体，家目录是参数（判据拿夹具喂它，不去动进程级环境变量）。
pub(crate) fn answer_at(home: &std::path::Path, cmd: &str, args: &Value) -> Answer {
    use crate::observe::{accounts_query, history_query, search_query};
    match cmd {
        // 〔GAP1 · `设计/15 §4.7 S1`〕这台后端自己的 stderr 诊断文件（尾部）—— 机器页「日志」经这台后端的只读面取回来看。
        "backend-log" => {
            let max = u64_arg(args, "maxBytes")?.map_or(LOG_TAIL_BYTES, |n| n.min(LOG_TAIL_BYTES));
            log_tail(BACKEND_LOG.get().map(|p| p.as_path()), max)
        }
        // 〔C4d · 第四波 4B〕`history-projects` / `history-sessions` 两臂搬走了：它们从此出成品（并注解 ＋ 判活 ＋ 远端那一跳），
        //   住 `history_join.rs`（历史跨机 join 的唯一的家）；这里只剩按行 / 按页的换壳。
        // 〔MOD · `05 §14.3` C 组〕子 agent 那一份出成品：列候选 ＋ 挑（description 精确串等 ＋ 时间戳最近）＋ 读 ＋ 解析都在这台后端，
        //   界面经通道直接问（原先 monitor `subagent.rs` 列了再挑、再读、再解析）。挑的规则只此一份（`history_query::pick_subagent`）。
        "history-subagent" => {
            let parent = str_arg(args, "parent")?;
            let description = str_arg(args, "description")?;
            let timestamp = str_arg(args, "timestamp")?;
            let mut listing = CappedBuf::default();
            let listed = history_query::list_subagents_into(home, parent, &mut listing);
            if listing.over {
                return Err(too_large(listing.seen));
            }
            listed?;
            let text = String::from_utf8_lossy(&listing.buf);
            let rows: Vec<&str> = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect();
            let Some(picked) = history_query::pick_subagent(&rows, description, timestamp) else {
                return Err((
                    "not_found",
                    copy_text(
                        "rsSubagent.load.notFound",
                        &[("description", &format!("{description:?}"))],
                    ),
                ));
            };
            let picked_str = picked.to_string_lossy().into_owned();
            let (target, face) = record_face(home, &picked_str)?;
            let bytes =
                std::fs::read(&target).map_err(|e| ("failed", format!("read failed: {e}")))?;
            let records = crate::observe::record_page::all_records(&face, &bytes);
            let v = json!({
                "path": picked_str,
                "agent_id": picked.file_stem().and_then(|s| s.to_str()).and_then(|s| s.strip_prefix("agent-")).unwrap_or(""),
                "records": records,
            });
            capped(v)
        }
        // 〔MOD〕这台后端的漂移账（看不懂的记录类型）出成品：`{faces: [...]}`（各家的面并在一起，注册序）。
        "drift-report" => {
            let faces: Vec<Value> = crate::agents::drift_reports()
                .into_iter()
                .flat_map(|r| r["faces"].as_array().cloned().unwrap_or_default())
                .collect();
            Ok(json!({ "faces": faces }))
        }
        "history-search" => {
            let query = str_arg(args, "query")?;
            let rest = search_rest(args)?;
            lines(|out| {
                search_query::search_into(home, query, &rest, out).map_err(|e| ("failed", e))
            })
        }
        "accounts-sessions" => {
            let rows =
                accounts_query::lines_for_frame(home, accounts_query::FrameAccounts::BySession);
            let size: usize = rows.iter().map(|l| l.len() + 1).sum();
            if size > LINES_CAP_BYTES {
                return Err(too_large(size));
            }
            Ok(json!({ "lines": rows }))
        }
        // 〔C4c · 第四波 4B〕账号清单**出成品**（主会话裁：账号域读自己那台的 apikey 表、agent 随请求带）：
        //   monitor 那一份行解析 / 降级说明 / 本机并表（`local_accounts::with_apikey_table`）删了〔散文墓碑〕，界面经通道直接问。
        //   并的是**这台机器自己**那份表（与 `apikey-read` · 中转里的上游选择同一个出处）—— 远端从此第一次并上它自己的表。
        "accounts-list" => {
            let agent = str_arg(args, "agent")?;
            let rows = crate::accounts::upstream::file_face::rows_at(
                &crate::accounts::upstream::file_face::machine_path(),
            );
            let v = accounts_query::list_product(
                &rows,
                agent,
                crate::accounts::upstream::CREDENTIALS_FILE_AGENT,
            );
            let size = v.to_string().len();
            if size > LINES_CAP_BYTES {
                return Err(too_large(size));
            }
            Ok(v)
        }
        // 〔C4c · 第四波 4B〕换号前的信任预检上帧面（替掉最后两条仍逐次拨号的子命令）。
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
        // 〔MOD〕按字节分页读，出**行摘要**（monitor 旁路快照那一页）：`{rows: [{end, hash, message?, cwd?}], next, eof}`，
        //   每个可计行一条（`observe/record_page.rs::rows_of`）。原先回 `text`、由 monitor 切行解析。
        "history-read" => {
            let path = str_arg(args, "path")?;
            let offset = u64_arg(args, "offset")?.unwrap_or(0);
            let until = u64_arg(args, "until")?;
            let (_, face) = record_face(home, path)?;
            let page = history_query::read_page(
                home,
                path,
                offset,
                until,
                READ_PAGE_BYTES,
                LINE_CAP_BYTES,
            )?;
            Ok(json!({
                "rows": crate::observe::record_page::rows_of(&face, offset, &page.bytes),
                "next": page.next,
                "eof": page.eof,
            }))
        }
        // 〔MOD · `05 §14.3` C 组〕按字节分页读，出**记录行**（界面直接问：查看器整份读 · 骨架按偏移取一段）：
        //   `{lines, next, nextSeq, eof}`；`seq` ＝ `offset` 那一行的行号（缺 ＝ 0）、`nextSeq` 原样交回下一问。
        //   `whole` ＝ 这是「整份读进查看器」那一件：读过 [`WHOLE_SESSION_MAX_BYTES`] 就明拒（不许静默截断，F06）。
        "history-page" => {
            let path = str_arg(args, "path")?;
            let offset = u64_arg(args, "offset")?.unwrap_or(0);
            let until = u64_arg(args, "until")?;
            let seq = u64_arg(args, "seq")?.unwrap_or(0);
            let whole = args.get("whole").and_then(Value::as_bool).unwrap_or(false);
            let (target, face) = record_face(home, path)?;
            let page = history_query::read_page(
                home,
                path,
                offset,
                until,
                READ_PAGE_BYTES,
                LINE_CAP_BYTES,
            )?;
            if whole && page.next > WHOLE_SESSION_MAX_BYTES {
                return Err((
                    "too_large",
                    copy_text(
                        "rsHistory.session.truncated",
                        &[
                            ("max", &WHOLE_SESSION_MAX_BYTES.to_string()),
                            ("read", &page.next.to_string()),
                            ("lines", &seq.to_string()),
                        ],
                    ),
                ));
            }
            let (lines, next_seq) =
                crate::observe::record_page::record_lines_of_page(&face, &target, seq, &page.bytes);
            Ok(json!({
                "lines": lines,
                "next": page.next,
                "nextSeq": next_seq,
                "eof": page.eof,
            }))
        }
        // 〔CF2 · 第四波 4B〕按**行号**取回（不依赖骨架索引的那条取回路，`history_query::read_lines` 头注）。
        // 〔MOD〕出**记录行**（原先回可计行原文、由 monitor 解析）：`{from, next, eof, lines}`。
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
            let (lines, _) = crate::observe::record_page::record_lines(
                &face,
                &target,
                page.from,
                page.lines.iter().map(|l| l.as_bytes()),
            );
            Ok(json!({
                "from": page.from,
                "next": page.next,
                "eof": page.eof,
                "lines": lines,
            }))
        }
        // 〔SR1a · 2026-09-24〕`frame_query::STILL_DIALED` 缩到只剩真该拨号的那两条：骨架索引与大纲清单
        // 是**每开一个大会话就要一次**的查询，不是「点一次才发一次」。与 CLI 那一臂同一个扫描。
        // 〔C4b · 第四波 4B〕这三条**出成品**（不再是按行 `{"lines":[头, …, 尾]}`）：monitor 那一份「核头尾、剥行」
        //   删了，界面经通道直接问、按形状收。头尾三段只属于 CLI 那一臂（stdout 要分帧才认得出截断）；
        //   一帧应答是原子的 —— 装不下就是 `too_large` 明拒，没有「有头没尾」这一形。
        "history-index" => {
            let path = str_arg(args, "path")?;
            let offset = u64_arg(args, "offset")?.unwrap_or(0);
            let until = u64_arg(args, "until")?;
            let r =
                history_query::open_session_at(home, path, offset).map_err(|e| ("failed", e))?;
            let mut rows = CappedRows::default();
            let scanned = history_query::scan_session_index(r, offset, until, |row| rows.push(row));
            let (_, end) = rows.finish(scanned)?;
            Ok(json!({ "from": offset, "end": end, "rows": rows.rows }))
        }
        "history-user-inputs" => {
            let path = str_arg(args, "path")?;
            let from = u64_arg(args, "from")?.unwrap_or(0);
            let r =
                history_query::open_user_inputs_at(home, path, from).map_err(|e| ("failed", e))?;
            let mut rows = CappedRows::default();
            let scanned =
                crate::observe::user_inputs::scan_user_inputs(r, from, |row| rows.push(row));
            let (_, end) = rows.finish(scanned)?;
            Ok(json!({ "from": from, "end": end, "entries": rows.rows }))
        }
        // 〔SR1a × SE2 · 09-24〕会话内查找（Ctrl+F，SE2 的 `--find-in-session`）随骨架索引与大纲一起上帧面。
        // `limit` 超封顶按封顶算、缺席取缺省 —— 与 CLI 那一臂的 `parse_find_args` 同一对常量。
        "history-find" => {
            use crate::observe::search_query::{FIND_DEFAULT_LIMIT, FIND_MAX_LIMIT};
            let path = str_arg(args, "path")?;
            let query = str_arg(args, "query")?;
            let include_tools = args
                .get("include_tools")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let limit = u64_arg(args, "limit")?
                .map_or(FIND_DEFAULT_LIMIT, |n| (n as usize).min(FIND_MAX_LIMIT));
            let target = history_query::session_path_at(home, path).map_err(|e| ("failed", e))?;
            let mut hits = CappedRows::default();
            // 〔GAP1 · `设计/10 §7` 第 10 条〕先走 SX1 常驻索引（不再每次从头扫）；不归它管 ⇒ 现扫。
            let scanned =
                match search_query::find_indexed(home, &target, query, include_tools, limit, |h| {
                    hits.push(h)
                }) {
                    Some(r) => r,
                    None => {
                        let r = history_query::open_session_at(home, path, 0)
                            .map_err(|e| ("failed", e))?;
                        search_query::scan_session_find(r, query, include_tools, limit, |h| {
                            hits.push(h)
                        })
                    }
                };
            let (_, total) = hits.finish(scanned)?;
            Ok(json!({ "total": total, "hits": hits.rows }))
        }
        // 〔STC · `设计/90 §4` 阶段 C〕会话事实出成品（分叉血缘 · 改动文件集 · agent 列表 · 最新 usage）。
        //   `prior` = 调用方上一次拿到的应答**原样**（续传令牌，后端零状态）：缺席 / `null` ⇒ 从字节 0 扫；
        //   给了 ⇒ 形状必须恰好是本命令出的那一形（`facts_query::prior_from`），从它的 `end` 接着扫、累加在它上面。
        //   续点越过文件尾 / 不在行边界 ⇒ `failed`（调用方从 0 重要一份，同大纲清单）。
        "history-facts" => {
            use crate::observe::facts_query;
            let path = str_arg(args, "path")?;
            let prior = match args.get("prior") {
                None | Some(Value::Null) => facts_query::SessionFacts::default(),
                Some(v) => facts_query::prior_from(v)
                    .map_err(|e| ("bad_args", crate::common::contract::malformed(&e)))?,
            };
            let r =
                history_query::open_facts_at(home, path, prior.end).map_err(|e| ("failed", e))?;
            let facts = facts_query::scan_facts(r, prior)
                .map_err(|e| ("failed", format!("stream failed: {e}")))?;
            let v = serde_json::to_value(&facts).map_err(|e| ("failed", e.to_string()))?;
            let size = v.to_string().len();
            if size > LINES_CAP_BYTES {
                return Err(too_large(size));
            }
            Ok(v)
        }
        "history-tail" => {
            let path = str_arg(args, "path")?;
            let n = u64_arg(args, "n")?.ok_or((
                "bad_args",
                crate::common::contract::malformed("missing `n`"),
            ))?;
            let (plan, _) =
                history_query::tail_plan(home, path, n as usize).map_err(|e| ("failed", e))?;
            Ok(json!({
                "total": plan.total,
                "tail_from": plan.tail_from,
                "split_at": plan.split_at,
                "end": plan.end,
            }))
        }
        // 〔U4b · 第四波〕这条会话的记录还在不在（resume 之前问；本体住 `history_query::record_in`）。
        // 〔GP1 · 第四波〕可选 `configDir`：这次 resume 要用的那个账号根（缺席 / `null` ⇒ 这台的家目录）。
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
            Ok(json!({ "present": probe.present, "root": probe.root }))
        }
        other => Err((
            "bad_args",
            crate::common::contract::malformed(&format!("this face has no command `{other}`")),
        )),
    }
}

/// 〔MOD〕过围栏 ⇒ 这份记录是哪一家的（注册表按它落在谁的根下认），解释交那一家。
fn record_face(
    home: &std::path::Path,
    path: &str,
) -> Result<(std::path::PathBuf, crate::agents::RecordFace), (&'static str, String)> {
    let target =
        crate::observe::history_query::session_path_at(home, path).map_err(|e| ("refused", e))?;
    let face = crate::agents::record_face_of(&target).ok_or((
        "failed",
        crate::common::contract::malformed("no adapter reads this record"),
    ))?;
    Ok((target, face))
}

/// 整份成品过 [`LINES_CAP_BYTES`] ⇒ `too_large`（不截断）。
fn capped(v: Value) -> Answer {
    let size = v.to_string().len();
    if size > LINES_CAP_BYTES {
        return Err(too_large(size));
    }
    Ok(v)
}

/// 跑一个「往 `out` 里逐行写」的查询，收成 `{"lines": [...]}`。
fn lines(f: impl FnOnce(&mut CappedBuf) -> Result<(), (&'static str, String)>) -> Answer {
    let mut out = CappedBuf::default();
    let res = f(&mut out);
    if out.over {
        return Err(too_large(out.seen));
    }
    res?;
    let text = String::from_utf8_lossy(&out.buf);
    let rows: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    Ok(json!({ "lines": rows }))
}

/// `accounts-trust` 的拒绝码（CLI 那一臂给的是 `String`）→ 帧面的 `&'static str`。**闭集，与 `inbound.rs`
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

/// 〔C4b · 第四波 4B〕出成品那三条的收集器：逐条收成 JSON 值，整份（按序列化字节计）过 [`LINES_CAP_BYTES`]
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
    args.get(key).and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed(&format!("missing `{key}` (a string)")),
    ))
}

fn u64_arg(args: &Value, key: &str) -> Result<Option<u64>, (&'static str, String)> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_u64().map(Some).ok_or((
            "bad_args",
            crate::common::contract::malformed(&format!("`{key}` must be a non-negative integer")),
        )),
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
