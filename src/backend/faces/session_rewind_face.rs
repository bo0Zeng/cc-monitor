//! 帧面 `session-rewind-options` 的宿主壳：编辑已发的一句、发送那一下弹的选项。
//!
//! 只读，不碰会话（改动全攒在发件那一端，只有确认那一下随那句进输入队）：会话所在那台照那一家申报的「怎么退」几项交出
//! （`control/rewind.rs::options_of`），主 agent 在干活才多给「什么时候退」。会话不在这台 ⇒ 经输入队那一跳交给那台
//! （[`super::session_queue_hop::route`]），够不到 ⇒ `offline`。

use super::session_queue_hop::{route, Aimed};
use crate::dial::remote_ask::{DialRemote, REACH};
use crate::wire::spec::Fail;
use crate::wire::typed::{self, codes, Codes as _, Command};
use cmd_wire::rewind::{RewindOptions, SessionRewindOptionsReq};
use cmd_wire::Machine;
use copy_core::copy_text;

codes! {
    /// `session-rewind-options` 的码。
    #[allow(dead_code, reason = "同 `SessionRewindOptionsCmd`：命令表那一行合并时补")]
    pub(crate) enum SessionRewindOptionsErr {
        /// 那台找不到这条会话的记录。
        NoSuchSession = "no_such_session",
        /// 会话所在那台够不到（不在机器表 · 链路坏 · 常驻没在跑 · 过了期限）。
        Offline = "offline",
    }
}

impl Aimed for SessionRewindOptionsReq {
    fn machine(&self) -> &Machine {
        &self.machine
    }
    fn here(&self) -> Self {
        Self {
            machine: Machine::local(),
            ..self.clone()
        }
    }
}

/// `session-rewind-options`：发送那一下给哪几项（问会话所在那台）。
#[allow(
    dead_code,
    reason = "命令表那一行在 `stream/inbound/registry/`（不在 A2-13 写区，合并时补，补的时候摘掉这一条）"
)]
pub(crate) struct SessionRewindOptionsCmd;

impl Command for SessionRewindOptionsCmd {
    const NAME: &'static str = "session-rewind-options";
    type Req = SessionRewindOptionsReq;
    type Reply = RewindOptions;
    type Err = SessionRewindOptionsErr;
}

impl typed::Handle for SessionRewindOptionsCmd {
    type Needs = typed::needs::Nothing;
    fn run(
        req: SessionRewindOptionsReq,
        cx: typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RewindOptions, SessionRewindOptionsErr>> + Send + 'static
    {
        async move {
            let sid = req.sid.to_string();
            route::<Self, _>(
                &req,
                move || here(&sid),
                SessionRewindOptionsErr::ALL,
                &cx.tz,
                cx.until,
                &REACH,
                &DialRemote,
            )
            .await
            .map_err(|f| typed::of_fail(f, SessionRewindOptionsErr::NoSuchSession))
        }
    }
}

/// 这台那一份：记录在 ⇒ 那一家申报的几项 ＋ 此刻在不在干活。
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "同 `SessionRewindOptionsCmd`：命令表那一行合并时补"
    )
)]
pub(crate) fn here(sid: &str) -> Result<RewindOptions, Fail> {
    let face = crate::control::rewind::face_here(sid).map_err(|raw| {
        Fail::new(
            "no_such_session",
            copy_text("beSessionCommands.session.noRecord", &[]),
        )
        .with_raw(Some(&raw))
    })?;
    Ok(crate::control::rewind::options_of(
        face.as_ref(),
        crate::control::rewind::busy_here(sid),
    ))
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_rewind_face_tests.rs"]
mod tests;
