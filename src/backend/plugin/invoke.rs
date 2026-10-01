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

use copy_core::copy_text;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// 插件往 **stderr** 写的一行以它开头 ⇒ 那是**一格进度**（前缀后面那段原样交调用方的回调），不进诊断。
///
/// 通用方言（同 `--probe` 那套 `key=value`），不认识任何具体插件：格里是什么由调用方解释。
/// 只有调用方给了回调（[`run_abortable_reporting`]）才分拣；没给 ⇒ 这种行照旧当普通 stderr 留着。
pub(crate) const PROGRESS_PREFIX: &str = "progress=";

/// `timeout` 那条命令超时时的退出码（GNU coreutils）。
///
/// ⚠ 引用它的文案里**别把它写成命令名紧跟左括号**的形状 —— 零定时器护栏按调用形态扫，
/// 它剥注释但**不剥字符串**，写在错误消息里会被当成一处定时器调用。
pub(crate) const TIMED_OUT_CODE: i32 = 124;

/// ★★ 起插件时**从宿主环境继承过去的键** —— 白名单，**一个具名常量、一个家**。
///
/// # 病：这一层此前不清环境，于是子进程拿到的是后端的**整份**环境
///
/// 那不是一个抽象的风险：常驻监听口的地址与令牌（`listen::ENV_PORT` /
/// `listen::ENV_TOKEN`）是 backend **自己从环境读**的，也就是说它们一定在后端的环境里。
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

/// ★ **本层唯一一处起进程**（已登记进 `readonly_guard::spawn_registry::ALLOWED`）——
/// 两种等法（[`run`] 同步等 · [`run_abortable`] 异步等、可被打断）**共用这一处构造**，
/// 程序 · argv · 环境 · `stdin` 一个字都不分叉。
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
fn command_for(bin: &Path, args: &[&str], deadline_secs: u64, env: &[(&str, &str)]) -> Command {
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
    cmd
}

/// 起不来的那一刻 → [`NotRun`]（两种等法共用）。
fn not_run(bin: &Path, e: std::io::Error) -> NotRun {
    // ★ 参数太长要单独说：实测 200KB 必炸、120KB 能过 —— 内核的单参数上限是 128 KiB。
    #[cfg(unix)]
    if e.raw_os_error() == Some(libc::E2BIG) {
        return NotRun::ArgListTooLong;
    }
    NotRun::Failed(copy_text(
        "beInvoke.notRun.failed",
        &[("bin", &(bin.display()).to_string()), ("e", &e.to_string())],
    ))
}

/// 起它、**同步**等它退出（阻塞档的调用方用：它们本来就跑在 `spawn_blocking` 的线程上）。
pub(crate) fn run(
    bin: &Path,
    args: &[&str],
    deadline_secs: u64,
    env: &[(&str, &str)],
) -> Result<Done, NotRun> {
    match command_for(bin, args, deadline_secs, env).output() {
        Ok(out) => Ok(Done {
            code: out.status.code(),
            stdout: out.stdout,
            stderr: out.stderr,
        }),
        Err(e) => Err(not_run(bin, e)),
    }
}

/// 起它、**异步**等它退出 —— **这个 future 被丢掉 = 子进程被杀**（可取消档的调用方用）。
///
/// # 为什么要有它（`RM1b.md §3.3` ③「长期限 ＋ 可取消」的后一半）
///
/// [`run`] 是同步 `output()`：调用方只能把它放进 `spawn_blocking`，而那一档 `abort()` 是空操作
/// （后端据此对它回 `not_cancellable`，不撒谎）。建索引一趟可到分钟级，点下去就只能等。
/// ⇒ 这一形把「等」换成异步：调用方的 future 在任何 `.await` 点被丢（`cancel` 命中
/// ⇒ `AbortHandle::abort()`），子进程**连同它起的孙进程**一起没了。
///
/// # 杀谁：**整组**，不只是直接子进程
///
/// 有 `timeout(1)` 前缀时，直接子进程是那条期限命令，真正干活的是它起的**孙进程**；
/// 只杀直接子进程（`kill_on_drop`）会让孙进程成孤儿、照跑到期限为止 —— 那等于没取消。
/// ⇒ 起的时候让子进程**自成一组**（[`crate::platform::detach::detach`]，与起脱离的中转同一格原语；
/// `timeout` 不带 `--foreground` 时自己也会 `setpgid(0,0)`，同一组），被丢时对**整组**发终止信号
/// （[`crate::platform::signal::kill_group`]）。
/// ★ 组号就是子进程的 pid，而**被丢的那一刻它还没被收尸**（异步等没完成）⇒ pid 不会被复用，
/// 这一枪打不到无关进程。守卫**声明在子进程之后** ⇒ 丢弃时先于子进程句柄析构（逆序），
/// 那一刻收尸更不可能已经发生。
/// ⚠ 非 unix：没有「一组」这一格（`detach` 那一臂诚实地说 `Err`）⇒ 只靠 `kill_on_drop`
/// 杀直接子进程 —— Windows 上找不到 `timeout(1)`，直接子进程就是插件本身。
///
/// ★ 零定时器不破：等的是子进程退出与两条流读到头，不是时钟。
///
/// # 每条流最多留 `keep ＋ 1` 字节
///
/// 对端是一个**别人的程序**：它坏了、或者压根不是我们要的那个，整读它的输出就是无界堆分配
/// （`byte_cap_registry` 那条判据的病）。⇒ 每条流只留 `keep ＋ 1` 字节，多出来的**照读照丢**
/// （不读的话子进程写满管道就卡住，要等到期限才退）。调用方看 `len() > keep` 就知道超了 ——
/// 「超了怎么说」是它自己的话（上限本身也是调用方给的：多大算太大是那个插件的事）。
pub(crate) async fn run_abortable(
    bin: &Path,
    args: &[&str],
    deadline_secs: u64,
    env: &[(&str, &str)],
    keep: u64,
) -> Result<Done, NotRun> {
    abortable(bin, args, deadline_secs, env, keep, None).await
}

/// 同 [`run_abortable`]，另把 stderr 上的**进度行**（[`PROGRESS_PREFIX`] 开头、整行）边读边交 `on_progress`
/// （前缀后面那段，去掉行尾）；进度行不进 [`Done::stderr`]（诊断照旧是那条失败的话）。一行也最多读 `keep ＋ 1` 字节：
/// 超长的那一截不算进度、照普通 stderr 留（上限同一个）。
pub(crate) async fn run_abortable_reporting(
    bin: &Path,
    args: &[&str],
    deadline_secs: u64,
    env: &[(&str, &str)],
    keep: u64,
    on_progress: &mut (dyn FnMut(&str) + Send),
) -> Result<Done, NotRun> {
    abortable(bin, args, deadline_secs, env, keep, Some(on_progress)).await
}

/// 两种可打断的等法共用的本体（给没给进度回调只差 stderr 那条流怎么读）。
async fn abortable(
    bin: &Path,
    args: &[&str],
    deadline_secs: u64,
    env: &[(&str, &str)],
    keep: u64,
    on_progress: Option<&mut (dyn FnMut(&str) + Send)>,
) -> Result<Done, NotRun> {
    let mut std_cmd = command_for(bin, args, deadline_secs, env);
    std_cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let grouped = crate::platform::detach::detach(&mut std_cmd).is_ok();
    let mut cmd = tokio::process::Command::from(std_cmd);
    cmd.kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| not_run(bin, e))?;
    let mut guard = KillGroupOnDrop {
        group: if grouped { child.id() } else { None },
    };
    let (mut so, mut se) = (child.stdout.take(), child.stderr.take());
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let (status, (), ()) = tokio::join!(
        child.wait(),
        keep_then_drain(so.as_mut(), keep, &mut stdout),
        stderr_side(se.as_mut(), keep, on_progress, &mut stderr)
    );
    // 收过尸了 ⇒ 组号可能被复用，从这一刻起守卫不许再开枪。
    guard.group = None;
    let status = status.map_err(|e| {
        NotRun::Failed(copy_text(
            "beInvoke.runAbortable.waitFailed",
            &[("bin", &(bin.display()).to_string()), ("e", &e.to_string())],
        ))
    })?;
    Ok(Done {
        code: status.code(),
        stdout,
        stderr,
    })
}

/// 读一条子进程的流：留前 `keep ＋ 1` 字节，其余照读照丢（见 [`run_abortable`]「每条流」一段）。
async fn keep_then_drain<R: tokio::io::AsyncRead + Unpin>(
    r: Option<&mut R>,
    keep: u64,
    out: &mut Vec<u8>,
) {
    use tokio::io::AsyncReadExt;
    let Some(r) = r else { return };
    let _ = (&mut *r)
        .take(keep.saturating_add(1))
        .read_to_end(out)
        .await;
    let _ = tokio::io::copy(r, &mut tokio::io::sink()).await;
}

/// stderr 那条流：没给回调 ⇒ 同 [`keep_then_drain`]；给了 ⇒ 逐行读，整行的进度行交回调，其余留前 `keep ＋ 1` 字节。
async fn stderr_side<R: tokio::io::AsyncRead + Unpin>(
    r: Option<&mut R>,
    keep: u64,
    on_progress: Option<&mut (dyn FnMut(&str) + Send)>,
    out: &mut Vec<u8>,
) {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt};
    let Some(on) = on_progress else {
        return keep_then_drain(r, keep, out).await;
    };
    let Some(r) = r else { return };
    let cap = keep.saturating_add(1);
    let mut rd = tokio::io::BufReader::new(r);
    let mut line = Vec::new();
    loop {
        line.clear();
        // 对端是别人的程序：一行无界就是无界堆分配 ⇒ 一趟最多读 `keep ＋ 1` 字节。
        match (&mut rd).take(cap).read_until(b'\n', &mut line).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        match line.strip_prefix(PROGRESS_PREFIX.as_bytes()) {
            Some(cell) if line.ends_with(b"\n") => {
                on(String::from_utf8_lossy(cell).trim_end_matches(['\n', '\r']))
            }
            _ => {
                let room = usize::try_from(cap.saturating_sub(out.len() as u64)).unwrap_or(0);
                out.extend_from_slice(&line[..line.len().min(room)]);
            }
        }
    }
}

/// [`run_abortable`] 被丢时对子进程那一组开一枪（见那里「杀谁」一段）。
struct KillGroupOnDrop {
    /// `Some(组号)` ⇒ 还没收尸、被丢就杀这一组；`None` ⇒ 不开枪（没分组 / 已收尸）。
    group: Option<u32>,
}

impl Drop for KillGroupOnDrop {
    fn drop(&mut self) {
        if let Some(g) = self.group.take() {
            let _ = crate::platform::signal::kill_group(g);
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/plugin/invoke_tests.rs"]
mod tests;
