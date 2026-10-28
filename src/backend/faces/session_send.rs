//! 往一条会话**送进**一句的本体：只由会话输入队的投递那一下调（`faces/session_queue_deliver.rs`）；往会话说一句的入口只有
//! `session-queue-put`（投进队），没有直送的路。
//! 那台说这条会话在等人（批准 · 回答）⇒ 不送（框开着时打的字会落进框里）；不在等 ⇒ 照 `terminal-input` 那一条送、补回车。
//! 挂着它的那个窗格前台只剩 shell（agent 退了）⇒ 不送进去，当作终端没了（`terminals::say_on`）。
//! 送的时候这台已经没有它的终端（名单里没有 · 送的那一刻没了 · 只剩 shell）⇒ 按记下的号恢复这条会话，那一句当第一句交出去
//! （模型 · 权限档取会话记录里最后那一条，同 `history-facts` 那一处扫描；起法同 `session-open` 恢复进 tmux 那一支，`session_open::resume_saying`）。
//! 带附件的那一句：编号 ⇒ 会话工作目录下那一份，照那一家的写法拼（[`crate::agents::typed_with`]）；投进队那一刻也照同一份判法（`attachment_dir::missing`）判过一次。

use crate::control::launch_account::AccountAsk;
use crate::control::session_open::{self as open, Item, Landed};
use crate::control::terminals::CmdErr;
use crate::control::terminals_wire::{Sent, SentResult, TerminalInputReq};
use crate::wire::typed::Files;

/// 投递那一下送一句的结局：只投递内部用，不上线（送字那一侧回的线上形是 [`Sent`]，这里收成投递要分的那几样）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Delivery {
    /// 送进去了（照 `terminal-input` 送到 · 不知道送没送到：都不重发）；`resumed` ＝ 终端没了、按记下的号恢复了这条会话、那一句当第一句交了出去。
    In { resumed: bool },
    /// 那台说这条会话在等人（批准 · 回答）：一个字没送，照排着、等它答完。
    Waiting,
    /// 送不进去：那一条标未送达（为什么记日志）。
    No(Undeliverable),
}

/// 送不进去的那几种。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Undeliverable {
    /// 送字那一侧被拒（那一份原样：别的前端的 · 不归我们管 · 不止一个终端 …）。
    Refused(Sent),
    /// 带的附件里有那台已经不在的（过了七天清掉 · 撤过）：一个字都没送，列出那几个编号。
    AttachmentGone(Vec<String>),
    /// 终端已经没了，按记下的号恢复这条会话没成（原话记日志）。
    NotResumed,
    /// 这台 tmux 里没有它的终端、可另有活进程在写它（开在 tmux 之外的终端里）：不另起一个。
    Elsewhere,
    /// 编辑已发的那一句没退成 · 退了没送进（那一句照回退那一串写好的，标在那一条上）。
    NotRewound(String),
}

impl Delivery {
    /// 送字那一侧的回话 ⇒ 投递的结局：送到 · 不知道 ⇒ 送进去了；被拒 ⇒ 送不进去（那一份原样）。
    pub(crate) fn of(sent: Sent) -> Delivery {
        match sent.result {
            SentResult::Delivered | SentResult::Unsure => Delivery::In { resumed: false },
            SentResult::Refused => Delivery::No(Undeliverable::Refused(sent)),
        }
    }
}

/// 送进去的那一句。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Say {
    /// 会话 id。
    pub(crate) sid: cmd_wire::Sid,
    /// 那一句（字面字，原样送；多行按粘贴送；补回车）。
    pub(crate) text: String,
    /// 当这次请求自报的前端（过「哪个前端的会话」那一维，同 `terminal-input`）。
    pub(crate) client: Option<String>,
    /// 随这一句发的附件编号（按先后）。
    pub(crate) attachments: Vec<String>,
}

/// 生产那一份送一句：会话工作目录与哪一家从会话记录读（同恢复那一支）；读不出 ⇒ 每一份附件都算不在。
pub(crate) fn here(say: &Say, files: Files) -> Result<Delivery, CmdErr> {
    let home = crate::history::listing::agent_home();
    let waiting = |sid: &str| crate::history::live_table::now(&home).waiting(sid);
    let words = if say.attachments.is_empty() {
        plain(&say.text)
    } else {
        let found = super::session_open_face::resume_of(say.sid.as_str());
        let (agent, cwd) = found.map_or_else(Default::default, |r| (r.agent, r.cwd));
        match typed_of(say, &agent, std::path::Path::new(&cwd)) {
            Ok(t) => t,
            Err(no) => return Ok(no),
        }
    };
    let first = words.first_say.clone();
    let resume = |r: &Say| resume_here(&r.sid, r.client.as_deref(), Some(&first), files);
    say_typed(
        say,
        &words,
        &waiting,
        &crate::control::terminals_wire::say,
        &resume,
    )
}

/// 恢复的生产那一份（恢复再送 · 恢复再附着同一份）：记录在这台记录树里找；哪一家 · 工作目录 · 模型 · 权限档从会话事实那一处扫描读（上限表用不着，不读）。
/// 号跟随（这条会话上次用的号）；启动命令 · 模型表读这台 `prefs.json`（`session_batch_face` 那一份 `Deps`）。`say` ＝ 随起交的第一句。任一找不到 ⇒ `None`（不恢复）。
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

/// 不带附件的那一句：原样送、原样当恢复的第一句。
pub(crate) fn plain(text: &str) -> crate::agents::Typed {
    crate::agents::Typed {
        pastes: Vec::new(),
        text: text.to_string(),
        first_say: text.to_string(),
    }
}

/// 带附件的那一句：编号 ⇒ 工作目录 `cwd` 下那一份（[`crate::common::attachment_dir::find`]），照 `agent` 那一家的写法拼（[`crate::agents::typed_with`]）。
/// 有不在的 ⇒ 送不进去（附件已不在，列出那几个编号），一个字都不送。
pub(crate) fn typed_of(
    say: &Say,
    agent: &str,
    cwd: &std::path::Path,
) -> Result<crate::agents::Typed, Delivery> {
    use crate::common::attachment_dir as layout;
    let mut found = Vec::new();
    let mut missing = Vec::new();
    for id in &say.attachments {
        match layout::find(cwd, id) {
            Some(name) => {
                let rel = layout::rel_of(id, &name);
                found.push((name, cwd.join(&rel).display().to_string(), rel));
            }
            None => missing.push(id.clone()),
        }
    }
    if !missing.is_empty() {
        return Err(Delivery::No(Undeliverable::AttachmentGone(missing)));
    }
    let refs: Vec<crate::agents::AttachRef<'_>> = found
        .iter()
        .map(|(name, abs, rel)| crate::agents::AttachRef { name, abs, rel })
        .collect();
    Ok(crate::agents::typed_with(agent, &say.text, &refs))
}

/// 送一句的本体：`waiting` 答这条会话此刻在不在等人，`send` 照 `terminal-input` 送 `words`（各单独粘一次的 ＋ 正文）。
pub(crate) fn say_typed(
    say: &Say,
    words: &crate::agents::Typed,
    waiting: &dyn Fn(&str) -> bool,
    send: &dyn Fn(&TerminalInputReq) -> Result<Sent, CmdErr>,
    resume: &dyn Fn(&Say) -> Option<Reopened>,
) -> Result<Delivery, CmdErr> {
    if waiting(&say.sid) {
        return Ok(Delivery::Waiting);
    }
    let sent = send(&TerminalInputReq {
        sid: Some(say.sid.to_string()),
        text: Some(words.text.clone()),
        enter: Some(true),
        client: say.client.clone(),
        pastes: words.pastes.clone(),
        ..TerminalInputReq::default()
    })?;
    // 这台已经没有它的终端（名单里没有 · 送的那一刻没了）⇒ 恢复再送；别的被拒原样回。
    if !crate::control::terminals::terminal_gone(sent.why) {
        return Ok(Delivery::of(sent));
    }
    let Some(a) = resume(say) else {
        return Ok(Delivery::of(sent));
    };
    let why = match &a {
        Ok(o) if matches!(o.landed, Landed::Started | Landed::InPlace) => {
            return Ok(Delivery::In { resumed: true })
        }
        Ok(o) => {
            tracing::warn!(
                "[session-send] {} 终端没了、恢复时已在跑（{:?}），那一句没送",
                say.sid,
                o.session
            );
            Undeliverable::NotResumed
        }
        Err((code, said, _, raw)) => {
            tracing::warn!(
                "[session-send] {} 终端没了、恢复没成：{code} {said} {raw:?}",
                say.sid
            );
            if *code == "already_live" {
                Undeliverable::Elsewhere
            } else {
                Undeliverable::NotResumed
            }
        }
    };
    Ok(Delivery::No(why))
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_send_tests.rs"]
mod tests;
