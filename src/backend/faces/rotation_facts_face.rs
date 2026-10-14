//! 帧面 `rotation-facts`（typed）：这台这一家每个号此刻的事实（用量 · 重置 · 用速 · 被拒 · 超额 · 登录 · 用不了的那一句）。
//! 只读；拼装只在 [`crate::accounts::upstream_select::rotate::Hop::facts`] 一处。

use crate::accounts::quota::facts::Fact;
use crate::wire::typed::{Blocking, Command, Cx};

/// `rotation-facts` 的入参。
#[derive(Debug, Default, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct RotationFactsReq {
    /// 哪一家（适配器 id）；缺 ⇒ 有账号库的那一家。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) agent: Option<String>,
}

/// `rotation-facts` 的应答。
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationFactsReply {
    /// 这台此刻（unix 秒）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) now: u64,
    /// 照账号库的先后。
    pub(crate) accounts: Vec<Fact>,
}

crate::wire::typed::codes! {
    /// `rotation-facts` 自己的码（只有入参不对那一种）。
    pub(crate) enum RotationFactsErr {}
}

/// `rotation-facts`：读额度账与账号库 ⇒ 阻塞档。
pub(crate) struct RotationFacts;

impl Command for RotationFacts {
    const NAME: &'static str = "rotation-facts";
    type Req = RotationFactsReq;
    type Reply = RotationFactsReply;
    type Err = RotationFactsErr;
}

impl Blocking for RotationFacts {
    fn run(req: RotationFactsReq, cx: &Cx) -> Result<RotationFactsReply, RotationFactsErr> {
        let ctx = crate::faces::rotation_face::Ctx::here();
        Ok(facts_with(
            &ctx,
            &req,
            crate::accounts::quota::now_unix(),
            &cx.tz,
        ))
    }
}

/// 本体（测试给假的账号库 · 额度账 · 钟）。
pub(crate) fn facts_with(
    ctx: &crate::faces::rotation_face::Ctx,
    req: &RotationFactsReq,
    now: u64,
    tz: &crate::Tz,
) -> RotationFactsReply {
    let agent = req
        .agent
        .as_deref()
        .unwrap_or(crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT);
    let row = |a: &str| (ctx.rows)(agent, a);
    RotationFactsReply {
        now,
        accounts: ctx.hop.facts(agent, &row, now, tz),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/rotation_facts_face_tests.rs"]
mod tests;
