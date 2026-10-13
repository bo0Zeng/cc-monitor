//! 帧面 `session-say` 的宿主壳：往一条会话说一句（聊天屏 ➤ · 压缩上下文 · 清空会话 · 点 Skill 都走它）。
//! 那台说这条会话在等人（批准 · 回答）⇒ 不送（框开着时打的字会落进框里）；不在等 ⇒ 照 `terminal-input` 那一条送、补回车。

use crate::common::contract::malformed;
use crate::control::terminals::{no_said, CmdErr, Refused};
use crate::control::terminals_wire::{Sent, SentResult, TerminalInputReq};
use crate::stream::inbound::typed::{self, Blocking, Command, Cx};
use serde::Deserialize;

/// `session-say` 的入参。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionSayReq {
    /// 会话 id。
    pub(crate) sid: String,
    /// 那一句（字面字，原样送；多行按粘贴送；后端补回车）。
    pub(crate) text: String,
    /// 自报的前端（过「哪个前端的会话」那一维，同 `terminal-input`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) client: Option<String>,
}

typed::codes! {
    /// `session-say` 的码：认会话 · 列名单 · 过门 · 送（同 `terminal-input`）。在等人 · 认不出 · 不归我们 · 已经没了不是错（`result: "refused"`）。
    pub(crate) enum SessionSayErr {
        /// tmux 起不来。
        NoTmux = "no_tmux",
        /// 那个终端在送的时候没了。
        NoSuchSession = "no_such_session",
        /// 列名单失败。
        Unobservable = "unobservable",
        /// 那一发 `tmux` 过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

/// `session-say`：往一条会话说一句。阻塞档（读一次 pidfile · 列名单 · 起 tmux）。
pub(crate) struct SessionSay;

impl Command for SessionSay {
    const NAME: &'static str = "session-say";
    type Req = SessionSayReq;
    type Reply = Sent;
    type Err = SessionSayErr;
}

impl Blocking for SessionSay {
    fn run(req: SessionSayReq, _cx: &Cx) -> Result<Sent, SessionSayErr> {
        let home = crate::observe::history_query::agent_home();
        let asked = crate::control::asks::sids();
        let waiting = |sid: &str| crate::observe::accounts_query::waiting_now(&home, sid, &asked);
        say_with(&req, &waiting, &crate::control::terminals_wire::input)
            .map_err(|f| typed::of_fail(f, SessionSayErr::Unobservable))
    }
}

/// `session-say` 的本体：`waiting` 答这条会话此刻在不在等人，`send` 照 `terminal-input` 送。
pub(crate) fn say_with(
    req: &SessionSayReq,
    waiting: &dyn Fn(&str) -> bool,
    send: &dyn Fn(&TerminalInputReq) -> Result<Sent, CmdErr>,
) -> Result<Sent, CmdErr> {
    if !shell_quote_core::session_id_ok(&req.sid) {
        return Err(CmdErr::new(
            "bad_args",
            malformed("`sid` must be a session id"),
        ));
    }
    if waiting(&req.sid) {
        return Ok(Sent {
            result: SentResult::Refused,
            why: Some(Refused::Waiting),
            said: Some(no_said(Refused::Waiting)),
            screen: None,
        });
    }
    send(&TerminalInputReq {
        sid: Some(req.sid.clone()),
        text: Some(req.text.clone()),
        enter: Some(true),
        client: req.client.clone(),
        ..TerminalInputReq::default()
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_say_face_tests.rs"]
mod tests;
