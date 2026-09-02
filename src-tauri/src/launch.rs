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
///   ★ 同一条推理本仓在别处写对过：`doc/IPC-PROTOCOL.md` 逐字
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
/// ⇒ 出口作为参数进来，测试传 `None` 走无窗口那条。
///
/// ⚠ 代价如实登记：**开窗那条路因此没有行为级判据**（只有形状判据）。
/// 真机验收归 `auto-e2e`，别把「形状对」读成「窗口真开出来了」。
#[cfg(not(windows))]
fn launch_local_posix_via(cmd: &str, cwd: Option<&str>, term: Option<&str>) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let argv = build_local_posix_argv(cmd)?;
    // ★★ P5L-Y1（`U14`〔用 08-12〕「要做，功能必须一样」）：**真开一个终端窗口**。
    //
    // 改之前这里是 `Stdio::null()` 直接 spawn ⇒ 产出一个**无 tty、无窗口**的进程
    //（`P3t` 摸底记下的那条：用户敲进去的字会被脚本吃掉）。
    //
    // ⚠ **载荷一个字不改**：`argv` 原样进终端的参数位。本件只加「怎么开窗」，不碰「开什么」。
    let (mut builder, opened_window) = match term {
        Some(term) => {
            let mut b = Command::new(term);
            // `xdg-terminal-exec` / `x-terminal-emulator` 都收 `-- <argv…>` 之后的命令行。
            b.arg("--").args(&argv);
            (b, true)
        }
        None => {
            // 诚实降级：一个规范化出口都没有 ⇒ 回落到改之前那条无窗口的路，**并说清**。
            // 不静默、也不报一个与真实原因无关的错。
            tracing::warn!(
                "本机没有规范化终端出口（试过 {TERMINAL_EXITS:?}）—— \
                 回落到无窗口直起；命令仍会跑，但没有可交互的终端"
            );
            let mut b = Command::new(&argv[0]);
            b.args(&argv[1..]);
            (b, false)
        }
    };
    // 只有真实存在的目录才作起始目录（与 Windows 那条路同一条纪律）。
    if let Some(d) = cwd.filter(|c| std::path::Path::new(c).is_dir()) {
        builder.current_dir(d);
    }
    // F06b-1d（C9）：backend 把 daemon 路径交给它亲手开的这个窗口 —— 窗口里那次 `ccm resume`
    // 据此去调 `--resolve`（`shared/ccm::resolve_from_daemon`）。sidecar 不在就不设。
    if let Some((k, v)) =
        crate::backend::control::local_backend::daemon_bin_env_for_window(env!("CCM_TARGET_TRIPLE"))
    {
        builder.env(k, v);
    }
    builder
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = builder
        .spawn()
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
#[cfg(windows)]
pub fn launch_powershell_window(ps_command: &str, local_cwd: Option<&str>) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

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
    if wt.spawn().is_ok() {
        tracing::info!("launch: powershell window via wt.exe");
        return Ok(());
    }

    // Plan B：powershell.exe + CREATE_NEW_CONSOLE，conhost 兜底。
    let mut builder = Command::new("powershell.exe");
    builder.args(ps_args);
    builder.creation_flags(CREATE_NEW_CONSOLE);
    // F06b-1d（C9）：同上。这一格是**真新起的进程**，env 一定继承。
    if let Some((k, v)) = daemon_env {
        builder.env(k, v);
    }
    if let Some(d) = start_dir {
        builder.current_dir(d);
    }
    builder
        .spawn()
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
    std::process::Command::new("where.exe")
        .arg("ssh")
        .output()
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
mod tests {
    use super::*;

    fn cfg(host: &str, user: &str, port: u16, key: Option<&str>) -> RemoteConfig {
        RemoteConfig {
            host: host.into(),
            label: "t".into(),
            port,
            user: user.into(),
            key_path: key.map(String::from),
            daemon_path: "d".into(),
            host_key_fingerprint: None,
            addresses: Vec::new(),
            jump: None,
            daemonless: false,
        }
    }

    /// ★ U8b：**POSIX 上「不开终端窗口」是既定设计，文案不许暗示「以后会支持」。**
    ///
    /// 原文案「拉起终端窗口仅支持 Windows（v1）」里那个 `(v1)` 在撒谎 —— L1 早就裁决过
    /// 反方向（`launch_local_posix` 头注：开窗要先猜终端模拟器，是平白引入一个会在别人
    /// 机器上错的决定）。用户在 Linux 上每次点 ↗ 都会读到那句话。
    /// ★★ **F06b-1d（C9）：backend 开的每一个终端窗口都必须带上 daemon 路径。**
    ///
    /// 判据形态：**零命中守卫**（跑法：单测扫生产源码 · 钉的性质：**生产接线** ——
    /// 两维分开写，见 `ROADMAP §4` 登记的计量缺陷）。
    ///
    /// # 它防的是什么
    ///
    /// 「给窗口设 env」这件事**没有集中落点**：本文件有三个各自 spawn 的开窗点
    /// （POSIX 一个、Windows 的 `wt.exe` 与 conhost 兜底各一个）。**新加第四个而忘了带 env**，
    /// 后果是那条路上的 `ccm resume` **静默地**永远走本地 —— 与名字打错同一族的静默失败。
    /// ⇒ 用「`Command::new(` 的总数」当触发器：多一个就红，逼人回来看这条。
    ///
    /// ⚠ **别手搓剥测试的尺子**：用 `guard_core::production_code`（仓里已有那把）。
    /// 不剥的话，下面那两条**测试里的字符串字面量** `"Command::new(\"gnome-terminal\")"`
    /// 会被数进去 —— 实测裸数是 6，生产里只有 4。
    #[test]
    fn every_terminal_window_backend_opens_carries_the_daemon_path() {
        let src = include_str!("launch.rs");
        let prod = guard_core::production_code(src);
        let spawns = prod.matches("Command::new(").count();
        // ⚠ 钉**真正的动作** `.env(k, v)`，不是 helper 的调用次数：
        //    Windows 那个函数**只调一次 helper**，把结果给两个 spawn 点共用
        //    ⇒ 按 helper 数写地板会写成 3，实测 2（第一版就这么错的，被本条自己逮住）。
        let helper = prod.matches("daemon_bin_env_for_window(").count();
        let carried = prod.matches(".env(k, v)").count();
        // 抽取器自检：剥完必须还看得见东西，且**确实剥掉了**测试里那两条字面量。
        assert!(
            prod.len() > 5_000,
            "剥完只剩 {} 字节 —— 剥过头了，本条会零命中地绿",
            prod.len()
        );
        assert!(
            !prod.contains("gnome-terminal"),
            "测试段没剥干净（`gnome-terminal` 只出现在测试的字符串字面量里）"
        );
        // 生产里 4 个：三个开窗点 + 一个 `where.exe` 探测（它不是开窗，不需要 env）。
        assert_eq!(
            spawns, 5,
            "`launch.rs` 生产代码里的 `Command::new(` 从 4 变成了 {spawns}。\n\
             若新增的是**开终端窗口**，它必须也带上 `daemon_bin_env_for_window(...)` 的 env，\n\
             否则那条路上的 `ccm resume` 会**静默地**永远走本地（与名字打错同一族的静默失败）；\n\
             若新增的只是探测进程（如 `where.exe`），把本条的数字与这句说明一起更新。"
        );
        assert_eq!(
            carried, 3,
            "真的把 env 交给窗口的 spawn 点从 3 变成了 {carried} —— 有开窗点漏了，或有人删了它"
        );
        assert_eq!(
            helper, 2,
            "解析 sidecar 路径的调用点从 2 变成了 {helper}（POSIX 一次 · Windows 一次给两个 spawn 共用）"
        );
        // 那个不带 env 的必须是探测，不是开窗：钉住它的身份，别让「探测」变成豁免借口。
        assert!(
            prod.contains("Command::new(\"where.exe\")"),
            "唯一允许不带 daemon env 的 `Command::new` 是 `where.exe` 探测；它不见了 ⇒ \n\
             要么被改名，要么 4-3=1 这个差额现在对应的是一个**真开窗点**"
        );
    }

    /// ★ **那一句无条件断言不许出现在散文里**〔audit-0805 F08 下半〕
    ///
    /// ⚠ **08-06 把这条标题收窄了**：原来写的是「『容器一定是 tmux』这个**无条件说法**
    /// 不许出现」—— 那比它实际做的宽。实测两个洞：
    /// ① **同义改写不认**：往被扫文件里写「远端会话的容器一定是 tmux」，本条**不红**
    ///    （needle 是一个精确短语，不是「无条件说法」这个语义）；
    /// ② **扫描面是洞**（已修）：把**原句**写进 `doc/DEVELOPMENT.md`（原先只扫三份）也**不红**。
    ///
    /// ①**刻意不追**，理由是量过的：想把它翻成本仓偏好的枚举式白名单
    /// （「每一行同时出现『容器』与 tmux 的散文都必须说清是哪条路」），
    /// 实测 11 行里 **9 行会误红** —— 那 9 行大多是**订正句**（「那是假的」）与本守卫自己的代码。
    /// 又一次「人群不同质」（同 `platform/fallback_guard` 那条）：
    /// 关于同一话题的散文里混着**断言、订正、判据本身**三类，白名单分不开它们。
    /// ⇒ 保留精确 needle，但**把话说准**：它挡的是那一句原文的回潮，不是一族说法。。
    ///
    /// 它今天在三处散文里当**理由**用（解释 POSIX 为什么不开终端窗口），而代码说的相反：
    /// POSIX 远端 `↺` 走 `runRemoteResume` → `planResumeDirect`，
    /// 那里逐字写着 `container: { kind: "none" }`（`launch-requests.ts:45`，
    /// 是全文件唯一一个 `none`，其余四个 plan 才是 tmux）。
    ///
    /// F08 上半订正过**两条**同源假头注（`launch.rs:122` 与 `src/fork-start.ts`），
    /// 但**漏了这三处** —— 因为它们在**另一条路**（远端）上，看起来像是另一件事。
    /// ⇒ 复核时才发现（E1：台账是筛子不是免检章）。
    ///
    /// ⚠ 这不是说「远端永远不进 tmux」：`planResumeTmux`（F52）那条**就是** tmux。
    /// 假的是**无条件的那个说法**，以及拿它当「不开终端窗口」的理由。
    /// 要说容器，就得说清是哪条路。
    ///
    /// ⚠ needle **运行时拼**：写成字面量的话，本条会在**自己的注释里**找到它 ⇒ 恒红
    /// （F23 那一族的镜像）。
    #[test]
    fn no_prose_claims_the_session_container_is_always_tmux() {
        let needle = format!("会话容器{}是 tmux", "本来就");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根");
        // 〔08-06 扩面〕原来只扫三份。实测：把**原句**写进 `doc/DEVELOPMENT.md`
        // （不在那三份里）**不会红** —— 扫描面本身就是个洞。⇒ 改成「全 `doc/` + 两份 README + 本文件」。
        //
        // ⚠ 用 `scan_tree!` 而**不是裸 `read_dir`**：`scanning_guard_registry` 那条元判据
        // 当场把裸遍历拦下了（理由是「判据在自己那份里找到自己 ⇒ 恒绿」，实测五次）。
        // 这里扫的是 `doc/` 的 md、与本文件不同族，但**规矩就是规矩** —— 而且它本来就更省事。
        let mut files: Vec<(String, String)> = Vec::new();
        for (q, body) in guard_core::scan_tree!(&root.join("doc"), &["md"]) {
            files.push((
                format!("doc/{}", q.file_name().expect("文件名").to_string_lossy()),
                body,
            ));
        }
        for f in ["README.md", "README.en.md", "src-tauri/src/launch.rs"] {
            let body = std::fs::read_to_string(root.join(f))
                .unwrap_or_else(|e| panic!("{f} 读不到：{e} —— 文件搬了就把本条一起改"));
            files.push((f.to_string(), body));
        }
        // ★★ 〔P3b 08-12 第二次扩面〕**加 `e2e/` 与 `src/`**。
        //
        // 08-06 那次（`c87d123`）的账是「扫描面本身是个洞」，扩到了 `doc/` + 两份 README。
        // 今天再量：**洞还在，只是挪了个位置** —— `e2e/restart-cmd-driver.ts:6` 那句
        // 「GUI 全链在 Linux 结构性不可达（launch.rs 仅 Windows→回退剪贴板）」
        // 就躺在扫不到的地方，而 `launch.rs::launch_local_posix` 明明就在本文件里。
        //
        // ⇒ **这不是巧合**：扫描面按「想到哪扫哪」长出来，而假话按「写在哪就在哪」分布。
        // 两者的形状不一样，所以「上次扩过了」不等于「这次够了」。
        for (dir, exts) in [("e2e", &["ts", "sh", "md"][..]), ("src", &["ts"][..])] {
            for (q, body) in guard_core::scan_tree!(&root.join(dir), exts) {
                files.push((
                    format!("{dir}/{}", q.file_name().expect("文件名").to_string_lossy()),
                    body,
                ));
            }
        }
        // ★★ **扩面自检用「比例」不用「绝对下限」**〔D 阶段补审 08-12 自查改的〕。
        //
        // 第一版写 `>= 90`，而实测真实人群是 **365**（`doc` 11 + `e2e` 30 + `src` 321 + 固定 3）
        // ⇒ 可以**静默少扫 275 份**而本条照绿。
        //
        // ⚠ 这个仓**刚刚才教过我**：`shell_lint_registry` 的账逐字写着
        // 「`≥` 正是它落后三次的成因 —— 加脚本时它不响，于是没人回来棘」，
        // 而我在同一天写了同一个形状。⇒ 「读过那条教训」不等于「写代码时想得起来」。
        //
        // 不写成等号是因为**这个人群天天在变**（`src/*.ts` 加一个文件就变）——
        // 等号会天天假红，那正是铁律 18 说的「假阳会训练人绕过判据」。
        // 折中：**按目录分族各自要够**（少了哪一族就点名哪一族），
        // 总数只做一个「明显坏了」的兜底。人群不同质 ⇒ 自检也不该只有一个数。
        {
            let by = |pre: &str| files.iter().filter(|(f, _)| f.starts_with(pre)).count();
            for (pre, floor) in [("doc/", 8usize), ("e2e/", 20), ("src/", 250)] {
                let n = by(pre);
                assert!(
                    n >= floor,
                    "`{pre}` 只收到 {n} 份（地板 {floor}）—— 那一族的扫描面塌了，\n\
                     而总数可能因为别族还在而看起来正常。**分族数就是为了让这种塌方点名可见**。"
                );
            }
            assert!(
                files.len() >= 300,
                "总共只收到 {} 份散文 —— 明显坏了（08-12 实测 365）",
                files.len()
            );
        }
        let mut total = 0usize;
        let mut hits = Vec::new();
        for (f, body) in &files {
            total += body.len();
            let n = body.matches(needle.as_str()).count();
            if n > 0 {
                hits.push(format!("  {f}：{n} 处"));
            }
        }
        // 抽取器自检：全部散文都读到了才算数（读空了下面会零命中地绿）。
        assert!(
            total > 20_000,
            "三份散文只读到 {total} 字节 —— 抽取器坏了，本条此刻是空转的"
        );
        assert!(
            hits.is_empty(),
            "这些散文还在无条件断言「会话容器就是 tmux」，而代码说的相反：\n{}\n\n\
             POSIX 远端 `↺` 走 `planResumeDirect`，那里是 `container: {{ kind: \"none\" }}`\n\
             （`launch-requests.ts:45`，全文件唯一一个 `none`）。判据在\n\
             `launch-requests.vitest.ts` 的「远端 resume 的会话容器」那一组。\n\
             ★ 要说 tmux，就得说清**是哪条路**（`planResumeTmux` 那条才是）——\n\
             无条件的说法是假的，而它今天正被当成「POSIX 不开终端窗口」的理由。",
            hits.join("\n")
        );
    }

    #[test]
    fn the_posix_message_states_a_decision_not_a_missing_feature() {
        let m = POSIX_NO_TERMINAL_WINDOW;
        assert!(
            !m.contains("v1") && !m.contains("v2"),
            "文案里带版本号会被读成「以后会支持」：{m}"
        );
        assert!(m.contains("刻意"), "没说清这是刻意的：{m}");
        assert!(
            m.contains("tmux"),
            "没说清会话容器是什么，用户不知道去哪找：{m}"
        );
        assert!(m.contains("既定设计"), "没有把「这不是没做完」说出来：{m}");
    }

    /// ★ U8b **跨轨对拍**：前端匹配的那个标记，必须真的在后端那句话里。
    ///
    /// 前端据它把标题从「拉起失败」换成「本机不开终端窗口」。两边漂开的症状是
    /// **静默退回**：用户又开始在 Linux 上每次点 ↗ 都读到「拉起失败」，而两边各自看都对。
    #[test]
    fn the_posix_marker_is_the_one_the_frontend_matches_on() {
        const RUNNER: &str = include_str!("../../src/remote-launch-run.ts");
        let key = "export const POSIX_NO_WINDOW_MARKER = \"";
        let at = RUNNER
            .find(key)
            .expect("前端找不到 POSIX_NO_WINDOW_MARKER —— 抽取坏了，本断言在空转");
        let rest = &RUNNER[at + key.len()..];
        let marker = &rest[..rest.find('"').expect("字面量没收尾")];
        assert!(
            marker.chars().count() >= 6,
            "抽到的标记太短（{marker:?}）—— 抽取坏了"
        );
        assert!(
            POSIX_NO_TERMINAL_WINDOW.contains(marker),
            "\n前端按 {marker:?} 判「这是既定设计」，但后端那句话里没有它：\n  {POSIX_NO_TERMINAL_WINDOW}\n\
             ⇒ 用户会退回去看到「拉起失败」。两边必须一起改。"
        );
        // 反面：标记不许宽到把**真失败**也软化掉。
        for real_failure in [
            "未找到远端配置: \"x\"",
            "refuse launch: 远端命令含控制字符",
            "spawn powershell failed: No such file",
        ] {
            assert!(
                !real_failure.contains(marker),
                "标记 {marker:?} 太宽，会把真失败 {real_failure:?} 也报成「既定设计」"
            );
        }
    }

    /// ★ P5L-Y1/Y3：**候选表有序，且第一个存在的胜出**。
    #[test]
    fn the_terminal_exit_is_picked_in_declared_order() {
        // 都在 ⇒ 取第一个（`xdg-terminal-exec` 优先，见 `TERMINAL_EXITS` 头注）。
        assert_eq!(
            pick_terminal_exit_from(TERMINAL_EXITS, &|_| true),
            Some("xdg-terminal-exec")
        );
        // 只有第二个在 ⇒ 取第二个。
        // 只有它不在 ⇒ `None`（表里今天只有一个，见 `TERMINAL_EXITS` 的 D 阶段补审）。
        assert_eq!(
            pick_terminal_exit_from(TERMINAL_EXITS, &|c| c == "x-terminal-emulator"),
            None
        );
        // 一个都不在 ⇒ `None`，调用方据此诚实降级（`P5L-Y2`）。
        assert_eq!(pick_terminal_exit_from(TERMINAL_EXITS, &|_| false), None);
        // 表本身：**只准放规范化出口**，一个具名终端都不许有。
        // ⚠ **每加一个出口都要先核它的参数约定**（D 阶段补审：`--` 只对 `xdg-terminal-exec`
        // 与 ptyxis 核实过；`x-terminal-emulator` 在别的机器上可能是 `-e`）。
        assert_eq!(TERMINAL_EXITS, &["xdg-terminal-exec"]);
    }

    /// ★ P5L-Y1：**载荷原样进终端的参数位** —— 本件只加「怎么开窗」，不碰「开什么」。
    ///
    /// 钉的是源码形状：`argv` 整个进 `args(&argv)`，且前面隔着一个 `--`
    ///（否则终端会把 `bash` 之后的东西当成自己的选项解析）。
    #[test]
    fn opening_a_window_does_not_touch_the_payload() {
        let prod = guard_core::production_code(include_str!("launch.rs"));
        assert!(
            prod.contains("b.arg(\"--\").args(&argv);"),
            "开窗那条路没有把 `argv` **整个原样**交出去 —— \n\
             本件的全部承诺是「只加怎么开窗，不碰开什么」，这一行就是那句话本身。"
        );
        // 无窗口那条回落必须还在（`P5L-Y2`）：它是「一个出口都没有」时的唯一去处。
        assert!(
            prod.contains("Command::new(&argv[0])"),
            "无窗口回落没了 —— 那样在没有规范化出口的机器上会变成「点了没反应」"
        );
        // ⚠ **分流必须真的看 `term`**〔变异 M2 逼出来的〕：只钉「源码里有开窗那段」的话，
        // 把 `match term` 换成 `match None::<&str>` 它照样绿 —— 那段代码还在，只是**走不到**。
        assert!(
            prod.contains("match term {"),
            "开窗那条分流不再按入参 `term` 走 —— 那段代码可能还在，但已经**走不到**了"
        );
        // P5L-Y2：降级必须**说清**，不是静默。
        //
        // ⚠ **钉「它是一条真日志」，不只是钉那句话在**〔变异 M4 逼出来的〕：
        // 把 `tracing::warn!` 换成 `format!`，文案原样留着，判据照样绿 ——
        // 而那时那句话**谁也看不到**。⇒ 按位置钉：文案前面不远处必须有 `tracing::warn!`。
        let at = prod
            .find("回落到无窗口直起")
            .expect("降级那条路不再说明原因 —— 用户会看到「点了没反应」而无从归因");
        let before = &prod[at.saturating_sub(200)..at];
        assert!(
            before.contains("tracing::warn!"),
            "降级的说明不是一条真日志（前 200 字节里没有 `tracing::warn!`）——\n\
             文案留着而日志没了，等于那句话谁也看不到。"
        );
    }

    /// ★ P5L-Y3：候选表的**理由**必须写在源码里（必需词守卫，同 `P4b-Y3` / `PS1`）。
    #[test]
    fn the_terminal_exit_table_says_why_it_refuses_to_pick() {
        let me = include_str!("launch.rs");
        // ⚠ 数次数而不是 `contains` —— 本判据自己的字面量也在这个文件里
        //（本会话第四次栽在「判据被自己要钉的名字命中」上）。
        for must in ["绝不在这里列", "交还给桌面"] {
            assert!(
                me.matches(must).count() >= 2,
                "`TERMINAL_EXITS` 的头注里少了 {must:?}。\n\
                 那段话记的是「为什么不探测具体终端」——用户逐字「暂时不考虑其他终端」。\n\
                 删掉它，下一个人就会顺手加一行 `gnome-terminal`。"
            );
        }
    }

    /// ★ U8b：**`launch.rs` 的生产段不许出现任何具名终端模拟器。**
    ///
    /// 零命中型判据，钉的是 L1 那条裁决。它挡的是很自然的一个「顺手改进」：
    /// 有人看到 Linux 上开不了窗，加一段 `gnome-terminal` / `alacritty` 探测。
    /// 那不是清理，是**产品决定** —— 要做就先答「探测顺序是什么、找不到怎么办」，
    /// 而不是静默挑一个（挑错了用户会看到一个空白窗口或什么都没有，且极难归因）。
    ///
    /// # ★★ P5L（08-12）：那两问**答了**，于是本条放行两个**规范化出口**
    ///
    /// `U14`〔用 08-12〕已裁「**要做，功能必须一样**」，而本条头注自己写的开锁条件
    /// （「先答探测顺序 / 找不到怎么办」）逐条答完：
    /// · **顺序**：`TERMINAL_EXITS` 是一张具名的有序常量（`xdg-terminal-exec` → `x-terminal-emulator`）；
    /// · **找不到**：诚实降级回无窗口那条，并 `warn` 说清（`P5L-Y2`）。
    ///
    /// ⇒ 放行的是**规范化出口**，不是终端本身：这两个都把「用哪个终端」交还给桌面/发行版配置
    /// （用户逐字「**纯 bash 的意思是暂时不考虑其他终端**」反对的是终端**专属集成**）。
    /// **具名终端一个都不放行** —— 那才是「挑」，也正是本条原本要挡的东西。
    #[test]
    fn no_terminal_emulator_is_ever_spawned_from_this_file() {
        // 运行时拼，避免命中本行自己。
        let emulators: Vec<String> = [
            "gnome-termin",
            "konsol",
            "xterm",
            "alacritt",
            "kitt",
            "wezter",
            "foot",
            "Terminal.ap",
            "iTerm",
        ]
        .iter()
        .map(|s| format!("{s}{}", ""))
        .collect();
        // 匹配器自检：独立手写的样本必须被这份名单命中（防名单写坏了导致零命中恒绿）。
        for sample in [
            "Command::new(\"gnome-terminal\")",
            "Command::new(\"alacritty\")",
            "Command::new(\"kitty\")",
        ] {
            assert!(
                emulators.iter().any(|e| sample.contains(e.as_str())),
                "匹配器漏了这种写法：{sample} —— 下面那句「零命中」对它毫无意义"
            );
        }
        let prod = guard_core::production_code(include_str!("launch.rs"));
        let hits: Vec<&String> = emulators
            .iter()
            .filter(|e| prod.contains(e.as_str()))
            .collect();
        assert!(
            hits.is_empty(),
            "`launch.rs` 的生产段出现了终端模拟器（{hits:?}）。\n\
             L1 裁决过：POSIX 上没有「唯一的终端」，挑一个是平白引入一个会在别人机器上错的决定。\n\
             真要做就先答「探测顺序 / 找不到怎么办」，并当成产品决定走一遍计划 —— 别静默挑一个。\n\
             ⚠ P5L 已按这条开锁条件放行了**规范化出口**（`TERMINAL_EXITS`：xdg-terminal-exec / \
             x-terminal-emulator）——它们把选择权交还给桌面，与「挑一个具名终端」是两回事。"
        );
    }

    /// ★ L1 的验收判据（主计划 §2「关键判断」第 1 条逐字）：
    /// **给同一个 plan 换 transport，除 ssh 包装外输出逐字节相同。**
    ///
    /// 这条测试就是那句话的机器版：把远端命令体里的 ssh 包装层层剥掉，
    /// 剩下的必须与本地 argv **逐字节**相等。任何一侧偷偷加/减修饰都会红。
    #[test]
    fn local_and_remote_share_the_same_payload() {
        let payload = "unset CLAUDE_CONFIG_DIR; cd '/home/z/p' && ccm --tmux claude --resume s1";
        let local = build_local_posix_argv(payload).unwrap();
        assert_eq!(
            local,
            vec!["bash", "-lic", payload],
            "本地：直接 exec，无 ssh 包"
        );

        let c = cfg("h", "u", 22, None);
        let remote = build_remote_ssh_ps_command(&c, payload).unwrap();
        // 剥 ssh 层 → 剥 PS 单引号层 → 得到与本地同构的 `bash -lic <quoted>`
        let after_dashdash = remote.rsplit_once("-- ").unwrap().1;
        let inner = after_dashdash
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
            .expect("ps quoted")
            .replace("''", "'");
        assert_eq!(
            inner,
            format!("bash -lic {}", posix_quote(payload)),
            "远端：同一个 payload，只多了 ssh + PS 两层包装"
        );
        // ★ 逐字节：把远端**每一层包装都反解**之后，得到的必须就是本地那一串。
        //（不能写成 `inner.contains(payload)` —— payload 里有单引号，`posix_quote`
        //  会把它变成 `'\''`；那条会误报，而它误报说明的恰恰是「包装确实存在」。）
        let unwrapped = inner
            .strip_prefix("bash -lic '")
            .and_then(|s| s.strip_suffix('\''))
            .expect("bash -lic 层")
            .replace(r"'\''", "'");
        assert_eq!(
            unwrapped, local[2],
            "剥净包装后，两条路送的是同一串（逐字节）"
        );
    }

    /// 与传输无关的三条校验，本地那条路**同样**生效（不是只有远端才验）。
    #[test]
    fn local_argv_shares_the_transport_agnostic_validation() {
        assert!(build_local_posix_argv("   ").is_err(), "空命令");
        assert!(
            build_local_posix_argv(&"x".repeat(MAX_REMOTE_CMD + 1)).is_err(),
            "超长"
        );
        assert!(build_local_posix_argv("a\nb").is_err(), "控制字符");
        assert!(build_local_posix_argv("claude --resume s1").is_ok());
    }

    /// ★ 「拒绝双引号」是 **PowerShell 5.1 的怪癖**，不是命令本身的性质
    /// ⇒ 它只该拦远端那条路，**不该**跟着搬到 POSIX 本地。
    ///
    /// 判据落在性质上，不落在表面特征上：把一个 Windows 传参畸变套到 Linux 上，
    /// 会让本地路径无端拒绝一批合法命令。
    #[test]
    fn double_quote_rejection_is_powershell_only() {
        let with_dq = r#"claude --append-system-prompt "be brief""#;
        assert!(
            build_remote_ssh_ps_command(&cfg("h", "u", 22, None), with_dq).is_err(),
            "远端（走 PowerShell）应拒"
        );
        assert!(
            build_local_posix_argv(with_dq).is_ok(),
            "POSIX 本地不经 PowerShell，不该拦"
        );
    }

    /// ★ L1：`launch_local_posix` 的 **spawn 那半**真的会跑起来（此前无覆盖）。
    ///
    /// 用一条无害命令写一个标记文件来观测。**刻意不起任何 agent**。
    /// 它不是 hermetic 的（`bash -lic` 会 source 用户 rc）—— 但要验的正是
    /// 「按我们给的 argv 真的 exec 了」，而 rc 的存在恰恰是生产形态的一部分。
    #[cfg(not(windows))]
    #[test]
    fn local_posix_spawn_actually_runs_the_command() {
        let dir = std::env::temp_dir().join(format!("l1-spawn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let marker = dir.join("ran");
        let cmd = format!("printf ok > {}", marker.display());
        // ★ P5L：**必须走 `None`（无窗口）那条** —— 否则这条真跑的判据会在开发者桌面上
        // **弹出一个真终端窗口**（实测跑过一次，本机 `xdg-terminal-exec` → `ptyxis`）。
        // 开窗那条路只有形状判据，真机验收归 `auto-e2e`（如实登记在 `launch_local_posix_via` 头注）。
        launch_local_posix_via(&cmd, dir.to_str(), None).expect("spawn 应成功");
        // 轮询等它落地（spawn 是异步的；上限宽松，判的是「跑没跑」不是快慢）。
        let mut seen = false;
        for _ in 0..100 {
            if marker.is_file() {
                seen = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let content = if seen {
            std::fs::read_to_string(&marker).unwrap_or_default()
        } else {
            String::new()
        };
        let _ = std::fs::remove_dir_all(&dir);
        assert!(seen, "5s 内没看到标记文件——spawn 那半没真跑");
        assert_eq!(content, "ok", "命令跑了但内容不对");
    }

    #[test]
    fn build_basic_agent_auth() {
        let c = cfg("pi.local", "pi", 22, None);
        let remote = "unset X; cd '/home/pi' && claude --resume s1";
        let got = build_remote_ssh_ps_command(&c, remote).unwrap();
        assert!(
            got.starts_with("& ssh -t -p 22 pi@pi.local -- "),
            "基本形态（agent 无 -i）: {got}"
        );
        // 解码 PS 单引号层（'' → '，全量双写故非重叠替换可逆）应还原传输包装形态。
        let payload = got.rsplit_once("-- ").unwrap().1;
        let inner = payload
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
            .expect("ps quoted");
        assert_eq!(
            inner.replace("''", "'"),
            format!("bash -lic {}", posix_quote(remote))
        );
    }

    #[test]
    fn build_key_and_port() {
        let c = cfg("10.0.0.2", "u", 2222, Some(r"C:\Users\z's\id_ed25519"));
        let got = build_remote_ssh_ps_command(&c, "claude --resume s1").unwrap();
        assert!(got.starts_with("& ssh -t -p 2222 -i 'C:\\Users\\z''s\\id_ed25519' u@10.0.0.2 -- "));
        // PS 单引号层把 ' 双写：bash -lic 'claude…' → ''claude…''
        assert!(got.contains("bash -lic ''claude --resume s1''"), "{got}");
    }

    #[test]
    fn build_ipv6_host_ok() {
        let c = cfg("[::1]", "u", 22, None);
        assert!(build_remote_ssh_ps_command(&c, "claude --resume s1").is_ok());
    }

    #[test]
    fn build_jump_arg_variants() {
        // F56：默认 port 省 :port,非默认带;非法 user/host 拒。
        assert_eq!(
            build_jump_arg("pi", "jump.local", 22).unwrap(),
            " -J pi@jump.local"
        );
        assert_eq!(
            build_jump_arg("u", "10.0.0.1", 2222).unwrap(),
            " -J u@10.0.0.1:2222"
        );
        assert!(build_jump_arg("bad user", "h", 22).is_err(), "非法 user 拒");
        assert!(build_jump_arg("u", "h;rm -rf", 22).is_err(), "非法 host 拒");
    }

    #[test]
    fn reject_bad_inputs() {
        let c = cfg("h", "u", 22, None);
        assert!(build_remote_ssh_ps_command(&c, "").is_err(), "空命令拒");
        assert!(
            build_remote_ssh_ps_command(&c, "a\nb").is_err(),
            "控制字符拒"
        );
        assert!(
            build_remote_ssh_ps_command(&c, "cc --x \"y\"").is_err(),
            "双引号拒（PS native 畸变面）"
        );
        assert!(
            build_remote_ssh_ps_command(&c, &"a".repeat(5000)).is_err(),
            "超长拒"
        );
        let bad_user = cfg("h", "u ser", 22, None);
        assert!(
            build_remote_ssh_ps_command(&bad_user, "x").is_err(),
            "user 空格拒"
        );
        let empty_user = cfg("h", "", 22, None);
        assert!(
            build_remote_ssh_ps_command(&empty_user, "x").is_err(),
            "user 空拒"
        );
        let bad_host = cfg("h; rm", "u", 22, None);
        assert!(
            build_remote_ssh_ps_command(&bad_host, "x").is_err(),
            "host 注入拒"
        );
    }

    #[test]
    fn remote_cmd_single_quotes_survive_both_layers() {
        // cwd 带单引号：前端 posixQuote 产出 '\'' 序列，PS 层再双写——验证嵌套后形态可逆。
        let c = cfg("h", "u", 22, None);
        let remote = r"cd '/a'\''b' && claude --resume s1";
        let got = build_remote_ssh_ps_command(&c, remote).unwrap();
        // PS 单引号字面量内：每个 ' 变 ''。解码（'' → '）应还原出 bash -lic 'POSIX(remote)'。
        let ps_payload = got.rsplit_once("-- ").map(|(_, p)| p).expect("has payload");
        let inner = ps_payload
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
            .expect("ps quoted");
        let decoded = inner.replace("''", "'");
        assert_eq!(decoded, format!("bash -lic {}", posix_quote(remote)));
    }
}
