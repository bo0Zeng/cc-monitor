//! **分叉之后起：新会话的工作目录 / 号 / 终端从哪知道** —— 逐格「知道（值 ＋ 来源码）」或「不知道（原因码）」，**绝不猜**。
//!
//! 号与终端只有源会话**此刻活着**才知道：pidfile 随进程消失；各号的 `projects/` 都链回共享的记录树，记录住在哪说明不了号；
//! 记录里也没有号的字段 ⇒ 已退出的会话一律「不知道」，不拿当前号顶替（那会静默用错身份跑一条对话）。
//! 两个信号各答各的、不互相顶替：活没活 / 在哪个终端 ← 终端名单（`@ccm_sid` 精确匹配、前台是 agent）；用哪个号 ← 进程名单（同 `accounts-sessions`）。
//!
//! 句子不在这里：只回码，由各前端照码说。事实由入口（`faces/fork_face.rs`）收齐交进来。

use super::session_batch::{carriers, Standing, TmuxEntry};
use serde_json::Value;

/// 起分叉会话要的三格的线上形：住 `cmd-wire::session`（号那一格在线上是「名字 · 账号 0 ⇒ `null`」）。
pub(crate) use cmd_wire::session::{
    ForkLaunch as Launch, ForkSlot as Slot, ForkTerminal as TerminalFact, ForkWhence as Whence,
    ForkWhy as Why,
};

/// 号：账号 0（不设配置目录）或账号库里的一个号的名字。线上 `null` ＝ 账号 0、串 ＝ 号名。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(into = "Option<String>")]
pub(crate) enum AccountFact {
    Base,
    Named(String),
}

impl From<AccountFact> for Option<String> {
    fn from(a: AccountFact) -> Self {
        match a {
            AccountFact::Base => None,
            AccountFact::Named(n) => Some(n),
        }
    }
}

/// 源会话的事实（收齐之后）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Source {
    /// 源会话此刻还活着（进程在跑）。
    pub(crate) live: bool,
    /// 记录里的工作目录（任何会话都有，含已退出的）。
    pub(crate) cwd: Option<String>,
    /// 活着时那个进程用的号；`None` ＝ 说不出（只有 `live` 时才看）。
    pub(crate) account: Option<AccountFact>,
    /// 活着时它在的终端（句柄）；`None` ＝ 不在任何终端里（只有 `live` 时才看）。
    pub(crate) terminal: Option<String>,
}

/// 逐格推断。纯函数。
pub(crate) fn infer(s: &Source) -> Launch {
    let cwd = match s.cwd.as_deref().filter(|c| !c.trim().is_empty()) {
        Some(c) => Slot::Known {
            value: c.to_string(),
            from: Whence::Record,
        },
        None => Slot::Unknown { why: Why::NoCwd },
    };
    // 号：活着才可能知道；已退出一律不知道，不看 `account` 里塞了什么。
    let account = match (s.live, &s.account) {
        (false, _) => Slot::Unknown { why: Why::Exited },
        (true, None) => Slot::Unknown {
            why: Why::LiveNoAccount,
        },
        (true, Some(a)) => Slot::Known {
            value: a.clone().into(),
            from: Whence::Process,
        },
    };
    let terminal = if s.live {
        Slot::Known {
            value: match &s.terminal {
                Some(t) if !t.trim().is_empty() => TerminalFact::Hosted {
                    terminal: t.clone(),
                },
                _ => TerminalFact::None,
            },
            from: Whence::TerminalList,
        }
    } else {
        Slot::Unknown { why: Why::Exited }
    };
    Launch {
        cwd,
        account,
        terminal,
    }
}

/// 进程名单里的一行（`accounts-sessions` 那一行里推断要的几格）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessRow {
    pub(crate) sid: String,
    pub(crate) alive: bool,
    pub(crate) config_dir: Option<String>,
    pub(crate) account: Option<String>,
    /// 活着、环境读得到、没设配置目录 ＝ 账号 0。
    pub(crate) bare: bool,
}

/// `accounts-sessions` 的一行 ⇒ [`ProcessRow`]（缺 sid 的不要）。
pub(crate) fn process_row_of(v: &Value) -> Option<ProcessRow> {
    let s = |k: &str| v.get(k).and_then(Value::as_str).filter(|x| !x.is_empty());
    Some(ProcessRow {
        sid: s("sessionId")?.to_string(),
        alive: v.get("alive").and_then(Value::as_bool) == Some(true),
        config_dir: s("configDir").map(str::to_string),
        account: s("account").map(str::to_string),
        bare: v.get("bare").and_then(Value::as_bool) == Some(true),
    })
}

/// 那个进程用的号：没设配置目录（且环境读得到）⇒ 账号 0；设了且账号库认得那个目录 ⇒ 那个号；别的 ⇒ 说不出。
fn account_of(r: &ProcessRow) -> Option<AccountFact> {
    match (&r.config_dir, &r.account) {
        (None, _) if r.bare => Some(AccountFact::Base),
        (Some(_), Some(name)) => Some(AccountFact::Named(name.clone())),
        _ => None,
    }
}

/// 两份名单 ⇒ 源会话的事实。`terminals` ＝ 这台的终端名单（`None` ＝ 没有 / 看不见：都当不在任何终端里）。
/// 活着 ＝ 进程名单说它活着，或终端名单里有挂着它、前台是 agent 的（命中多个取第一个）；只剩 shell 的不算。
pub(crate) fn source_of(
    processes: &[ProcessRow],
    terminals: Option<&[TmuxEntry]>,
    sid: &str,
    cwd: Option<String>,
) -> Source {
    let row = processes.iter().find(|r| r.sid == sid && r.alive);
    let running = match terminals.map(|t| carriers(t, sid)) {
        Some(Standing::Running(e)) => Some(e.terminal.clone()),
        Some(Standing::Ambiguous(es)) => es.first().map(|e| e.terminal.clone()),
        _ => None,
    };
    Source {
        live: row.is_some() || running.is_some(),
        cwd,
        account: row.and_then(account_of),
        terminal: running,
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/fork_launch_tests.rs"]
mod tests;
