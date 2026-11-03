//! 命令面板打机器名（本机后端答）：
//! - `machine-find {query}`：打的字是不是恰好一台 —— 机器表的名字 · 本机那个词 · `~/.ssh/config` 里非通配的 Host（同名以机器表为准）；
//!   名字后跟数字（`nas2` · `nas 2`）⇒ 那台 ＋ 第几条会话；切不开就是没打中，不猜。
//! - `machine-menu {machine}`：那台此刻能做的几样，把现成事实拼一包 —— 头是那台的机器状态成品（`machine_state`，不另判），
//!   动作几样的副文 · 灰着时那一句 · 照做时的提醒都在这里写；哪样灰照成品的态挑。
//!
//! 两条都只读机器表与 `~/.ssh/config`（阻塞档），不拨号、不问那台。

use cmd_wire::machine::{MachineState, MachineStateKind};
use copy_core::copy_text;

use crate::platform::ssh_client::SshClient;

/// 打中的那台从哪来。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "snake_case")]
pub(crate) enum MachineFrom {
    /// 本机。
    Local,
    /// 这一端的机器表。
    Table,
    /// 只在 `~/.ssh/config` 里（没加进机器表）。
    SshConfig,
}

/// 打中的那一台。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub(crate) struct MachineHit {
    /// 那台的名字（本机 ⇒ `<local>`；机器表里的写法）。
    pub(crate) machine: cmd_wire::Machine,
    pub(crate) from: MachineFrom,
    /// 名字后跟的数字：那台的第几条会话（从 1 起）；没跟 ⇒ 空。
    pub(crate) nth: Option<u32>,
}

/// 字里有没有 ssh 的通配（`*` · `?` · 打头的 `!`）。
fn wildcard(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.starts_with('!')
}

/// 打的字认成哪一台（见模块头）。`table` ＝ 机器表里的名字，`aliases` ＝ `~/.ssh/config` 的 Host。
pub(crate) fn find_in(query: &str, table: &[String], aliases: &[String]) -> Option<MachineHit> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }
    let local = copy_core::local_machine();
    let named = |s: &str| -> Option<(cmd_wire::Machine, MachineFrom)> {
        if s.eq_ignore_ascii_case(&local) {
            return Some((cmd_wire::Machine::local(), MachineFrom::Local));
        }
        table
            .iter()
            .find(|n| n.eq_ignore_ascii_case(s))
            .map(|n| (cmd_wire::Machine::named(n.clone()), MachineFrom::Table))
    };
    if let Some((machine, from)) = named(q) {
        return Some(MachineHit {
            machine,
            from,
            nth: None,
        });
    }
    if let Some(a) = aliases.iter().find(|a| {
        a.eq_ignore_ascii_case(q) && !wildcard(a) && crate::dial::ssh_config::is_safe_alias(a)
    }) {
        return Some(MachineHit {
            machine: cmd_wire::Machine::named(a.clone()),
            from: MachineFrom::SshConfig,
            nth: None,
        });
    }
    // 名字 ＋ 尾数：尾数从短往长试（`gpu-012` 在没有 `gpu-012` 时先试 `gpu-01` ＋ 2）。
    let digits = q.chars().rev().take_while(char::is_ascii_digit).count();
    for k in 1..=digits.min(9) {
        let (head, tail) = q.split_at(q.len() - k);
        let Ok(n) = tail.parse::<u32>() else { continue };
        if n == 0 {
            continue;
        }
        if let Some((machine, from)) = named(head.trim_end()) {
            return Some(MachineHit {
                machine,
                from,
                nth: Some(n),
            });
        }
    }
    None
}

/// 面板那一块里的一样动作是什么（标题是界面的骨架字）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "snake_case")]
pub(crate) enum MenuAct {
    OpenTerminal,
    NewSession,
    Files,
    MachineStatus,
    AddMachine,
}

/// 一样动作：副文（标题后那一段灰字）· 灰着时那一句（有 ⇒ 不可用）· 照做时的提醒（第二行）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub(crate) struct MenuAction {
    pub(crate) act: MenuAct,
    pub(crate) sub: Option<String>,
    pub(crate) blocked: Option<String>,
    pub(crate) note: Option<String>,
}

/// `machine-menu` 的回包。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct MachineMenu {
    pub(crate) from: MachineFrom,
    /// 那台的机器状态成品（同设置 › 机器卡那一份）；只在 ssh 配置里的那一种没有。
    pub(crate) head: Option<MachineState>,
    pub(crate) actions: Vec<MenuAction>,
}

fn action(act: MenuAct, sub: Option<String>, blocked: Option<String>) -> MenuAction {
    MenuAction {
        act,
        sub,
        blocked,
        note: None,
    }
}

/// 那台的几样动作（见模块头）。`machine` ＝ 名字（本机 ⇒ `<local>`），`powershell` ＝ 本机终端是不是 PowerShell（没装 ssh 那一句随它）。
pub(crate) fn menu_in(
    machine: &str,
    from: MachineFrom,
    head: Option<MachineState>,
    ssh: &SshClient,
    powershell: bool,
) -> MachineMenu {
    let shown = head
        .as_ref()
        .map_or_else(|| machine.to_string(), |h| h.name.clone());
    let kind = head.as_ref().map(|h| h.state);
    let offline = head.as_ref().is_some_and(|h| h.offline);
    // 灰着时那一句：名字 ＋ 成品的词（词空 ⇒ 成品那一句离线）。
    let not_now = head.as_ref().map(|h| {
        if h.word.is_empty() {
            h.offline_said.clone()
        } else {
            copy_text(
                "beMachineMenu.action.notNow",
                &[("machine", &shown), ("word", &h.word)],
            )
        }
    });

    let mut terminal = action(
        MenuAct::OpenTerminal,
        Some(if from == MachineFrom::Local {
            copy_text("beMachineMenu.terminal.local", &[])
        } else {
            copy_text("beMachineMenu.terminal.ssh", &[("machine", &shown)])
        }),
        None,
    );
    if kind == Some(MachineStateKind::HostKeyChanged) {
        terminal.blocked = Some(copy_text("beMachineMenu.terminal.hostKey", &[]));
    } else if from != MachineFrom::Local {
        if let Err(f) = crate::dial::terminal::ssh_path(ssh, powershell) {
            if matches!(ssh, SshClient::Missing) {
                terminal.blocked = Some(f.said);
            }
        }
    }
    if terminal.blocked.is_none() && offline {
        terminal.note = Some(copy_text("beMachineMenu.terminal.anyway", &[]));
    }

    let mut out = vec![terminal];
    if from == MachineFrom::SshConfig {
        out.push(action(
            MenuAct::AddMachine,
            Some(copy_text("beMachineMenu.add.where", &[])),
            None,
        ));
        return MachineMenu {
            from,
            head,
            actions: out,
        };
    }
    let up = kind == Some(MachineStateKind::Up);
    let reachable = matches!(
        kind,
        Some(MachineStateKind::Up | MachineStateKind::NeedsUpdate | MachineStateKind::Newer)
    );
    out.push(action(
        MenuAct::NewSession,
        None,
        (!up).then(|| not_now.clone().unwrap_or_default()),
    ));
    if from != MachineFrom::Local {
        out.push(action(
            MenuAct::Files,
            Some(copy_text("rsFilewinChrome.side.home", &[])),
            (!reachable).then(|| not_now.clone().unwrap_or_default()),
        ));
    }
    out.push(action(
        MenuAct::MachineStatus,
        Some(copy_text(
            "beMachineMenu.status.where",
            &[("machine", &shown)],
        )),
        None,
    ));
    MachineMenu {
        from,
        head,
        actions: out,
    }
}

/// `machine-find` 的入参：框里此刻的字。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct MachineFindReq {
    pub(crate) query: String,
}

/// `machine-find` 的回包：打中的那台；没打中 ⇒ 空。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct MachineFind {
    pub(crate) hit: Option<MachineHit>,
}

crate::wire::typed::codes! {
    /// `machine-find` 没有别的码（没打中是空，不是错）。
    pub enum MachineFindErr {}
}

/// 机器表此刻那几台的名字。
fn table_names() -> Vec<String> {
    crate::dial::machines_now()
        .iter()
        .map(|h| h.name().to_string())
        .collect()
}

/// `machine-find`：读机器表 ＋ `~/.ssh/config` ⇒ 阻塞档。
pub struct MachineFindCmd;

impl crate::wire::typed::Command for MachineFindCmd {
    const NAME: &'static str = "machine-find";
    type Req = MachineFindReq;
    type Reply = MachineFind;
    type Err = MachineFindErr;
}

impl crate::wire::typed::Handle for MachineFindCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: MachineFindReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<MachineFind, MachineFindErr> {
        Ok(MachineFind {
            hit: find_in(
                &req.query,
                &table_names(),
                &crate::dial::ssh_config::list_aliases(),
            ),
        })
    }
}

/// `machine-menu` 的入参：那台的名字（`machine-find` 交回的那一份）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct MachineMenuReq {
    pub(crate) machine: cmd_wire::Machine,
}

crate::wire::typed::codes! {
    /// `machine-menu` 的码：机器表与 `~/.ssh/config` 里都没有那台。
    pub enum MachineMenuErr {
        NoSuchMachine = "no_such_machine",
    }
}

/// `machine-menu`：读机器表 · 机器状态表 · `~/.ssh/config` · 找 ssh 客户端 ⇒ 阻塞档。
pub struct MachineMenuCmd;

impl crate::wire::typed::Command for MachineMenuCmd {
    const NAME: &'static str = "machine-menu";
    type Req = MachineMenuReq;
    type Reply = MachineMenu;
    type Err = MachineMenuErr;
}

impl crate::wire::typed::Handle for MachineMenuCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: MachineMenuReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<MachineMenu, MachineMenuErr> {
        let name = req.machine.to_string();
        let from = if req.machine.is_local() {
            MachineFrom::Local
        } else if table_names().contains(&name) {
            MachineFrom::Table
        } else if crate::dial::ssh_config::list_aliases()
            .iter()
            .any(|a| *a == name && !wildcard(a))
        {
            MachineFrom::SshConfig
        } else {
            return Err(MachineMenuErr::NoSuchMachine(copy_text(
                "beMachineMenu.menu.noSuch",
                &[("machine", &name)],
            )));
        };
        let head = (from != MachineFrom::SshConfig)
            .then(|| crate::control::machine_state::read().remove(&name))
            .flatten();
        Ok(menu_in(
            &name,
            from,
            head,
            &crate::platform::ssh_client::locate(),
            crate::platform::shell::LOCAL_TERMINAL_IS_POWERSHELL,
        ))
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/machine_menu_tests.rs"]
mod tests;
