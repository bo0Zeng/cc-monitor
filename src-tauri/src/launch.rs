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
fn posix_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
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

/// 构造远端拉起的 PowerShell 命令体（不含 `-EncodedCommand` 编码）。
///
/// 形态：`& ssh -t[ -J <跳板>] -p <port> [-i '<key>'] <user>@<host> -- '<bash -lic ''…''>'`
/// （F45 竞发落地后 host 换成连接大脑当前胜者地址；F56 jump 有值插 `-J`——本函数签名不变。）
pub fn build_remote_ssh_ps_command(cfg: &RemoteConfig, remote_cmd: &str) -> Result<String, String> {
    if remote_cmd.trim().is_empty() {
        return Err("refuse launch: 远端命令为空".into());
    }
    if remote_cmd.len() > MAX_REMOTE_CMD {
        return Err(format!(
            "refuse launch: 远端命令过长（{} > {MAX_REMOTE_CMD}）",
            remote_cmd.len()
        ));
    }
    if remote_cmd.chars().any(|c| c.is_control()) {
        return Err("refuse launch: 远端命令含控制字符".into());
    }
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

#[cfg(not(windows))]
pub fn launch_powershell_window(_ps_command: &str, _local_cwd: Option<&str>) -> Result<(), String> {
    Err("拉起终端窗口仅支持 Windows（v1）".into())
}

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
pub fn launch_remote_terminal(origin: String, remote_cmd: String) -> Result<(), String> {
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
        }
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
