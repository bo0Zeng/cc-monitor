//! 帧面 `session-self`（agent 自问：我是哪条会话 · 在哪几组 · 组员有谁）：本体住 `history/session_self.rs`，
//! 转给常驻问各台那一段住 `control/cc_bus_scope.rs::groups_via_resident`（cc-bus 的范围判定也读它，只一处）。
//! 标记类型住这里：`Handle` 要接控制面那一段，按孤儿规则只能和标记类型同在一个包（回包与码住 `backend-history`）。

use crate::history::session_self as me;

/// `session-self`：调用进程往上找这台的哪一条会话、它在哪几组。
pub struct SessionSelfCmd;

impl crate::wire::typed::Command for SessionSelfCmd {
    const NAME: &'static str = "session-self";
    type Req = crate::wire::typed::NoArgs;
    type Reply = me::SessionSelf;
    type Err = me::SessionSelfErr;
}

impl crate::wire::typed::Handle for SessionSelfCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: crate::wire::typed::NoArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<me::SessionSelf, me::SessionSelfErr>> + Send + 'static
    {
        async move {
            let sid = me::who_at(&crate::history::listing::agent_home(), std::process::id())
                .ok_or_else(|| {
                    me::SessionSelfErr::NotInSession(crate::common::contract::malformed(
                        "no process above this one is a session on this machine",
                    ))
                })?;
            let tz = cx.tz.clone();
            Ok(me::answer(
                sid,
                crate::control::cc_bus_scope::groups_via_resident(&tz),
                crate::history::groups::read_here(tz.clone()),
            )
            .await)
        }
    }
}
