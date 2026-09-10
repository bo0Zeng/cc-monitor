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

/// ★★ 起插件时**从宿主环境继承过去的键** —— 白名单，**一个具名常量、一个家**。
///
/// # 病：这一层此前不清环境，于是子进程拿到的是 daemon 的**整份**环境
///
/// 那不是一个抽象的风险：常驻监听口的地址与令牌（`listen::ENV_PORT` /
/// `listen::ENV_TOKEN`）是 daemon **自己从环境读**的，也就是说它们一定在 daemon 的环境里。
/// ⇒ 任何被这一处口起出来的插件，读一读自己的环境就能接上宿主、过鉴权、发全部基础命令，
/// 而这条回程**没有协议、没有权限模型、没有审计**。
/// 设计文档里没有它，代码里也没写它 —— 它是**默认行为**长出来的（`K-R26`）。
///
/// ⇒ 修法是 [`run`] 先 `env_clear()`，再**只**按本表喂。
/// 关键在方向：不在表里的键**按构造**到不了子进程，不需要有人去维护一张「别漏这个」的黑名单。
///
/// # 每个键一句为什么（**给不出论据的不进** —— 下面「考虑过而没进」那一段逐条记着）
///
/// | 键 | 为什么非它不可 |
/// |---|---|
/// | `PATH` | 被起的那个东西自己还要去找别的命令。`e2e/daemon-cc-bus.sh` 的 `[10]` 逐字写着「`PATH` 清空是不行的 —— 那样它自己的 `awk` 也没了」；而且本层 [`super::discover`] 的兜底档本来就按 `PATH` 找插件本体 ⇒ 找得到却跑不动是自相矛盾 |
/// | `HOME` | 插件按 `~` 定位自己的东西（装机位 · 数据目录）。同一套 e2e 的 `[6]` 正是靠换 `HOME` 造出「没装」那台机器 ⇒ 这个键决定它去哪儿找自己 |
/// | `TMPDIR` | 子进程写临时文件的落点。它与下面那个是同一形状：**这一族键的作用是把子进程的写面圈小**，清掉它写面**反而变大**（落回全局临时目录） |
/// | `TMUX_TMPDIR` | 唯一决定裸调的终端复用命令连到**哪一台服务端**的键。清掉它 ⇒ 子进程回落到默认套接字，也就是**用户自己正在用的那一台** ⇒ 清掉它不是更安全，是更危险 |
/// | `LC_ALL` · `LC_CTYPE` · `LANG` | **三个一起进，因为它们决定的是同一档事**（优先级 `LC_ALL` > `LC_CTYPE` > `LANG`）：子进程的字符编码。宿主这边是按 UTF-8 把它的输出读回来的（[`first_line`] · [`super::probe`]），只带其中一部分会给子进程一个**与宿主不同**的 locale —— 而那正是本仓 `common::tmux_utf8` 那个口径当初立起来的原因 |
///
/// # 考虑过而**没进**的（写下来，免得下一个人以为是漏了）
///
/// ⚠ **下面三条的分母只有一个**（说清楚，别读成全称）：今天经本函数起东西的**生产调用方
/// 只有 `control/` 下那一个适配层**，而「被调方读了哪些环境变量」是拿它那一族脚本量的
/// （`shared/` 下那一个目录）。⇒ 「一处都没读」= **在我量过的这一族里没有**，
/// 不是「所有插件都不读」。将来第二族插件接进来时，这三条都要重问一遍。
///
/// - `TMUX` / `TMUX_PANE`：它们说的是「宿主此刻在哪个窗格里」。我量过的那一族里
///   **没有一处**要子进程知道这件事；反过来，`e2e/daemon-cc-bus.sh` 的 `[13]` 逐字要把这两个键
///   **摘掉**才测得出正确行为（不摘的话，被起的那个东西会把**宿主的**窗格身份当成发信人）
///   ⇒ 给不出论据，不进。
/// - `TZ`：说得出的用途只有「时间戳两边同一个时区」，而今天经这一处口起出来的东西
///   **没有一条**把时间戳交回宿主 ⇒ 论据是想出来的，不是量出来的，不进。
/// - `USER` / `LOGNAME` / `SHELL` / `TERM`：现打，我量过的那一族脚本里**一处都没读**它们；
///   `stdin` 本来就是关掉的，`TERM` 更没有意义。
///
/// # ⚠ 诚实边界（删了不会红的话，写在这里）
///
/// - 本表管的是**继承**这一侧。调用方经 [`run`] 的 `env` 入参**显式**交办的那几项照旧生效 ——
///   那不是继承，是交办，两件事刻意分开。
/// - 🔴 **它会关掉一些今天靠继承活着的东西**：把某个插件自己的配置键从宿主环境
///   「顺下去」这条路，从此不通。正解是**让调用方显式喂**（`env` 入参），
///   不是把那个键加进本表 —— 加进来就等于让这一层认识一个具体插件，
///   而那正是 `plugin::layer_guard` 那几条判据在拦的事。
/// - 本表**不是**一道安全边界的全部：子进程仍然有 `PATH`、有 `HOME`，
///   它能读的东西与用户自己在终端里敲同一条命令一样多。本表关掉的是
///   「**宿主进程内部的秘密**顺着环境漏出去」这一条，不多也不少。
pub(crate) const INHERITED_ENV_KEYS: &[(&str, &str)] = &[
    (
        "PATH",
        "被起的那个东西还要去找别的命令；本层找插件本体也走它",
    ),
    ("HOME", "插件按 ~ 定位自己的装机位与数据目录"),
    ("TMPDIR", "子进程写临时文件的落点；清掉它写面反而变大"),
    (
        "TMUX_TMPDIR",
        "决定裸调的复用命令连到哪一台服务端；清掉会回落到用户那一台",
    ),
    ("LC_ALL", "字符编码那一档，三个键一起进（这个优先级最高）"),
    ("LC_CTYPE", "字符编码那一档，三个键一起进"),
    ("LANG", "字符编码那一档，三个键一起进（这个优先级最低）"),
];

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
///
/// ★ **环境是白名单，不是继承**（`K-R26`）：先 `env_clear()`，再按
/// [`INHERITED_ENV_KEYS`] 逐键喂，最后才轮到调用方**显式**交办的 `env`。
/// 次序是承重的 —— 调用方那一趟排在后面，它盖得住白名单里的同名键（显式压过继承）。
pub(crate) fn run(
    bin: &Path,
    args: &[&str],
    deadline_secs: u64,
    env: &[(&str, &str)],
) -> Result<Done, NotRun> {
    let (prog, argv) = argv_for(bin, args, deadline_secs, deadline_bin().as_deref());
    let mut cmd = Command::new(&prog);
    cmd.args(&argv).stdin(Stdio::null());
    // ★ 先清空：不在白名单里的键**按构造**到不了子进程。
    cmd.env_clear();
    for (k, _why) in INHERITED_ENV_KEYS {
        // 宿主没有这个键 ⇒ 子进程也不该有一个空的它（`""` 与「没有」不是同一件事：
        // 空的 `PATH` 会让子进程在**当前目录**里找命令，那比没有更糟）。
        if let Some(v) = std::env::var_os(k) {
            cmd.env(k, v);
        }
    }
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
