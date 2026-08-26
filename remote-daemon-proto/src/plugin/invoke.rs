//! ③ **传 argv 起它** + ④ 的**骨架** —— 本层**唯一一处**起进程。
//!
//! # ★★ 期限住在**子进程**里，不在宿主里
//!
//! 病先说清楚：`Command::output()` **无限等**。被调的那个程序卡住（等锁、等网络、
//! 自己写了个死循环），宿主这边就一直挂着 —— 而阻塞档的调用会占死一个 worker，
//! `cancel` 对它是**空操作**。
//!
//! 而「在宿主里等一个期限」这条路是**被封死的**：宿主侧一个计时器都不许有
//!（先 `try_wait` 轮询、后用带期限的接收，两版都被零定时器护栏当场逮住，而它是对的）。
//!
//! ⇒ 正确形状是**让子进程自己有期限**：找得到 `timeout(1)` 就拿它当 argv 前缀，
//! 宿主这边仍然只是老老实实 `wait` 一个**注定会退出**的子进程 —— 零计时器。
//!
//! ⚠ 找不到 `timeout(1)` 就**如实降级**：裸跑、没有期限，不假装有保障。
//! ★ 这句话此前只是一句头注（删了不会红）。现在它由 [`argv_for`] 的纯函数判据钉着：
//! 没有 `timeout` 时 argv 的第一个词必须是那个插件自己，`deadline_secs` 一个字都不许出现。
//!
//! # ④ 为什么只抽骨架
//!
//! 两个真实实现的退出码**语义互斥**（同一个码在一侧是「路由层拒绝」、在另一侧是
//! 「撞名、该重试」）⇒ **码 → 语义的映射一定是每插件一份**。
//! 本层只提供：怎么拿到码 · 怎么认出「被信号打断」（`code == None`）·
//! `timeout(1)` 超时时用的那个码 · 怎么从两条流里摘一行诊断。
//!
//! ★ **这一段此前只是散文（删了不会红）**。D1（08-26）的硬读数：把 `control/cc_bus.rs`
//! 的码表原样抬进本文件、**只擦掉插件的名字** ⇒ `436 passed; 0 failed`，**一条不红**。
//! ⇒ 现在它由 `plugin::layer_guard` 里形状那一族钉着（两条，见 `plugin` 模块头注那张表）：
//! 抬一张写整数字面量的表上来 ⇒ `the_generic_port_does_not_translate_exit_codes` 红；
//! 改用具名 `i32` 常量绕过去 ⇒ `the_only_exit_code_constants_here_are_the_registered_generic_ones` 红。
//! ⚠ 两条各自**认不出**什么（具名常量来自本层与 control/observe 之外 · 非 `i32` 的码 ·
//! 运行期从数据里读的表 · `mod.rs` 本身不在扫描面），逐条写在它们自己的头注里 —— 别读成全覆盖。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// `timeout` 那条命令超时时的退出码（GNU coreutils）。
///
/// ⚠ 引用它的文案里**别把它写成命令名紧跟左括号**的形状 —— 零定时器护栏按调用形态扫，
/// 它剥注释但**不剥字符串**，写在错误消息里会被当成一处定时器调用。
pub(crate) const TIMED_OUT_CODE: i32 = 124;

/// **根本没跑起来**的那一类 —— 与「跑了、退了、码是几」完全不同层。
pub(crate) enum NotRun {
    /// 参数塞不进一次命令调用（内核的单参数上限）。
    ///
    /// ★ 它必须与兜底桶分开：调用方要分得出「我给的东西太大」（自己能修）
    /// 与「那个程序坏了」（自己修不了）。合成一个码就等于**归错因**。
    ArgListTooLong,
    /// 其余起不来的原因（不存在 / 没权限 / 别的 IO 错），原文带上。
    Failed(String),
}

/// 跑完了：拿到码与两条流。**语义不在这里**。
pub(crate) struct Done {
    /// 退出码；`None` = **被信号打断**（两件不同的事，别合并）。
    pub(crate) code: Option<i32>,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

impl Done {
    /// 子进程是被那条期限命令收掉的吗。
    pub(crate) fn timed_out(&self) -> bool {
        self.code == Some(TIMED_OUT_CODE)
    }

    /// 摘一行诊断：**stderr 优先**，空了才退回 stdout。
    pub(crate) fn diagnosis(&self) -> String {
        let e = first_line(&self.stderr);
        if e.is_empty() {
            first_line(&self.stdout)
        } else {
            e
        }
    }
}

/// 取第一行非空内容（拿来当诊断）。
pub(crate) fn first_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .to_string()
}

/// ★ 真正要起的 `(程序, argv)` —— **纯函数**，好测。
///
/// 有期限命令：`<期限命令> <秒数> <插件> <参数…>`；
/// 没有：`<插件> <参数…>`（**裸跑，没有期限** —— 这条降级由判据钉着）。
pub(crate) fn argv_for(
    bin: &Path,
    args: &[&str],
    deadline_secs: u64,
    deadline_bin: Option<&Path>,
) -> (PathBuf, Vec<String>) {
    let (prog, mut argv): (PathBuf, Vec<String>) = match deadline_bin {
        Some(t) => (
            t.to_path_buf(),
            vec![deadline_secs.to_string(), bin.display().to_string()],
        ),
        None => (bin.to_path_buf(), Vec::new()),
    };
    argv.extend(args.iter().map(|a| (*a).to_string()));
    (prog, argv)
}

/// 在 `PATH` 上找那条期限命令。找不到就是 `None`（调用方会裸跑）。
fn deadline_bin() -> Option<PathBuf> {
    super::discover::on_path("timeout")
}

/// ★ **本层唯一一处起进程**（已登记进 `readonly_guard::spawn_registry::ALLOWED`）。
///
/// 入参：`bin` 是已经找到的那个可执行文件（[`super::discover::find`] 的产出）；
/// `args` **直传，不过 shell** ⇒ 参数里的元字符不构成注入面；
/// `deadline_secs` 是给**子进程**的期限（`u64`，不是时长类型 —— 见模块头注第二段）；
/// `env` 是额外的环境变量（有些插件的契约走 env 而不是参数）。
///
/// `stdin` 一律关掉：被调的程序不该从宿主的输入里读东西。
pub(crate) fn run(
    bin: &Path,
    args: &[&str],
    deadline_secs: u64,
    env: &[(&str, &str)],
) -> Result<Done, NotRun> {
    let (prog, argv) = argv_for(bin, args, deadline_secs, deadline_bin().as_deref());
    let mut cmd = Command::new(&prog);
    cmd.args(&argv).stdin(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    match cmd.output() {
        Ok(out) => Ok(Done {
            code: out.status.code(),
            stdout: out.stdout,
            stderr: out.stderr,
        }),
        Err(e) => {
            // ★ 参数太长要单独说：实测 200KB 必炸、120KB 能过 —— 内核的单参数上限是 128 KiB。
            #[cfg(unix)]
            if e.raw_os_error() == Some(libc::E2BIG) {
                return Err(NotRun::ArgListTooLong);
            }
            Err(NotRun::Failed(format!("起不来 `{}`：{e}", bin.display())))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★ 有期限命令时：它当**前缀**，秒数紧随其后，插件与参数原样跟在后面。
    #[test]
    fn the_deadline_command_becomes_the_prefix() {
        let t = PathBuf::from("/usr/bin/timeout");
        let bin = PathBuf::from("/opt/p/tool");
        let (prog, argv) = argv_for(&bin, &["--", "a b", "c"], 7, Some(&t));
        assert_eq!(prog, t, "程序应当是那条期限命令");
        assert_eq!(
            argv,
            vec![
                "7".to_string(),
                "/opt/p/tool".to_string(),
                "--".to_string(),
                "a b".to_string(),
                "c".to_string()
            ],
            "秒数没排在插件前面，或参数被改了形状"
        );
    }

    /// ★★ **降级那条路**：找不到期限命令就裸跑 —— 这一格此前只有一句头注。
    ///
    /// 钉两件事：① 起的就是插件自己；② 秒数**一个字都不许出现在 argv 里**
    ///（不然会变成插件自己的第一个参数，那是货真价实的错传）。
    #[test]
    fn without_a_deadline_command_it_runs_bare_and_says_so_in_the_argv() {
        let bin = PathBuf::from("/opt/p/tool");
        let (prog, argv) = argv_for(&bin, &["x"], 7, None);
        assert_eq!(prog, bin, "裸跑时起的应当是插件自己");
        assert_eq!(argv, vec!["x".to_string()], "裸跑时不该有任何前缀参数");
        assert!(
            !argv.contains(&"7".to_string()),
            "秒数漏进了插件的 argv —— 那会被它当成一个真参数：{argv:?}"
        );
    }

    /// 没有参数的调用，两种形态都不该多出空串。
    #[test]
    fn an_empty_arg_list_stays_empty() {
        let bin = PathBuf::from("/opt/p/tool");
        let (_, bare) = argv_for(&bin, &[], 3, None);
        assert!(bare.is_empty(), "{bare:?}");
        let t = PathBuf::from("/usr/bin/timeout");
        let (_, pre) = argv_for(&bin, &[], 3, Some(&t));
        assert_eq!(pre, vec!["3".to_string(), "/opt/p/tool".to_string()]);
    }

    /// 「被信号打断」与「退出码是几」是**两件事**，骨架必须分得开。
    #[test]
    fn a_signal_death_is_not_an_exit_code() {
        let killed = Done {
            code: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        assert!(!killed.timed_out(), "码是 None 不该被算成超时");
        let expired = Done {
            code: Some(TIMED_OUT_CODE),
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        assert!(expired.timed_out());
        let ok = Done {
            code: Some(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        assert!(!ok.timed_out());
    }

    /// 诊断取法：**stderr 优先**，空了才退回 stdout；两条都空就是空串。
    #[test]
    fn the_diagnosis_prefers_stderr_and_falls_back_to_stdout() {
        let d = Done {
            code: Some(1),
            stdout: b"out-1\nout-2\n".to_vec(),
            stderr: b"\n  \nerr-1\nerr-2\n".to_vec(),
        };
        assert_eq!(d.diagnosis(), "err-1", "stderr 里的第一行非空内容没被取到");
        let d2 = Done {
            code: Some(1),
            stdout: b"out-1\n".to_vec(),
            stderr: b"   \n".to_vec(),
        };
        assert_eq!(d2.diagnosis(), "out-1", "stderr 全空白时没退回 stdout");
        let d3 = Done {
            code: Some(1),
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        assert_eq!(d3.diagnosis(), "");
    }

    /// 非 UTF-8 的输出不许让取诊断这一步炸掉。
    #[test]
    fn invalid_utf8_output_still_yields_a_line() {
        assert_eq!(first_line(&[0xff, 0xfe, b'\n', b'x']), "\u{fffd}\u{fffd}");
    }
}
