//! 帧面 `session-model` 的宿主壳：换模型。这台已经没有它的终端 ⇒ 记在这条会话上、下次恢复照它起（`history_annotations::keep_for_resume`）；
//! 开着 ⇒ 往会话里送那一家声明的那一行（Claude：`/model <值>`），
//! 再从记录里此后长出的那一条读回换成了什么（适配层 `LaunchFace.model_switch` 认）。在等人不送；读不回 ⇒ 结果未知，不猜。
//! 对话里已有回复时那一家先弹确认框 ⇒ 适配层声明了框的样子（`ModelSwitch.confirm`），认出来就替人确认（换模型本来就是人点的）；认不出的框不按。
//! 那一家换模型时也把它存成以后新会话的默认 —— 这条会话用的那个号的那份设置跟别的号共用（链回共享库）⇒ 读回那一句说「各号」，它自己一份 ⇒「此号」。

use crate::agents::ModelSaid;
use crate::common::contract::malformed;
use crate::common::said::Said;
use crate::control::screen_drive::{settle, Drive, REDRAWS_AFTER_SEND};
use crate::control::terminals::{no_said, Refused};
use crate::control::terminals_wire::{Sent, SentResult, TerminalInputReq};
use crate::faces::session_mode_face::SwitchResult;
use crate::wire::typed::{self, Command};
use copy_core::copy_text;

/// 送出那一行之后最多等多久读回（毫秒）：那一家处理本地命令是当场的，这是兜底的期限。
const READBACK_MS: u64 = 15_000;

/// `session-model` 的入参：线上形住 `cmd-wire::session`（名字与回包挂在 `cmd_wire::Call` 上）。
pub use cmd_wire::session::SessionModelReq;

/// `session-model` 的回包：线上形住 `cmd-wire::session`。
pub(crate) use cmd_wire::session::ModelSwitched;

typed::codes! {
    /// `session-model` 的码：认会话 · 装读回的耳朵 · 列名单 · 过门 · 送（同 `terminal-input`）。在等人 · 认不出 · 不归我们不是错（`result`）。
    pub enum SessionModelErr {
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
    keep: &dyn Fn() -> Result<String, Said>,
) -> Result<Option<ModelSwitched>, Said> {
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

/// 读回（期限内没读到 ⇒ `None`）⇒ 结局。`shared` ＝ 换模型顺带写的那份设置各号共用（那一句照它说「各号」·「此号」）。
pub fn settled(got: Option<ModelSaid>, shared: bool) -> ModelSwitched {
    let Some(s) = got else {
        return reply(
            SwitchResult::Unsure,
            None,
            Some(copy_text("beSessionModel.unsure.noReadback", &[])),
        );
    };
    let said = if s.kept {
        copy_text("beSessionModel.set.kept", &[("model", &s.name)])
    } else if shared {
        copy_text("beSessionModel.set.savedShared", &[("model", &s.name)])
    } else {
        copy_text("beSessionModel.set.saved", &[("model", &s.name)])
    };
    ModelSwitched {
        result: SwitchResult::Done,
        model: Some(s.name),
        why: None,
        said: Some(said),
    }
}

/// 找确认框那几样副作用（判据拿合成的屏喂）。
pub(crate) trait ConfirmIo {
    /// 读回到了没有（到了 ⇒ 那一份；不等）。
    fn arrived(&mut self) -> Option<ModelSaid>;
    /// 等下一次重画（到期没变 ⇒ `false`）。
    fn redrawn(&mut self) -> bool;
    /// 此刻那一屏（纯文本行）。
    fn screen(&mut self) -> Result<Vec<String>, Said>;
    /// 按一下键。
    fn press(&mut self, key: &'static str) -> Result<Sent, Said>;
}

/// 找确认框的结局。
pub(crate) enum Confirmed {
    /// 读回先到了（没弹框）。
    Arrived(ModelSaid),
    /// 认出了框、按下确认送到了：接着等读回。
    Pressed,
    /// 按的那一下没送到 · 不知道送没送到：就是结局。
    Ended(ModelSwitched),
    /// 没见到认得出的框（屏不再变了 · 看满了次数）：照旧等读回，不乱按。
    Nothing,
}

/// 看一屏看到的：读回先到了 · 认出了框 · 都没有。
enum Look {
    Arrived(ModelSaid),
    Boxed,
    Nothing,
}

fn look(bx: &crate::agents::ScreenBox, io: &mut dyn ConfirmIo) -> Result<Look, Said> {
    if let Some(s) = io.arrived() {
        return Ok(Look::Arrived(s));
    }
    Ok(if (bx.seen)(&io.screen()?) {
        Look::Boxed
    } else {
        Look::Nothing
    })
}

/// 送出那一行之后：读回到了就停；每看一屏认一次框，认出 ⇒ 按一下确认键；屏不再变 · 看满了次数 ⇒ 停。
pub(crate) fn confirm_with(
    bx: &crate::agents::ScreenBox,
    io: &mut dyn ConfirmIo,
) -> Result<Confirmed, Said> {
    let first = look(bx, io)?;
    let seen = settle(
        first,
        REDRAWS_AFTER_SEND,
        |l| !matches!(l, Look::Nothing),
        || io.redrawn().then(|| look(bx, io)).transpose(),
    )?;
    Ok(match seen {
        Look::Arrived(s) => Confirmed::Arrived(s),
        Look::Nothing => Confirmed::Nothing,
        Look::Boxed => {
            let sent = io.press(bx.key)?;
            match sent.result {
                SentResult::Delivered => Confirmed::Pressed,
                SentResult::Unsure => {
                    Confirmed::Ended(reply(SwitchResult::Unsure, None, sent.said))
                }
                SentResult::Refused => {
                    Confirmed::Ended(reply(SwitchResult::Refused, sent.why, sent.said))
                }
            }
        }
    })
}

/// 生产那一份：读屏驱动（抓屏不带色 · 按键同 `session-say` 那一道 · 等重画用实时预览那个只读客户端），读回问那只耳朵。
struct LiveBox<'a> {
    drive: Drive<'a>,
    ears: &'a mut crate::observe::one_wait::Armed<ModelSaid>,
}

impl ConfirmIo for LiveBox<'_> {
    fn arrived(&mut self) -> Option<ModelSaid> {
        self.ears.take_now()
    }
    fn redrawn(&mut self) -> bool {
        self.drive.redrawn()
    }
    fn screen(&mut self) -> Result<Vec<String>, Said> {
        self.drive.screen()
    }
    fn press(&mut self, key: &'static str) -> Result<Sent, Said> {
        self.drive.press(key)
    }
}

/// 看屏的那只只读客户端：终端收（同 `session-say` 那一道门）才起；起不来 ⇒ `None`（不找框，照旧等读回）。
fn box_drive<'a>(sid: &'a str, client: Option<String>) -> Option<Drive<'a>> {
    let mut d = Drive::new(sid, client);
    matches!(d.open(), Ok(None)).then_some(d)
}

/// `session-model`：换模型（终端没了 ⇒ 记下、恢复照它起）。异步档：送那一行在阻塞线程池里做，等读回是一次有界等待（文件事件叫醒）。
pub struct SessionModel;

impl Command for SessionModel {
    typed::by_call!(SessionModelReq);
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

/// 送之前那几步的结局：已经有结局了 · 送到了、等读回（带着那份设置各号共不共用）。
enum Sent2 {
    Done(ModelSwitched),
    Wait(crate::observe::one_wait::Armed<ModelSaid>, bool),
}

async fn answer(req: SessionModelReq, own: crate::platform::Own) -> Result<ModelSwitched, Said> {
    let pre = tokio::task::spawn_blocking(move || send(&req, &own))
        .await
        .map_err(|_| {
            crate::wire::spec::fail("unobservable", malformed("model task did not finish"))
        })??;
    Ok(match pre {
        Sent2::Done(r) => r,
        Sent2::Wait(ears, shared) => settled(ears.fetch(READBACK_MS).await, shared),
    })
}

/// 认会话 · 核模型 · 判在不在等人 · 先装读回的耳朵 · 再送那一行（同 `session-say` 那一道：窗格里只剩 shell 不送）·
/// 那一家声明了确认框 ⇒ 送出之后看屏，认出来就替人确认。
fn send(req: &SessionModelReq, own: &crate::platform::Own) -> Result<Sent2, Said> {
    let home = crate::history::listing::agent_home();
    let record = crate::observe::query::locate::session_record(&home, &req.sid).map_err(|_| {
        crate::wire::spec::fail(
            "no_such_session",
            copy_text("beSessionCommands.session.noRecord", &[]),
        )
    })?;
    let face = crate::agents::pick_kind(crate::agents::record_kind_of(&record))
        .ok()
        .map(|(_, f)| f);
    let Some(sw) = face.and_then(|f| f.models.and(f.model_switch)) else {
        return Err(crate::wire::spec::fail(
            "unsupported",
            copy_text("beSessionModel.switch.unsupported", &[]),
        ));
    };
    let Some(choice) = face.and_then(|f| f.model_choice(&req.model)) else {
        return Err(crate::wire::spec::fail(
            "bad_args",
            malformed("`model` is not one this agent offers"),
        ));
    };
    let live = crate::history::live_table::now(&home);
    if live.waiting(&req.sid) {
        return Ok(Sent2::Done(waiting_reply()));
    }
    // 先装耳朵再送：送出之后才长出的那一条才算读回。看屏的客户端也先起：送出之后的重画才都看得见。
    let mut ears = crate::observe::one_wait::record_pick(&record, sw.read)
        .map_err(|e| crate::wire::spec::fail("unobservable", malformed(&e)))?;
    let watch = sw
        .confirm
        .and_then(|bx| box_drive(&req.sid, req.client.clone()).map(|d| (bx, d)));
    let shared = sw.saves.is_some_and(|rel| {
        crate::accounts::manage::library::session_home(&live, &req.sid)
            .is_some_and(|h| crate::accounts::manage::library::shared_with_other_accounts(&h, rel))
    });
    let sent = crate::control::terminals_wire::say(&TerminalInputReq {
        sid: Some(req.sid.to_string()),
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
    if let Some(r) = after_send(sent, &keep)? {
        return Ok(Sent2::Done(r));
    }
    let Some((bx, drive)) = watch else {
        return Ok(Sent2::Wait(ears, shared));
    };
    let mut io = LiveBox {
        drive,
        ears: &mut ears,
    };
    Ok(match confirm_with(&bx, &mut io)? {
        Confirmed::Arrived(s) => Sent2::Done(settled(Some(s), shared)),
        Confirmed::Ended(r) => Sent2::Done(r),
        Confirmed::Pressed | Confirmed::Nothing => Sent2::Wait(ears, shared),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_model_face_tests.rs"]
mod tests;
