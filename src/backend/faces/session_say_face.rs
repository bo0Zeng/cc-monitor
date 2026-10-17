//! 帧面 `session-say` 的宿主壳：往一条会话说一句（聊天屏 ➤ · 压缩上下文 · 清空会话 · 点 Skill · cc-bus 敲门都走它）。
//! 那台说这条会话在等人（批准 · 回答）⇒ 不送（框开着时打的字会落进框里）；不在等 ⇒ 照 `terminal-input` 那一条送、补回车。
//! 挂着它的那个窗格前台只剩 shell（agent 退了）⇒ 不送进去，当作终端没了（`terminals::say_on`）。
//! 送的时候这台已经没有它的终端（名单里没有 · 送的那一刻没了 · 只剩 shell）⇒ 按记下的号恢复这条会话，那一句当第一句交出去
//! （模型 · 权限档取会话记录里最后那一条，同 `history-facts` 那一处扫描；起法同 `session-open` 恢复进 tmux 那一支，`session_open::resume_saying`）。

use crate::common::contract::malformed;
use crate::control::launch_account::AccountAsk;
use crate::control::session_open::{self as open, Item, Landed};
use crate::control::terminals::{input_reply, no_said, CmdErr, Refused};
use crate::control::terminals_wire::{Sent, SentResult, TerminalInputReq};
use crate::wire::typed::{self, Command, Files};
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
    /// 送之前看到的那一屏的指纹（`terminal-preview` 的 `screen`）：画面已经变了 ⇒ 不送、回 `refused` ＋ `screen_changed`（同 `terminal-input`）。
    /// 给了它，送的那一刻终端没了也不恢复（看见的那一屏已经不在了）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) seen_screen: Option<String>,
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

/// `session-say`：往一条会话说一句。阻塞档（读一次 pidfile · 列名单 · 起 tmux；恢复那一支起之前预标信任，要文件管理面）。
pub(crate) struct SessionSay;

impl Command for SessionSay {
    const NAME: &'static str = "session-say";
    type Req = SessionSayReq;
    type Reply = Sent;
    type Err = SessionSayErr;
}

impl crate::wire::typed::Handle for SessionSay {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        req: SessionSayReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Sent, SessionSayErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let files = needs;

            let home = crate::history::listing::agent_home();
            let asked = crate::control::asks::sids();
            let waiting =
                |sid: &str| crate::observe::accounts_query::waiting_now(&home, sid, &asked);
            let resume =
                |r: &SessionSayReq| resume_here(&r.sid, r.client.as_deref(), Some(&r.text), files);
            say_with(
                &req,
                &waiting,
                &crate::control::terminals_wire::say,
                &resume,
            )
            .map_err(|f| typed::of_fail(f, SessionSayErr::Unobservable))
        }
    }
}

/// 恢复的生产那一份（恢复再送 · 恢复再附着同一份）：记录在这台记录树里找；哪一家 · 工作目录 · 模型 · 权限档从会话事实那一处扫描读（上限表用不着，不读）。
/// 号跟随（这条会话上次用的号）；启动命令 · 模型表读这台 `prefs.json`（[`batch`] 那一份 `Deps`）。`say` ＝ 随起交的第一句。任一找不到 ⇒ `None`（不恢复）。
pub(crate) fn resume_here(
    sid: &str,
    client: Option<&str>,
    say: Option<&str>,
    files: Files,
) -> Option<Reopened> {
    let r = super::session_open_face::resume_of(sid)?;
    let item = Item {
        sid: sid.to_string(),
        cwd: r.cwd,
        account: AccountAsk::Follow,
        say: say.map(str::to_string),
        model: r.model,
        permission_mode: r.permission_mode,
    };
    let client = crate::control::gate::requester(client).ok()?;
    Some(crate::faces::session_batch_face::with_deps_as(
        client.as_deref(),
        &r.agent,
        Some(files),
        |d| open::resume_saying(&item, &r.agent, d),
    ))
}

/// 恢复那一下的结局（同 `session-open` 恢复那一支）。
pub(crate) type Reopened = Result<open::Opened, open::Failed>;


/// `session-say` 的本体：`waiting` 答这条会话此刻在不在等人，`send` 照 `terminal-input` 送。
pub(crate) fn say_with(
    req: &SessionSayReq,
    waiting: &dyn Fn(&str) -> bool,
    send: &dyn Fn(&TerminalInputReq) -> Result<Sent, CmdErr>,
    resume: &dyn Fn(&SessionSayReq) -> Option<Reopened>,
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
            resumed: false,
        });
    }
    let sent = send(&TerminalInputReq {
        sid: Some(req.sid.clone()),
        text: Some(req.text.clone()),
        enter: Some(true),
        client: req.client.clone(),
        seen_screen: req.seen_screen.clone(),
        ..TerminalInputReq::default()
    })?;
    // 这台已经没有它的终端（名单里没有 · 送的那一刻没了）⇒ 恢复再送；别的被拒原样回。带了看见的那一屏 ⇒ 不恢复。
    if req.seen_screen.is_some() || !matches!(sent.why, Some(Refused::NotKnown | Refused::Ended)) {
        return Ok(sent);
    }
    let Some(a) = resume(req) else {
        return Ok(sent);
    };
    let why = match &a {
        Ok(o) if matches!(o.landed, Landed::Started | Landed::InPlace) => {
            return Ok(Sent {
                resumed: true,
                ..input_reply(SentResult::Delivered, None, None)
            })
        }
        Ok(o) => {
            tracing::warn!(
                "[session-say] {} 终端没了、恢复时已在跑（{:?}），那一句没送",
                req.sid,
                o.session
            );
            Refused::NotResumed
        }
        Err((code, said, _, raw)) => {
            tracing::warn!(
                "[session-say] {} 终端没了、恢复没成：{code} {said} {raw:?}",
                req.sid
            );
            if *code == "already_live" {
                Refused::Elsewhere
            } else {
                Refused::NotResumed
            }
        }
    };
    Ok(input_reply(SentResult::Refused, Some(why), None))
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_say_face_tests.rs"]
mod tests;
