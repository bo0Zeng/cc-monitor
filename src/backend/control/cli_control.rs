//! 控制面的 CLI 入口：与 SSH 帧入口共用 [`crate::stream::inbound::REGISTRY`] 里同一个 `run`。给本机脚本与 skill 集成用。
//!
//! # 它不实现任何一条命令
//!
//! 把 `--<name>` 还原成 `<name>`，去 `REGISTRY` 查那条 `CommandSpec`，跑它自己的 `run` —— 两个入口在类型上落到同一个闭包。
//! 由此：`readonly_guard::spawn_registry` 的起进程点一处不增（本模块零 `Child::new`）；
//! `launch.rs` 那条「argv 直传，不过 shell」原样继承 —— CLI 面收 stdin JSON，不把参数摊进 argv。
//!
//! # §安全边界：理由与帧入口不是同一条
//!
//! 本入口的调用方是本机任意进程。理由是：本机进程本来就能直接跑 `tmux kill-session` / `tmux new-session`
//! （同一个 tmux server、同一个 `$TMUX_TMPDIR` 下的 socket，权限由文件系统把守）⇒ 本入口不新授任何权力。
//! 这条理由只覆盖「起 / 杀 tmux 会话」这一族；将来若有一条命令能做本机进程原本做不到的事，必须单独立一条理由。
//!
//! # 信封（与 `--resolve` 同形）
//!
//! · 入：那条命令的 `args`（一段 JSON；空 = `{}`），两个口**二选一**，都是一处读（[`read_args`]），每条子命令自动都有：
//!   - stdin：默认读到 EOF；带 [`STDIN_LINE_FLAG`]（任意位置）⇒ 只读一行（给 stdin 关不掉的调用方：远端命令经 capture 那一跳交载荷，
//!     capture 不关远端 stdin）。stdin 开着却 [`STDIN_QUIET`] 内一个字节都没来 ⇒ `no_input`，不挂住。
//!   - argv：[`crate::ARGS_B64_FLAG`] `<base64 的 JSON>`（任意位置；给写不了 stdin 的调用方：第二个前端的执行通道只有 stdout）。
//!     给了它就**不碰 stdin**；与 [`STDIN_LINE_FLAG`] 一起给 ⇒ `bad_args`。
//!   上限：stdin [`MAX_CLI_STDIN`]；argv [`MAX_ARGS_B64_LEN`]（编码后，留在系统单个参数的上限之内）。超了 `args_too_large`，不截断。
//! · 出：stdout 一行紧凑 JSON（命令没有返回值时是 `{}`），exit 0。例外是 [`crate::TEXT_FLAG`]：只给 `quota-read`，
//!   同一份回包排成给人看的字（`control/quota_text.rs`）；别的命令带它 ⇒ `bad_args`。
//! · 期限：[`crate::WITHIN_MS_FLAG`] `<毫秒>`（任意位置）与帧面请求信封的 `within_ms` 同名同义，到点回的是同一个码。
//! · 错：exit 2 + stderr 一行 `{code, message, detail, data?}` —— 与帧面失败应答同一份（[`Failed`]）；带 [`crate::TEXT_FLAG`] ⇒ 那一句 ＋ 复制详情。
//! · exec 模型：1 exec = 1 请求 1 响应 1 退出，无 request-id。

use crate::common::contract;
use crate::stream::detail::Failed;
use crate::stream::inbound::spec::Fail;
use crate::stream::inbound::{CommandSpec, Run, REGISTRY};
use crate::stream::wire::Request;
use std::io::{Read, Write};
use std::time::Duration;

/// stdin 上限（兜 DoS，不是兜格式；同 `resolve_query::MAX_RESOLVE_STDIN`）。超限是拒收，不是截断：
/// 截半的 JSON 会报成「解析失败」，真实原因却是「太大了」⇒ 多读一个字节，超了就说超了。
pub(crate) const MAX_CLI_STDIN: u64 = 1024 * 1024;

/// 能力探测口。集成方按能力兼容，不按版本号；出 JSON（不是 `ccm --ccm-probe` 那种 `key=value` 行：那是 ccm 专用的方言）。
pub(crate) const PROBE_FLAG: &str = "--backend-probe";

/// 「只读一行 stdin」那个修饰词住 [`crate::STDIN_LINE_FLAG`]（argv 三分表那一家；理由见那里的头注）。
pub(crate) use crate::STDIN_LINE_FLAG;

/// argv 形载荷口 [`crate::ARGS_B64_FLAG`] 的值（base64 编码后）的上限。
///
/// 系统先卡一道：Linux 单个参数 ≤ `MAX_ARG_STRLEN`（32 页 ＝ 131072 字节，含结尾 NUL）；经 ssh 时整行命令是登录 shell `-c` 的
/// **一个**参数 ⇒ 同一个上限管整行（载荷之外还有后端路径与几个旗标）。Windows 一整行命令 ≤ 32767 个字符，更紧。
/// 超过系统那一道，本后端根本起不来（exec 回 E2BIG，调用方看到的是 shell / sshd 那一层的错），轮不到这里回码 ——
/// 所以这道放在系统那一道**之内**：系统放得进来、这里嫌大的，回 `args_too_large`。4 的倍数：刚好到上限的合法 base64 不被拒。
/// 要交更大的载荷走 stdin（[`MAX_CLI_STDIN`]）。
pub(crate) const MAX_ARGS_B64_LEN: usize = 131_068;

/// stdin 开着、却这么久一个字节都没来 ⇒ 当「没给入参」立即回 `no_input`，不陪着挂。
///
/// 为什么不能读到 EOF 才算：有的调用方关不掉 stdin（capture 那一跳 · 第二个前端的执行通道），读到 EOF 就是永远挂住。
/// 为什么 1 s 够：载荷是起进程的同一刻写进管道的（本机 `printf … |`；远端经 ssh 时载荷跟着 exec 请求走同一条连接，
/// 远端登录 shell 还没起完它就已经在管道里）。第一个字节到了之后照旧读到 EOF / 换行，不再计时。
pub(crate) const STDIN_QUIET: Duration = Duration::from_millis(1000);

/// 入参从哪个口来：argv 上的修饰词决定，**位置无关**（同 [`crate::TEXT_FLAG`]）。
#[derive(Debug, PartialEq, Eq)]
enum Source<'a> {
    Stdin { one_line: bool },
    Argv(&'a str),
}

fn bad_args(why: &str) -> (&'static str, String) {
    ("bad_args", contract::malformed(why))
}

/// 读 `opts`（子命令之后的那些词）认出入参走哪个口。用法错 ⇒ `bad_args`。
fn source_of(opts: &[String]) -> Result<Source<'_>, (&'static str, String)> {
    let one_line = opts.iter().any(|a| a == STDIN_LINE_FLAG);
    let mut b64: Option<&str> = None;
    let mut i = 0;
    while i < opts.len() {
        if opts[i] == crate::ARGS_B64_FLAG {
            let Some(v) = opts.get(i + 1) else {
                return Err(bad_args(&format!("{} needs a value", crate::ARGS_B64_FLAG)));
            };
            if b64.replace(v.as_str()).is_some() {
                return Err(bad_args(&format!("{} given twice", crate::ARGS_B64_FLAG)));
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    match b64 {
        Some(_) if one_line => Err(bad_args(&format!(
            "give the args either on stdin ({STDIN_LINE_FLAG}) or in argv ({}), not both",
            crate::ARGS_B64_FLAG
        ))),
        Some(v) => Ok(Source::Argv(v)),
        None => Ok(Source::Stdin { one_line }),
    }
}

/// argv 那一形：base64 → UTF-8 串（JSON 由调用处与 stdin 那一形同一处解）。
fn decode_argv(b64: &str) -> Result<String, (&'static str, String)> {
    if b64.len() > MAX_ARGS_B64_LEN {
        return Err((
            "args_too_large",
            contract::malformed(&format!(
                "{} value over the {MAX_ARGS_B64_LEN}-byte cap, refused (not truncated); send larger args on stdin",
                crate::ARGS_B64_FLAG
            )),
        ));
    }
    let bytes = crate::stream::wire::b64_decode(b64).map_err(|e| ("bad_request", e))?;
    String::from_utf8(bytes).map_err(|e| {
        (
            "bad_request",
            contract::malformed(&format!("{} is not UTF-8: {e}", crate::ARGS_B64_FLAG)),
        )
    })
}

/// stdin 那一形，带静默窗：读端交给一条线程读；[`STDIN_QUIET`]（测试传更短的）内第一个字节（或 EOF）没到 ⇒ `no_input`。
/// 到了之后照旧 [`read_input`]（同一个上限、同一种拒法）。没读完就回码时那条线程留在那儿 —— 一次性进程马上就退。
fn read_stdin_within<R: Read + Send + 'static>(
    r: R,
    one_line: bool,
    quiet: Duration,
) -> Result<String, (&'static str, String)> {
    enum Ev {
        Arrived,
        Done(Result<String, (&'static str, String)>),
    }
    let (tx, rx) = std::sync::mpsc::channel::<Ev>();
    std::thread::spawn(move || {
        let mut br = std::io::BufReader::new(r);
        // `fill_buf` 返回 ＝ 第一个字节到了，或 EOF（空）。读错交给下面那一趟再报。
        let _ = std::io::BufRead::fill_buf(&mut br);
        if tx.send(Ev::Arrived).is_ok() {
            let _ = tx.send(Ev::Done(read_input(br, one_line)));
        }
    });
    let gone = || {
        (
            "stdin_read_failed",
            "read stdin failed: the reader stopped".to_string(),
        )
    };
    match rx.recv_timeout(quiet) {
        Ok(Ev::Arrived) => match rx.recv() {
            Ok(Ev::Done(got)) => got,
            _ => Err(gone()),
        },
        Ok(Ev::Done(got)) => got,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err((
            "no_input",
            contract::malformed(&format!(
                "stdin is open but sent nothing within {} ms; pass the args with {} <base64 JSON>, or close stdin",
                quiet.as_millis(),
                crate::ARGS_B64_FLAG
            )),
        )),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(gone()),
    }
}

/// **读入参的唯一一处**：`opts` 是子命令之后的那些词，`stdin` 是进程的 stdin（测试喂替身）。回那段 JSON 的原文（空 ＝ 没给）。
pub(crate) fn read_args<R: Read + Send + 'static>(
    opts: &[String],
    stdin: R,
    quiet: Duration,
) -> Result<String, (&'static str, String)> {
    match source_of(opts)? {
        Source::Argv(b64) => decode_argv(b64),
        Source::Stdin { one_line } => read_stdin_within(stdin, one_line, quiet),
    }
}

/// 读入参那一段（可喂任意读端 —— 生产交进程的 stdin）。`one_line` ⇒ 读到第一个换行就停，**不再多要一个字节**
/// （调用方的 stdin 可能永远不关）；否则读到 EOF。两形同一个上限、同一种拒法。
pub(crate) fn read_input<R: std::io::BufRead>(
    r: R,
    one_line: bool,
) -> Result<String, (&'static str, String)> {
    let mut buf: Vec<u8> = Vec::new();
    // 多读一个字节，好把「刚好装满」与「超了」分开 —— 只读上限那么多是分不开的。
    let mut capped = r.take(MAX_CLI_STDIN + 1);
    let got = if one_line {
        std::io::BufRead::read_until(&mut capped, b'\n', &mut buf)
    } else {
        capped.read_to_end(&mut buf)
    };
    if let Err(e) = got {
        return Err(("stdin_read_failed", format!("read stdin failed: {e}")));
    }
    if buf.len() as u64 > MAX_CLI_STDIN {
        return Err((
            "args_too_large",
            // 不截断：截半的 JSON 会被报成 bad_request，那句话与真实原因无关。
            contract::malformed(&format!(
                "args JSON over the {MAX_CLI_STDIN}-byte cap, refused (not truncated)"
            )),
        ));
    }
    String::from_utf8(buf).map_err(|e| ("stdin_read_failed", format!("read stdin failed: {e}")))
}

/// 本入口回显给命令的 `id`。**帧面的 `id` 由客户端发号且不透明**，而一次性 exec
/// 天然 1:1、没有并发的第二条请求可混淆 ⇒ 这里给一个固定值，不假装有号段。
const CLI_REQUEST_ID: &str = "cli";

/// 一条命令**上不上 CLI 面** —— 从 `REGISTRY` 派生，不是手抄一张表。
///
/// 判据是 `Run::Builtin`：那种命令的实现住在 `inbound::dispatch` 的硬臂里，
/// 要 `replies` 通道与在飞表才能跑，而**一次性 exec 里两样都不存在**。
///
/// 另一条：派生出来的名字是 ccm 自己的诊断口（`--ccm-print` 这类）⇒ 不上。二进制叫 `ccm` 时
/// 按 `SUBCOMMANDS` 分流（`control::ccm::intercept`），占了 ccm 的词就把 `ccm --ccm-print` 抢进后端。
/// 第三条：[`STREAM_ONLY`] 那几条能跑，但在一次性进程里结构上答不了 ⇒ 不上；[`UI_ONLY`] 那几条答得了、只是除了界面没人用 ⇒ 暂不上。
pub(crate) fn cli_exposed(spec: &CommandSpec) -> bool {
    !matches!(spec.run, Run::Builtin)
        && !crate::control::ccm::argv::is_ccm_word(&flag_of(spec.name))
        && !STREAM_ONLY.contains(&spec.name)
        && !UI_ONLY.contains(&spec.name)
}

/// **一次性进程里结构上答不了**的命令：答出来是假话（读的是常驻进程才有的状态：watcher · 中转监听 · 转发账 · 可达表），
/// 或做的事活不过这个进程（换号重启要拿退出排空的票钉住常驻进程）。逐条理由是数据，住测试那一侧的 `STREAM_ONLY_WHY`（两向相等）。
///
/// 只收这一种理由。「答得了，只是除了界面没人用」是另一种，住 [`UI_ONLY`] —— 两种混在一张表里时，
/// 后一种被读成前一种，第二个前端（只有一次性 CLI 与流两条路进后端）就够不着。
pub(crate) const STREAM_ONLY: &[&str] = &[
    "resync",
    "apikey-routing",
    "relay-optin",
    "launch-local",
    "forward-start",
    "forward-stop",
    "forward-list",
    "ext-list",
    "ext-hub-preview",
    "ext-hub-apply",
    "session-restart",
];

/// **答得出真话、只是除了界面没人用得着**的命令。这是关于「调用方是谁」的产品判断，不是结构限制 ——
/// 有新的调用方（第二个前端）要它，就挪出去。理由不许是「命令行那一侧直接敲 `ccm` 就是它」：那挡的是第二个**入口**，挡不住第二个**前端**。
/// 逐条理由住测试那一侧的 `UI_ONLY_WHY`（两向相等）。
///
/// 2026-10-09 复核（第二个前端的审计 §4）：起新会话框三问 · tab 栏批量停 / 起 / 问样子 · 铸终端名 · 开终端那一串 · 各台搜索结果合并
/// 九条放出（事实全在这台：tmux 名单现探、会话快照问一次重探一次、记录在不在现读、合并是纯计算 ⇒ 一次性进程答的与常驻那一个同样是真话）。
/// 留下的两条是 ↗ 拉前那一问的两半，只有能拉前桌面窗口的那一方用得着，第二个前端自己也说不用。
pub(crate) const UI_ONLY: &[&str] = &["session-terminals", "terminal-processes"];

/// 命令名 → CLI 子命令（`launch` → `--launch`）。
pub(crate) fn flag_of(name: &str) -> String {
    crate::cli_flag(name)
}

/// CLI 子命令 → 它在 `REGISTRY` 上的那条登记。
pub(crate) fn spec_for(flag: &str) -> Option<&'static CommandSpec> {
    REGISTRY
        .iter()
        .find(|s| cli_exposed(s) && flag_of(s.name) == flag)
}

/// 这条命令要不要读 stdin：读 `spec.takes_input`（每条命令自己说，理由在 `CommandSpec::takes_input` 头注）。
/// 无条件读会让 `--ping` 这类探活口在一条不关的 stdin 上永远挂住；拿「有没有字段」代用也不对（`bus-list` 无输入、有输出字段）。
pub(crate) fn reads_stdin(spec: &CommandSpec) -> bool {
    spec.takes_input
}

/// 本入口认不认这个 flag。`main` 的分派臂只问它，**不写命令字面量** ——
/// 于是「帧面加一条命令」不需要回来改 `main`。
pub fn handles(flag: &str) -> bool {
    flag == PROBE_FLAG || spec_for(flag).is_some()
}

/// `control/resident.rs` 那两条子命令也走这一份（不另立第 N 份信封，`readonly_guard::error_envelope_registry`）。
/// `cmd` 进复制详情的「命令」那一项。
pub fn emit_err(cmd: &str, code: &str, message: impl Into<String>) -> i32 {
    let f = Failed::new(Some(cmd), code, message.into(), None, None);
    emit_failed(&mut std::io::stderr(), &f, false)
}

/// 失败那一份投到 CLI 面（[`Failed::emit_to`]：与别的 CLI 出口同一处）。
fn emit_failed(err: &mut dyn Write, f: &Failed, text: bool) -> i32 {
    f.emit_to(err, text)
}

/// 能力探测：`{proto, buildId, commands}`。`commands` 必须派生：手抄一份，探测口就会说谎，而 skill 按它的话决定走不走新路。
fn probe() -> i32 {
    let commands: Vec<String> = REGISTRY
        .iter()
        .filter(|s| cli_exposed(s))
        .map(|s| flag_of(s.name))
        .collect();
    let body = serde_json::json!({
        "proto": crate::PROTO_VERSION,
        "buildId": crate::BUILD_ID,
        "commands": commands,
    });
    println!("{body}");
    0
}

/// CLI 控制面的一次性入口。返回进程退出码。
pub async fn run(args: &[String]) -> i32 {
    run_io(
        args,
        std::io::stdin(),
        STDIN_QUIET,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
    .await
}

/// [`run`] 的本体：stdin / stdout / stderr 是入参（测试喂替身，逐字比两个口的应答）。
pub(crate) async fn run_io<R: Read + Send + 'static>(
    args: &[String],
    stdin: R,
    quiet: Duration,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let flag = args.first().map(String::as_str).unwrap_or_default();
    if flag == PROBE_FLAG {
        return probe();
    }
    let opts = &args[1..];
    // 「给人看」那一形：成功只给 `quota-read`（`control/quota_text.rs`）；失败那一形不分命令（那一句 ＋ 复制详情）。
    let text = opts.iter().any(|a| a == crate::TEXT_FLAG);
    let Some(spec) = spec_for(flag) else {
        // 走不到（`main` 只把已知 flag 派到这里），但**不许 panic**：
        // backend 的一次性模式对未知参数的既定行为是 exit 2 + 结构化 stderr。
        let f = Failed::new(
            None,
            "unknown_command",
            contract::malformed(&format!("unknown CLI flag {flag}")),
            None,
            None,
        );
        return emit_failed(err, &f, text);
    };
    let refuse = |err: &mut dyn Write, code: &str, message: String| {
        emit_failed(
            err,
            &Failed::new(Some(spec.name), code, message, None, None),
            text,
        )
    };
    if text && spec.name != "quota-read" {
        return refuse(
            err,
            "bad_args",
            copy_core::copy_text("acct.text.onlyQuota", &[]),
        );
    }
    let within_ms = match within_of(opts) {
        Ok(ms) => ms,
        Err((code, message)) => return refuse(err, code, message),
    };
    let mut input = String::new();
    if reads_stdin(spec) {
        match read_args(opts, stdin, quiet) {
            Ok(s) => input = s,
            Err((code, message)) => return refuse(err, code, message),
        }
    } else if opts.iter().any(|a| a == crate::ARGS_B64_FLAG) {
        // 不收入参的命令带 argv 载荷：用法错，不悄悄忽略（同 `--text`）。
        return refuse(
            err,
            "bad_args",
            contract::malformed(&format!("{flag} takes no args")),
        );
    }
    let trimmed = input.trim();
    let cli_args: serde_json::Value = if trimmed.is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => return refuse(err, "bad_request", format!("args JSON parse failed: {e}")),
        }
    };
    let req = Request {
        id: CLI_REQUEST_ID.to_string(),
        cmd: spec.name.to_string(),
        args: cli_args.clone(),
        within_ms,
        // 与帧面同一处换算：发起方期限从收到起算、减余量换成截止时刻。
        until: crate::stream::inbound::until_of(within_ms),
    };
    // 总期限与帧面同一处装：登记了上限的（都在阻塞档）按上限与发起方期限里早的那个装；别的命令不装（`None`）。
    let total = crate::stream::inbound::install_total(&req);
    // 这几行是本模块的全部：派发落到 `REGISTRY` 自己的 `run`。
    let outcome: Result<Option<serde_json::Value>, Fail> = match spec.run {
        Run::Blocking(f) => f(req).map_err(Fail::from),
        Run::BlockingData(f) => f(req),
        Run::Async(f) => f(req).await.map_err(Fail::from),
        Run::AsyncData(f) => f(req).await,
        Run::Builtin => {
            return refuse(
                err,
                "not_available_in_cli",
                contract::malformed("this command is only served on the frame channel"),
            )
        }
    };
    drop(total);
    match outcome {
        Ok(v) => {
            let v = v.unwrap_or_else(|| serde_json::json!({}));
            if text {
                let _ = writeln!(out, "{}", crate::control::quota_text::render_here(&v));
            } else {
                let _ = writeln!(out, "{v}");
            }
            0
        }
        // 失败那一份与帧面同一处出（「码 → 句」表 · 复制详情 · 按码定形的 `data`）。
        Err(f) => emit_failed(err, &f.settle(spec.name, &cli_args), text),
    }
}

/// 期限口 [`crate::WITHIN_MS_FLAG`]（位置不限）：缺值 · 给两次 ⇒ `bad_args`（同 [`crate::ARGS_B64_FLAG`]）；
/// 值的读法与帧面信封那一格同一处（[`crate::stream::wire::within_ms_of`]：不是正整数 ⇒ 当没带）。
fn within_of(opts: &[String]) -> Result<Option<u64>, (&'static str, String)> {
    let mut seen: Option<Option<u64>> = None;
    let mut i = 0;
    while i < opts.len() {
        if opts[i] == crate::WITHIN_MS_FLAG {
            let Some(v) = opts.get(i + 1).filter(|v| !v.starts_with("--")) else {
                return Err(bad_args(&format!(
                    "{} needs a value",
                    crate::WITHIN_MS_FLAG
                )));
            };
            let ms = serde_json::from_str::<serde_json::Value>(v)
                .ok()
                .and_then(|v| crate::stream::wire::within_ms_of(&v));
            if seen.replace(ms).is_some() {
                return Err(bad_args(&format!("{} given twice", crate::WITHIN_MS_FLAG)));
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    Ok(seen.flatten())
}

#[cfg(test)]
#[path = "../../../tests/backend/control/cli_control_tests.rs"]
mod tests;
