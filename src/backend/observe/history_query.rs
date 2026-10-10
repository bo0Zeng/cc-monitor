//! issue #16 P1a：一次性历史查询模式。
//!
//! cc-monitor 通过**独立 SSH 连接**一次性 exec 本二进制并带参数：
//!
//! - `--list-projects`                → 列举 `<claude_dir>/projects/` 下各项目
//! - `--list-sessions <project_dir>`  → 列举某项目目录下的历史会话（带元数据）
//! - `--read-session <jsonl_path>`    → 原样透传该 jsonl 文件内容（monitor 侧解析）
//! - `--read-session-from-offset <jsonl_path> <offset>` → 从字节 `offset`（0-based）透传
//!   [offset, EOF]，= aterm `tail -c +(offset+1)`（offset 续拉/重连恢复）
//!
//! 输出协议：`--list-*` 每行一个 JSON 对象（**不是** wire::Frame——查询模式与流式
//! 协议互不混用，旧后端不认参数会照常进流模式发 hello，monitor 以"首行是
//! hello 帧"识别旧版并优雅降级）；`--read-session` 输出原始文件字节。
//! 错误：stderr 写原因 + 退出码 2。成功退出码 0。
//!
//! 安全：所有路径参数严格限制在 `<claude_dir>/projects/` 之内（canonicalize 后
//! 前缀校验，防 `../` 穿越）；project_dir 参数不允许含路径分隔符。
//! 只读铁律（cc-monitor 不写远端）在此同样成立：本模块只 read_dir / read。

use crate::observe::fence::Fence;
use crate::observe::fs::{mtime_ms, split_whole_lines, Step, Tailed};
use crate::observe::listing_scan::SessionScan;
use copy_core::copy_text;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// 一条会话的状态：`live` 在跑 · `ended` 已结束 · `unknown` 这条路上答不了（合成历史没有 pidfile）。
/// 历史清单的行与全文搜索的命中同一个函数。
pub(crate) fn status_of(is_live: Option<bool>) -> &'static str {
    match is_live {
        Some(true) => "live",
        Some(false) => "ended",
        None => "unknown",
    }
}

/// **这条会话能做什么**（`can`）——界面按它画，不另判：
/// - `resume`：`yes` 能恢复 · `switch` 在跑（切过去，不再起第二份）· `bg` 分身会话（要接着聊得恢复主会话）；
/// - `accounts`：恢复时能不能选号（那一家有没有账号这一维）· `fork`：能不能从某一轮分叉；
/// - `delete`：`yes` · `live` 在跑（先结束它）· `unsure` 说不清在不在跑（确认框里多说一句）。
pub(crate) fn can_of(kind: &str, status: &str, bg: bool) -> serde_json::Value {
    let resume = if status == "live" {
        "switch"
    } else if bg {
        "bg"
    } else {
        "yes"
    };
    let delete = match status {
        "live" => "live",
        "unknown" => "unsure",
        _ => "yes",
    };
    serde_json::json!({
        "resume": resume,
        "accounts": crate::agents::account_env_of(kind).is_some(),
        "fork": crate::agents::record_face(kind).is_some_and(|r| r.branch.is_some()) && !bg,
        "delete": delete,
    })
}

/// 查询模式入口。返回进程退出码。
pub fn run(agent_home: &Path, args: &[String]) -> i32 {
    let result = match args.first().map(String::as_str) {
        Some("--list-projects") => {
            return match list_projects_to(agent_home, &mut std::io::stdout().lock()) {
                Ok(()) => 0,
                Err((Some(code), said)) => coded_failure(code, &said),
                Err((None, e)) => query_failed(&e),
            }
        }
        Some("--list-sessions") => match args.get(1) {
            Some(dir) => list_sessions(agent_home, dir),
            None => Err("--list-sessions requires <project_dir> argument".into()),
        },
        Some("--read-session-tail") => match (args.get(1), args.get(2)) {
            (Some(p), Some(n)) => match n.parse::<usize>() {
                Ok(n) => read_session_tail(agent_home, p, n),
                Err(_) => Err("--read-session-tail <jsonl_path> <N>: N must be a number".into()),
            },
            _ => Err("--read-session-tail requires <jsonl_path> <N> arguments".into()),
        },
        Some("--read-session") => match args.get(1) {
            Some(p) => read_session(agent_home, p),
            None => Err("--read-session requires <jsonl_path> argument".into()),
        },
        Some("--read-session-from-offset") => match parse_from_offset_args(&args[1..]) {
            Ok((opts, pos)) => match (pos.first(), pos.get(1)) {
                (Some(p), Some(o)) => match o.parse::<u64>() {
                    Ok(o) if opts.index => session_index(agent_home, p, o, opts.until),
                    Ok(o) => read_session_from_offset(agent_home, p, o, opts.until),
                    Err(_) => Err(
                        "--read-session-from-offset <jsonl_path> <offset>: offset must be a number"
                            .into(),
                    ),
                },
                _ => Err(
                    "--read-session-from-offset requires <jsonl_path> <offset> arguments".into(),
                ),
            },
            Err(e) => Err(e),
        },
        // 大纲的数据源：「你说过的话」清单（判定住 `observe::user_inputs`）。
        Some("--list-user-inputs") => match parse_user_inputs_args(&args[1..]) {
            Ok((from, p)) => list_user_inputs(agent_home, p, from),
            Err(e) => Err(e),
        },
        // 会话内查找（口径与 `--search` 同一份，内核住 `observe::search_query`）。
        Some("--find-in-session") => match parse_find_args(&args[1..]) {
            Ok(a) => find_in_session(agent_home, &a),
            Err(e) => Err(e),
        },
        Some(other) => Err(format!("unknown argument: {other}")),
        None => Err("no query argument".into()),
    };
    match result {
        Ok(()) => 0,
        Err(e) => query_failed(&e),
    }
}

/// `--list-projects`：每个项目目录一行 JSON：
/// `{"dirName","projectPath","sessionCount","lastActivityMs"}`
/// projectPath = 该项目**最新** jsonl 的项目目录（适配层 `RecordFace.project_dir`：真实目录，
/// 而非编码过的目录名）；读不到则空字符串，monitor 侧回退显示 dirName。
///
/// 老 CLI 那一面照旧**出声**（rc=2）：它是远端 / 一次性问者的契约，零行会被读成「这家没有会话」
/// （`agents/fake` 那条 S6-Z3 钉着）；但那一形带上结构化的码 [`NO_RECORD_TREE`]（`{code, message}` 信封），
/// 问的那台后端认码 ⇒ 画「这台还没有会话记录」而不是「没加载上」。其余失败无码（`Err((None, 原因))`）。
pub(crate) fn list_projects_to(
    agent_home: &Path,
    out: &mut dyn Write,
) -> Result<(), (Option<&'static str>, String)> {
    if list_projects_into(agent_home, out).map_err(|e| (None, e))? {
        return Ok(());
    }
    Err((
        Some(NO_RECORD_TREE),
        copy_text(
            "beHistory.records.none",
            &[(
                "path",
                &records_root(agent_home)
                    .unwrap_or_else(|| agent_home.to_path_buf())
                    .display()
                    .to_string(),
            )],
        ),
    ))
}

/// `--list-projects` 在「记录树根不在」时信封里的码（生产方住这里；问它的那一方认码，不认话）。
pub(crate) const NO_RECORD_TREE: &str = "no_record_tree";

/// 带码的那一行：CLI 错误信封（与 CLI 控制面同一份失败载体 `stream::detail::Failed`：`{code, message, detail}`；读信封的是 `remote_ask::settle_pulled`）。
/// 不调 `emit_err`：观测层不往控制层伸手（`layering_guard`）。
fn coded_failure(code: &str, said: &str) -> i32 {
    let f = crate::stream::detail::Failed::new(
        Some("list-projects"),
        code,
        said.to_string(),
        None,
        None,
    );
    f.emit()
}

/// 一次性查询失败的那一行（无码的旧形）。
fn query_failed(e: &str) -> i32 {
    eprintln!("cc-monitor-backend query error: {e}");
    2
}

/// `--list-projects` 的本体，出口是参数：CLI 写 stdout，资产目录（`asset_catalog`）写内存。
///
/// 回「记录树根在不在」：不在 ⇒ `Ok(false)`、一行不写（这台还没起过会话 —— 判定只在这一处）；
/// 怎么说由调用方定：资产目录当零个项目 · CLI 照旧出声（带码）。
pub(crate) fn list_projects_into(agent_home: &Path, out: &mut dyn Write) -> Result<bool, String> {
    let Some(root) = records_root(agent_home) else {
        return Ok(false);
    };
    let entries = match std::fs::read_dir(&root) {
        Ok(it) => it,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(unreadable_dir(&root, &e)),
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        // 空目录（全删过/只剩本机后端）不展示；一个记录目录里会话的真实目录不止一个 ⇒ 一组一行。
        for line in project_rows(&dir, &dir_name) {
            writeln!(out, "{line}").map_err(|e| format!("stdout write failed: {e}"))?;
        }
    }
    Ok(true)
}

/// 一个记录目录里的会话按**真实目录**分组 —— 记录目录名把非 ASCII 字符、`.`、`/` 都折成 `-`，
/// 不同的目录会撞成同一个名字。输入每个会话的 `(读出的目录, 修改时刻)`，回每个会话归哪一组（目录）。
/// 读不出目录的归最近修改的那个读得出目录的会话那一组；一个都读不出 ⇒ 空串一组。
/// 项目清单（[`project_rows`]）与平铺清单（[`sessions_by_dir`]）用的是这同一个函数。
pub(crate) fn group_by_cwd(items: &[(Option<String>, i64)]) -> Vec<String> {
    let fallback = items
        .iter()
        .filter_map(|(c, m)| c.as_ref().map(|c| (*m, c)))
        .max_by_key(|(m, _)| *m)
        .map(|(_, c)| c.clone())
        .unwrap_or_default();
    items
        .iter()
        .map(|(c, _)| c.clone().unwrap_or_else(|| fallback.clone()))
        .collect()
}

/// 会话记录目录读不了 ⇒ 给人看的那一句（按错误的**种类**说；系统原话只进日志 —— 不露实现词）。
fn unreadable_dir(dir: &Path, e: &std::io::Error) -> String {
    tracing::warn!("history_query: 读不了 {}：{e}", dir.display());
    let path = dir.display().to_string();
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        copy_text("beHistory.dir.denied", &[("path", &path)])
    } else {
        copy_text("beHistory.dir.unreadable", &[("path", &path)])
    }
}

/// 一个项目目录 → `--list-projects` 的那几行（按会话的真实目录分组，一组一行，[`group_by_cwd`]）；
/// 目录下没有会话记录 ⇒ 一行都没有（不展示）。
///
/// # 为什么它是**一个函数**而不是 `list_projects` 里的一段
///
/// `list_projects` 的出口是 `stdout`，红线内测不了；判据要判的是**这一行带了什么**。
/// 同样的分法在本文件里已有先例：`analyze_session` 也是把「算出那一行」与「把它印出去」分开的。
fn project_rows(dir: &Path, dir_name: &str) -> Vec<serde_json::Value> {
    project_rows_hiding(dir, dir_name, &hidden_cwd)
}

/// 这台家里那个不出会话的工作目录（`~/.cc-monitor/autostart/`，名字住 [`crate::common::own_dir::HIDDEN_WORK_DIR`]）：
/// 在那里跑的 `claude`（例如替号开额度窗口的那一句、`quota-probe` 报用量的那一次）历史页与会话列表都不出。观测侧只按它藏。
const HIDDEN_DIR: &str = crate::common::own_dir::HIDDEN_WORK_DIR;

/// 工作目录是 [`HIDDEN_DIR`] 的会话不出。**判据只在这一处**（项目清单按它摘组、观测侧按 pidfile 的目录不宣告）。
pub(crate) fn hidden_cwd(cwd: &str) -> bool {
    crate::platform::paths::data_home().is_some_and(|home| hidden_cwd_in(&home, cwd))
}

/// [`hidden_cwd`] 的本体：`data_home` 由调用方给（判据喂夹具）。记录里的目录是那一家解析过链接的那一形 ⇒ 两形都认
/// （解开链接经 observe 唯一那道路径解析 [`super::fence::Fence`]）。
pub(crate) fn hidden_cwd_in(data_home: &Path, cwd: &str) -> bool {
    let dir = data_home.join(HIDDEN_DIR);
    let cwd = Path::new(cwd);
    cwd == dir || super::fence::Fence::at(&dir).is_ok_and(|f| cwd == f.root())
}

/// [`project_rows`] 的本体：`hide(目录)` ⇒ 那一组不出。
fn project_rows_hiding(
    dir: &Path,
    dir_name: &str,
    hide: &dyn Fn(&str) -> bool,
) -> Vec<serde_json::Value> {
    // 目录与修改时刻与 `--list-sessions` 那条同源（`project_dir_of` · `mtime_ms`），两边分出来的组才一样。
    let mut sessions: Vec<(Option<String>, i64)> = Vec::new();
    if let Ok(files) = std::fs::read_dir(dir) {
        for f in files.flatten() {
            let p = f.path();
            if p.is_file() && crate::agents::is_tree_session_file(&p) {
                sessions.push((crate::agents::project_dir_of(&p), mtime_ms(&p)));
            }
        }
    }
    let mut groups: std::collections::BTreeMap<String, (u32, i64)> = Default::default();
    for ((_, mtime), path) in sessions.iter().zip(group_by_cwd(&sessions)) {
        let g = groups.entry(path).or_default();
        g.0 += 1;
        g.1 = g.1.max(*mtime);
    }
    groups
        .into_iter()
        .filter(|(project_path, _)| !hide(project_path))
        .map(|(project_path, (count, last_activity_ms))| {
            serde_json::json!({
                "dirName": dir_name,
                "projectPath": project_path,
                "sessionCount": count,
                "lastActivityMs": last_activity_ms,
            })
        })
        .collect()
}

/// `--list-sessions <project_dir>`：该项目每个 jsonl 一行 JSON：
/// `{"sessionId","jsonlPath","startedAtMs","updatedAtMs","messageCountApprox",
///   "firstUserExcerpt","aiTitle","cwd"}`
/// 元数据在远端 CPU 上扫整个文件提取（对齐本地 analyze 口径的精简版）。
fn list_sessions(agent_home: &Path, project_dir: &str) -> Result<(), String> {
    list_sessions_into(agent_home, project_dir, &mut std::io::stdout().lock())
}

/// `--list-sessions` 的本体，出口是参数（CLI 与平铺清单 [`sessions_by_dir`] 同一个函数）。
pub(crate) fn list_sessions_into(
    agent_home: &Path,
    project_dir: &str,
    out: &mut dyn Write,
) -> Result<(), String> {
    list_sessions_for(agent_home, project_dir, None, out)
}

/// [`list_sessions_into`] 的本体。`only` ＝ 只要这一个会话的整行（按 sid 问清单那一条）：别的会话只出分组要的那几格
/// （sid · 读出的目录 · 修改时刻 —— 都不用扫整份），整行只扫叫这个名字的那一份。
fn list_sessions_for(
    agent_home: &Path,
    project_dir: &str,
    only: Option<&str>,
    out: &mut dyn Write,
) -> Result<(), String> {
    // project_dir 是目录名而非路径：拒绝任何分隔符 / 上跳
    if project_dir.contains('/') || project_dir.contains('\\') || project_dir.contains("..") {
        return Err(format!("invalid project dir name: {project_dir}"));
    }
    // 与 read_session 对齐（也兑现本文件头部"canonicalize 后前缀校验"的承诺）：名字
    // 合法但 projects/ 下若有指向外部的 symlink 目录，read_dir 会跟随逃逸出 projects/
    // ——canonicalize 解析 symlink 后做前缀校验挡住。
    // **改调共享围栏**（E3）：此前这里是一份内联副本，
    // 注释写着「与 `read_session` 对齐」—— 靠手工对齐的两份迟早会漂。
    let dir = fence_under_projects(agent_home, Path::new(project_dir))?;
    let entries = std::fs::read_dir(&dir).map_err(|e| unreadable_dir(&dir, &e))?;
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() || !crate::agents::is_tree_session_file(&p) {
            continue;
        }
        let meta = match only {
            Some(sid) if p.file_stem().is_none_or(|s| s != sid) => serde_json::json!({
                "sessionId": p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                "cwd": crate::agents::project_dir_of(&p),
                "updatedAtMs": mtime_ms(&p),
            }),
            _ => analyze_session_cached(&p),
        };
        writeln!(out, "{meta}").map_err(|e| format!("stdout write failed: {e}"))?;
    }
    Ok(())
}

/// 平铺清单（`history-list`）那一份：这台**每个**记录目录 ⇒ `(目录名, 那一目录的会话行 | 读不了的那一句)`。
/// 会话行同 `--list-sessions`（同一个 [`list_sessions_into`]），`cwd` 换成按 [`group_by_cwd`] 归的那一组 ——
/// 与项目清单同一个分组（记录目录名会撞，按真实目录分）；归到藏起来的那个目录（[`hidden_cwd`]）的不出。
/// 记录树根不在 ⇒ `Ok(None)`（这台还没起过会话，同 [`list_projects_into`]）。
pub(crate) fn sessions_by_dir(
    agent_home: &Path,
) -> Result<Option<Vec<(String, Result<Vec<serde_json::Value>, String>)>>, String> {
    sessions_by_dir_for(agent_home, None)
}

/// [`sessions_by_dir`] 的本体；`only` 见 [`list_sessions_for`]（别的会话那几行只够分组用，调用方按 sid 滤掉）。
pub(crate) fn sessions_by_dir_for(
    agent_home: &Path,
    only: Option<&str>,
) -> Result<Option<Vec<(String, Result<Vec<serde_json::Value>, String>)>>, String> {
    let Some(root) = records_root(agent_home) else {
        return Ok(None);
    };
    let entries = match std::fs::read_dir(&root) {
        Ok(it) => it,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(unreadable_dir(&root, &e)),
    };
    let dirs: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    // 〔perfC〕按目录分给几条线程扫（冷的时候是整台每份会话从头扫一遍，单线程要几秒）。
    // 每条线程从同一个计数器领下一个目录；结果最后按目录名排，与单线程逐字相同。
    let next = std::sync::atomic::AtomicUsize::new(0);
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .clamp(1, LISTING_WORKERS)
        .min(dirs.len().max(1));
    let mut out: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(dir_name) = dirs.get(i) else {
                            break;
                        };
                        if let Some(got) = rows_of_dir(agent_home, dir_name, only) {
                            mine.push((dir_name.clone(), got));
                        }
                    }
                    mine
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    });
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(Some(out))
}

/// 扫清单时最多几条线程（[`sessions_by_dir`]）。
const LISTING_WORKERS: usize = 8;

/// 一个记录目录的会话行（`None` ＝ 这一目录一行都没有，不出）。
fn rows_of_dir(
    agent_home: &Path,
    dir_name: &str,
    only: Option<&str>,
) -> Option<Result<Vec<serde_json::Value>, String>> {
    let mut buf = Vec::new();
    let rows = list_sessions_for(agent_home, dir_name, only, &mut buf).map(|()| {
        let mut rows: Vec<serde_json::Value> = String::from_utf8_lossy(&buf)
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        let keys: Vec<(Option<String>, i64)> = rows
            .iter()
            .map(|v| {
                (
                    v["cwd"].as_str().map(str::to_string),
                    v["updatedAtMs"].as_i64().unwrap_or(0),
                )
            })
            .collect();
        for (v, cwd) in rows.iter_mut().zip(group_by_cwd(&keys)) {
            v["cwd"] = serde_json::json!(cwd);
        }
        rows.retain(|v| !hidden_cwd(v["cwd"].as_str().unwrap_or_default()));
        rows
    });
    if matches!(&rows, Ok(r) if r.is_empty()) {
        return None;
    }
    Some(rows)
}

/// 一份记录扫出的那一行，连同扫到哪了（[`ListingEntry`]）：没变不再扫，变长只扫新增的那一段。
/// 常驻进程里平铺清单（`history-list`）每次都要过这台**全部**会话；一次性进程里这张表只活一趟（等于没有）。
static SESSION_META: std::sync::Mutex<std::collections::BTreeMap<PathBuf, ListingEntry>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// [`SESSION_META`] 的上限（条）：超了整张清掉重来（不做淘汰次序 —— 一台上会话数到这个量级之前它都不会触发）。
const SESSION_META_CAP: usize = 50_000;

/// 清单缓存里的一份：读到哪了（与全文搜索索引共用的 [`Tailed`]）· 完整行累计的那几格 · 末尾半行的那几格 · 成品行。
struct ListingEntry {
    read: Tailed,
    done: SessionScan,
    tail: SessionScan,
    row: serde_json::Value,
    /// 上一次带到这一问时真读了多少字节（判据看它：变长只读尾巴）。
    last_bytes: u64,
}

impl ListingEntry {
    /// 从头逐行流式扫一份（不整份进内存）：完整行进 `done`、交给账本；末尾没写完的那截进 `tail`。
    fn scan(p: &Path, mtime: Option<std::time::SystemTime>) -> Self {
        use std::io::BufRead;
        let mut e = Self {
            read: Tailed::default(),
            done: SessionScan::default(),
            tail: SessionScan::default(),
            row: serde_json::Value::Null,
            last_bytes: 0,
        };
        let mut end = 0u64;
        if let Ok(file) = std::fs::File::open(p) {
            let mut r = std::io::BufReader::new(file);
            let mut line = Vec::new();
            loop {
                line.clear();
                match r.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => end += n as u64,
                }
                if line.ends_with(b"\n") {
                    e.done.absorb_bytes(&line);
                    e.read.took(&line);
                } else {
                    e.tail.absorb_bytes(&line);
                }
            }
        }
        e.read.saw((mtime, end));
        e.last_bytes = end;
        e.row = e.done.joined(&e.tail).row(p);
        e
    }

    /// 尾巴上新读到的那一段并进来（`new` 从上次的完整行之后起，可能以半行结尾）。
    fn extend(&mut self, p: &Path, mtime: Option<std::time::SystemTime>, new: &[u8]) {
        let end = self.read.consumed() + new.len() as u64;
        let (whole, rest) = split_whole_lines(new);
        for line in whole.split_inclusive(|&b| b == b'\n') {
            self.done.absorb_bytes(line);
        }
        self.read.took(whole);
        self.read.saw((mtime, end));
        self.tail = SessionScan::default();
        self.tail.absorb_bytes(rest);
        self.last_bytes = new.len() as u64;
        self.row = self.done.joined(&self.tail).row(p);
    }
}

/// 后端一起来就后台把清单缓存热好（一条一次性线程，降到低优先级；不是定时器，扫完就退）：
/// 第一次打开历史页 / 按 sid 打开查看窗时不必再等整台从头扫一遍。远端流后端与本机常驻后端同一个调用点（`main.rs`）。
pub fn warm_listing_in_background(agent_home: PathBuf) {
    let spawned = std::thread::Builder::new()
        .name("listing-warm".into())
        .spawn(move || {
            let lowered = crate::platform::proc::lower_this_thread();
            let n = warm_listing(&agent_home);
            tracing::info!("历史清单缓存：后台热好 {n} 份（低优先级：{lowered}）");
        });
    if let Err(e) = spawned {
        tracing::warn!("历史清单缓存：后台热缓存的线程起不来（{e}），第一次问清单会现扫");
    }
}

/// [`warm_listing_in_background`] 的本体（判据直接调它）：整台扫一遍清单，回缓存里有几份。
pub(crate) fn warm_listing(agent_home: &Path) -> usize {
    // 读不了记录树 ⇒ 这一趟热不了，说一句；第一次问清单时照样现扫、照样把那一句交给界面。
    if let Err(e) = sessions_by_dir(agent_home) {
        tracing::warn!("历史清单缓存：后台热缓存读不了记录树（{e}）");
    }
    SESSION_META.lock().unwrap_or_else(|e| e.into_inner()).len()
}

/// [`analyze_session`] 过一层 [`SESSION_META`]：没变 ⇒ 上次那一行；变长且前面没被改写 ⇒ 只扫新增的；其余整份重扫。
fn analyze_session_cached(p: &Path) -> serde_json::Value {
    let lock = || SESSION_META.lock().unwrap_or_else(|e| e.into_inner());
    let prev = lock().remove(p);
    // 读到过读不动的一行（之后的都不算）⇒ 不追加读：那一行之后的内容要整份重扫才对得上。
    let may_append = prev.as_ref().is_some_and(|e| !e.done.stopped);
    let mut bytes = 0u64;
    let Ok((step, seen)) = Tailed::step(prev.as_ref().map(|e| &e.read), p, may_append, &mut bytes)
    else {
        return analyze_session(p);
    };
    let entry = match (step, prev) {
        (Step::Same, Some(mut e)) => {
            e.last_bytes = 0;
            e
        }
        (Step::Appended(new), Some(mut e)) => {
            e.extend(p, seen.0, &new);
            e.last_bytes = bytes;
            e
        }
        _ => ListingEntry::scan(p, seen.0),
    };
    let row = entry.row.clone();
    // 拿不到修改时刻 ⇒ 不留（下次认不出它变没变）。
    if seen.0.is_some() {
        let mut t = lock();
        if t.len() >= SESSION_META_CAP {
            t.clear();
        }
        t.insert(p.to_path_buf(), entry);
    }
    row
}

// **围栏住 `observe/fence.rs`**〔审计 F 🔴-6〕：这里原来是具名围栏
// `fence_under_projects` 的本体（「全文件唯一的一处 `canonicalize` + 前缀校验」，audit-0805 08-06 定框 E3 从
// `list_sessions` 的内联副本收成一份）。E3 只收到了**本文件**，`search_query` 里还有一份内联的（头注逐字「复刻 history_query」）
// ⇒ 判定本体（解开根 · 解开目标 · 前缀比）搬去 observe 内部唯一的家 `observe/fence.rs::Fence`；这里只剩「以 `projects/` 为根」
//   那一行（根是哪一个属 Claude 的目录布局，留在认得它的这一侧），三条按路径读的路照旧调它，报错原话逐字不变。
// LOC1b 在这里把本体提成了「根是参数」的一形（为各家合成历史面给的记录根）—— 那一形就是
// `observe/fence.rs::Fence::at(根)?.admit(候选)`（报错取根目录名，与 LOC1b 那一版逐字同形），下面 [`validate_session_path_among`] 改调它。

/// `<agent_home>/projects/` 这道围栏放行一个候选路径（本体在 [`Fence`]；`candidate` 相对按根拼、绝对直用）。
fn fence_under_projects(agent_home: &Path, candidate: &Path) -> Result<std::path::PathBuf, String> {
    Fence::at(&records_root(agent_home).ok_or(NO_TREE)?)?.admit(candidate)
}

/// 这台记录树的根（记录树那一家的布局，注册表 `RecordFace.tree`）。没有记录树那一家 ⇒ `None`。
fn records_root(agent_home: &Path) -> Option<PathBuf> {
    crate::agents::records_root(agent_home)
}

/// 注册表里没有记录树那一家时，按路径读的那几条拒的那一句。
const NO_TREE: &str = "no agent here keeps a record tree";

/// 一个子运行的记录住哪：父记录 ＋（子运行是哪个 ‖ 派出它的那次工具调用）⇒（那份记录, 子运行）。只问适配层给的那几格：
/// 子运行的记录在哪几份（`ChildFace::sources`）· 一条记录属于哪个子运行（`run_of`）· 派出链接（`child_link`）。
///
/// 给的是工具调用 id ⇒ 在父记录里找那次调用的派出链接（只解析含这个 id 的行）；还没对上（前台子运行跑完才回结果）⇒ `not_found`。
pub(crate) fn run_source(
    agent_home: &Path,
    parent: &str,
    run: Option<&str>,
    tool: Option<&str>,
) -> Result<(std::path::PathBuf, String), (&'static str, String)> {
    let parent_path = validate_session_path(agent_home, parent).map_err(|e| ("path_refused", e))?;
    let faces = crate::agents::record_face_of(&parent_path)
        .map(|f| crate::agents::RunFaces::of(&f))
        .unwrap_or(crate::agents::RunFaces::NONE);
    let run = match (run, tool) {
        (Some(r), _) if !r.is_empty() => r.to_string(),
        (_, Some(t)) if !t.is_empty() => run_of_tool(&parent_path, &faces, t)
            .ok_or_else(|| ("not_found", copy_text("rsRun.load.notLinked", &[])))?,
        _ => {
            return Err((
                "bad_args",
                crate::common::contract::malformed("history-run needs run or tool"),
            ))
        }
    };
    for c in faces.sources(&parent_path) {
        if first_run_of(&c, &faces).as_deref() == Some(run.as_str()) {
            return Ok((c, run));
        }
    }
    Err(("not_found", copy_text("rsRun.load.notFound", &[])))
}

/// 父记录里「这次工具调用派出的是哪个子运行」（派出链接里带了子运行的那一条）。
fn run_of_tool(parent: &Path, faces: &crate::agents::RunFaces, tool: &str) -> Option<String> {
    use std::io::BufRead;
    let f = std::fs::File::open(parent).ok()?;
    for line in std::io::BufReader::new(f).lines().map_while(Result::ok) {
        if !line.contains(tool) {
            continue;
        }
        let Ok(v) =
            serde_json::from_str::<serde_json::Value>(line.trim_start_matches('\u{feff}').trim())
        else {
            continue;
        };
        if let Some(r) = faces
            .child_links(&v)
            .into_iter()
            .find(|l| l.tool.as_deref() == Some(tool))
            .and_then(|l| l.run)
        {
            return Some(r);
        }
    }
    None
}

/// 一份子运行记录属于哪个子运行：头几条里第一条答得出的（头几条可能是不归任何运行的元数据）。
fn first_run_of(path: &Path, faces: &crate::agents::RunFaces) -> Option<String> {
    use std::io::BufRead;
    const HEAD_LINES: usize = 8;
    let f = std::fs::File::open(path).ok()?;
    std::io::BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .take(HEAD_LINES)
        .filter_map(|l| {
            serde_json::from_str::<serde_json::Value>(l.trim_start_matches('\u{feff}').trim()).ok()
        })
        .find_map(|v| faces.run_of(&v))
        .map(|m| m.run)
}

/// 按路径读一份会话之前的围栏：Claude 的 `projects/` ∪ 注册表里各家合成历史面给的记录根。
fn validate_session_path(
    agent_home: &Path,
    jsonl_path: &str,
) -> Result<std::path::PathBuf, String> {
    validate_session_path_among(agent_home, &crate::agents::history_roots(), jsonl_path)
}

/// [`validate_session_path`] 的本体，「另外认哪几个根」是参数（判据喂临时目录，不去动进程环境）。
///
/// 历史浏览器本机远端都会列出 Codex 会话（C4d 起后端合成），而本机冷读也改走后端之后，
/// 这道围栏只认 `projects/` ⇒ 列得出、打不开。根由适配层给（`HistoryFace.root`），这里不写死路径。
/// 另外那几个根**只收绝对路径**（相对路径的意思只在 `projects/` 下有定义）；
/// 都不在 ⇒ 回 `projects/` 那一句拒绝（它是今天所有调用方认得的那一句）。
fn validate_session_path_among(
    agent_home: &Path,
    extra_roots: &[std::path::PathBuf],
    jsonl_path: &str,
) -> Result<std::path::PathBuf, String> {
    let candidate = Path::new(jsonl_path);
    let target = match fence_under_projects(agent_home, candidate) {
        Ok(t) => t,
        Err(refused) if candidate.is_absolute() => extra_roots
            .iter()
            .find_map(|r| Fence::at(r).and_then(|f| f.admit(candidate)).ok())
            .ok_or(refused)?,
        Err(refused) => return Err(refused),
    };
    if !crate::agents::is_own_session_file(&target) {
        return Err("refusing to read non-jsonl file".into());
    }
    Ok(target)
}

/// `--read-session <jsonl_path>`：路径校验后原样透传文件内容（这两行原先挂在围栏头上，随围栏搬家挪回它说的那个函数）。
/// 透传原字节：这是冻结给第二个前端的那一面（`IPC-PROTOCOL.md` §7），按原样留着。要**成品**（逐行解析好的记录）走
/// `history-read`（后端自己的 `agents/claudecode/parse.rs::parse_line`；monitor 侧早已没有自己的解析）。
fn read_session(agent_home: &Path, jsonl_path: &str) -> Result<(), String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    let mut f = std::fs::File::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    std::io::copy(&mut f, &mut out).map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// backend-02（Phase 1 offset 续拉）：`--read-session-from-offset <path> <offset>`——
/// seek 到字节 `offset`（0-based）后原样透传 [offset, EOF]，**语义逐字节 = aterm
/// `tail -c +(offset+1)`**（`TailTransport.kt:33` + `SkeletonScan.windowContentCommand`）。
/// `offset` = 客户端从 Line 帧 `byte_offset` 持久化的续点（重连/断线后带上）。
/// 截断/重写（远端 size < offset）**不在此判**——同 aterm 由客户端另经 size 查检测后
/// 决策 reset（`offsetByPath`），此处 seek 过 EOF → 读空 → 透传空，安全无副作用。
/// 透传原字节，理由同 `read_session`（冻结的那一面；成品走 `history-read`）。
///
/// 〔骨架〕加了两个**选项**（不是新子命令 —— 见 [`FromOffsetOpts`] 的头注）：
/// `--until <end>` 把透传收成半开区间 `[offset, end)`；`--index` 不透传字节，改出
/// **骨架索引**（[`session_index`]）。两个都不带时字节一个不变（老调用方零回归）。
fn read_session_from_offset(
    agent_home: &Path,
    jsonl_path: &str,
    offset: u64,
    until: Option<u64>,
) -> Result<(), String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    let mut f = std::fs::File::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    stream_from_offset(&mut f, offset, until, &mut out)
        .map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// 生产 seek+copy 内核（`read_session_from_offset` 与单测共用）——seek 到字节 `offset`（0-based）
/// 透传 [offset, EOF] = `tail -c +(offset+1)`；offset > 文件长 → seek 过 EOF、copy 空（不 panic）。
/// **审计 quality/correctness**：抽出泛型 `W` 让单测直接对 `Vec<u8>` 驱动**发货的 seek 路径本身**、
/// 断言真实续拉字节，闭「只测 `slice_from_offset` 助手、生产 seek 路径无字节断言」的 dup-drift。
///
/// `until = Some(end)`：只透传 `[offset, end)`（`end ≤ offset` ⇒ 空）。骨架按需物化
/// 「这一段正文」走它 —— 两端都取自索引里的行边界，所以切出来的恰好是整行。
/// ⚠ **它不替调用方对齐行边界**：传一个行中间的数，切出来的就是半行（与 `offset` 同一口径）。
fn stream_from_offset<W: std::io::Write>(
    f: &mut std::fs::File,
    offset: u64,
    until: Option<u64>,
    out: &mut W,
) -> std::io::Result<u64> {
    use std::io::{Read, Seek};
    f.seek(std::io::SeekFrom::Start(offset))?;
    match until {
        None => std::io::copy(f, out),
        Some(end) => std::io::copy(&mut f.take(end.saturating_sub(offset)), out),
    }
}

/// `--read-session-from-offset` 的两个**选项**：`--index`（出骨架索引，不出字节）·
/// `--until <end>`（右端收口）。
///
/// # 🔴 为什么是选项，不是一条新子命令 `--session-index`
///
/// 加子命令 ⇒ `build_id_guard` 的指纹变 ⇒ 必须 bump `BUILD_ID`（远端才会判 stale 重装）。
/// 当时（10 路并行）**明令不许 bump**，而那时选项还不进指纹，于是选成了选项。
/// ⚠ 那个前提已经不在：从 `p4m-tail` 起 `SUBCOMMAND_OPTIONS` 也进 `build_id_guard` 的指纹（`#options` 段），
/// 今天加一个选项和加一条子命令一样逼出 bump。留成选项的理由只剩下面两节（语义上就是「从偏移读」· 老后端上认得出来）。
/// **老后端上的行为已设计成可认出来**：
/// 老后端不认 `--index`/`--until`（它只读 `args[1..=2]`，多余参数不看）⇒ 照旧透传字节
/// ⇒ 首行不是 `{"kind":"session_index",…}` ⇒ monitor 据此判「对面不会出索引」并诚实降级
/// （`--until` 同理：多拿到的尾巴由 monitor 自己按 `end` 截掉，结果仍然对，只是多传了字节）。
///
/// # 语义上它也**就是**「从偏移读」
///
/// 索引从 `offset` 起算 ⇒ 冷启动传 0 拿全量；续传传上次的 `end` 拿增量（
/// 「续传令牌只能用字节偏移」）。与透传字节是同一个读、两种出法。
///
/// ⚠ 未知的 `--选项` 与多余的位置参数都**报错**（不静默忽略）：老 monitor 从不带，新 monitor 只带这两个；
/// 多出来的一定是写错了，而静默忽略会让它拿到一份形状不对的输出还以为成功。
///
/// # 🔴 选项可以写在位置参数**前面** —— 而 monitor 就该这么写（让老后端**快速失败**）
///
/// 老后端只看 `args[1]`/`args[2]`：把 `--index` 写在路径**后面**，老后端会把**整份会话**透传回来
/// （弱网上几十 MB，只为了让 monitor 看一眼首行认出「它不会」）。写在**前面**，老后端拿路径当 offset
/// 解析 ⇒ `offset must be a number` ⇒ **零字节、退出 2**。新后端两种位置都认。
/// （现打：基线 `161ffa6` 的 release 后端对一份 50 955 695 字节的会话 —— 选项在后 stdout 50 955 695 字节 /
/// 退出 0；选项在前 stdout 0 字节 / 退出 2。）
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct FromOffsetOpts {
    pub(crate) index: bool,
    pub(crate) until: Option<u64>,
}

pub(crate) fn parse_from_offset_args(
    rest: &[String],
) -> Result<(FromOffsetOpts, Vec<&String>), String> {
    let mut opts = FromOffsetOpts::default();
    let mut pos: Vec<&String> = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--index" => opts.index = true,
            "--until" => {
                let v = it
                    .next()
                    .ok_or("--until requires <end> (byte offset)")?
                    .parse::<u64>()
                    .map_err(|_| "--until <end>: end must be a number")?;
                opts.until = Some(v);
            }
            other if other.starts_with("--") => {
                return Err(format!(
                    "--read-session-from-offset: unknown option {other}"
                ))
            }
            _ => pos.push(a),
        }
    }
    if pos.len() > 2 {
        return Err(format!(
            "--read-session-from-offset takes <jsonl_path> <offset>, got {} positional arguments",
            pos.len()
        ));
    }
    Ok((opts, pos))
}

/// `--list-user-inputs [--from <offset>] <jsonl_path>` 的 argv：一个位置参数 ＋ 一个可选的 `--from`。
///
/// 选项在位置参数前后都认；客户端写在前面（与骨架索引那一形同一条纪律；monitor 不再经 argv 发它，
/// 界面经帧命令 `history-user-inputs` 问）。未知的 `--选项`、多余的位置参数都**报错**，
/// 不静默忽略 —— 静默忽略会让调用方拿到一份形状不对的输出还以为成功了。
pub(crate) fn parse_user_inputs_args(rest: &[String]) -> Result<(u64, &String), String> {
    let mut from: u64 = 0;
    let mut pos: Vec<&String> = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--from" => {
                from = it
                    .next()
                    .ok_or("--from requires <offset> (byte offset)")?
                    .parse::<u64>()
                    .map_err(|_| "--from <offset>: offset must be a number")?;
            }
            other if other.starts_with("--") => {
                return Err(format!("--list-user-inputs: unknown option {other}"))
            }
            _ => pos.push(a),
        }
    }
    match pos.as_slice() {
        [p] => Ok((from, *p)),
        _ => Err(format!(
            "--list-user-inputs takes exactly one <jsonl_path>, got {} positional arguments",
            pos.len()
        )),
    }
}

/// `--list-user-inputs`：从字节 `from` 起（冷启动 0 / 增量传上次的 `end`）出「你说过的话」清单。
/// 形状与口径见 [`crate::observe::user_inputs`] 的头注。
///
/// `from` 超过文件长度 ⇒ **报错**（文件被截断或重写过 —— 调用方手上的 `end` 已经不指向这份文件），
/// 不回一份空清单假装「没有新的」。路径守卫与 `--read-session` 同一套。
fn list_user_inputs(agent_home: &Path, jsonl_path: &str, from: u64) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    list_user_inputs_into(agent_home, jsonl_path, from, &mut out)?;
    out.flush().map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// `--list-user-inputs` 的本体，出口是参数 ——帧面那条（`history-user-inputs`）
/// 与 CLI 这条**跑的是同一个函数**，只是 `out` 一个是 stdout、一个是内存里那份应答（同 `list_projects_into`）。
pub(crate) fn list_user_inputs_into(
    agent_home: &Path,
    jsonl_path: &str,
    from: u64,
    mut out: &mut dyn Write,
) -> Result<(), String> {
    // 从头要 ⇒ 共用扫描图那一份（回退掉的那几句已经不在里面，与帧面同一份）。
    if from == 0 {
        let map = cold_scan(agent_home, jsonl_path)?;
        return crate::observe::user_inputs::write_rows(&map.inputs, map.end, &mut out)
            .map_err(|e| format!("stream failed: {e}"));
    }
    crate::observe::user_inputs::write_user_inputs(
        open_user_inputs_at(agent_home, jsonl_path, from)?,
        from,
        &mut out,
    )
    .map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// 冷开那几问共用的扫描图（过读会话那同一道围栏；没变就是留着的那张，见 [`super::record_scan`]）。
pub(crate) fn cold_scan(
    agent_home: &Path,
    jsonl_path: &str,
) -> Result<std::sync::Arc<super::record_scan::ScanMap>, String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    super::record_scan::global()
        .get(&target)
        .map_err(|e| format!("open failed: {e}"))
}

/// 尾段那张图：扫描图留着且没变 ⇒ 从它切；否则现数一遍行（只数行、不解析，比等整遍扫描快得多 —— 冷开时界面最先要它）。
pub(crate) fn tail_now(agent_home: &Path, jsonl_path: &str, n: usize) -> Result<TailPlan, String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    if let Some(map) = super::record_scan::global().peek(&target) {
        return Ok(map.tail(n));
    }
    tail_plan(agent_home, jsonl_path, n).map(|(plan, _)| plan)
}

/// 大纲清单的打开口（围栏 ＋「起点越过文件尾 ⇒ 报错」＋ 定位）—— CLI 臂与帧面臂共用。
pub(crate) fn open_user_inputs_at(
    agent_home: &Path,
    jsonl_path: &str,
    from: u64,
) -> Result<std::io::BufReader<std::fs::File>, String> {
    use std::io::{Seek, SeekFrom};
    let target = validate_session_path(agent_home, jsonl_path)?;
    let mut f = std::fs::File::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let len = f.metadata().map_err(|e| format!("stat failed: {e}"))?.len();
    if from > len {
        return Err(format!(
            "--from {from} is past EOF ({len} bytes): file was truncated or rewritten"
        ));
    }
    f.seek(SeekFrom::Start(from))
        .map_err(|e| format!("seek failed: {e}"))?;
    Ok(std::io::BufReader::new(f))
}

/// `history-facts` 的项目目录那一格：过读会话那同一道围栏，再问记录归属的那一家（只读开头）。
pub(crate) fn facts_project_dir(agent_home: &Path, jsonl_path: &str) -> Option<String> {
    let target = validate_session_path(agent_home, jsonl_path).ok()?;
    crate::agents::project_dir_of(&target)
}

/// `history-facts` 的 `agent`：这份记录是哪一家的（适配层按记录认）。
pub(crate) fn facts_agent(agent_home: &Path, jsonl_path: &str) -> Option<String> {
    let target = validate_session_path(agent_home, jsonl_path).ok()?;
    crate::agents::record_kind_of(&target).map(str::to_string)
}

/// `history-facts` 的续点：从 `from` 接着读之前先核两件事，任一不成立 ⇒ 报错（调用方从 0 重要一份）：
/// ① `from` 不越过文件尾（越过 = 截断 / 重写）；② `from > 0` 时文件第 `from-1` 字节是 `\n`
/// （续点恒是某个完整行的末字节 —— 不在行边界上 = 被重写过，接着读会从半行起、把后面的事实算歪）。
/// 挡不住的一形：重写成更长、而旧续点恰好也落在新内容的行边界上（与大纲同一个口子）。
pub(crate) fn open_facts_at(
    agent_home: &Path,
    jsonl_path: &str,
    from: u64,
) -> Result<std::io::BufReader<std::fs::File>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let target = validate_session_path(agent_home, jsonl_path)?;
    let mut f = std::fs::File::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let len = f.metadata().map_err(|e| format!("stat failed: {e}"))?.len();
    if from > len {
        return Err(format!(
            "resume point {from} is past EOF ({len} bytes): file was truncated or rewritten"
        ));
    }
    if from > 0 {
        f.seek(SeekFrom::Start(from - 1))
            .map_err(|e| format!("seek failed: {e}"))?;
        let mut last = [0u8; 1];
        f.read_exact(&mut last)
            .map_err(|e| format!("read failed: {e}"))?;
        if last[0] != b'\n' {
            return Err(format!(
                "resume point {from} is not at a line boundary: file was rewritten"
            ));
        }
    }
    f.seek(SeekFrom::Start(from))
        .map_err(|e| format!("seek failed: {e}"))?;
    Ok(std::io::BufReader::new(f))
}

/// `--find-in-session` 的 argv。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FindArgs<'a> {
    pub(crate) path: &'a String,
    pub(crate) query: &'a String,
    pub(crate) include_tools: bool,
    pub(crate) limit: usize,
}

/// `--find-in-session [--include-tools] [--limit <n>] --query <q> <jsonl_path>`。
///
/// 🔴 **查询串是 `--query` 的值，不是位置参数**：用户要找的就可能是 `--force` 这种以 `--` 起头的词，
/// 作为位置参数它会被当成一个写错的选项。选项的值原样取下一个 token，不看它长什么样。
/// 选项在位置参数前后都认；客户端写在前面（monitor 不再经 argv 发它，界面经帧命令 `history-find` 问）。
/// 未知的 `--选项`、缺 `--query`、位置参数不是恰好一个 ⇒ **报错**（不静默忽略 —— `--search` 那种
/// 「未知选项容错忽略」正是本命令不做成它的一个选项的理由之一，见 `IPC-PROTOCOL.md §10.5`）。
/// `--limit` 超出封顶按封顶算（`FIND_MAX_LIMIT`）；0 ⇒ 只数不列。
pub(crate) fn parse_find_args(rest: &[String]) -> Result<FindArgs<'_>, String> {
    use crate::observe::search_query::{FIND_DEFAULT_LIMIT, FIND_MAX_LIMIT};
    let mut query: Option<&String> = None;
    let mut include_tools = false;
    let mut limit = FIND_DEFAULT_LIMIT;
    let mut pos: Vec<&String> = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--query" => query = Some(it.next().ok_or("--query requires <text>")?),
            "--include-tools" => include_tools = true,
            "--limit" => {
                limit = it
                    .next()
                    .ok_or("--limit requires <n>")?
                    .parse::<usize>()
                    .map_err(|_| "--limit <n>: n must be a number")?
                    .min(FIND_MAX_LIMIT);
            }
            other if other.starts_with("--") => {
                return Err(format!("--find-in-session: unknown option {other}"))
            }
            _ => pos.push(a),
        }
    }
    let query = query.ok_or("--find-in-session requires --query <text>")?;
    match pos.as_slice() {
        [p] => Ok(FindArgs {
            path: p,
            query,
            include_tools,
            limit,
        }),
        _ => Err(format!(
            "--find-in-session takes exactly one <jsonl_path>, got {} positional arguments",
            pos.len()
        )),
    }
}

/// `--find-in-session`：在**一份**会话里找（路径守卫与 `--read-session` 同一套）。形状见
/// [`crate::observe::search_query::write_session_find`] 的头注。
fn find_in_session(agent_home: &Path, a: &FindArgs<'_>) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    find_in_session_into(
        agent_home,
        a.path,
        a.query,
        a.include_tools,
        a.limit,
        &mut out,
    )?;
    out.flush().map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// 会话内查找的本体，出口是参数 ——帧面那条（`history-find`）与 CLI 这条
/// （`--find-in-session`）**跑的是同一个函数**（同 `list_projects_into`）。
pub(crate) fn find_in_session_into(
    agent_home: &Path,
    jsonl_path: &str,
    query: &str,
    include_tools: bool,
    limit: usize,
    mut out: &mut dyn Write,
) -> Result<(), String> {
    crate::observe::search_query::write_session_find(
        open_session_at(agent_home, jsonl_path, 0)?,
        query,
        include_tools,
        limit,
        &mut out,
    )
    .map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// `--read-session-from-offset <path> <offset> --index [--until <end>]`：**骨架索引**。
///
/// 出三段（逐行 JSON）：
/// 1. 头 `{"kind":"session_index","v":1,"from":<offset>}` —— **首行就能认出**「对面会出索引」；
/// 2. 每个**可计行**一行 [`IndexRow`]（口径 = [`line_counts`]，与 watcher/monitor 的 seq 空间一字一致：
///    第 k 行就是 seq `base + k`，`base` = `offset` 之前的可计行数，由调用方持有）；
/// 3. 尾 `{"kind":"session_index_end","count":N,"end":E}` —— `end` = 最后一个**完整行**的末字节
///    （torn 残尾不计，F14 口径）＝ 下一次续传该带的 `offset`。**没有尾行 ⇒ 输出被截断了**，
///    调用方不许把前面那些行当成全量。
///
/// 不含正文（已定：骨架不带正文）。每行约 100 字节 × 条数。
fn session_index(
    agent_home: &Path,
    jsonl_path: &str,
    offset: u64,
    until: Option<u64>,
) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    session_index_into(agent_home, jsonl_path, offset, until, &mut out)?;
    out.flush().map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// 骨架索引的本体，出口是参数 ——帧面那条（`history-index`）
/// 与 CLI 这条（`--read-session-from-offset --index`）**跑的是同一个函数**（同 `list_projects_into`）。
pub(crate) fn session_index_into(
    agent_home: &Path,
    jsonl_path: &str,
    offset: u64,
    until: Option<u64>,
    mut out: &mut dyn Write,
) -> Result<(), String> {
    write_session_index(
        open_session_at(agent_home, jsonl_path, offset)?,
        offset,
        until,
        &mut out,
    )
    .map_err(|e| format!("stream failed: {e}"))?;
    Ok(())
}

/// 过了围栏的会话文件路径（`history-find` 先拿它问常驻索引）。
pub(crate) fn session_path_at(
    agent_home: &Path,
    jsonl_path: &str,
) -> Result<std::path::PathBuf, String> {
    validate_session_path(agent_home, jsonl_path)
}

/// 过围栏、打开、定位到 `offset` —— 骨架索引与会话内查找的 CLI 臂和帧面臂共用这一处
/// （帧面那一臂出成品，不经 `out`，见 `read_face.rs`）。
pub(crate) fn open_session_at(
    agent_home: &Path,
    jsonl_path: &str,
    offset: u64,
) -> Result<std::io::BufReader<std::fs::File>, String> {
    use std::io::{Seek, SeekFrom};
    let target = validate_session_path(agent_home, jsonl_path)?;
    let mut f = std::fs::File::open(&target).map_err(|e| format!("open failed: {e}"))?;
    f.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("seek failed: {e}"))?;
    Ok(std::io::BufReader::new(f))
}

/// [`session_index`] 的内核：读 `r`（已定位在 `from`）逐行出索引。**纯 I/O 泛型**，单测直接喂字节。
///
/// `until`：只收**起点** `< until` 的行（起点在界内的那一行整行收，不劈半行）。
/// 返回写出的行数（不含头尾）。
pub(crate) fn write_session_index<R: std::io::BufRead, W: std::io::Write>(
    r: R,
    from: u64,
    until: Option<u64>,
    out: &mut W,
) -> std::io::Result<u64> {
    writeln!(
        out,
        "{{\"kind\":\"session_index\",\"v\":1,\"from\":{from}}}"
    )?;
    let (count, end) = scan_session_index(r, from, until, |row| {
        serde_json::to_writer(&mut *out, row)?;
        out.write_all(b"\n")
    })?;
    writeln!(
        out,
        "{{\"kind\":\"session_index_end\",\"count\":{count},\"end\":{end}}}"
    )?;
    Ok(count)
}

/// [`write_session_index`] 的中段：每个可计行一条 [`IndexRow`] 交给 `on_row`，回 `(count, end)`。
/// CLI 那一臂（写头尾三段）与帧面那一臂（`read_face.rs` 的 `history-index`，装成成品 `{from, end, rows}`）
/// 跑的是**同一个**扫描；「这一行占不占 seq」仍只住 [`line_counts`]。
pub(crate) fn scan_session_index<R: std::io::BufRead>(
    mut r: R,
    from: u64,
    until: Option<u64>,
    mut on_row: impl FnMut(&IndexRow) -> std::io::Result<()>,
) -> std::io::Result<(u64, u64)> {
    let mut pos = from;
    let mut end = from;
    let mut count: u64 = 0;
    let mut buf: Vec<u8> = Vec::new();
    loop {
        if until.is_some_and(|u| pos >= u) {
            break;
        }
        buf.clear();
        let read = r.read_until(b'\n', &mut buf)?;
        if read == 0 || buf.last() != Some(&b'\n') {
            break; // EOF / torn 残尾不计（F14 口径）
        }
        let start = pos;
        pos += read as u64;
        end = pos;
        let body = &buf[..buf.len() - 1];
        if !line_counts(body) {
            continue; // 空行不占 seq（判定只有 `line_counts` 一个住址）
        }
        on_row(&index_row(body, start, read as u64))?;
        count += 1;
    }
    Ok((count, end))
}

/// 骨架索引的一行：**位置 ＋ 身份 ＋ 宽度无关料**（第一格 · `§2.5b 路 D`）。
///
/// 键名刻意短（每条记录一行，大会话上万行）；**缺省即零 / 假**，零值不序列化。
///
/// # 宽度无关料是什么、**不是**什么
///
/// 是「这条记录渲染出来**会有多少东西**」的计数 —— 与列宽无关，所以用户拉窗口不会让它失效；
/// 前端拿它乘以当前列宽下的字宽/行高做**第一级粗估**（`height-estimate.ts::estimateFromFacts`）。
/// **不是**高度：高度依赖列宽，后端不知道列宽（`§2.5b` 那条「决定性的理由」）。
///
/// ⚠ **口径是近似的，而且近似得有方向**：它不知道前端哪些记录最终不建卡（`stripInternalNoise`、
/// ESC 折叠、slash/compact 细条…），只按记录的**原料**数。前端据 `t` / `sp` / `fd` 分档，
/// 分不准的那几档落到偏保守的常数（`§2.5b`：「粗估用偏保守的常数，精算后往下修」）。
#[derive(Debug, Default, serde::Serialize, PartialEq, Eq)]
pub(crate) struct IndexRow {
    /// 行起点字节偏移（绝对，0-based）—— 按需取正文就是 `[o, o+n)`。
    pub(crate) o: u64,
    /// 行字节长（**含**结尾 `\n`）。
    pub(crate) n: u64,
    /// 记录 `type`；解析不出（非 JSON / 没有 type）⇒ 省略。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) t: Option<String>,
    /// `uuid`（前端 `uuidToIdx` —— 跳转与对账的锚）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) u: Option<String>,
    /// 这条记录属于某个子运行（适配层 `RecordFace::run_of` 答得出）。
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) sc: bool,
    /// user 记录是谁说的（`Speaker` 的 `kind`），人说的与工具结果省略（那两种按正文 / 折叠单元就分得清）。
    /// 界面据它决定这一行建不建卡（判定在适配层，画不画在界面）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sp: Option<&'static str>,
    /// 正文字符数（**代码块之外**；Unicode 标量计，不含换行）。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) ch: u32,
    /// 其中 CJK/全宽字符数（口径 = `height-estimate.ts::fallbackTextHeight` 的 `> 0x2E80`）。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) cj: u32,
    /// 正文**非空**硬行数（代码块之外）。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) pl: u32,
    /// 围栏代码块数（```` ``` ```` / `~~~` 开合一对算一个；没合上的算到文末）。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) cb: u32,
    /// 代码块内总行数。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) cl: u32,
    /// 折叠单元数：`tool_use` / `tool_result` / `thinking` / `redacted_thinking` / `image` 块。
    /// 它们渲染成一行 summary（或并进工具组），与正文长短无关。
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) fd: u32,
    /// 这一行是一条**用户输入**（大纲的一项）⇒ 它的摘要；不是 ⇒ 省略。
    ///
    /// 判定只住 [`crate::observe::user_inputs::user_input_of`]（与 `--list-user-inputs` **同一个函数**），
    /// 摘要同 `excerpt`、uuid 就是本行的 `u`。有了它，首屏的「索引」与「大纲清单」合成一趟读：
    /// 前端见到索引里**有** `x` ⇒ 对面是会出它的后端、每一条用户输入都带着 ⇒ 不再单独要清单；
    /// 一个 `x` 都没有 ⇒ 分不清「老后端」还是「真的零条」⇒ 照旧要一份（形状登记 `IPC-PROTOCOL.md §10.3`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) x: Option<String>,
    /// 同上那一行的 `timestamp`（空串 ⇒ 省略；清单那边的空串 == 这里缺席）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ts: Option<String>,
}

fn is_false(b: &bool) -> bool {
    !*b
}
fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// 一行 → [`IndexRow`]。**纯函数**，不碰文件系统。
///
/// 解析失败（半截 / 非 JSON）**不丢这一行** —— 它仍占一个 seq，只是没有料（`t` 省略）；
/// 丢了它，后面每一行的 seq 都会错一位（那是「计数对不上而且不会报错」的那一形）。
pub(crate) fn index_row(line: &[u8], offset: u64, len: u64) -> IndexRow {
    index_row_of(super::record_scan::parse_record(line).as_ref(), offset, len)
}

/// [`index_row`] 的后半：这一行已经解析过（解析不了 ⇒ `None`，那一行照样占位）。
pub(crate) fn index_row_of(v: Option<&serde_json::Value>, offset: u64, len: u64) -> IndexRow {
    let mut row = IndexRow {
        o: offset,
        n: len,
        ..IndexRow::default()
    };
    let Some(v) = v else {
        return row;
    };
    row.t = v.get("type").and_then(|t| t.as_str()).map(str::to_string);
    row.u = v.get("uuid").and_then(|u| u.as_str()).map(str::to_string);
    // 骨架索引读的是记录树那一家的记录。
    let kind = crate::agents::record_tree_kind().unwrap_or_default();
    row.sc = crate::agents::run_of_record(kind, &v).is_some();
    let said = crate::agents::user_text_of(kind, &v);
    row.sp = said
        .as_ref()
        .map(|u| u.speaker.kind())
        .filter(|k| !matches!(*k, "human" | "toolResult"));
    // 大纲那一项（判定只住 `user_inputs`；这里只搬字段）
    if let Some(ui) = crate::observe::user_inputs::user_input_given(&v, said.as_ref()) {
        row.x = Some(ui.excerpt);
        row.ts = Some(ui.timestamp).filter(|t| !t.is_empty());
    }
    // 正文在哪：user/assistant 在 `message.content`（字符串或块数组）；system 在顶层 `content`。
    let content = v
        .get("message")
        .and_then(|m| m.get("content"))
        .or_else(|| v.get("content"));
    match content {
        Some(serde_json::Value::String(s)) => count_prose(s, &mut row),
        Some(serde_json::Value::Array(blocks)) => {
            for b in blocks {
                match b.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(s) = b.get("text").and_then(|t| t.as_str()) {
                            count_prose(s, &mut row);
                        }
                    }
                    Some(
                        "tool_use" | "tool_result" | "thinking" | "redacted_thinking" | "image",
                    ) => row.fd += 1,
                    _ => {}
                }
            }
        }
        _ => {}
    }
    row
}

/// 数一段 markdown 正文：围栏代码块内外分开数（代码走等宽行高、正文走折行估计，两者算法不同）。
fn count_prose(s: &str, row: &mut IndexRow) {
    let mut in_code = false;
    for line in s.split('\n') {
        let lead = line.trim_start_matches(' ');
        // 围栏：行首至多 3 个空格后 ``` 或 ~~~（CommonMark 口径的近似；不区分开合的字符种类）
        if line.len() - lead.len() <= 3 && (lead.starts_with("```") || lead.starts_with("~~~")) {
            if !in_code {
                row.cb += 1;
            }
            in_code = !in_code;
            continue;
        }
        if in_code {
            row.cl += 1;
            continue;
        }
        let l = line.trim_end_matches('\r');
        if l.trim().is_empty() {
            continue;
        }
        row.pl += 1;
        for c in l.chars() {
            row.ch += 1;
            if c as u32 > 0x2e80 {
                row.cj += 1;
            }
        }
    }
}

/// 纯：offset 续拉的字节切片语义（**仅供单测**对拍 aterm `tail -c +(offset+1)`；生产走
/// `stream_from_offset` 的 `File::seek`、不调本函数，故 `#[cfg(test)]` 不进生产二进制）。
/// = `bytes[min(offset,len)..]`——offset ≤ len 时取 [offset, EOF]；offset > len
/// （截断）时取空（与 `File::seek` 过 EOF 后读空一致，不 panic）。
#[cfg(test)]
fn slice_from_offset(bytes: &[u8], offset: u64) -> &[u8] {
    let o = (offset as usize).min(bytes.len());
    &bytes[o..]
}

/// Batch9-F30：`--read-session-tail <path> <N>`——尾部优先输出：
/// 首行 meta `{"kind":"snapshot_meta","total":T,"tail_from":F}`（T/F 均按
/// **可计行**口径：完整（`\n` 收尾）且非 BOM/全空白——与 watcher/monitor 的
/// 行号空间一字一致），随后原样输出可计行 [F,T)（最新 N 行）、再输出 [0,F)。
/// monitor 据 meta 编 seq：前 T-F 行 = F+i，其余 = i。空文件 → 仅 meta。
fn read_session_tail(agent_home: &Path, jsonl_path: &str, n: usize) -> Result<(), String> {
    use std::io::{Read, Seek, SeekFrom};
    let (plan, mut f) = tail_plan(agent_home, jsonl_path, n)?;
    let TailPlan {
        total,
        tail_from,
        split_at,
        end: complete_end,
    } = plan;
    let meta =
        format!("{{\"kind\":\"snapshot_meta\",\"total\":{total},\"tail_from\":{tail_from}}}\n");
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

/// `--read-session-tail` 那一趟扫描的结果：可计行总数 · 尾段起点行号 · 两段的字节边界。
///
/// 抽出来是因为帧面那条（`history-tail`）只要**这张图**，
/// 正文按字节区间另走 `history-read` 分页拉 —— 一帧应答装不下几十 MB 的会话，
/// 而 CLI 这条仍然一口气印完。**两条路扫的是同一个函数**，行号口径因此只有一份。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) struct TailPlan {
    /// 可计行总数（`line_counts` 口径）。
    pub total: u64,
    /// 尾段第一行的行号。
    pub tail_from: u64,
    /// 尾段第一行的字节起点（＝ 头段的字节终点）。
    pub split_at: u64,
    /// 最后一个完整行（`\n` 收尾）之后的字节位置；torn 残尾不在任何一段里。
    pub end: u64,
}

/// 扫一遍，出 [`TailPlan`] 与已打开的那份文件（调用方按区间去读）。
pub(crate) fn tail_plan(
    agent_home: &Path,
    jsonl_path: &str,
    n: usize,
) -> Result<(TailPlan, std::fs::File), String> {
    let target = validate_session_path(agent_home, jsonl_path)?;
    // 审计 D：整文件 std::fs::read 在 Pi 级设备上对数百 MB 会话有 OOM 风险
    // （旧 --read-session 是 io::copy 流式）——改单遍流式扫描（环形缓冲只存
    // 最近 N 个可计行的字节偏移，O(N) 内存）+ 两次 seek 范围拷贝。
    let f = std::fs::File::open(&target).map_err(|e| format!("open failed: {e}"))?;
    let mut reader = std::io::BufReader::new(f);
    let plan = tail_plan_of(&mut reader, n).map_err(|e| format!("scan failed: {e}"))?;
    Ok((plan, reader.into_inner()))
}

/// [`tail_plan`] 的扫描本体：读 `reader`（从字节 0 起）出 [`TailPlan`]。**纯 I/O 泛型**。
pub(crate) fn tail_plan_of<R: std::io::BufRead>(
    mut reader: R,
    n: usize,
) -> std::io::Result<TailPlan> {
    let mut recent: std::collections::VecDeque<u64> = std::collections::VecDeque::new();
    let keep = n.max(1);
    let mut total: u64 = 0;
    let mut pos: u64 = 0;
    let mut complete_end: u64 = 0;
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        let read = reader.read_until(b'\n', &mut buf)?;
        if read == 0 {
            break;
        }
        let line_start = pos;
        pos += read as u64;
        if *buf.last().unwrap() != b'\n' {
            break; // torn 残尾不计（F14 口径）
        }
        complete_end = pos;
        if !line_counts(&buf[..buf.len() - 1]) {
            continue; // 空行不计（与 watcher/monitor 口径一致 —— 判定只有 `line_counts` 一个住址）
        }
        total += 1;
        recent.push_back(line_start);
        if recent.len() > keep {
            recent.pop_front();
        }
    }
    let tail_from = total - recent.len() as u64;
    let split_at = recent.front().copied().unwrap_or(complete_end);
    Ok(TailPlan {
        total,
        tail_from,
        split_at,
        end: complete_end,
    })
}

/// 帧面按字节区间分页读一份会话的**一页**（`history-read`）。
///
/// # 为什么要分页（`C1` · 2026-09-24）
///
/// 一次性子命令把整份会话**流**回去（`io::copy`），连接关了就是 EOF；帧面是一问一答，
/// 一帧应答要整个进内存、整个过线 —— 而本仓见过 270 MB 的会话，monitor 那头单帧上限
/// 64 MiB。⇒ 调用方给 `[offset, until)`，本函数回**不超过 `page` 字节、切在行尾**的一页，
/// 外加续点 `next`；调用方循环到 `eof`。
///
/// # 切法
///
/// - 这一页之后区间里还有字节 ⇒ 切在**最后一个 `\n` 之后**，不把半行交出去；
/// - 一页里一个 `\n` 都没有（单行比 `page` 还长）⇒ 往后续读到那一行的 `\n`，
///   但**不超过 `line_cap`** —— 超了回 `oversized_line`（不许截半行冒充整行）；
/// - 区间到头（`until` 或读时的文件长度）⇒ 余下的全给，含 torn 残尾（与 `--read-session`
///   的 `io::copy` 同口径：残尾怎么处置由调用方定）。
///
/// 错误是 `(code, message)`：`refused`（围栏拒）· `failed`（读失败）· `oversized_line`
/// （不叫 `line_too_long`：那是入方向信封的**协议级** code，命令级不许撞名）。
pub(crate) fn read_page(
    agent_home: &Path,
    jsonl_path: &str,
    offset: u64,
    until: Option<u64>,
    page: usize,
    line_cap: usize,
) -> Result<ReadPage, (&'static str, String)> {
    use std::io::{Read, Seek, SeekFrom};
    let target = validate_session_path(agent_home, jsonl_path).map_err(|e| ("refused", e))?;
    let mut f =
        std::fs::File::open(&target).map_err(|e| ("failed", format!("open failed: {e}")))?;
    let len = f
        .metadata()
        .map_err(|e| ("failed", format!("stat failed: {e}")))?
        .len();
    let limit = until.map_or(len, |u| u.min(len));
    if offset >= limit {
        return Ok(ReadPage {
            bytes: Vec::new(),
            next: offset,
            eof: true,
        });
    }
    f.seek(SeekFrom::Start(offset))
        .map_err(|e| ("failed", format!("seek failed: {e}")))?;
    let mut src = f.take(limit - offset);
    let mut bytes = Vec::with_capacity(page.min((limit - offset) as usize));
    (&mut src)
        .take(page as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| ("failed", format!("read failed: {e}")))?;
    let reached_limit = offset + bytes.len() as u64 >= limit;
    if !reached_limit {
        match bytes.iter().rposition(|&b| b == b'\n') {
            Some(i) => bytes.truncate(i + 1),
            None => {
                // 单行比一页还长：续读到它的行尾，但不超过 `line_cap`。
                let mut more = [0u8; 64 * 1024];
                loop {
                    let k = src
                        .read(&mut more)
                        .map_err(|e| ("failed", format!("read failed: {e}")))?;
                    if k == 0 {
                        break;
                    }
                    let done = match more[..k].iter().position(|&b| b == b'\n') {
                        Some(i) => {
                            bytes.extend_from_slice(&more[..=i]);
                            true
                        }
                        None => {
                            bytes.extend_from_slice(&more[..k]);
                            false
                        }
                    };
                    // ⚠ 上限在**每一次**续读之后判，包括找到行尾的那一次 ——
                    //   只在「没找到」那一支判的话，行尾恰好落在这一块里的超长行会被整行交出去。
                    if bytes.len() > line_cap {
                        return Err((
                            "oversized_line",
                            copy_text(
                                "beHistoryQuery.readPage.lineTooLong",
                                &[
                                    ("lineCap", &line_cap.to_string()),
                                    ("offset", &offset.to_string()),
                                ],
                            ),
                        ));
                    }
                    if done {
                        break;
                    }
                }
            }
        }
    }
    let next = offset + bytes.len() as u64;
    Ok(ReadPage {
        bytes,
        next,
        eof: next >= limit,
    })
}

/// [`read_page`] 的一页。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ReadPage {
    /// 原始字节（调用方负责解码 —— 帧面那层用 `from_utf8_lossy`）。
    pub bytes: Vec<u8>,
    /// 下一页从这里起。
    pub next: u64,
    /// 区间到头了。
    pub eof: bool,
}

/// 帧面按**行号**取回一段（`history-lines`）：第 `[from, until)` 个可计行的原文。
///
/// # 为什么要有它
///
/// 按字节取正文（[`read_page`]）要调用方先知道字节边界 —— 那只有骨架索引给得出。
/// 没接骨架的会话（后台 tab · 老后端 · seq 对不上的）丢掉的正文就无处可回 ⇒ monitor 的重放缓冲
/// 不敢设上界（级 3）。本函数是**不依赖索引**的那条取回路：调用方只说行号。
///
/// # 口径
///
/// - 「第几行」只有一个家：[`line_counts`]（BOM 与全空白不占号）＋ 没 `\n` 收尾的残尾不计 ——
///   与 [`tail_plan`] 的 `total` · [`scan_session_index`] 的行号 · 实时 `line` 帧的 `seq` 同一个空间。
/// - 只交**可计行**（空行在号外，交出去调用方也不会给它编号）；第 k 条就是第 `from + k` 行。
/// - `until` 缺 ＝ 到最后一个完整行为止。
///
/// # 一帧装得下
///
/// 交出的原文累计达到 `page` 字节就停（至少交一行）；单行超过 `line_cap` ⇒ `oversized_line`
/// （同 [`read_page`]：不截半行）。停在中途时 `eof == false`、`next < until`，调用方接着要。
///
/// # 代价（写清楚，`CF2.md §1.2`）
///
/// **O(第 `from` 行之前的字节)**：后端零状态，每次从文件头数。本机最大一份会话（111 MB / 4.2 万行）
/// 热缓存从头数到尾 ≈ 22 ms；取回只在用户往上翻过了前端手里最老那一条时才发生，一次一批。
/// 真撞上「弱设备上超大会话翻得卡」的读数时再加锚（`CF2.md §1.1` 候选 C），不先加。
///
/// 错误同 [`read_page`]：`refused` · `failed` · `oversized_line`。
pub(crate) fn read_lines(
    agent_home: &Path,
    jsonl_path: &str,
    from: u64,
    until: Option<u64>,
    page: usize,
    line_cap: usize,
) -> Result<LinesPage, (&'static str, String)> {
    let target = validate_session_path(agent_home, jsonl_path).map_err(|e| ("refused", e))?;
    let f = std::fs::File::open(&target).map_err(|e| ("failed", format!("open failed: {e}")))?;
    read_lines_from(std::io::BufReader::new(f), from, until, page, line_cap)
}

/// [`read_lines`] 的内核：读 `r`（从文件头起）。**纯 I/O 泛型**，单测直接喂字节。
pub(crate) fn read_lines_from<R: std::io::BufRead>(
    mut r: R,
    from: u64,
    until: Option<u64>,
    page: usize,
    line_cap: usize,
) -> Result<LinesPage, (&'static str, String)> {
    let until = until.unwrap_or(u64::MAX);
    let mut lines: Vec<String> = Vec::new();
    let mut starts: Vec<u64> = Vec::new();
    let mut bytes: usize = 0;
    let mut n: u64 = 0; // 下一个可计行的行号
    let mut pos: u64 = 0; // 这一行的起点字节偏移
    let mut buf: Vec<u8> = Vec::new();
    let eof = loop {
        if n >= until {
            break false;
        }
        buf.clear();
        let start = pos;
        let read = r
            .read_until(b'\n', &mut buf)
            .map_err(|e| ("failed", format!("scan failed: {e}")))?;
        pos += read as u64;
        if read == 0 || buf.last() != Some(&b'\n') {
            break true; // 文件到头；torn 残尾不计（同 `tail_plan`）
        }
        let body = &buf[..buf.len() - 1];
        if !line_counts(body) {
            continue;
        }
        let at = n;
        n += 1;
        if at < from {
            continue;
        }
        if body.len() > line_cap {
            return Err((
                "oversized_line",
                copy_text(
                    "beHistoryQuery.readLinesFrom.lineTooLong",
                    &[("at", &at.to_string()), ("lineCap", &line_cap.to_string())],
                ),
            ));
        }
        bytes += body.len();
        lines.push(String::from_utf8_lossy(body).into_owned());
        starts.push(start);
        if bytes >= page {
            break false;
        }
    };
    let next = from + lines.len() as u64;
    Ok(LinesPage {
        from,
        lines,
        starts,
        next,
        eof,
    })
}

/// [`read_lines`] 的一段。不变量：`next == from + lines.len()`。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LinesPage {
    /// 第一条的行号（＝ 请求里的 `from`）。
    pub from: u64,
    /// 可计行的原文（不含行尾 `\n`；`\r` 与 BOM 原样留着，调用方剥）。
    pub lines: Vec<String>,
    /// 每一条在文件里的起点字节偏移（与 `lines` 逐条对齐）。
    pub starts: Vec<u64>,
    /// 下一段从这一行起。
    pub next: u64,
    /// 读到了最后一个完整行之后（后面没有了）。
    pub eof: bool,
}

/// 「这条会话的记录还在不在」—— `history-record` 帧命令的本体（`read_face` 是它的宿主）。
///
/// 最后一条逐字：「对方那份记录也没了 ⇒ 重开必失败，要诚实报错，不许静默变成『起了个新会话』」。
/// 前端 resume 一跳（`tab-session-actions.ts`）在开终端之前问一次；答「不在」就不开。
///
/// - **只收 sid、不收路径**（`INVARIANTS §41.6` 收窄第 3 条）：找文件那一步是两侧共用的
///   `agents::find_session_file`（经注册表那一格，本体住 `agents/claudecode/branch.rs`；与分叉 / 删会话同一份；符号链接不算命中）。
/// - sid 形状不合法 ⇒ `Err`，**先于任何 IO**；找不到 ⇒ `Ok(present = false)` —— 这是一个答案，不是错误。
/// - 回 `root`（查的是哪棵记录树）：报错时要说清查了什么。
///
/// ⚠ **射程如实写**：它查的是**这台后端**的记录树（`agent_home()/projects`）。会话若起在另一个
/// 账号的配置根下（`CLAUDE_CONFIG_DIR` 指向别处），这里答「不在」而那边其实有 —— 调用方拿到「不在」
/// 时报的话要说清是「这棵树里没有」，不是「世上没有」。
pub(crate) fn record_in(agent_home: &Path, sid: &str) -> Result<RecordProbe, String> {
    if !shell_quote_core::session_id_ok(sid) {
        return Err(crate::common::contract::malformed(&format!(
            "bad session id shape (letters, digits, hyphen; 1..=64): {sid:?}"
        )));
    }
    let Some(root) = records_root(agent_home) else {
        return Err(NO_TREE.to_string());
    };
    let present = crate::agents::find_session_file(&root, sid).is_ok();
    Ok(RecordProbe {
        present,
        root: root.to_string_lossy().into_owned(),
    })
}

/// 这台记录树里那条会话的记录文件（同 [`record_in`] 那一找）。
pub(crate) fn session_record(agent_home: &Path, sid: &str) -> Result<std::path::PathBuf, String> {
    crate::agents::find_session_file(&records_root(agent_home).ok_or(NO_TREE)?, sid)
}

/// [`record_in`] 按**这次 resume 要用的那个账号配置目录**查（`history-record` 的 `configDir`）。
///
/// 会话起在另一个账号根下（`CLAUDE_CONFIG_DIR` 指别处）时，只查这台后端自己的家目录会答「不在」、误拦 resume
/// （上面那段「射程如实写」）。monitor 把这次 resume 交给起会话那一格的同一个目录带过来，
/// 这里就在那棵树里找。
/// - `None` ⇒ 这台的家目录（与改之前逐字同一问）；
/// - `Some(d)` ⇒ 先过账号库那一个形状关（`accounts_query::is_safe_config_dir`：绝对 · 不上跳 · 无 shell 元字符与
///   欺骗字符），不过 ⇒ `Err`，**一次 IO 都不做**；过了 ⇒ 查 `<d>/projects`。
/// 只答在不在、查的是哪棵树 —— 不回内容，不收别的路径。
pub(crate) fn record_for(
    agent_home: &Path,
    config_dir: Option<&str>,
    sid: &str,
) -> Result<RecordProbe, String> {
    let tree = match config_dir {
        None => agent_home.to_path_buf(),
        Some(d) if super::accounts_query::is_safe_config_dir(d) => std::path::PathBuf::from(d),
        Some(d) => {
            return Err(crate::common::contract::malformed(&format!(
                "configDir is not a queryable account config dir \
                 (absolute path, no `..`, no shell metacharacters): {d:?}"
            )))
        }
    };
    record_in(&tree, sid)
}

/// [`record_in`] 的答案。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct RecordProbe {
    /// `<sid>.jsonl` 在记录树里（根那一层或项目目录那一层）找得到。
    pub present: bool,
    /// 查的那棵记录树的根。
    pub root: String,
}

/// 帧面那几条要的「这台的 agent 家目录」—— 与 `main.rs` 那一句**同一个出处**。
///
/// 问注册表里后端盯着的那一家（`agents::home_at`），照进程环境解。
pub(crate) fn agent_home() -> std::path::PathBuf {
    crate::agents::home_at(None, true)
}

/// 这一行**算不算一行**（＝ 口径 `watcher::read_new_lines`：BOM 与全空白跳过）。
///
/// 🔴 **全仓只有这一个住址** —— 扫描循环数 `total`、`split_tail` 数 `starts`，
/// 两边判得必须**一模一样**；口径一分家，计数就对不上，而那种错**不会报错**。
///
/// # 为什么先看字节（秤 7）
///
/// 原来这一跳对**每一行**做 `String::from_utf8_lossy(整行)` 再 `trim()`，
/// **只为回答「这行是不是空的」** —— 那是一次**整文件 UTF-8 校验**。
/// 实测（15.1 MiB 会话，laptop）：`tail` **19.52 → 3.79 ms**、其中「数行」那一趟
/// **17.89 → 2.98 ms（↓83%）**，**出字节一字节不差**。
/// 剩下的 ~3 ms/15.1 MiB（≈5 GiB/s ≈ `memchr`）才是说的
/// 那个**真·不可避免的 O(文件)**。
///
/// # 短路为什么是对的
///
/// - 任何 `< 0x80` 的字节都是**独立的 ASCII 字符** —— UTF-8 多字节序列的
///   首字节与续字节**全部 ≥ 0x80** ⇒ 不会把半个字符误读成 ASCII。
/// - BOM（`U+FEFF` ＝ `EF BB BF`）**三个字节全非 ASCII** ⇒ 短路条件碰不到它。
/// - ⇒ 只要有一个 ASCII 非空白字节，这行剥 BOM、`trim` 完**必然非空**。
/// - 短路**不成立**时退回原判定（保守），所以这不是近似，是**等价**。
///
/// ⚠⚠ **谓词必须是 `char::from(b).is_whitespace()`，不能写成 `b.is_ascii_whitespace()`**
/// —— 后者**不含 `U+000B`**（垂直制表），而 `str::trim` 含它。
/// 秤 7 的第一版变体就是这么写错的，**靠穷举才逮出来**（全部 1/2/3 字节共
/// 16 843 008 个 ＋ 300 万随机串，分歧 0）——
/// **单测没逮住，因为没有那个样本。**
pub(crate) fn line_counts(line: &[u8]) -> bool {
    // 快路：有 ASCII 非空白 ⇒ 必非空，不必解码
    if line
        .iter()
        .any(|b| b.is_ascii() && !char::from(*b).is_whitespace())
    {
        return true;
    }
    // 慢路：可能全是空白 / 多字节 ⇒ 按原判定来
    let text = String::from_utf8_lossy(line);
    !text.trim_start_matches('\u{feff}').trim().is_empty()
}

/// 纯函数：把文件字节按"最新 N 可计行优先"切成 (meta 行, 尾段, 头段)。
/// 只处理到最后一个 `\n`（torn 残尾不进任何段——F14 口径）。
/// 生产路径已流式化（read_session_tail，审计 D 内存修订）；本函数保留为
/// 口径锚点（tail_tests 锚定语义），流式版与它的等价性由本机行为验证对账
/// （真实 18MB 会话：meta/字节输出逐段一致，见 Batch9 feature 留档）。
/// **仅测**（生产走流式版、不调本函数）→ `#[cfg(test)]` 不进生产二进制。
#[cfg(test)]
fn split_tail(bytes: &[u8], n: usize) -> (String, &[u8], &[u8]) {
    let complete_end = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let complete = &bytes[..complete_end];
    // 收集每个可计行的起始字节偏移（口径 = watcher::read_new_lines：BOM/全空白跳过）
    let mut starts: Vec<usize> = Vec::new();
    let mut pos = 0usize;
    for line in complete.split_inclusive(|&b| b == b'\n') {
        if line_counts(&line[..line.len() - 1]) {
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

pub(super) fn created_ms_or_mtime(p: &Path) -> i64 {
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

/// 单个会话的元数据提取（整文件扫描，跑在那台机器的 CPU 上）：
/// - messageCountApprox = 非空行数
/// - firstUserExcerpt = 首条人说的话（适配层判「谁说的」，经注册表 `agents::human_speech`）的前 120 字符
///   （换行折成空格、超了加 `…`）
/// - aiTitle = 最后一条标题记录（`ai-title` 的 `aiTitle` 与 CC v2.1.x 起的 `custom-title` 的 `customTitle`，取最新）
/// - cwd = 会话的项目目录（适配层 `RecordFace.project_dir`：只读开头，与活 tab 的标题同一个函数）
/// - startedAtMs = 首条 user / assistant 记录的 `timestamp`；一条都解析不出 ⇒ 文件建立时刻（拿不到 ⇒ 修改时刻）
/// - forkedFromSessionId / forkedFromMessageUuid = 首条带 `forkedFrom` 的 user / assistant 记录（`/branch` 分叉来的）
///
/// **这一行从此是本机与远端共用的唯一口径**：本机的历史会话清单此前由 monitor 进程内自己扫
/// （`history·rs::analyze_jsonl`〔散文墓碑〕，经记录解析器），与这里的「精简版」各算各的 —— 开始时刻 / 摘录 / 标题 / fork 关系
/// 四格两边不一样。历史跨机 join 进了本机后端之后本机也读这一行 ⇒ 把 monitor 那份有、这里没有的三格（fork 关系 ·
/// `custom-title` · 首条时间戳）补进来，摘录取人说的话（经注册表 `agents::human_speech`，与全文搜索同一个家）、截断用 `observe/search_rules.rs`。条数仍是「非空行数」。
fn analyze_session(p: &Path) -> serde_json::Value {
    // 〔audit-0805 F07 / 报告 B-6 第 5 环〕**流式**（[`ListingEntry::scan`] 逐行读）。原来是 `read_to_string(p)` 整读：
    // `--list-sessions` 对该项目**每个** jsonl 都调它一次，43 个项目 / 2.4 GB 的机器上
    // 一次列表就是把 2.4 GB 读进内存再逐行解析。扫描是必须的（要数行、要判 bg），**但不必先整份进内存**。
    let mtime = std::fs::metadata(p).and_then(|m| m.modified()).ok();
    ListingEntry::scan(p, mtime).row
}

/// **「这条记录说明本会话是从哪个会话分叉来的」的唯一住址**：
/// `user` / `assistant` 记录上的 `forkedFrom`（两个键都得是串）⇒ (源会话 id, 分叉处的消息 uuid)。
///
/// 两个读者调同一个函数：历史会话行（[`analyze_session`] 的 `forkedFromSessionId`）与活 tab 的会话事实
/// （`facts_query.rs` 的 `forkedFrom`）—— 搬之前活 tab 那一份住前端、口径更松（任何记录、只看 `sessionId`），
/// 同一个会话会在历史树与 tab 栏上一个认是分叉、一个不认。
pub(crate) fn fork_origin(v: &serde_json::Value) -> Option<(String, String)> {
    match v.get("type").and_then(|t| t.as_str()) {
        Some("user") | Some("assistant") => forked_from(v),
        _ => None,
    }
}

/// 一条记录的 `forkedFrom`（`/branch` 分叉出来的会话，每条复制过来的记录都带着）→ (源会话 id, 分叉处的消息 uuid)。
fn forked_from(v: &serde_json::Value) -> Option<(String, String)> {
    forked_pair(v.get("forkedFrom")?)
}

/// `forkedFrom` 那一格本身 → (源会话 id, 分叉处的消息 uuid)（两格都得是串）。
pub(super) fn forked_pair(f: &serde_json::Value) -> Option<(String, String)> {
    Some((
        f.get("sessionId")?.as_str()?.to_string(),
        f.get("messageUuid")?.as_str()?.to_string(),
    ))
}

// 按字符截断的那一份（`truncate_chars`〔散文墓碑〕）没了读者：摘录改用 `search_rules::truncate_excerpt`（同样不劈码点，
//   另把换行折成空格、超了加 `…` —— 与本机那条路从前的口径、与全文搜索的标题摘录同一个家）。

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_tail_tests.rs"]
mod tail_tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_f07_tests.rs"]
mod f07_tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_kr83_tests.rs"]
mod kr83_tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_index_tests.rs"]
mod index_tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/history_query_user_inputs_tests.rs"]
mod user_inputs_tests;
