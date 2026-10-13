//! 帧面 `session-say` 的宿主壳：往一条会话说一句（聊天屏 ➤ · 压缩上下文 · 清空会话 · 点 Skill 都走它）。
//! 那台说这条会话在等人（批准 · 回答）⇒ 不送（框开着时打的字会落进框里）；不在等 ⇒ 照 `terminal-input` 那一条送、补回车。
//! 送的时候这台已经没有它的终端（名单里没有 · 送的那一刻没了）⇒ 按记下的号恢复这条会话，那一句当第一句交出去
//! （模型 · 权限档取会话记录里最后那一条，同 `history-facts` 那一处扫描；起法同批量起的 tmux 那一形，`session_batch::resume_saying`）。

use crate::common::contract::malformed;
use crate::control::launch_account::AccountAsk;
use crate::control::session_batch::{self as batch, Answer, Item};
use crate::control::terminals::{input_reply, no_said, CmdErr, Refused};
use crate::control::terminals_wire::{Sent, SentResult, TerminalInputReq};
use crate::stream::inbound::typed::{self, Command, Cx, Files, WithFiles};
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

/// `session-say`：往一条会话说一句。阻塞档（读一次 pidfile · 列名单 · 起 tmux；恢复那一支起之前预标信任，要文件管理面）。
pub(crate) struct SessionSay;

impl Command for SessionSay {
    const NAME: &'static str = "session-say";
    type Req = SessionSayReq;
    type Reply = Sent;
    type Err = SessionSayErr;
}

impl WithFiles for SessionSay {
    fn run(req: SessionSayReq, _cx: &Cx, files: Files) -> Result<Sent, SessionSayErr> {
        let home = crate::observe::history_query::agent_home();
        let asked = crate::control::asks::sids();
        let waiting = |sid: &str| crate::observe::accounts_query::waiting_now(&home, sid, &asked);
        let resume = |r: &SessionSayReq| resume_here(r, files);
        say_with(
            &req,
            &waiting,
            &crate::control::terminals_wire::input,
            &resume,
        )
        .map_err(|f| typed::of_fail(f, SessionSayErr::Unobservable))
    }
}

/// 恢复再送的生产那一份：记录在这台记录树里找；哪一家 · 工作目录 · 模型 · 权限档从会话事实那一处扫描读（上限表用不着，不读）。
/// 号跟随（这条会话上次用的号）；启动命令 · 模型表读这台 `prefs.json`（[`batch`] 那一份 `Deps`）。任一找不到 ⇒ `None`（不恢复）。
fn resume_here(req: &SessionSayReq, files: Files) -> Option<Answer> {
    use crate::observe::history_query as hq;
    let home = hq::agent_home();
    let path = hq::session_record(&home, &req.sid).ok()?;
    let path = path.to_str()?;
    let agent = hq::facts_agent(&home, path)?;
    let cwd = hq::facts_project_dir(&home, path)?;
    let facts = hq::cold_scan(&home, path).ok()?.facts(&Vec::new(), None);
    let item = Item {
        sid: req.sid.clone(),
        cwd,
        account: AccountAsk::Follow,
        fresh: false,
        fork_of: None,
        say: Some(req.text.clone()),
        model: facts.usage.and_then(|u| u.model),
        permission_mode: facts.permission_mode,
    };
    let client = crate::control::gate::requester(req.client.as_deref()).ok()?;
    Some(crate::faces::session_batch_face::with_deps_as(
        client.as_deref(),
        &agent,
        Some(files),
        |d| batch::resume_saying(&item, &agent, d),
    ))
}

/// `session-say` 的本体：`waiting` 答这条会话此刻在不在等人，`send` 照 `terminal-input` 送。
pub(crate) fn say_with(
    req: &SessionSayReq,
    waiting: &dyn Fn(&str) -> bool,
    send: &dyn Fn(&TerminalInputReq) -> Result<Sent, CmdErr>,
    resume: &dyn Fn(&SessionSayReq) -> Option<Answer>,
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
        ..TerminalInputReq::default()
    })?;
    // 这台已经没有它的终端（名单里没有 · 送的那一刻没了）⇒ 恢复再送；别的被拒原样回。
    if !matches!(sent.why, Some(Refused::NotKnown | Refused::Ended)) {
        return Ok(sent);
    }
    let Some(a) = resume(req) else {
        return Ok(sent);
    };
    if a.outcome == "done" {
        return Ok(Sent {
            resumed: true,
            ..input_reply(SentResult::Delivered, None, None)
        });
    }
    let why = match a.why.as_deref() {
        Some(batch::ALREADY_LIVE) => Refused::Elsewhere,
        _ => Refused::NotResumed,
    };
    tracing::warn!(
        "[session-say] {} 终端没了、恢复没成：{} {:?} {}",
        req.sid,
        a.outcome,
        a.why,
        a.detail
    );
    Ok(input_reply(SentResult::Refused, Some(why), None))
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_say_face_tests.rs"]
mod tests;
