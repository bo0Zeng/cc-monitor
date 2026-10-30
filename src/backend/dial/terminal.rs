//! 〔「待迁」最后一行〕**给一台远端开终端要跑的那一串** —— `ssh -t[ -J …] …` 外壳，按这台电脑的终端方言渲
//! （Windows：PowerShell 窗口载荷；别处：一行 POSIX shell），在本机常驻后端里渲。
//!
//! 界面交来的是那台的名字（怎么拨照这一端的机器表组）＋ 要在那台跑的命令 `command`。组请求走唯一那一份 [`super::machine::request`]
//! （地址排序 · 跳板查无 / 环都在那里），这里只把它渲成一行：
//!
//! - PowerShell：`& '<ssh 全路径>' -t[ -J <跳板用户>@<跳板>[:口]] -p <口>[ -i '<钥匙>'] -F '<cc-monitor 那份 ssh 配置>' <用户>@<地址> -- '<bash -lic ''…''>'`
//! - POSIX：同一组参数，不带 `& `，引号按 POSIX 单引号写（`'bash -lic '\''…'\'''`）。
//!
//! - 地址取竞速顺序的第一条（照表里的次序）；
//! - 远端命令包成 `bash -lic '<命令>'`（PATH / 别名按「用户粘贴进交互终端」解析），再以本机方言的单引号字面量嵌入；
//! - 主机钥匙认 cc-monitor 那份库（[`super::known_hosts`]，经 `-F` 那份配置，跳板那一跳也认同一份），终端里不再问、不写用户的 `~/.ssh`；
//! - 钥匙路径尾 `\` 剥掉（PS < 7.3 给含空格参数加壳时尾部 `\"` 会吃掉收尾引号）；没给钥匙 ⇒ 走 ssh-agent，不带 `-i`。
//!
//! 拒（`refused`，说清哪一格）：命令空 / 超长 / 含控制符 · PowerShell 那一形另拒双引号（PowerShell 5.1 向原生程序传参对内嵌 `"`
//! 有历史畸变，那是**那条送法**的约束）· 用户名 / 地址 / 跳板用户 / 跳板地址出了白名单（它们是拼进命令体的裸词）。
//!
//! ssh 客户端用这台上找到的那一份的全路径（[`crate::platform::ssh_client`]：只查文件、不起子进程）；
//! 没装 ⇒ 不出那一行、说没装（`no_ssh_client`）；查的时候出了错 ⇒ 说判不了（`unobservable`），不说没装。
//! 不拨号、不开窗（开窗是 monitor 的事，它只开窗）。

use copy_core::copy_text;
use serde_json::Value;

use super::Host;
use crate::platform::shell::dialect::ps_literal;
use crate::platform::shell::posix;
use crate::platform::ssh_client::SshClient;

/// 远端命令长度上限（防异常输入；正常 resume 命令 < 300 字节）。与 monitor 本机那条送法的上限同值。
pub(crate) const MAX_COMMAND: usize = 4096;

type CmdErr = (&'static str, String);

fn refused(msg: String) -> CmdErr {
    ("refused", msg)
}

/// 用户名：非空，仅 `[A-Za-z0-9._-]`、不以 `-` 打头（拼进命令体的裸词，白名单杜绝注入；打头的 `-` 会被 `ssh` 读成选项）。
fn user_ok(u: &str) -> bool {
    !u.is_empty()
        && !u.starts_with('-')
        && u.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// 地址：非空，仅 `[A-Za-z0-9._:\[\]-]`、不以 `-` 打头（域名 / IPv4 / IPv6 字面量）。
fn host_ok(h: &str) -> bool {
    !h.is_empty()
        && !h.starts_with('-')
        && h.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | '[' | ']'))
}

/// 要在那台跑的命令：非空 · 不超长 · 无控制符 · PowerShell 那一形另要无双引号。
fn check_command(cmd: &str, powershell: bool) -> Result<(), CmdErr> {
    if cmd.trim().is_empty() {
        return Err(refused(copy_text("beTerminal.refuse.empty", &[])));
    }
    if cmd.len() > MAX_COMMAND {
        return Err(refused(copy_text(
            "beTerminal.refuse.tooLong",
            &[
                ("len", &cmd.len().to_string()),
                ("max", &MAX_COMMAND.to_string()),
            ],
        )));
    }
    if cmd.chars().any(char::is_control) {
        return Err(refused(copy_text("beTerminal.refuse.control", &[])));
    }
    if powershell && cmd.contains('"') {
        return Err(refused(copy_text("beTerminal.refuse.doubleQuote", &[])));
    }
    Ok(())
}

/// ` -J user@host[:port],…`（口 = 22 不带后缀；多跳从最外一台起，逗号相连，同 `ssh -J`）。
fn jump_arg(chain: &[&super::JumpHop]) -> Result<String, CmdErr> {
    let mut hops = Vec::new();
    for j in chain {
        if !user_ok(&j.user) {
            return Err(refused(copy_text(
                "beTerminal.refuse.jumpUser",
                &[("jumpUser", &format!("{:?}", j.user))],
            )));
        }
        if !host_ok(&j.host) {
            return Err(refused(copy_text(
                "beTerminal.refuse.jumpHost",
                &[("jumpHost", &format!("{:?}", j.host))],
            )));
        }
        let port = if j.port == 22 {
            String::new()
        } else {
            format!(":{}", j.port)
        };
        hops.push(format!("{}@{}{port}", j.user, j.host));
    }
    Ok(format!(" -J {}", hops.join(",")))
}

/// ` -F '<cc-monitor 那份 ssh 配置>'`：目标与 `-J` 起的每台跳板都认 cc-monitor 那份主机钥匙库（`-J` 起的子 ssh 只继承 `-F`、不吃 `-o`），
/// 不再问一遍、也不写用户的 `~/.ssh/known_hosts`；用户与系统的 ssh 配置照读（那份配置里 `Include` 着）。说不出那份 ⇒ 不带。
pub(crate) fn config_arg(path: Option<&std::path::Path>, powershell: bool) -> String {
    match path {
        Some(p) => format!(" -F {}", literal(powershell, &p.to_string_lossy())),
        None => String::new(),
    }
}

/// 本机终端方言的单引号字面量。
fn literal(powershell: bool, s: &str) -> String {
    if powershell {
        ps_literal(s)
    } else {
        shell_quote_core::posix_quote(s)
    }
}

/// 一份拨号请求 ＋ 命令 ⇒ 这台电脑终端里跑的那一行（`powershell` ＝ 本机终端是不是 PowerShell，见模块头注；`ssh` ＝ ssh 客户端全路径）。
/// `run` ＝ `--` 后面跟不跟请求里那串命令（不跟 ⇒ 那台自己的登录 shell，那串不验也不渲）。
pub(crate) fn render(
    req: &super::DialRequest,
    powershell: bool,
    ssh: &str,
    run: bool,
) -> Result<String, CmdErr> {
    if run {
        check_command(&req.command, powershell)?;
    }
    if !user_ok(&req.user) {
        return Err(refused(copy_text(
            "beTerminal.refuse.user",
            &[("user", &format!("{:?}", req.user))],
        )));
    }
    let first = req
        .race_order()
        .into_iter()
        .next()
        .unwrap_or(super::Endpoint {
            host: req.host.clone(),
            port: req.port,
        });
    if !host_ok(&first.host) {
        return Err(refused(copy_text(
            "beTerminal.refuse.host",
            &[("host", &format!("{:?}", first.host))],
        )));
    }
    let key = match req
        .key_path
        .as_deref()
        .map(|k| k.trim().trim_end_matches('\\'))
    {
        Some(k) if !k.is_empty() => format!(" -i {}", literal(powershell, k)),
        _ => String::new(),
    };
    let chain = ssh_dial::connect::hop_chain(req);
    let jump = if chain.is_empty() {
        String::new()
    } else {
        jump_arg(&chain)?
    };
    let tail = if run {
        let wrapped = format!("bash -lic {}", shell_quote_core::posix_quote(&req.command));
        format!(" -- {}", literal(powershell, &wrapped))
    } else {
        String::new()
    };
    let known = config_arg(super::known_hosts::terminal_config().as_deref(), powershell);
    Ok(format!(
        "{call}{ssh} -t{jump} -p {port}{key}{known} {user}@{host}{tail}",
        call = if powershell { "& " } else { "" },
        ssh = literal(powershell, ssh),
        port = first.port,
        user = req.user,
        host = first.host,
    ))
}

/// 「在此打开终端」要在那台跑的那一串（窗口只交意图 `{cwd}`，命令由这里拼）。
///
/// 逐字：`cd <quoted> && exec ${SHELL:-bash} -l`（`cwd` 为空 ⇒ 只有后半段）。当前目录是那台列出来的**自由文本路径** ⇒
/// 拼进 `cd` 之前先过形式 ＋ 拒绝集（`shell_quote_core::posix_free_path_ok`：POSIX 绝对 · 无 `..` 段 · 不含 NUL / CR / LF；
/// **不拒 shell 元字符**，交给唯一那一处 quote，`INVARIANTS §47` ②）；非 UTF-8 的目录走字节形（`posix_quote_bytes`，`$'…'`）。
/// ⚠ **不用双引号**：[`check_command`] 会拒掉含双引号的命令（PowerShell 原生传参畸变那道防线）。
pub(crate) fn command_for_cwd(cwd: &Value) -> Result<String, CmdErr> {
    let Ok(cmd_wire::files::WirePath(bytes)) = serde_json::from_value(cwd.clone()) else {
        return Err((
            "bad_args",
            crate::common::contract::malformed("`cwd` must be a string or {\"b16\": \"<hex>\"}"),
        ));
    };
    let bad = |shown: &str| {
        refused(copy_text(
            "beTerminal.refuse.badCwd",
            &[("cwd", &format!("{shown:?}"))],
        ))
    };
    match std::str::from_utf8(&bytes) {
        Ok(s) => {
            let c = s.trim();
            if c.is_empty() {
                Ok(posix::cd_then_login_shell(None))
            } else if !shell_quote_core::posix_free_path_ok(c) {
                Err(bad(c))
            } else {
                Ok(posix::cd_then_login_shell(Some(
                    &shell_quote_core::posix_quote(c),
                )))
            }
        }
        Err(_) if !shell_quote_core::posix_free_path_bytes_ok(&bytes) => {
            Err(bad(&String::from_utf8_lossy(&bytes)))
        }
        Err(_) => Ok(posix::cd_then_login_shell(Some(
            &shell_quote_core::posix_quote_bytes(&bytes),
        ))),
    }
}

/// `terminal-ssh` 的入参与回包住 cmd-wire（壳经 `ask` 照同一份问）。
pub(crate) use cmd_wire::terminal::{SshCommand, TerminalSshReq};

crate::wire::typed::codes! {
    /// `terminal-ssh` 的码：跳板查无 / 成环 · 哪一格过不了白名单 · 这台没装 ssh 客户端 · 查的时候出了错（判不了）。
    pub(crate) enum TerminalSshErr {
        BadJump = "bad_jump",
        Refused = "refused",
        NoSshClient = "no_ssh_client",
        Unobservable = "unobservable",
    }
}

/// `terminal-ssh`：给一台远端开终端要跑的那一串。查几个文件找 ssh 客户端 ⇒ 阻塞档；不拨号、不起进程。
pub(crate) struct TerminalSsh;

impl crate::wire::typed::Command for TerminalSsh {
    crate::wire::typed::by_call!(TerminalSshReq);
    type Err = TerminalSshErr;
}

impl crate::wire::typed::Handle for TerminalSsh {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: TerminalSshReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<SshCommand, TerminalSshErr>> + Send + 'static
    {
        async move {
            // 带了 sid ⇒ 记录那一层目录先建好（前奏只写文件）。
            let opened = req
                .sid
                .as_ref()
                .and_then(|_| super::opened_windows::dir(&cx.needs));
            answer_with(
                &req,
                &super::machines_now(),
                &super::ssh_config::list_aliases(),
                crate::platform::shell::LOCAL_TERMINAL_IS_POWERSHELL,
                &crate::platform::ssh_client::locate(),
                opened.as_deref(),
            )
            .map_err(crate::wire::typed::of_pair(TerminalSshErr::Refused))
        }
    }
}

/// 按给定的本机终端方言与找 ssh 客户端的结果渲（判据各种都量）；`~/.ssh/config` 的别名按没有算，不记窗口标签。
pub(crate) fn answer_in(
    req: &TerminalSshReq,
    hosts: &[Host],
    powershell: bool,
    ssh: &SshClient,
) -> Result<SshCommand, CmdErr> {
    answer_with(req, hosts, &[], powershell, ssh, None)
}

/// 同 [`answer_in`]，另给这台 `~/.ssh/config` 里非通配的别名。`command` · `cwd` 都不给 ⇒ 登录 shell 那一形：
/// 本机 ⇒ 本机方言的登录 shell（不走 ssh）；机器表里那台 ⇒ `ssh -t …` 外壳、`--` 后面不跟命令；
/// 机器表里没有、别名里恰好有（过别名白名单）⇒ `ssh -t[ -F …] <别名>`，照用户的 ssh 配置连。
/// `sid` 只跟 `command`；给了、`opened` 那一层目录也在 ⇒ 回包另带前奏（`opened_windows::prelude`，开窗时接在那一行前面，开的那个 shell
/// 记下窗口标签）；那一行本身不带它（开不了窗时复制出去的只有那一行）。
pub(crate) fn answer_with(
    req: &TerminalSshReq,
    hosts: &[Host],
    aliases: &[String],
    powershell: bool,
    ssh: &SshClient,
    opened: Option<&std::path::Path>,
) -> Result<SshCommand, CmdErr> {
    if req.sid.is_some() && req.command.is_none() {
        return Err((
            "bad_args",
            crate::common::contract::malformed("`sid` only goes with `command`"),
        ));
    }
    let command = match (&req.command, &req.cwd) {
        (Some(c), None) => c.clone(),
        (None, Some(cwd)) => command_for_cwd(cwd)?,
        (None, None) => return login_shell(req, hosts, aliases, powershell, ssh),
        (Some(_), Some(_)) => {
            return Err((
                "bad_args",
                crate::common::contract::malformed(
                    "at most one of `command` (a string) or `cwd` (a path)",
                ),
            ))
        }
    };
    let dial = super::dial_named(
        hosts,
        &req.machine.to_string(),
        "stream",
        serde_json::json!({ "command": command }),
    )?;
    let line = render(&dial, powershell, ssh_path(ssh, powershell)?, true)?;
    let prelude = match (&req.sid, opened) {
        (Some(sid), Some(dir)) => super::opened_windows::prelude(
            powershell,
            &dir.join(sid.as_str()),
            &req.machine.to_string(),
        ),
        _ => String::new(),
    };
    Ok(SshCommand { prelude, line })
}

/// 没有前奏的那一形（登录 shell · 本机）。
fn plain(line: String) -> SshCommand {
    SshCommand {
        prelude: String::new(),
        line,
    }
}

/// 登录 shell 那一形（见 [`answer_with`]）。
fn login_shell(
    req: &TerminalSshReq,
    hosts: &[Host],
    aliases: &[String],
    powershell: bool,
    ssh: &SshClient,
) -> Result<SshCommand, CmdErr> {
    if req.machine.is_local() {
        return Ok(plain(local_login_shell(powershell)?));
    }
    let name = req.machine.to_string();
    if !hosts.iter().any(|h| h.name() == name)
        && aliases.iter().any(|a| *a == name)
        && super::ssh_config::is_safe_alias(&name)
        && !name.starts_with('-')
    {
        let ssh = ssh_path(ssh, powershell)?;
        let known = config_arg(super::known_hosts::terminal_config().as_deref(), powershell);
        return Ok(plain(format!(
            "{call}{ssh} -t{known} {name}",
            call = if powershell { "& " } else { "" },
            ssh = literal(powershell, ssh),
        )));
    }
    // 机器表里那台：同一组白名单与跳板，只是 `--` 后面不跟命令（载荷那一格不渲，填个占位）。
    let dial = super::dial_named(
        hosts,
        &name,
        "stream",
        serde_json::json!({ "command": ":" }),
    )?;
    Ok(plain(render(
        &dial,
        powershell,
        ssh_path(ssh, powershell)?,
        false,
    )?))
}

/// 本机那一台的登录 shell（不走 ssh）：POSIX `exec ${SHELL:-bash} -l`；PowerShell 窗口本身就是 shell ⇒ 只回家目录。
/// 说不出家目录 ⇒ 判不了，不猜一个。
pub(crate) fn local_login_shell(powershell: bool) -> Result<String, CmdErr> {
    if !powershell {
        return Ok(posix::cd_then_login_shell(None));
    }
    let Some(home) = crate::platform::paths::home_dir() else {
        return Err(("unobservable", copy_text("beTerminal.login.noHome", &[])));
    };
    let line =
        crate::platform::shell::powershell::set_location(&ps_literal(&home.to_string_lossy()));
    Ok(line.trim_end().trim_end_matches(';').to_string())
}

/// 这台找到的 ssh 客户端全路径；没装 / 判不了 ⇒ 那一句（不说没装）。
pub(crate) fn ssh_path(ssh: &SshClient, powershell: bool) -> Result<&str, CmdErr> {
    match ssh {
        SshClient::At(p) => Ok(p),
        SshClient::Missing => {
            let said = if powershell {
                copy_text("beTerminal.ssh.missingWindows", &[])
            } else {
                copy_text("beTerminal.ssh.missingPosix", &[])
            };
            Err(("no_ssh_client", said))
        }
        SshClient::Unknown(why) => Err((
            "unobservable",
            copy_text("beTerminal.ssh.unknown", &[("why", why)]),
        )),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_terminal_tests.rs"]
mod tests;
