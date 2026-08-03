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

/// L1：在 **POSIX 本机**跑一条命令 —— 不经 ssh、不经 PowerShell。
///
/// 这是 §40「本地 = 不走 ssh 的远端」在传输层的落点：远端那条路是
/// `ssh -t host -- 'bash -lic <cmd>'`，本地就是把 `ssh` 那一跳**去掉**，其余不变。
///
/// 三处设计：
/// - **不开 GUI 终端窗口**。POSIX 上没有「唯一的终端」这种东西，而会话容器本来就是
///   tmux（`ccm --tmux` 自己会建）。开窗口要先猜用户用哪个终端模拟器，是平白引入一个
///   会在别人机器上错的决定。⇒ 命令直接跑，会话留在 tmux 里等 attach。
/// - **脱离 app 的进程组**（`process_group(0)`）+ stdio 全 null：
///   否则子进程会跟着 app 的 Ctrl-C 一起走，也会把 app 的 stdio 占住。
/// - **起一条线程收尸**。`process_group` 不改变父子关系 ⇒ 不 `wait` 就留僵尸。
///   线程随子进程结束而结束（`ccm` 建完会话就返回，是短命进程）。
#[cfg(not(windows))]
pub fn launch_local_posix(cmd: &str, cwd: Option<&str>) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let argv = build_local_posix_argv(cmd)?;
    let mut builder = Command::new(&argv[0]);
    builder.args(&argv[1..]);
    // 只有真实存在的目录才作起始目录（与 Windows 那条路同一条纪律）。
    if let Some(d) = cwd.filter(|c| std::path::Path::new(c).is_dir()) {
        builder.current_dir(d);
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
    tracing::info!("launch: local posix exec (no ssh)");
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
    if Command::new("wt.exe").args(&wt_args).spawn().is_ok() {
        tracing::info!("launch: powershell window via wt.exe");
        return Ok(());
    }

    // Plan B：powershell.exe + CREATE_NEW_CONSOLE，conhost 兜底。
    let mut builder = Command::new("powershell.exe");
    builder.args(ps_args);
    builder.creation_flags(CREATE_NEW_CONSOLE);
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
/// 会在别人机器上错的决定**。而会话容器本来就是 tmux —— 命令跑完，会话留在那儿等 attach。
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
#[cfg(any(not(windows), test))]
pub const POSIX_NO_TERMINAL_WINDOW: &str =
    "本机不是 Windows：cc-monitor **刻意不替你挑终端模拟器**（会话容器是 tmux）——\
     命令已复制，在你自己的终端里粘贴执行即可。这是既定设计，不是没做完。";

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

    /// ★ U8b：**`launch.rs` 的生产段不许出现任何终端模拟器。**
    ///
    /// 零命中型判据，钉的是 L1 那条裁决。它挡的是很自然的一个「顺手改进」：
    /// 有人看到 Linux 上开不了窗，加一段 `gnome-terminal` / `x-terminal-emulator` 探测。
    /// 那不是清理，是**产品决定** —— 要做就先答「探测顺序是什么、找不到怎么办」，
    /// 而不是静默挑一个（挑错了用户会看到一个空白窗口或什么都没有，且极难归因）。
    #[test]
    fn no_terminal_emulator_is_ever_spawned_from_this_file() {
        // 运行时拼，避免命中本行自己。
        let emulators: Vec<String> = [
            "gnome-termin",
            "konsol",
            "xterm",
            "x-terminal-emulato",
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
            "spawn(\"x-terminal-emulator\")",
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
             真要做就先答「探测顺序 / 找不到怎么办」，并当成产品决定走一遍计划 —— 别静默挑一个。"
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
        launch_local_posix(&cmd, dir.to_str()).expect("spawn 应成功");
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
