//! ③ 传 argv 起它 + ④ 的骨架 —— 本层唯一一处起进程。
//!
//! # 期限
//!
//! 被调的那个程序卡住（等锁、等网络、死循环），阻塞档的调用会占死一个 worker、`cancel` 对它是空操作
//! ⇒ 经起子进程原语带期限跑（`deadline_secs` 由调用方给）：到点杀整组，交成 [`TIMED_OUT_CODE`]。不依赖 PATH 上的 `timeout(1)`，Windows 上也有期限。
//!
//! # ④ 只抽骨架
//!
//! 两个真实实现的退出码语义互斥（同一个码在一侧是「路由层拒绝」、在另一侧是「撞名、该重试」）⇒ 码 → 语义的映射每插件一份。
//! 本层只提供：怎么拿到码 · 怎么认出「被信号打断」（`code == None`）· 过了期限时交成的那个码 · 怎么从两条流里摘一行诊断。
//! 由 `plugin::layer_guard` 形状那一族钉着：抬一张写整数字面量的码表上来 ⇒ `the_generic_port_does_not_translate_exit_codes` 红；
//! 改用具名 `i32` 常量绕过去 ⇒ `the_only_exit_code_constants_here_are_the_registered_generic_ones` 红（两条各自认不出什么，写在它们自己的头注里）。

use crate::platform::child::{Child, ChildFail, Deadline};
use copy_core::copy_text;
use std::path::Path;

/// 过了期限（原语杀了整组）时交成的退出码：与 GNU `timeout` 超时的码相同，调用方的码表据此认「超时」。
///
/// ⚠ 引用它的文案里**别把它写成命令名紧跟左括号**的形状 —— 零定时器护栏按调用形态扫，
/// 它剥注释但**不剥字符串**，写在错误消息里会被当成一处定时器调用。
pub(crate) const TIMED_OUT_CODE: i32 = 124;

/// 起插件时从宿主环境继承过去的键 —— 白名单，一个具名常量、一个家。
///
/// 常驻监听口的地址与钥匙文件路径（`listen::ENV_PORT` / `listen::ENV_TOKEN_FILE`）在后端的环境里；子进程若拿到整份环境，读一读自己的环境
/// 就能接上宿主、过鉴权、发全部基础命令。⇒ [`run`] 清空环境、只按本表从宿主继承（原语的 `inherit_only`）：
/// 不在表里的键按构造到不了子进程，不靠一张「别漏这个」的黑名单。
///
/// | 键 | 为什么非它不可 |
/// |---|---|
/// | `PATH` | 被起的那个东西自己还要去找别的命令（`tests/e2e/backend-cc-bus.sh` 的 `[10]`：清空 `PATH` 它自己的 `awk` 也没了）；本层 [`super::discover`] 的兜底档本来就按 `PATH` 找插件本体 |
/// | `HOME` | 插件按 `~` 定位自己的东西（装机位 · 数据目录）；同一套 e2e 的 `[6]` 正是靠换 `HOME` 造出「没装」那台机器 |
/// | `TMPDIR` | 子进程写临时文件的落点：这一族键的作用是把子进程的写面圈小，清掉它写面反而变大（落回全局临时目录） |
/// | `TMUX_TMPDIR` | 唯一决定裸调的终端复用命令连到哪一台服务端的键。清掉它 ⇒ 子进程回落到默认套接字，也就是用户自己正在用的那一台 |
/// | `LC_ALL` · `LC_CTYPE` · `LANG` | 三个一起进（同一档事，优先级 `LC_ALL` > `LC_CTYPE` > `LANG`）：子进程的字符编码。宿主按 UTF-8 读回它的输出（[`first_line`] · [`super::probe`]），只带一部分会给子进程一个与宿主不同的 locale |
///
/// 考虑过而没进的（依据是今天唯一的生产调用方 —— `control/` 下那一个适配层那一族脚本；第二族插件接进来时要重问一遍）：
/// - `TMUX` / `TMUX_PANE`：「宿主此刻在哪个窗格里」，没有一处要子进程知道；`backend-cc-bus.sh` 的 `[13]` 要把它们摘掉才测得出正确行为
///   （不摘的话，被起的那个东西会把宿主的窗格身份当成发信人）。
/// - `TZ`：经这一处口起出来的东西没有一条把时间戳交回宿主。
/// - `USER` / `LOGNAME` / `SHELL` / `TERM`：那一族脚本里一处都没读；`stdin` 本来就是关掉的。
///
/// 边界：
/// - 本表管继承这一侧；调用方经 [`run`] 的 `env` 入参显式交办的照旧生效（交办与继承刻意分开）。插件自己的配置键要调用方显式喂，
///   不加进本表（加进来就等于让这一层认识一个具体插件，`plugin::layer_guard` 拦的正是这个）。
/// - 本表不是安全边界的全部：子进程仍然有 `PATH`、有 `HOME`，能读的与用户自己在终端里敲同一条命令一样多。
///   它关掉的是「宿主进程内部的秘密顺着环境漏出去」这一条。
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
    /// 其余起不来的原因（不存在 / 没权限 / 别的 IO 错）：句子只带原因词，系统原话另带。
    Failed(copy_core::said::Said),
}

/// 跑完了：拿到码与两条流。**语义不在这里**。
pub(crate) struct Done {
    /// 退出码；`None` = **被信号打断**（两件不同的事，别合并）。
    pub(crate) code: Option<i32>,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    /// 过了期限被收掉的那一次：原语交回的实际等了多久（整秒，向上取整；被命令总期限截短时是截短后的那个数）。没超时 ⇒ `None`。
    pub(crate) waited_secs: Option<u64>,
}

impl Done {
    /// 子进程是过了期限被收掉的吗。
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

/// 本层唯一一处起进程（已登记进 `readonly_guard::spawn_registry::ALLOWED`）的构造：程序 · argv · 环境。
///
/// `bin` 是已经找到的那个可执行文件（[`super::discover::find`] 的产出）；`args` 直传、不过 shell ⇒ 参数里的元字符不构成注入面；
/// `env` 是额外的环境变量（有些插件的契约走 env 而不是参数）。stdin 是空设备。
/// 环境是白名单：清空后只按 [`INHERITED_ENV_KEYS`] 从宿主继承，再轮到调用方显式交办的 `env`（显式压过继承）。
/// 宿主没有的键子进程也没有（空的 `PATH` 会让子进程在当前目录里找命令，比没有更糟）。
pub(crate) fn child_for(bin: &Path, args: &[&str], env: &[(&str, &str)]) -> Child {
    let keys: Vec<&'static str> = INHERITED_ENV_KEYS.iter().map(|(k, _)| *k).collect();
    let mut c = Child::new(bin).args(args).inherit_only(&keys);
    for (k, v) in env {
        c = c.env(k, v);
    }
    c
}

/// 起不来的那一刻 → [`NotRun`]。
fn not_run(bin: &Path, e: ChildFail) -> NotRun {
    // 参数太长要单独说：内核的单参数上限是 128 KiB。
    #[cfg(unix)]
    if let ChildFail::Io(io) = &e {
        if io.raw_os_error() == Some(libc::E2BIG) {
            return NotRun::ArgListTooLong;
        }
    }
    let why = match &e {
        ChildFail::NotFound(io) | ChildFail::Io(io) => copy_core::spawn_reason(io.kind()),
        ChildFail::TimedOut { .. } => copy_core::io_reason(std::io::ErrorKind::TimedOut),
    };
    NotRun::Failed(copy_core::said::Said::with_raw(
        copy_text(
            "beInvoke.notRun.failed",
            &[("bin", &(bin.display()).to_string()), ("why", &why)],
        ),
        &e,
    ))
}

/// 起它、**同步**等它退出（阻塞档的调用方用：它们本来就跑在 `spawn_blocking` 的线程上）。
/// `deadline_secs` 是给它的期限；过了 ⇒ 整组被杀、交成 [`TIMED_OUT_CODE`]（`waited_secs` 是原语说的实际等了多久）。
pub(crate) fn run(
    bin: &Path,
    args: &[&str],
    deadline_secs: u64,
    env: &[(&str, &str)],
) -> Result<Done, NotRun> {
    match child_for(bin, args, env).run(Deadline::secs(deadline_secs)) {
        Ok(out) => Ok(Done {
            code: out.status.code(),
            stdout: out.stdout,
            stderr: out.stderr,
            waited_secs: None,
        }),
        Err(ChildFail::TimedOut { after, .. }) => Ok(Done {
            code: Some(TIMED_OUT_CODE),
            stdout: Vec::new(),
            stderr: Vec::new(),
            waited_secs: Some(after.shown_secs()),
        }),
        Err(e) => Err(not_run(bin, e)),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/plugin/invoke_tests.rs"]
mod tests;
