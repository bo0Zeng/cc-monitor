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

use crate::platform::fs::Ro;
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
pub(crate) fn log_tail(
    path: Option<&std::path::Path>,
    max: u64,
) -> Result<LogTail, (&'static str, String)> {
    use std::io::{Read, Seek, SeekFrom};
    let Some(path) = path else {
        return Ok(LogTail {
            path: None,
            size: 0,
            text: String::new(),
            truncated: false,
        });
    };
    let mut f = Ro::open(path).map_err(|e| ("failed", e.to_string()))?;
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
    Ok(LogTail {
        path: Some(path.display().to_string()),
        size,
        text: String::from_utf8_lossy(body).into_owned(),
        truncated: from > 0,
    })
}

/// `backend-log` 的应答：这台后端 stderr 诊断文件的尾部。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct LogTail {
    /// 本进程 stderr 此刻落在的那份文件；没装（stdio 载体 · 没被交路径）⇒ `null`。
    pub(crate) path: Option<String>,
    /// 那份文件的总字节数。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) size: u64,
    /// 尾部正文（lossy UTF-8）；截断时从截点后第一个换行起，不给半行。
    pub(crate) text: String,
    /// 前面还有没回的字节。
    pub(crate) truncated: bool,
}

/// `backend-log` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct BackendLogReq {
    /// 只回尾部这么多字节；缺 ⇒ 封顶 256 KiB（给多了也照封顶）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable, type = "number"))]
    pub(crate) max_bytes: Option<u64>,
}

crate::stream::inbound::typed::codes! {
    /// `backend-log` 的码：那份文件读不了。
    pub(crate) enum BackendLogErr {
        Failed = "failed",
    }
}

/// `backend-log`：这台后端的 stderr 诊断文件尾部（读一份文件 ⇒ 阻塞档）。
pub(crate) struct BackendLog;

impl crate::stream::inbound::typed::Command for BackendLog {
    const NAME: &'static str = "backend-log";
    type Req = BackendLogReq;
    type Reply = LogTail;
    type Err = BackendLogErr;
}

impl crate::stream::inbound::typed::Blocking for BackendLog {
    fn run(
        req: BackendLogReq,
        _cx: &crate::stream::inbound::typed::Cx,
    ) -> Result<LogTail, BackendLogErr> {
        let max = req
            .max_bytes
            .map_or(LOG_TAIL_BYTES, |n| n.min(LOG_TAIL_BYTES));
        log_tail(BACKEND_LOG.get().map(|p| p.as_path()), max).map_err(
            crate::stream::inbound::typed::of_pair(BackendLogErr::Failed),
        )
    }
}

crate::stream::inbound::typed::codes! {
    /// `drift-report` 没有别的码。
    pub(crate) enum DriftReportErr {}
}

/// `drift-report`：这台后端的漂移账（看不懂的记录类型），各家的面并在一起、注册序（纯内存读一把锁，同 `forward-list`）。
pub(crate) struct DriftReportCmd;

impl crate::stream::inbound::typed::Command for DriftReportCmd {
    const NAME: &'static str = "drift-report";
    const TAKES_INPUT: bool = false;
    type Req = crate::stream::inbound::typed::NoArgs;
    type Reply = crate::agents::drift::DriftReport;
    type Err = DriftReportErr;
}

impl crate::stream::inbound::typed::Async for DriftReportCmd {
    fn run(
        _req: crate::stream::inbound::typed::NoArgs,
        _cx: crate::stream::inbound::typed::Cx,
    ) -> impl std::future::Future<Output = Result<crate::agents::drift::DriftReport, DriftReportErr>>
           + Send
           + 'static {
        std::future::ready(Ok(crate::agents::drift_report()))
    }
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
    use crate::observe::accounts_query;
    match cmd {
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
        "accounts-list" => {
            let agent = crate::accounts::upstream_select::endpoint::agent_arg(args)?;
            let rows = crate::accounts::upstream_select::file_face::machine_rows();
            let v = accounts_query::list_product(
                &rows,
                agent,
                crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT,
                crate::relay::our_relay_listening(relay_route_core::PORT),
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
        other => Err((
            "bad_args",
            crate::common::contract::malformed(&format!("this face has no command `{other}`")),
        )),
    }
}

/// 过围栏 ⇒ 这份记录是哪一家的（注册表按它落在谁的根下认），解释交那一家。
pub(crate) fn record_face(
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
pub(crate) fn reader<'a>(
    face: &'a crate::agents::RecordFace,
    target: &std::path::Path,
    offset: u64,
    tz: &crate::Tz,
) -> crate::observe::record_page::Reader<'a> {
    let (lead_at, lead) = crate::observe::record_page::lead_of(target, offset);
    crate::observe::record_page::Reader::new(face, lead_at, &lead, tz.clone())
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

pub(crate) fn too_large(size: usize) -> (&'static str, String) {
    too_large_at(size, LINES_CAP_BYTES)
}

/// 同 [`too_large`]，上限是参数（全文搜索那条按它判，判据拿小上限验）。
pub(crate) fn too_large_at(size: usize, cap: usize) -> (&'static str, String) {
    (
        "too_large",
        copy_text(
            "beReadFace.tooLarge.say",
            &[("size", &size.to_string()), ("cap", &cap.to_string())],
        ),
    )
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

#[cfg(test)]
#[path = "../../../tests/backend/faces/read_face_tests.rs"]
mod tests;

/// 会话的 `needs` 配上常驻进程里挂着的待答请求（`control::asks` 那一份登记表；配法住 `facts_query::dress_ask_with`）；
/// 没有挂着的、而答题口没接上（`off`，[`answer_off_of`]）⇒「只能在终端里答」那一句换成它为什么。
/// 会话事实 · 过程那一轮 · 需手动清单三处都经这里（同一条会话三处说的是同一句）。
pub(crate) fn dress_ask(
    needs: Option<crate::observe::facts_query::Needs>,
    sid: &str,
    off: Option<&crate::common::cells::Words>,
) -> Option<crate::observe::facts_query::Needs> {
    let now = crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    let mut needs = crate::observe::facts_query::dress_ask_with(
        needs,
        crate::control::asks::pending(sid).into_iter().next(),
        now,
    );
    if let (Some(n), Some(off)) = (needs.as_mut(), off) {
        if n.ask.is_none() {
            n.manual_text = Some(off.clone());
        }
    }
    needs
}

/// 答题口没接上 ⇒ 写好的那一句（会话事实 `answerOff`）；接上了 / 判不了 ⇒ `None`。按持着它的进程现查（`writers` 是它们的 pid）：
/// ccm 起它时挂不上答题 hook ⇒ 起的那个进程环境里带着那个变量（值说原因）；换了启动器那一支（ccm 的便条说插件是经环境交下去的）、
/// 持着它的进程环境里没有我们那个插件目录 ⇒ 启动器没把环境传下去；身上看不出 ccm 挂过 ⇒ 别处起的（`agent` 那一家有答题口才这么说）。
pub(crate) fn answer_off_of(
    writers: &[u32],
    agent: Option<&str>,
) -> Option<crate::common::cells::Words> {
    use crate::agents::asks::HookOff;
    use crate::platform::proc::EnvRead;
    writers.iter().find_map(|pid| {
        match crate::platform::proc::proc_env_var(*pid, crate::agents::asks::ANSWER_HOOK_OFF_ENV) {
            EnvRead::Value(v) => Some(crate::agents::asks::hook_off_words(&v)),
            EnvRead::Unset => {
                let off = if plugin_lost(*pid) {
                    Some(HookOff::EnvLost)
                } else if started_elsewhere(*pid, agent) {
                    Some(HookOff::Elsewhere)
                } else {
                    None
                };
                off.map(|o| crate::agents::asks::hook_off_words(o.env_value()))
            }
            EnvRead::Unreadable => None,
        }
    })
}

/// ccm 给这个进程留的便条说答题插件经环境交下去了，它的环境里却没有我们那个目录（启动器把环境清掉了）。
/// 持着会话的这个进程是不是别处起的（判法住 `agents::asks::started_elsewhere`；这里只把那几样从进程身上读出来）。
fn started_elsewhere(pid: u32, agent: Option<&str>) -> bool {
    let has_ask_face = agent.and_then(crate::agents::ask_face).is_some();
    let noted = crate::platform::paths::data_home().is_some_and(|data| {
        let started = crate::platform::proc::start_epoch_from_ticks(
            crate::platform::proc::proc_starttime(pid),
        );
        crate::control::launch_account::note_of(&data, pid, started).is_some()
    });
    let plugin_here = crate::platform::paths::home_dir().is_some_and(|home| {
        let ours = crate::agents::asks::plugin_dir_under(&home.to_string_lossy());
        crate::agents::plugin_envs().into_iter().any(|var| {
            matches!(
                crate::platform::proc::proc_env_var(pid, var),
                crate::platform::proc::EnvRead::Value(v) if crate::agents::asks::dirs_have(&v, &ours)
            )
        })
    });
    crate::agents::asks::started_elsewhere(
        has_ask_face,
        crate::platform::proc::proc_cmdline(pid).as_deref(),
        plugin_here,
        noted,
    )
}

fn plugin_lost(pid: u32) -> bool {
    let (Some(data), Some(home)) = (
        crate::platform::paths::data_home(),
        crate::platform::paths::home_dir(),
    ) else {
        return false;
    };
    let started =
        crate::platform::proc::start_epoch_from_ticks(crate::platform::proc::proc_starttime(pid));
    if !crate::control::launch_account::note_of(&data, pid, started).is_some_and(|n| n.plugin) {
        return false;
    }
    let ours = crate::agents::asks::plugin_dir_under(&home.to_string_lossy());
    crate::agents::plugin_envs().into_iter().all(|var| {
        !matches!(
            crate::platform::proc::proc_env_var(pid, var),
            crate::platform::proc::EnvRead::Value(v) if crate::agents::asks::dirs_have(&v, &ours)
        )
    })
}
