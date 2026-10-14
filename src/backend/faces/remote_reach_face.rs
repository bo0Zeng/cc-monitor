//! 帧面 `remote-reach` 的宿主壳：本机后端的可达表登记（本体 `dial::remote_ask::answer_reach_with`），
//! 登记成了再把从旧设置搬来、还待交给那一台的偏好后台交一次（`control::prefs_migration::after_reach`）。
//! 两边只在这里接上：拨号那一层不认识控制面（`layering_guard` 的 dial ↛ control）。

use crate::dial::remote_ask::{answer_reach_with, ReachReq, Reached, REACH};
use crate::wire::typed::{codes, Command};
use std::future::Future;

codes! {
    /// `remote-reach` 只有 `bad_args`（名字空 · 拨号读不动 · 表满了）。
    pub(crate) enum ReachErr {}
}

/// `remote-reach`：本机后端的可达表登记。纯内存（一把锁、插一行）⇒ 不进阻塞档。
pub(crate) struct RemoteReach;

impl Command for RemoteReach {
    const NAME: &'static str = "remote-reach";
    type Req = ReachReq;
    type Reply = Reached;
    type Err = ReachErr;
}

impl crate::wire::typed::Handle for RemoteReach {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: ReachReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl Future<Output = Result<Reached, ReachErr>> + Send + 'static {
        let got = answer_reach_with(&req, &REACH).map_err(|(_, said)| ReachErr::BadArgs(said));
        // 第 7 批换形：那台连上了 ⇒ 从旧设置搬来、还待交给它的那一份偏好后台交一次（只在常驻那一位；下一版删）。
        if let Ok(r) = &got {
            crate::control::prefs_migration::after_reach(&r.origin, cx.tz.clone());
        }
        std::future::ready(got)
    }
}
