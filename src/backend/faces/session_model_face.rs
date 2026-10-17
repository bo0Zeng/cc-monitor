//! 帧面 `session-model` 的宿主壳：换模型。这台已经没有它的终端 ⇒ 记在这条会话上、下次恢复照它起（`history_annotations::keep_for_resume`）；
//! 开着 ⇒ 往会话里送那一家声明的那一行（Claude：`/model <值>`），
//! 再从记录里此后长出的那一条读回换成了什么（适配层 `LaunchFace.model_switch` 认）。在等人不送；读不回 ⇒ 结果未知，不猜。
//! 那一家换模型时也把它存成这个号以后新会话的默认 —— 读回那一句照实说。

use crate::agents::ModelSaid;
use crate::common::contract::malformed;
use crate::control::terminals::{no_said, Refused};
use crate::control::terminals_wire::{Sent, SentResult, TerminalInputReq};
use crate::faces::session_mode_face::SwitchResult;
use crate::wire::spec::Fail;
use crate::wire::typed::{self, Command};
use copy_core::copy_text;
use serde::Deserialize;

/// 送出那一行之后最多等多久读回（毫秒）：那一家处理本地命令是当场的，这是兜底的期限。
const READBACK_MS: u64 = 15_000;

/// `session-model` 的入参。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionModelReq {
    /// 会话 id。
    pub(crate) sid: String,
    /// 要换成的模型（原值；必须是 `session-open-facts` 那一家 `models` 里的一个）。
    pub(crate) model: String,
    /// 自报的前端（过「哪个前端的会话」那一维，同 `terminal-input`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) client: Option<String>,
}

/// `session-model` 的回包：线上形住 `cmd-wire::session`。
pub(crate) use cmd_wire::session::ModelSwitched;

typed::codes! {
    /// `session-model` 的码：认会话 · 装读回的耳朵 · 列名单 · 过门 · 送（同 `terminal-input`）。在等人 · 认不出 · 不归我们不是错（`result`）。
    pub(crate) enum SessionModelErr {
        /// 那一家会话开着时换不了模型。
        Unsupported = "unsupported",
        /// 这台找不到这条会话的记录。
        NoSuchSession = "no_such_session",
        /// tmux 起不来。
        NoTmux = "no_tmux",
        /// 列名单 · 装读回的耳朵失败。
        Unobservable = "unobservable",
        /// 那一发 `tmux` 过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

fn reply(result: SwitchResult, why: Option<Refused>, said: Option<String>) -> ModelSwitched {
    ModelSwitched {
        result,
        model: None,
        effort: None,
        why,
        said,
    }
}

/// 在等人：不送。
pub(crate) fn waiting_reply() -> ModelSwitched {
    reply(
        SwitchResult::Refused,
        Some(Refused::Waiting),
        Some(no_said(Refused::Waiting)),
    )
}

/// 送那一行的结局：送到了 ⇒ `None`（接着等读回）；这台已经没有它的终端 ⇒ `keep` 记下、等恢复照它起，回换成了（`model` ＝ 清单里那一项的字）；
/// 别的没送到 · 不知道送没送到 ⇒ 就是结局。
pub(crate) fn after_send(
    sent: Sent,
    keep: &dyn Fn() -> Result<String, Fail>,
) -> Result<Option<ModelSwitched>, Fail> {
    if crate::control::terminals::terminal_gone(sent.why) {
        return Ok(Some(ModelSwitched {
            model: Some(keep()?),
            ..reply(SwitchResult::Done, None, None)
        }));
    }
    Ok(match sent.result {
        SentResult::Delivered => None,
        SentResult::Unsure => Some(reply(SwitchResult::Unsure, None, sent.said)),
        SentResult::Refused => Some(reply(SwitchResult::Refused, sent.why, sent.said)),
    })
}

/// 读回（期限内没读到 ⇒ `None`）⇒ 结局。
pub(crate) fn settled(got: Option<ModelSaid>) -> ModelSwitched {
    let Some(s) = got else {
        return reply(
            SwitchResult::Unsure,
            None,
            Some(copy_text("beSessionModel.unsure.noReadback", &[])),
        );
    };
    let said = if s.kept {
        copy_text("beSessionModel.set.kept", &[("model", &s.name)])
    } else {
        copy_text("beSessionModel.set.saved", &[("model", &s.name)])
    };
    ModelSwitched {
        result: SwitchResult::Done,
        model: Some(s.name),
        effort: s.effort,
        why: None,
        said: Some(said),
    }
}

/// `session-model`：换模型（终端没了 ⇒ 记下、恢复照它起）。异步档：送那一行在阻塞线程池里做，等读回是一次有界等待（文件事件叫醒）。
pub(crate) struct SessionModel;

impl Command for SessionModel {
    const NAME: &'static str = "session-model";
    type Req = SessionModelReq;
    type Reply = ModelSwitched;
    type Err = SessionModelErr;
}

impl crate::wire::typed::Handle for SessionModel {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: SessionModelReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ModelSwitched, SessionModelErr>> + Send + 'static
    {
        let (_cx, own) = cx.split();
        async move {
            answer(req, own)
                .await
                .map_err(|f| typed::of_fail(f, SessionModelErr::Unobservable))
        }
    }
}

/// 送之前那几步的结局：已经有结局了 · 送到了、等读回。
enum Sent2 {
    Done(ModelSwitched),
    Wait(crate::observe::one_wait::Armed<ModelSaid>),
}

async fn answer(req: SessionModelReq, own: crate::platform::Own) -> Result<ModelSwitched, Fail> {
    let pre = tokio::task::spawn_blocking(move || send(&req, &own))
        .await
        .map_err(|_| Fail::new("unobservable", malformed("model task did not finish")))??;
    Ok(match pre {
        Sent2::Done(r) => r,
        Sent2::Wait(ears) => settled(ears.fetch(READBACK_MS).await),
    })
}

/// 认会话 · 核模型 · 判在不在等人 · 先装读回的耳朵 · 再送那一行（同 `session-say` 那一道：窗格里只剩 shell 不送）。
fn send(req: &SessionModelReq, own: &crate::platform::Own) -> Result<Sent2, Fail> {
    if !shell_quote_core::session_id_ok(&req.sid) {
        return Err(Fail::new(
            "bad_args",
            malformed("`sid` must be a session id"),
        ));
    }
    let home = crate::history::listing::agent_home();
    let record = crate::observe::query::locate::session_record(&home, &req.sid).map_err(|_| {
        Fail::new(
            "no_such_session",
            copy_text("beSessionCommands.session.noRecord", &[]),
        )
    })?;
    let face = crate::agents::pick_kind(crate::agents::record_kind_of(&record))
        .ok()
        .map(|(_, f)| f);
    let Some(sw) = face.and_then(|f| f.models.and(f.model_switch)) else {
        return Err(Fail::new(
            "unsupported",
            copy_text("beSessionModel.switch.unsupported", &[]),
        ));
    };
    let Some(choice) = face.and_then(|f| f.model_choice(&req.model)) else {
        return Err(Fail::new(
            "bad_args",
            malformed("`model` is not one this agent offers"),
        ));
    };
    if crate::observe::accounts_query::waiting_now(&home, &req.sid, &crate::control::asks::sids()) {
        return Ok(Sent2::Done(waiting_reply()));
    }
    // 先装耳朵再送：送出之后才长出的那一条才算读回。
    let ears = crate::observe::one_wait::record_pick(&record, sw.read)
        .map_err(|e| Fail::new("unobservable", malformed(&e)))?;
    let sent = crate::control::terminals_wire::say(&TerminalInputReq {
        sid: Some(req.sid.clone()),
        text: Some((sw.command)(&req.model)),
        enter: Some(true),
        client: req.client.clone(),
        ..TerminalInputReq::default()
    })?;
    let keep = || {
        crate::history::history_annotations::keep_for_resume(
            own,
            &req.sid,
            &record,
            Some(&req.model),
            None,
        )
        .map(|()| (choice.text)())
    };
    Ok(match after_send(sent, &keep)? {
        Some(r) => Sent2::Done(r),
        None => Sent2::Wait(ears),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_model_face_tests.rs"]
mod tests;
