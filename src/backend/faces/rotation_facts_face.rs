//! 帧面 `rotation-facts`（typed）：这台这一家每个号此刻的事实（用量 · 重置 · 用速 · 被拒 · 超额 · 登录 · 用不了的那一句）。
//! 只读；拼装只在 [`crate::accounts::upstream_select::rotate::Hop::facts`] 一处。

use crate::wire::typed::Command;
pub(crate) use cmd_wire::rotation::{RotationFactsReply, RotationFactsReq};

crate::wire::typed::codes! {
    /// `rotation-facts` 自己的码（只有入参不对那一种）。
    pub enum RotationFactsErr {}
}

/// `rotation-facts`：读额度账与账号库 ⇒ 阻塞档。
pub struct RotationFacts;

impl Command for RotationFacts {
    crate::wire::typed::by_call!(RotationFactsReq);
    type Err = RotationFactsErr;
}

impl crate::wire::typed::Handle for RotationFacts {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: RotationFactsReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RotationFactsReply, RotationFactsErr>> + Send + 'static
    {
        let (cx, ()) = cx.split();
        async move {
            let cx = &cx;

            let ctx = crate::faces::rotation_face::Ctx::here();
            facts_with(&ctx, &req, crate::common::time::now_secs(), &cx.tz)
        }
    }
}

/// 本体（测试给假的账号库 · 额度账 · 钟）。认不出的 kind ⇒ `bad_args`。
pub(crate) fn facts_with(
    ctx: &crate::faces::rotation_face::Ctx,
    req: &RotationFactsReq,
    now: u64,
    tz: &crate::common::time::Tz,
) -> Result<RotationFactsReply, RotationFactsErr> {
    let agent = match &req.agent {
        None => crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT,
        Some(k) => crate::agents::pick_route(Some(k))
            .map_err(RotationFactsErr::BadArgs)?
            .id(),
    };
    let row = |a: &str| (ctx.rows)(agent, a);
    Ok(RotationFactsReply {
        now,
        accounts: ctx.hop.facts(agent, &row, now, tz),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/rotation_facts_face_tests.rs"]
mod tests;
