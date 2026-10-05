//! ③ **传 argv 起它** + ④ 的**骨架** —— 本层**唯一一处**起进程。
//!
//! # 期限
//!
//! 被调的那个程序卡住（等锁、等网络、死循环），阻塞档的调用会占死一个 worker、`cancel` 对它是空操作。
//! ⇒ 经起子进程原语带期限跑（`deadline_secs` 由调用方给）：到点杀整组，交成 [`TIMED_OUT_CODE`]。
//! 不再依赖 PATH 上有没有 `timeout(1)`，Windows 上也有期限。
//!
//! # ④ 为什么只抽骨架
//!
//! 两个真实实现的退出码**语义互斥**（同一个码在一侧是「路由层拒绝」、在另一侧是
//! 「撞名、该重试」）⇒ **码 → 语义的映射一定是每插件一份**。
//! 本层只提供：怎么拿到码 · 怎么认出「被信号打断」（`code == None`）·
//! 过了期限时交成的那个码 · 怎么从两条流里摘一行诊断。
//!
//! ★ **这一段此前只是散文（删了不会红）**。D1（08-26）的硬读数：把 `control/cc_bus.rs`
//! 的码表原样抬进本文件、**只擦掉插件的名字** ⇒ `436 passed; 0 failed`，**一条不红**。
//! ⇒ 现在它由 `plugin::layer_guard` 里形状那一族钉着（两条，见 `plugin` 模块头注那张表）：
//! 抬一张写整数字面量的表上来 ⇒ `the_generic_port_does_not_translate_exit_codes` 红；
//! 改用具名 `i32` 常量绕过去 ⇒ `the_only_exit_code_constants_here_are_the_registered_generic_ones` 红。
//! ⚠ 两条各自**认不出**什么（具名常量来自本层与 control/observe 之外 · 非 `i32` 的码 ·
//! 运行期从数据里读的表 · `mod.rs` 本身不在扫描面），逐条写在它们自己的头注里 —— 别读成全覆盖。

use crate::platform::child::{Child, ChildFail, Deadline};
use copy_core::copy_text;
use std::path::Path;

/// 过了期限（原语杀了整组）时交成的退出码：与 GNU `timeout` 超时的码相同，调用方的码表据此认「超时」。
///
/// ⚠ 引用它的文案里**别把它写成命令名紧跟左括号**的形状 —— 零定时器护栏按调用形态扫，
/// 它剥注释但**不剥字符串**，写在错误消息里会被当成一处定时器调用。
pub(crate) const TIMED_OUT_CODE: i32 = 124;

/// ★★ 起插件时**从宿主环境继承过去的键** —— 白名单，**一个具名常量、一个家**。
///
/// # 病：这一层此前不清环境，于是子进程拿到的是后端的**整份**环境
///
/// 那不是一个抽象的风险：常驻监听口的地址与钥匙文件路径（`listen::ENV_PORT` /
/// `listen::ENV_TOKEN_FILE`）是 backend **自己从环境读**的，也就是说它们一定在后端的环境里
/// （钥匙本身从前也在 —— 今天只交文件路径，但同用户的进程读得到那份文件）。
/// ⇒ 任何被这一处口起出来的插件，读一读自己的环境就能接上宿主、过鉴权、发全部基础命令，
/// 而这条回程**没有协议、没有权限模型、没有审计**。
/// 设计文档里没有它，代码里也没写它 —— 它是**默认行为**长出来的（`K-R26`）。
///
/// ⇒ 修法是 [`run`] 清空环境、**只**按本表从宿主继承（原语的 `inherit_only`）。
/// 关键在方向：不在表里的键**按构造**到不了子进程，不需要有人去维护一张「别漏这个」的黑名单。
///
/// # 每个键一句为什么（**给不出论据的不进** —— 下面「考虑过而没进」那一段逐条记着）
///
/// | 键 | 为什么非它不可 |
/// |---|---|
/// | `PATH` | 被起的那个东西自己还要去找别的命令。`tests/e2e/backend-cc-bus.sh` 的 `[10]` 逐字写着「`PATH` 清空是不行的 —— 那样它自己的 `awk` 也没了」；而且本层 [`super::discover`] 的兜底档本来就按 `PATH` 找插件本体 ⇒ 找得到却跑不动是自相矛盾 |
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
///   **没有一处**要子进程知道这件事；反过来，`tests/e2e/backend-cc-bus.sh` 的 `[13]` 逐字要把这两个键
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

/// ★ **本层唯一一处起进程**（已登记进 `readonly_guard::spawn_registry::ALLOWED`）的构造：程序 · argv · 环境。
///
/// 入参：`bin` 是已经找到的那个可执行文件（[`super::discover::find`] 的产出）；
/// `args` **直传，不过 shell** ⇒ 参数里的元字符不构成注入面；
/// `env` 是额外的环境变量（有些插件的契约走 env 而不是参数）。stdin 是空设备。
///
/// ★ **环境是白名单，不是继承**（`K-R26`）：清空后只按 [`INHERITED_ENV_KEYS`] 从宿主继承，
/// 再轮到调用方**显式**交办的 `env`（显式压过继承）。宿主没有的键子进程也没有
/// （空的 `PATH` 会让子进程在当前目录里找命令，比没有更糟）。
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
    // ★ 参数太长要单独说：实测 200KB 必炸、120KB 能过 —— 内核的单参数上限是 128 KiB。
    #[cfg(unix)]
    if let ChildFail::Io(io) = &e {
        if io.raw_os_error() == Some(libc::E2BIG) {
            return NotRun::ArgListTooLong;
        }
    }
    NotRun::Failed(copy_text(
        "beInvoke.notRun.failed",
        &[("bin", &(bin.display()).to_string()), ("e", &e.to_string())],
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
