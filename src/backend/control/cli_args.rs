//! CLI 面**读入参的唯一一处**：派生子命令（`cli_control`）与 `--resolve`（`resolve_query`，与仓外 aterm 冻结的那一条）都经 [`read_args`] 读。
//!
//! 两个口**二选一**，修饰词都认任意位置：
//! - stdin：默认读到 EOF；带 [`STDIN_LINE_FLAG`] ⇒ 只读一行（给 stdin 关不掉的调用方）。开着却 [`STDIN_QUIET`] 内一个字节都没来 ⇒ `no_input`。
//! - argv：[`crate::ARGS_B64_FLAG`] `<base64 的 JSON>`（给写不了 stdin 的调用方）。给了它就不碰 stdin；与 [`STDIN_LINE_FLAG`] 一起给 ⇒ `bad_args`。
//!
//! 上限：stdin [`MAX_CLI_STDIN`]；argv [`MAX_ARGS_B64_LEN`]。超了 `args_too_large`，不截断。会回的码全集是 [`READ_ARGS_CODES`]。
//!
//! 单独住一个文件：`resolve_query` 引它，不引 `cli_control`（那边挂着整张命令表，文件级引用图上会把 tmux 带进流上的 `resolve`）。

use crate::common::contract;
use std::io::Read;
use std::time::Duration;

/// stdin 上限（兜 DoS，不是兜格式；`--resolve` 也是这一道）。超限是拒收，不是截断：
/// 截半的 JSON 会报成「解析失败」，真实原因却是「太大了」⇒ 多读一个字节，超了就说超了。
pub(crate) const MAX_CLI_STDIN: u64 = 1024 * 1024;

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

/// [`read_args`] 会回的码，一个不多一个不少（`cli_args_tests` 对着本文件的生产源码两向钉住）。
/// 别的入口借这一处读入参时（`--resolve`）拿它算自己的码全集。读它的只有判据（测试档）。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const READ_ARGS_CODES: &[&str] = &[
    "bad_args",
    "bad_request",
    "args_too_large",
    "no_input",
    "stdin_read_failed",
];

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

#[cfg(test)]
#[path = "../../../tests/backend/control/cli_args_tests.rs"]
mod tests;
