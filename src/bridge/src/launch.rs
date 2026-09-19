//! Batch14-F41：远端终端拉起。
//!
//! 两块：
//! 1. [`launch_powershell_window`] —— 从 `history.rs::resume_impl` 抽出的通用「新终端窗口
//!    跑一条 PowerShell 命令」机械（wt.exe Plan A → CREATE_NEW_CONSOLE Plan B，
//!    `-NoExit -EncodedCommand`、**不带 `-NoProfile`**）。本地 resume 与远端族
//!    （F41 resume / F51 attach / F52 tmux / F53 launcher）共用此单一入口。
//! 2. [`build_remote_ssh_ps_command`] + [`launch_remote_terminal`] —— 远端终端拉起：
//!    按 origin 取 RemoteConfig，构造 `ssh -t …` 的 PowerShell 命令体并拉起
//!    （F41 用它跑 resume；后续 attach/tmux/launcher 同命令不同 remote_cmd）。
//!
//! ## 引号与注入（三层，各自独立）
//! - **远端命令**（前端 `remote-launch.ts` 构造）：sid 白名单 + launcher denylist +
//!   cwd POSIX 单引号——见前端模块文档。
//! - **传输包装**（本模块）：远端命令包成 `bash -lic '<cmd>'` 再交给 ssh——保证 PATH /
//!   别名 / 函数按「用户粘贴进交互终端」语义解析（非交互 ssh exec 里 `claude`/`cct`
//!   常不在 PATH；`-l` 进 profile、`-i` 进 bashrc 且别名展开）。已知限制：远端 shell
//!   是 zsh/fish 且 claude 只在其 rc 里进 PATH 时不覆盖——F52 tmux send-keys 彻底解决。
//! - **PowerShell 层**：全命令体经 `-EncodedCommand`（base64）穿 wt.exe（`;` 分 tab
//!   不会切碎）；remote_cmd 以 PS 单引号字面量嵌入（`'` → `''`）。**含双引号的
//!   remote_cmd 直接拒绝**——PowerShell 5.1 向 native 程序传参对内嵌 `"` 有历史畸变，
//!   拒绝后前端自动走剪贴板回退（launcher 需要引号参数时用单引号写法）。

use crate::ssh_source::RemoteConfig;

/// 远端命令长度上限（防 IPC 侧异常输入；正常 resume 命令 <300 字节）。
const MAX_REMOTE_CMD: usize = 4096;

/// POSIX 单引号 quote（与前端 `posixQuote` 同构）：`'…'` 包裹，内部 `'` → `'\''`。
pub(crate) fn posix_quote(s: &str) -> String {
    // U8c-2b-0（账本 S5）：实现收进 `shell-quote-core`（P4c 前叫 `launch-core`），此处只留名字。
    // `pub(crate)` 只为让 `quote_singleton_guard` 的行为对拍够得着它。
    shell_quote_core::posix_quote(s)
}

/// PowerShell 单引号字面量：`'…'` 包裹，内部 `'` → `''`。
fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// user 合法性：非空，仅 `[A-Za-z0-9._-]`（拼进 PS 命令体的裸 token，白名单杜绝注入）。
fn valid_user(u: &str) -> bool {
    !u.is_empty()
        && u.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// host 合法性：非空，仅 `[A-Za-z0-9._:\[\]-]`（域名 / IPv4 / IPv6 字面量）。
fn valid_host(h: &str) -> bool {
    !h.is_empty()
        && h.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | '[' | ']'))
}

/// F56：构造 OpenSSH `-J` 跳板参数（` -J user@host[:port]`，port≠22 才带端口后缀）。
/// 纯函数便于单测;跳板 user/host 过与主机同款非法字符校验。
fn build_jump_arg(jump_user: &str, jump_host: &str, jump_port: u16) -> Result<String, String> {
    if !valid_user(jump_user) {
        return Err(format!(
            "refuse launch: 跳板 user 含非法字符: {jump_user:?}"
        ));
    }
    if !valid_host(jump_host) {
        return Err(format!(
            "refuse launch: 跳板 host 含非法字符: {jump_host:?}"
        ));
    }
    let port_suffix = if jump_port == 22 {
        String::new()
    } else {
        format!(":{jump_port}")
    };
    Ok(format!(" -J {jump_user}@{jump_host}{port_suffix}"))
}

/// L1（local-as-remote）：**与传输无关**的那层命令校验 —— 三条送法一律适用。
///
/// 从 [`build_remote_ssh_ps_command`] 里抽出来，好让 POSIX 本地那条路
/// （[`build_local_posix_argv`]）用**同一份**判据，而不是各写一份会静默漂移的副本。
///
/// **刻意不含「拒绝双引号」那条**：它的理由是 PowerShell 5.1 向 native 程序传参对内嵌 `"`
/// 有历史畸变（见调用处），是**那条送法的**约束，不是命令本身的性质。
/// 把它一并搬进来，等于把一个 Windows 怪癖套到 Linux 上 —— 判据要落在性质上。
fn validate_launch_cmd(cmd: &str, what: &str) -> Result<(), String> {
    if cmd.trim().is_empty() {
        return Err(format!("refuse launch: {what}为空"));
    }
    if cmd.len() > MAX_REMOTE_CMD {
        return Err(format!(
            "refuse launch: {what}过长（{} > {MAX_REMOTE_CMD}）",
            cmd.len()
        ));
    }
    if cmd.chars().any(|c| c.is_control()) {
        return Err(format!("refuse launch: {what}含控制字符"));
    }
    Ok(())
}

/// L1：**POSIX 本地**送法 —— 直接 exec，**不要 ssh 包**。
///
/// 返回 argv 而不是命令串：本地没有「要穿过一层 shell」的问题，拼成串再让别人拆是
/// 白白造一个注入面。`bash -lic` 这层**保留**——它和远端那条路是同一个语义
///（PATH / 别名 / 函数按「用户粘贴进交互终端」解析），`ccm` 正是靠它才被找到。
///
/// ⇒ 与 [`build_remote_ssh_ps_command`] 的关系就是 §2「payload 共享、transport 只管送」：
/// 同一个 `cmd`，本地是 `bash -lic <cmd>`，远端是把这同一串再包进 ssh。
/// 有测试逐字节钉住这条（`local_and_remote_share_the_same_payload`）。
pub fn build_local_posix_argv(cmd: &str) -> Result<Vec<String>, String> {
    validate_launch_cmd(cmd, "本地命令")?;
    Ok(vec!["bash".into(), "-lic".into(), cmd.into()])
}

/// 上一条的**下一跳**：`(argv, 终端出口)` → 真正要 spawn 的 `(program, args)`。
///
/// # 为什么它非抽出来不可〔`K-H2b` `D6` 第九拍，08-29〕
///
/// `K-H2b` 那条「中转前缀真的拼上去了」的判据量的是
/// **`history::launch_local` 交给送法的那一串**。送法拿到之后再动手，那条判据看不见 ——
/// 收工前按铁律 15 找「第八层」时实打过两刀，两刀都在这一跳附近：
/// - 在 [`build_local_posix_argv`] 里剥掉 `export ANTHROPIC_BASE_URL=…; ` ⇒ **当时全绿**
///   （隔壁那条透传判据只喂一个不带前缀的 payload ⇒ 输入域 = 1，与 `D6 阻-2` 同族）；
/// - 在 `launch_local_posix_via` 里、`build_local_posix_argv` **之后**剥 ⇒ **也全绿**
///   （那一段当时就地拼 `Command`，没有任何纯函数可打）。
///
/// ⇒ 两刀的修法是同一条：**把这一跳也变成纯函数**，让「载荷逐字节走过去」有东西钉。
///
/// # 🔴 本函数返回之后那 3 行 —— **今天有判据了，两支各一条**〔`D7 阻-1` / `D7 阻-2`，08-29〕
///
/// 本函数返回之后只剩 3 行（`Command::new(program)` · `.args(&args)` ·
/// 以及 `cwd` / `env` / `stdio` / `process_group` 那几个设置）——
/// **在那 3 行里再剥一次前缀，实打 `1229 passed` 全绿**（刀 `Z1c`，08-29）。
///
/// 🔴🔴 **先前这里逐字写着「要买它得真开一个窗口再去看那个进程的 argv」—— 那是假话。**
/// `D7` 照既有那条 `local_posix_spawn_actually_runs_the_command` 的形状写了一条判据量过：
/// 带刀 `Z1c` `1229 passed; 1 failed`（红）· 干净树 `1230 passed; 0 failed`（绿）
/// ⇒ **不用开窗 · 不用真终端 · 不用动 `paths.rs` · 不用 `set_var("HOME")` ·
/// 不用 `--test-threads=1` · 不用 `#[ignore]`。**
/// ★ 「做不到」也是一个断言，而那一句只对**它当时想写的那一种**判据成立
///（同族第二次；上一次是 `RelayFactSources` 头注 ㈠ 那条「要动真实家目录」）。
///
/// ⇒ 今天那 3 行由**两条**判据看着，一支一条（`term` 是这条链的分叉点，两支都买）：
/// - `term = None`（无窗口回落）⇒ `the_spawned_process_really_gets_the_relay_prefix_without_a_terminal`
///   （观测点 = 起出去的那个进程自己看到的 `ANTHROPIC_BASE_URL`）；
/// - `term = Some(...)`（开窗）⇒ `the_terminal_we_hand_the_command_to_really_gets_the_relay_prefix`
///   （观测点 = **假终端**自己收到的 argv；`#!/bin/sh` 脚本打 `"$@"`，**没有窗口会弹出来**）。
///
/// 🔴 **为什么非两条不可**：刀 `L10`（剥前缀**只放在 `term.is_some()` 那一支**）
/// **活过了只买 `None` 那一支的修法** —— 装上走 `None` 的判据之后它仍然 `1229` 全绿、
/// `GATE: OK`，而**生产上装了规范化终端出口的机器走的正是开窗那一支**。
/// ⇒ **这条链上的每一条判据，两支都要有；只买一支买到的是「判据去的那一支」。**
/// **「跑什么」的全部** —— 载荷校验 + argv + 终端包装，一跳到底。
///
/// ★ `launch_local_posix_via` 因此只剩「**怎么起**」（`cwd` / `env` / `stdio` / `process_group`）。
/// 这条边界是**实打逼出来的**：先前那两段（[`build_local_posix_argv`] 与就地拼 `Command`）
/// 中间隔着一跳没有判据，在那里剥掉中转前缀 ⇒ 全绿。
#[cfg(not(windows))]
fn local_posix_spawn_plan(cmd: &str, term: Option<&str>) -> Result<(String, Vec<String>), String> {
    let argv = build_local_posix_argv(cmd)?;
    Ok(build_local_posix_spawn(term, &argv))
}

#[cfg(not(windows))]
fn build_local_posix_spawn(term: Option<&str>, argv: &[String]) -> (String, Vec<String>) {
    match term {
        // `xdg-terminal-exec` / `x-terminal-emulator` 都收 `-- <argv…>` 之后的命令行。
        Some(t) => (
            t.to_string(),
            std::iter::once("--".to_string())
                .chain(argv.iter().cloned())
                .collect(),
        ),
        None => (argv[0].clone(), argv[1..].to_vec()),
    }
}

/// P5L-Y3：**规范化**终端出口，有序候选。
///
/// # 为什么是这两个，为什么**不探测具体终端**
///
/// 用户逐字〔用 08-12〕：「**纯 bash 的意思是暂时不考虑其他终端, 但是所有的功能都要一样**」。
/// 那句话反对的是**终端专属集成**（像 Windows 那条 `wt.exe` 带专属参数的路）——
/// 而下面这两个恰恰是**不挑**的出口：freedesktop 的 `xdg-terminal-exec` 与 Debian 的
/// `x-terminal-emulator` 都把「用哪个终端」交还给桌面/用户配置。
///
/// ⇒ **绝不在这里列 gnome-terminal / konsole / alacritty 之类的具名终端**。
/// 那才是「挑」，也正是「会在别人机器上错」的来源。
///
/// # ★★ 为什么表里**只有一个**（D 阶段补审 08-12）
///
/// 第一版还放了 `x-terminal-emulator`。D 阶段核参数约定时发现**它们的约定未必一样**：
/// · `xdg-terminal-exec` 的用法串逐字 `[options] [--] [command [arguments ...]]` ⇒ `--` **已核实**；
/// · `x-terminal-emulator` 是 **Debian alternatives 的间接层** —— 本机指向 `ptyxis`
///   （man 逐字 `[-- PROGRAM ARGUMENTS]`，且「In general, you should use `--`」，对得上），
///   但**别的机器上它可能指向 `xterm`/`gnome-terminal`，那些的传统约定是 `-e <command>`**。
///
/// ⇒ 用一个**未核实**的约定去开窗，失败形态正是原判据担心的那个：
/// 终端开出来了、命令没跑，用户看到一个空白窗口且极难归因。
/// **宁可少覆盖一台机器，也不要开一个空白窗口。**
///
/// 要加回来的话，正确形状是「每个出口带自己的参数构造器」，而不是共用一个 `--`。
#[cfg(not(windows))]
const TERMINAL_EXITS: &[&str] = &["xdg-terminal-exec"];

/// P5L-Y1：挑一个存在的终端出口。都不在 ⇒ `None`（调用方诚实降级，见 `launch_local_posix`）。
///
/// **纯函数化的那一半**（`pick_terminal_exit_from`）供判据用 —— 生产这条只是喂它一个真实探针。
#[cfg(not(windows))]
fn pick_terminal_exit() -> Option<&'static str> {
    pick_terminal_exit_from(TERMINAL_EXITS, &|c| which_exists(c))
}

/// 判据入口：候选表与「在不在」的判定都作为参数进来，好在不依赖真实机器的前提下钉顺序。
#[cfg(not(windows))]
fn pick_terminal_exit_from(
    candidates: &[&'static str],
    exists: &dyn Fn(&str) -> bool,
) -> Option<&'static str> {
    candidates.iter().copied().find(|c| exists(c))
}

#[cfg(not(windows))]
fn which_exists(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|d| {
                let p = d.join(cmd);
                std::fs::metadata(&p).map(|m| m.is_file()).unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// L1：在 **POSIX 本机**跑一条命令 —— 不经 ssh、不经 PowerShell。
///
/// 这是 §40「本地 = 不走 ssh 的远端」在传输层的落点：远端那条路是
/// `ssh -t host -- 'bash -lic <cmd>'`，本地就是把 `ssh` 那一跳**去掉**，其余不变。
///
/// 三处设计：
/// - **不开 GUI 终端窗口**。POSIX 上没有「唯一的终端」这种东西：开窗口要先猜用户用哪个
///   终端模拟器，是平白引入一个会在别人机器上错的决定。
///   ⚠⚠ **这条原本还有半句，断言容器一定是 tmux（`ccm --tmux` 自己会建）—— 那是假的**
///   〔audit-0805 F08 / 报告 B-2〕：生产构造出来的是 `cc --resume <sid>`，**不带 `--tmux`**
///   （带 `--tmux` 的别名是 `cct`），而 `shared/ccm` 的 `use_tmux` 默认 0
///   ⇒ 走的是非容器分支 `exec "${argv[@]}"`。加上这里 stdio 全 null，
///   **产出的是一个无 tty、无 tmux 的进程**，不是「留在 tmux 里等 attach」。
///   ★ 同一条推理本仓在别处写对过：`src/doc/IPC-PROTOCOL.md` 逐字
///   「决定性的事实是 `stdin` 不接键盘（`stdin=DEVNULL`）—— 用户敲进去的字会被脚本吃掉」。
///   ★★ **P3t（2026-08-11）已经改了一半，本段随之更新。**
///   `history.rs::launch_local` 现在**先过 CLI 渲染器**（`render_local_ccm`），渲得出来就带
///   `--tmux` ⇒ ccm 走容器分支、会话留在 tmux 里有真 tty，本函数只负责把它拉起来。
///   上面那句「产出的是一个无 tty、无 tmux 的进程」现在只描述**回落那条路**
///   （渲染器拒了才走的 `build_local_posix_command`），由
///   `the_local_resume_payload_has_no_session_container_today` 继续钉；
///   正面事实由 `the_rendered_local_command_really_carries_the_container` 钉。
///   ⚠ **今天生产上还到不了正面那条**：会话名要由前端 `mintTmuxName` 传下来（P3t-Y2b），
///   而本机的「已占用名字」集合还不存在（ROADMAP `U11`）⇒ 名字恒为 `None` ⇒ 恒走回落。
///   功能后果（claude 在 `stdin=/dev/null` 下具体怎么表现）红线内**没实测**，是推的。
/// - **脱离 app 的进程组**（`process_group(0)`）+ stdio 全 null：
///   否则子进程会跟着 app 的 Ctrl-C 一起走，也会把 app 的 stdio 占住。
/// - **起一条线程收尸**。`process_group` 不改变父子关系 ⇒ 不 `wait` 就留僵尸。
///   线程随子进程结束而结束（`ccm` 建完会话就返回，是短命进程）。
#[cfg(not(windows))]
pub fn launch_local_posix(cmd: &str, cwd: Option<&str>) -> Result<(), String> {
    launch_local_posix_via(cmd, cwd, pick_terminal_exit())
}

/// P5L：把「用哪个终端出口」做成**入参**。
///
/// ★ 这不是为了好看，是**判据不能在开发者桌面上开真窗口**：
/// 既有的 `local_posix_spawn_actually_runs_the_command` 是一条**真跑**的行为测试，
/// 改之前它直接 spawn `bash`；接上终端出口之后它会**真的弹出一个终端窗口**
/// —— 实测跑了一次（本机 `xdg-terminal-exec` → `ptyxis`）。
/// ⇒ 出口作为参数进来，判据传 `None` 走无窗口那条 —— **或者传一个自己造的假终端脚本**。
///
/// 🔴🔴 **订正〔`D7 阻-2`，08-29〕：先前这里逐字写着「开窗那条路因此没有行为级判据
/// （只有形状判据）」—— 那句话在 `P5L` 那一轮是对的，今天是假话，而且它宽了两格。**
/// - 载荷那一半：`the_local_argv_hands_the_command_through_byte_for_byte_prefix_and_all`
///   喂 `Some("xdg-terminal-exec")` 走的就是开窗那一支（`D7 §A3` 现打）；
/// - spawn 那一半：`the_terminal_we_hand_the_command_to_really_gets_the_relay_prefix`
///   喂一个**假终端**（临时目录里的 `#!/bin/sh` 脚本，把 `"$@"` 写进文件）——
///   `Command::new(program)` 拿绝对路径直接 exec 它，**没有任何窗口会弹出来**。
/// ⇒ **「不能在开发者桌面上开真窗口」≠「开窗那一支买不到」**：前者是真的，后者是
///   把「做不到的那一种做法」说成了「做不到这件事」。
///
/// ⚠ **今天仍然没有的**：真终端（`xdg-terminal-exec` → 用户配的那个模拟器）拿到 argv
/// 之后**会不会照着跑**。那是它自己的约定，本仓测不到 ⇒ 真机验收归 `auto-e2e`。
/// 别把「我们交出去的那一份是对的」读成「窗口真开出来了」。
#[cfg(not(windows))]
fn launch_local_posix_via(cmd: &str, cwd: Option<&str>, term: Option<&str>) -> Result<(), String> {
    use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};
    use std::process::{Command, Stdio};

    // ★★ `K-H2b` `D6` 第九拍：**「跑什么」整段收成一个纯函数**，本函数只剩「怎么起」。
    //    先前这里是 `build_local_posix_argv` + 就地拼 `Command` 两段，中间那一跳
    //    **没有任何判据** —— 实打：在这两段之间插一个剥掉 `export ANTHROPIC_BASE_URL=…; `
    //    的映射，`1229 passed` 全绿（第八层的下游版本）。
    let (program, args) = local_posix_spawn_plan(cmd, term)?;
    // ★★ P5L-Y1（`U14`〔用 08-12〕「要做，功能必须一样」）：**真开一个终端窗口**。
    //
    // 改之前这里是 `Stdio::null()` 直接 spawn ⇒ 产出一个**无 tty、无窗口**的进程
    //（`P3t` 摸底记下的那条：用户敲进去的字会被脚本吃掉）。
    //
    // ⚠ **载荷一个字不改**：`argv` 原样进终端的参数位。本件只加「怎么开窗」，不碰「开什么」。
    let opened_window = term.is_some();
    if !opened_window {
        // 诚实降级：一个规范化出口都没有 ⇒ 回落到改之前那条无窗口的路，**并说清**。
        // 不静默、也不报一个与真实原因无关的错。
        tracing::warn!(
            "本机没有规范化终端出口（试过 {TERMINAL_EXITS:?}）—— \
             回落到无窗口直起；命令仍会跑，但没有可交互的终端"
        );
    }
    let mut builder = Command::new(&program);
    builder.args(&args);
    // 只有真实存在的目录才作起始目录（与 Windows 那条路同一条纪律）。
    if let Some(d) = cwd.filter(|c| std::path::Path::new(c).is_dir()) {
        builder.current_dir(d);
    }
    // F06b-1d（C9）：backend 把 daemon 路径交给它亲手开的这个窗口 —— 窗口里那次 `ccm resume`
    // 据此去调 `--resolve`（旧 `shared/ccm::resolve_from_daemon` 〔散文墓碑〕，`K-R48` 已删；
    // 今天那一问在后端进程内直接答）。sidecar 不在就不设。
    if let Some((k, v)) =
        crate::backend::control::local_backend::daemon_bin_env_for_window(env!("CCM_TARGET_TRIPLE"))
    {
        builder.env(k, v);
    }
    builder.stdin(Stdio::null()).stdout(Stdio::null());
    // 三条策略（`00 §1.5.2`）：
    // · `Hidden` —— POSIX 上没有「控制台窗口」这回事，这一条在这儿是空的；
    //   **窗口是 `program` 自己开的**（`local_posix_spawn_plan` 选出来的那个终端出口），
    //   不是 `CreateProcess` 的 flag 开的。别把它读成「这条路不开窗」。
    // · `Detached` —— 先前那句 `process_group(0)` 就是这一条：否则终端里 Ctrl-C 的
    //   SIGINT 会打到整个前台进程组，monitor 和用户刚开的会话一起走。
    // · `Null` —— 它的 stdio 本来就全 null（stdin/stdout 在上面），真正说话的是它开出来的
    //   那个终端窗口；接进我们的滚动日志只会把终端的噪声灌进去。
    let mut child = spawn_managed_cmd(
        &mut builder,
        ConsolePolicy::Hidden,
        Lifetime::Detached,
        StderrSink::Null,
    )
    .map_err(|e| format!("spawn 本地命令失败: {e}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    tracing::info!(
        "launch: local posix exec (no ssh){}",
        if opened_window {
            " · 已开终端窗口"
        } else {
            " · 无窗口回落"
        }
    );
    Ok(())
}

/// Windows 宿主上没有这条路：Windows **本地**走 L2 的 PowerShell 分支，不是本函数。
/// 保留同名同签名，是为了让调用点不必自己写 `cfg`（平台差异收在这一层）。
#[cfg(windows)]
pub fn launch_local_posix(_cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
    Err("POSIX 本地拉起不适用于 Windows 宿主（Windows 本地属 L2 的 PowerShell 分支）".into())
}

/// 构造远端拉起的 PowerShell 命令体（不含 `-EncodedCommand` 编码）。
///
/// 形态：`& ssh -t[ -J <跳板>] -p <port> [-i '<key>'] <user>@<host> -- '<bash -lic ''…''>'`
/// （F45 竞发落地后 host 换成连接大脑当前胜者地址；F56 jump 有值插 `-J`——本函数签名不变。）
pub fn build_remote_ssh_ps_command(cfg: &RemoteConfig, remote_cmd: &str) -> Result<String, String> {
    validate_launch_cmd(remote_cmd, "远端命令")?;
    if remote_cmd.contains('"') {
        return Err(
            "refuse launch: 远端命令含双引号（PowerShell 5.1 native 传参畸变面）。\
             launcher 参数请改用单引号写法，或使用复制粘贴回退"
                .into(),
        );
    }
    if !valid_user(&cfg.user) {
        return Err(format!("refuse launch: user 含非法字符: {:?}", cfg.user));
    }
    // F45：拨号地址取连接大脑当前胜者（已连过 = last-good 胜者;否则 = host）。让
    // PowerShell 的 ssh 走与 russh 数据源同一条路,避免 monitor 连内网 IP、终端却盲连
    // 可能已死的 host 字段。
    let winner = crate::ssh_source::winner_address(cfg);
    if !valid_host(&winner.host) {
        return Err(format!("refuse launch: host 含非法字符: {:?}", winner.host));
    }

    // 尾 `\` 剥掉：key 是文件路径不应以 \ 结尾，而 PS<7.3 给含空格参数加壳时
    // 尾部 `\"` 会转义掉收尾引号（native 传参已知畸变），防御性 trim。
    let key_part = match cfg
        .key_path
        .as_deref()
        .map(|k| k.trim().trim_end_matches('\\'))
    {
        Some(k) if !k.is_empty() => format!(" -i {}", ps_quote(k)),
        _ => String::new(), // ssh-agent（Windows OpenSSH agent），无 -i
    };
    // F56：跳板 ProxyJump——jump 有值 → 解析跳板 cfg → 插 OpenSSH `-J user@host[:port]`。
    // fail-closed:跳板配置查无 → Err（绝不静默直连目标）;自引用环 → Err。
    let jump_part = match cfg.jump.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(jump_label) => {
            if jump_label == cfg.origin_label() {
                return Err("refuse launch: 跳板不能指向自己".into());
            }
            let jump_cfg = crate::load_remote_config_by_label(jump_label)
                .ok_or_else(|| format!("refuse launch: 跳板配置未找到: {jump_label:?}"))?;
            build_jump_arg(&jump_cfg.user, &jump_cfg.host, jump_cfg.port)?
        }
        None => String::new(),
    };
    // 传输包装：交互式 login bash 里执行（见模块文档）。
    let wrapped = format!("bash -lic {}", posix_quote(remote_cmd));
    Ok(format!(
        "& ssh -t{jump_part} -p {port}{key_part} {user}@{host} -- {cmd}",
        port = winner.port,
        user = cfg.user,
        host = winner.host,
        cmd = ps_quote(&wrapped),
    ))
}

/// 在新终端窗口跑一条 PowerShell 命令（加载用户 profile、`-NoExit` 保留窗口）。
///
/// Plan A：wt.exe（Windows Terminal）新标签；Plan B：powershell.exe +
/// CREATE_NEW_CONSOLE 独立控制台。`local_cwd`＝Some 且为本地存在目录时作为窗口
/// 起始目录（远端拉起传 None——cwd 是远端路径）。
///
/// # 🔴 **登记：这个函数体分两半 —— 文本那一半买得到而且已经买了，行为那一半买不到**
///
/// 〔`K-H2b` `D8 阻-6` 08-29 立 · `D9 阻-2`/`阻-4` 打回 · `C` 第十轮 09-02 重写〕
///
/// ## 🔴🔴 先记这段登记自己犯过的病（别删，它就是这一格的病理）
///
/// 上一版这 24 行里**同时躺着一条全称和它的反例**：
/// 一句写「本机门禁**在构造上**看不见这个函数体」＋「**唯一的买法**是 CI 上一条 Windows job，
/// 落点不在写区」，另一句写「量文本的判据照样看得见它」。
/// **而被拿去定路由（判成「买不到 / 落点在写区外」）的是假的那一条。**
/// ⇒ 要订正的不是措辞，是**这段登记自相矛盾**。〔`D9` 逮到；PM `§17 裁二` 接〕
///
/// ## ① 量**文本**的那一类：**看得见，而且今天真的有牙**
///
/// `#[cfg(windows)]` 是**类型检查**那一层剔的；而 `guard_core::production_code`
/// 只剥 `#[cfg(test)] mod X { }` 段与注释（整行的与行尾的都剥 —— 行尾那一半是 `K-R3`
/// 09-01 才补上的），**它不剥任何 `cfg`** ⇒ 本函数体在**文本**这一层原样在场。
///
/// 现打（量具 `tests/evidence/K-H2b-C10-cfgwin-visibility.py`，喂的是本工作树的
/// `src/bridge/src/launch.rs`，09-02；量具先拿本文件那条真判据钉的三个等号自检过
/// 复刻对不对得上 —— 对不上它就拒绝出读数）：
///
/// | 构造 | 剥完全文件几处 | 其中在本函数体内 |
/// |---|---|---|
/// | `Command::new(` | 4 | **2** |
/// | `.env(k, v)` | 3 | **2** |
/// | `daemon_bin_env_for_window(` | 2 | **1** |
///
/// ⇒ 本文件那条**普通 `#[test]`**
/// `launch_tests.rs::every_terminal_window_backend_opens_carries_the_daemon_path`
/// （就在 `cargo` 门里跑）用**三条等号断言**钉着这 5 个构造。
///
/// **实打（`C` 第十轮 刀 `R10M1`，沙箱快道 `cargo test -p monitor --lib`，09-02）**：
/// 把本函数体里 Plan B 那个 `if let Some((k, v)) = daemon_env { builder.env(k, v); }`
/// 换成 `let _ = daemon_env;`（＝真实缺陷形状「开窗点漏了带 env」；锚点是那三行，**全文命中 1**）
/// ⇒ **`1243 passed; 2 failed`**（同树干净分母 **`1245 passed; 0 failed`**），红名单**恰好两条**：
/// `launch_tests.rs::every_terminal_window_backend_opens_carries_the_daemon_path` 与
/// `payload_tests.rs::the_population_that_renders_env_prefixes_for_the_agent_process_is_enumerated`。
///
/// ⚠ **分母话**：那是**一刀打出来的红名单**，不是「全部判据」的枚举 ——
/// 我没有逐条去数还有几条判据碰得到这个函数体。⇒ 只能写「**我这一刀量到的是这两条**」。
///
/// ## ② 量**行为**的那一类：在本机上**造不出来**，这一半才是真的买不到
///
/// 要驱动本函数体，先得有这个 item；而 Linux 上它在类型检查之前就没了
/// ⇒ 任何「真的调用它、看它干了什么」的判据在本机**不存在**。
/// **它的 `cfg` 就写在下面那一行** —— 这正是 PM 08-29 那条落法要的凭据：
/// **标「平台判不了」要给得出 `cfg`；给不出 `cfg` = 它在本平台编译 = 不是判不了。**
///
/// **实打（刀 `R10M2`，`D8P35` 同形，第九轮 `R9M9` 的复打，09-02）**：在
/// `let encoded = crate::utils::powershell_encoded_command(ps_command);`
/// 之前把 `$env:ANTHROPIC_BASE_URL=…; ` 前缀剥掉（锚点是那一行，**全文命中 1**）
/// ⇒ 🔴 **`1245 passed; 0 failed`**，与同树干净分母逐字相同 —— **零感知**。
///
/// ⇒ 买**这一半**要 CI 上一条 Windows job（或交叉编译 ＋ `cargo test --target`），
/// 落点 `.github/workflows/ci.yml`，**不在 `K-H2b` 的写区** ⇒ 归 PM 立跟进件。
/// ⚠ `tests/scripts/gate.sh` 头注自陈「本地门禁比 CI 严」，而**这一格恰是反过来的那一格**，
/// 别把那句话读成全称。
///
/// ## 🔴 `阻-4`：这里不许再写全称，要写清是哪一类
///
/// 上一版那句「**本机门禁在构造上看不见这个函数体**」是**全称**（分母＝门禁七格里的
/// 全部判据），而它在 ① 那一格上当场为假。今天的写法是**两半各自带读数、判据指名点姓**。
/// ⇒ 这一族今天的口径逐字是：
/// **「带 `#[cfg(windows)]`」蕴含「量行为的判据看不见它」，不蕴含「所有判据都看不见它」。**
///
/// ⚠ 同族第三次（`D8` 表里的 `F3` · `D9` 抓的这一处 · PM `§17 裁一` 自陈的那句）——
/// 三次都是同一个动作：**把一个准确的局部读数放大成全称**。
///
/// ## ⚠ 别把这一条读宽（两处邻居，都不属于本族）
///
/// - 同一条腿上的 [`crate::utils::powershell_encoded_command`] **没有 `cfg`、在 Linux 上真编译**
///   ⇒ 不属于本族（`D8 阻-2`）；第九轮已给它配了
///   `utils_tests.rs::the_relay_prefix_survives_the_powershell_encoding_byte_for_byte`。
/// - `history.rs::PRODUCTION_LAUNCH_SINK` 的 `#[cfg(windows)]` 那一支（`D8` 表里的 `F3`，
///   `D8` **没打**、标着「推的」）第九轮打了、**是红的**；`C` 第十轮刀 `R10M8` 复打，
///   读数一致：**`1244 passed; 1 failed`**，红的是
///   `payload_tests.rs::nobody_reaches_the_relay_take_points_without_going_through_the_seam`。
#[cfg(windows)]
pub fn launch_powershell_window(ps_command: &str, local_cwd: Option<&str>) -> Result<(), String> {
    use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};
    use std::process::Command;

    // 仅当是本地存在的目录才作为起始目录（远端路径/无效路径一律忽略）。
    let start_dir = local_cwd.filter(|c| std::path::Path::new(c).is_dir());

    // -EncodedCommand（base64 of UTF-16LE）：命令含空格 / 括号 / `;`，直接当字符串穿
    // wt.exe（用 `;` 分隔多 tab）会被切碎。编码后只含 [A-Za-z0-9+/=]，任何一层 shell
    // 都不会误解析。详 utils::powershell_encoded_command。
    let encoded = crate::utils::powershell_encoded_command(ps_command);
    // 不带 `-NoProfile`：必须加载用户 PowerShell profile（cc / __ccm_bind / 代理 env）。
    // -NoExit：命令退出后窗口保留（可读错误、可继续敲）。用系统自带 powershell.exe。
    let ps_args = ["-NoExit", "-EncodedCommand", encoded.as_str()];

    // Plan A：wt.exe 新标签里跑 powershell。
    let mut wt_args: Vec<String> = Vec::new();
    if let Some(d) = start_dir {
        wt_args.push("-d".into());
        wt_args.push(d.into());
    }
    wt_args.push("powershell.exe".into());
    for a in ps_args {
        wt_args.push(a.into());
    }
    // F06b-1d（C9）：同 POSIX 那条 —— 见 `daemon_bin_env_for_window` 头注。
    // ⚠ **这一格的诚实边界**：`wt.exe` 多半只是把请求转交给**已在跑的** Windows Terminal 进程，
    //   新标签的环境来自那个进程、不是本次 spawn ⇒ **这里设的 env 未必落得进去**。
    //   下面 Plan B（CREATE_NEW_CONSOLE 直起 powershell）是真正会继承的那条。
    let daemon_env = crate::backend::control::local_backend::daemon_bin_env_for_window(env!(
        "CCM_TARGET_TRIPLE"
    ));
    let mut wt = Command::new("wt.exe");
    wt.args(&wt_args);
    if let Some((k, v)) = daemon_env.clone() {
        wt.env(k, v);
    }
    // ★★ 三条策略（`00 §1.5.2`）。**Plan A 与 Plan B 只有第一格不同，而那是照着盘面写的：**
    //   · `Inherit`（本处，Plan A）—— 🔴 `wt.exe` 今天**一个 creation flag 都没带**，
    //     而且它多半只是把请求转交给**已在跑的** Windows Terminal 进程（这一格的诚实边界
    //     上面那段注释已经写过一次）⇒ 真正开窗的不是这次 `CreateProcess`。
    //     ⚠ 刻意**不**顺手改成 `NewVisible`：那是一次**没有任何 Windows 读数支持**的
    //     行为改动，而这条路正是本产品的主用途。本轮只搬「谁来写这三格」，不动盘面。
    //   · `Detached` —— 用户的终端**不该随 monitor 一起死**：关掉界面不等于关掉他正在
    //     敲字的那个会话 ⇒ 绝不能进 `JobKillOnClose` 那个 Job。
    //   · `Inherit` —— 窗口里的话说给用户听，不该灌进我们的滚动日志。
    if spawn_managed_cmd(
        &mut wt,
        ConsolePolicy::Inherit,
        Lifetime::Detached,
        StderrSink::Inherit,
    )
    .is_ok()
    {
        tracing::info!("launch: powershell window via wt.exe");
        return Ok(());
    }

    // Plan B：powershell.exe + CREATE_NEW_CONSOLE，conhost 兜底。
    let mut builder = Command::new("powershell.exe");
    builder.args(ps_args);
    // F06b-1d（C9）：同上。这一格是**真新起的进程**，env 一定继承。
    if let Some((k, v)) = daemon_env {
        builder.env(k, v);
    }
    if let Some(d) = start_dir {
        builder.current_dir(d);
    }
    // ★★ 第一格与 Plan A 不同，也是照着盘面写的：先前那句 `creation_flags(CREATE_NEW_CONSOLE)`
    // 逐字就是 `ConsolePolicy::NewVisible`。**全仓唯一一处 `NewVisible`，而且是刻意的** ——
    // 设计稿逐字：「别把 `launch.rs` 的 `CREATE_NEW_CONSOLE` 一起改掉……这正说明为什么要
    // 唯一出口 + 显式声明，而不是全局加一个 flag」。后两格与 Plan A 相同。
    spawn_managed_cmd(
        &mut builder,
        ConsolePolicy::NewVisible,
        Lifetime::Detached,
        StderrSink::Inherit,
    )
    .map_err(|e| format!("spawn powershell failed: {e}"))?;
    tracing::info!("launch: powershell window via fallback console");
    Ok(())
}

/// POSIX 宿主上**刻意没有**这条路 —— 不是还没做，是 L1 裁决过的**反方向**。
///
/// # 为什么（与 [`launch_local_posix`] 头注同一条裁决）
///
/// POSIX 上没有「唯一的终端」这种东西。要开窗就得先猜用户用哪个终端模拟器
/// （gnome-terminal / konsole / alacritty / kitty / wezterm / …），**那是一个平白引入的、
/// 会在别人机器上错的决定**。
///
/// ⚠⚠ **本段原先还接着「而容器一定是 tmux —— 命令跑完，会话留在那儿等 attach」，那是假的**
/// 〔audit-0805 F08 下半〕。本函数服务的正是**远端**那条路，而它走
/// `runRemoteResume` → `planResumeDirect`，那里逐字是 `container: { kind: "none" }`
/// （`launch-requests.ts:45`，全文件唯一一个 `none`；其余四个 plan 才是 tmux）。
/// ⇒ 「不开终端窗口」这个决定**站得住**（POSIX 没有唯一的终端，这条理由本身没问题），
/// 但**不能拿「反正在 tmux 里」当理由** —— 那个前提不成立。
/// 要 tmux 得走 `planResumeTmux`（F52）那条**另一条路**。
///
/// ⚠ **原文案是「拉起终端窗口仅支持 Windows（v1）」，那个 `(v1)` 在撒谎**：
/// 它暗示「v2 会支持」，而实际上这件事**没排期、而且方向是反的**（U8b 订正）。
///
/// ⚠ **这不代表 POSIX 上「远端拉起」这件事就该只复制命令** —— 那是另一个缺口：
/// 本机 resume 有 OS 分派（`history.rs::launch_local`），远端**没有**（`launch_remote_terminal`
/// 一律走本函数）。补它要等前端改成发结构化请求（U8c）之后走 daemon 的 `launch`，
/// 登记在 **U8a-2c**。今天硬补只能 fire-and-forget，而那会**静默失败**（见 U8b 计划）。
#[cfg(not(windows))]
pub fn launch_powershell_window(_ps_command: &str, _local_cwd: Option<&str>) -> Result<(), String> {
    Err(POSIX_NO_TERMINAL_WINDOW.into())
}

/// 非 Windows 上「不开终端窗口」的**唯一**说法。前后端共用同一句话的口径
/// （前端据 `hostOs` 决定标题，正文原样带上这句）。
///
/// ⚠ **「刻意不替你挑终端模拟器」这半句是跨语言标记，不许换措辞**：
/// 前端 `POSIX_NO_WINDOW_MARKER` 按它判「这是既定设计」，换了用户会退回去看到「拉起失败」
/// （`the_posix_marker_is_the_one_the_frontend_matches_on` 当场判红 —— 08-12 实测撞过）。
/// 08-12 用户裁定「attach 暂时就用纯 linux bash」⇒ 只把「你自己的终端」**说实成**
/// 「你自己的 bash」，标记那半句原样保留。**不挑终端模拟器**与**shell 用 bash**
/// 是两件事，不冲突。
#[cfg(any(not(windows), test))]
pub const POSIX_NO_TERMINAL_WINDOW: &str =
    "本机不是 Windows：cc-monitor **刻意不替你挑终端模拟器**（会话容器是 tmux）——\
     命令已复制，在你自己的 bash 里粘贴执行即可。这是既定设计，不是没做完。";

/// Windows 本机 ssh.exe 可用性预检：缺 OpenSSH 客户端时 spawn 出的窗口只会报
/// "not recognized"（spawn 本身成功→前端误报成功）——预检失败直接 Err 走剪贴板回退。
#[cfg(windows)]
fn ssh_client_available() -> bool {
    use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};
    let mut cmd = std::process::Command::new("where.exe");
    cmd.arg("ssh").stdout(std::process::Stdio::piped());
    // 三条策略（`00 §1.5.2`）：
    // · `Hidden` —— 🔴 **这处先前是裸 `.output()`，也就是没人回答过这个问题**：
    //   monitor 是 `windows_subsystem = "windows"` 的 GUI app，起一个控制台子系统的
    //   `where.exe` 而不带 `CREATE_NO_WINDOW` ⇒ 用户桌面上会闪一个黑框。
    //   这正是「唯一出口」顺手关掉的那一族。
    // · `JobKillOnClose` —— 一次性探测，我们就地等它退；掐断时不许留后代。
    // · `Captured` —— 它的输出**是返回值**（`status.success()` 那一格），不是被丢了。
    spawn_managed_cmd(
        &mut cmd,
        ConsolePolicy::Hidden,
        Lifetime::JobKillOnClose,
        StderrSink::Captured,
    )
    .and_then(|c| c.wait_with_output())
    .map(|o| o.status.success())
    .unwrap_or(false)
}

/// 通用「远端终端拉起」命令（账本最终形态；F41 resume / F51 attach / F52 tmux /
/// F53 launcher 共用——remote_cmd 语义由前端 `remote-launch.ts` 的各 build 函数决定）。
/// `remote_cmd` 前端已过 sid 白名单 / launcher denylist / POSIX 引号，本侧再验一层
/// （控制字符 / 双引号 / 长度）——双层防线。
#[tauri::command]
pub async fn launch_remote_terminal(origin: String, remote_cmd: String) -> Result<(), String> {
    // ★★ **本机也走这条**〔用户裁定 08-12：「attach 暂时就用纯 linux bash 以及 windows 的
    // PowerShell + Windows Terminal」〕。
    //
    // 在此之前 `<local>` 会掉进下面那句 `load_remote_config_by_label`，报
    // **「未找到远端配置: "<local>"」** —— 与真实原因毫无关系的一句话。
    // 同一族错误文案本轮第四次遇到（前三次：`daemon_kill` · `list_remote_tmux` · 本条）。
    //
    // 两侧各按裁定走，**没有 ssh 那一跳**：
    // · Windows → `launch_powershell_window`（PowerShell + Windows Terminal，与远端同一个函数）；
    // · POSIX   → 那个函数的非 Windows 臂回 `POSIX_NO_TERMINAL_WINDOW`，前端据此把命令交给用户
    //   在自己的 bash 里执行。**这不是失败**，前端有专门的标题分档（`POSIX_NO_WINDOW_MARKER`）。
    if origin == crate::inbound_client::LOCAL_ORIGIN {
        return tokio::task::spawn_blocking(move || {
            launch_powershell_window(&remote_cmd, None)?;
            tracing::info!("launch: local terminal (no ssh)");
            Ok::<(), String>(())
        })
        .await
        .map_err(|e| format!("拉起终端任务失败: {e}"))?;
    }
    // §10（Phase G 对齐）:体含 `where.exe .output()`(阻塞)+ 进程 spawn 等阻塞 OS 调用,
    // 挪到阻塞线程池,不堵 IPC 派发线程(与本地 resume 命令 issue #12 同处理,批内唯一
    // 遗留的 sync tauri 命令——F41 从 history.rs 抽 launch.rs 时漏跟)。
    tokio::task::spawn_blocking(move || {
        let cfg = crate::load_remote_config_by_label(&origin)
            .ok_or_else(|| format!("未找到远端配置: {origin:?}"))?;
        #[cfg(windows)]
        if !ssh_client_available() {
            return Err(
                "本机未检测到 OpenSSH 客户端（ssh.exe）——请安装 Windows 可选功能「OpenSSH 客户端」"
                    .into(),
            );
        }
        let ps_command = build_remote_ssh_ps_command(&cfg, &remote_cmd)?;
        launch_powershell_window(&ps_command, None)?;
        tracing::info!("launch: remote terminal via ssh origin={origin}");
        Ok(())
    })
    .await
    .map_err(|e| format!("拉起终端任务失败: {e}"))?
}

#[cfg(test)]
#[path = "../../../tests/bridge/launch_tests.rs"]
mod tests;
