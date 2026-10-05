//! 〔「待迁」最后一行〕**给一台远端开终端要跑的那一串** —— `ssh -t[ -J …] …` 外壳，按这台电脑的终端方言渲
//! （Windows：PowerShell 窗口载荷；别处：一行 POSIX shell），在本机常驻后端里渲。
//!
//! 界面交来的是那台机器的配置（`{machine, saved?, jump?, prefer?}`，monitor 从它自己的机器表与「上次赢的那条」给出，
//! 同 `remote-probe` / `pubkey-push` 那几格）＋ 要在那台跑的命令 `command`。组请求走唯一那一份 [`super::machine::resolve`]
//! （地址排序 · 跳板查无 / 环都在那里），这里只把它渲成一行：
//!
//! - PowerShell：`& ssh -t[ -J <跳板用户>@<跳板>[:口]] -p <口>[ -i '<钥匙>'] -o 'UserKnownHostsFile=<cc-monitor 那份>' <用户>@<地址> -- '<bash -lic ''…''>'`
//! - POSIX：同一组参数，不带 `& `，引号按 POSIX 单引号写（`'bash -lic '\''…'\'''`）。
//!
//! - 地址取竞速顺序的第一条（交了 `prefer` 且它仍在这台的地址里 ⇒ 就是上次赢的那条，F45：终端与数据源走同一条路）；
//! - 远端命令包成 `bash -lic '<命令>'`（PATH / 别名按「用户粘贴进交互终端」解析），再以本机方言的单引号字面量嵌入；
//! - 主机钥匙认 cc-monitor 自己那份 known_hosts（[`super::known_hosts`]：握手时认下的那把），终端里不再问、不写用户的 `~/.ssh`；
//!   跳板那一跳（`-J`）是 ssh 另起的一个进程，不吃 `-o`，仍走 ssh 自己的默认；
//! - 钥匙路径尾 `\` 剥掉（PS < 7.3 给含空格参数加壳时尾部 `\"` 会吃掉收尾引号）；没给钥匙 ⇒ 走 ssh-agent，不带 `-i`。
//!
//! 拒（`refused`，说清哪一格）：命令空 / 超长 / 含控制符 · PowerShell 那一形另拒双引号（PowerShell 5.1 向原生程序传参对内嵌 `"`
//! 有历史畸变，那是**那条送法**的约束）· 用户名 / 地址 / 跳板用户 / 跳板地址出了白名单（它们是拼进命令体的裸词）。
//! 只算不起：不拨号、不开窗（开窗是 monitor 的事，它只开窗）。

use copy_core::copy_text;
use serde_json::{json, Value};

use crate::platform::shell::dialect::ps_literal;
use crate::platform::shell::posix;

/// 远端命令长度上限（防异常输入；正常 resume 命令 < 300 字节）。与 monitor 本机那条送法的上限同值。
pub(crate) const MAX_COMMAND: usize = 4096;

type CmdErr = (&'static str, String);

fn refused(msg: String) -> CmdErr {
    ("refused", msg)
}

/// 用户名：非空，仅 `[A-Za-z0-9._-]`（拼进命令体的裸词，白名单杜绝注入）。
fn user_ok(u: &str) -> bool {
    !u.is_empty()
        && u.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// 地址：非空，仅 `[A-Za-z0-9._:\[\]-]`（域名 / IPv4 / IPv6 字面量）。
fn host_ok(h: &str) -> bool {
    !h.is_empty()
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

/// ` -J user@host[:port]`（口 = 22 不带后缀）。
fn jump_arg(j: &super::JumpHop) -> Result<String, CmdErr> {
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
    Ok(format!(" -J {}@{}{port}", j.user, j.host))
}

/// `-o UserKnownHostsFile=<那份>` 的值：`ssh` 自己按空白切词（单引号成对、`\'` 是字面的 `'`），`%` 是它的占位符 ⇒ 照它的规矩写。
fn known_hosts_value(path: &str) -> String {
    let p = path.replace('%', "%%");
    if p.chars().any(|c| c.is_whitespace() || c == '\'') {
        format!("'{}'", p.replace('\'', "\\'"))
    } else {
        p
    }
}

/// ` -o 'UserKnownHostsFile=<cc-monitor 那份>'`：终端那一跳认 cc-monitor 握手时认下的那把钥匙（`super::known_hosts`），
/// 不再问一遍、也不写用户的 `~/.ssh/known_hosts`；主机钥匙核对照旧开着。家目录说不出 ⇒ 不带（照 ssh 自己的默认）。
pub(crate) fn known_hosts_arg(path: Option<&std::path::Path>, powershell: bool) -> String {
    match path {
        Some(p) => format!(
            " -o {}",
            literal(powershell, &format!(
                "UserKnownHostsFile={}",
                known_hosts_value(&p.to_string_lossy())
            ))
        ),
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

/// 一份拨号请求 ＋ 命令 ⇒ 这台电脑终端里跑的那一行（`powershell` ＝ 本机终端是不是 PowerShell，见模块头注）。
pub(crate) fn render(req: &super::DialRequest, powershell: bool) -> Result<String, CmdErr> {
    check_command(&req.command, powershell)?;
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
    let jump = match &req.jump {
        Some(j) => jump_arg(j)?,
        None => String::new(),
    };
    let wrapped = format!("bash -lic {}", shell_quote_core::posix_quote(&req.command));
    let known = known_hosts_arg(super::known_hosts::path().as_deref(), powershell);
    Ok(format!(
        "{call}ssh -t{jump} -p {port}{key}{known} {user}@{host} -- {cmd}",
        call = if powershell { "& " } else { "" },
        port = first.port,
        user = req.user,
        host = first.host,
        cmd = literal(powershell, &wrapped),
    ))
}

fn command_arg(args: &Value) -> Result<&str, CmdErr> {
    args.get("command").and_then(Value::as_str).ok_or_else(|| {
        (
            "bad_args",
            crate::common::contract::malformed("missing `command` (a string)"),
        )
    })
}

/// 「在此打开终端」要在那台跑的那一串（原住文件窗口那一侧的 `build_open_terminal_cmd`〔散文墓碑〕 · `_at`，逐字搬来：
/// 窗口只交意图 `{cwd}`，命令由这里拼）。
///
/// 逐字：`cd <quoted> && exec ${SHELL:-bash} -l`（`cwd` 为空 ⇒ 只有后半段）。当前目录是那台列出来的**自由文本路径** ⇒
/// 拼进 `cd` 之前先过形式 ＋ 拒绝集（`shell_quote_core::posix_free_path_ok`：POSIX 绝对 · 无 `..` 段 · 不含 NUL / CR / LF；
/// **不拒 shell 元字符**，交给唯一那一处 quote，`INVARIANTS §47` ②）；非 UTF-8 的目录走字节形（`posix_quote_bytes`，`$'…'`）。
/// ⚠ **不用双引号**：[`check_command`] 会拒掉含双引号的命令（PowerShell 原生传参畸变那道防线）。
pub(crate) fn command_for_cwd(cwd: &Value) -> Result<String, CmdErr> {
    let Some(bytes) = crate::common::path_wire::from_json(cwd) else {
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

/// 帧命令 `terminal-ssh`：`{machine, saved?, jump?, prefer?, command | cwd}` ⇒ `{command: "<本机终端里跑的那一行>"}`。
/// `command`（主界面交成品命令）与 `cwd`（文件窗口「在此打开终端」只交意图，命令由 [`command_for_cwd`] 拼）**恰好给一个**。
pub(crate) fn answer(args: &Value) -> Result<Value, CmdErr> {
    answer_in(args, crate::platform::shell::LOCAL_TERMINAL_IS_POWERSHELL)
}

/// [`answer`] 按给定的本机终端方言渲（判据两种方言都量）。
pub(crate) fn answer_in(args: &Value, powershell: bool) -> Result<Value, CmdErr> {
    let req = super::machine::resolve(&dial_args(args)?)?;
    Ok(json!({ "command": render(&req, powershell)? }))
}

/// `command` 与 `cwd` 恰好给一个；给的是 `cwd` ⇒ 由 [`command_for_cwd`] 拼好放进 `command`（拨号请求只认 `command`）。
fn dial_args(args: &Value) -> Result<Value, CmdErr> {
    match (args.get("command"), args.get("cwd")) {
        (Some(_), Some(_)) => Err((
            "bad_args",
            crate::common::contract::malformed(
                "exactly one of `command` (a string) or `cwd` (a path)",
            ),
        )),
        (None, Some(cwd)) => {
            let command = command_for_cwd(cwd)?;
            let mut dial = args.as_object().cloned().unwrap_or_default();
            dial.remove("cwd");
            dial.insert("command".to_string(), Value::String(command));
            Ok(Value::Object(dial))
        }
        _ => command_arg(args).map(|_| args.clone()),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_terminal_tests.rs"]
mod tests;
