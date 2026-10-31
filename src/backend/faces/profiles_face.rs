//! 配置文件那两条要问控制面的命令（typed）：`profiles-read`（这台有没有 tmux）· `profiles-resolve`（渲 ccm 那一行）。
//! 本体住 `assets::aliases::page`；探 tmux（`control::terminals::tmux_here`）与起会话那一整套计划与渲染
//! （本文件的 `preview_resolved`：控制面那一份并上这台的账号清单成品）由这里交进去。两边只在这里接上：资产域不认识控制面（`layering_guard` 的 assets ↛ control）。

use crate::assets::aliases::page::{
    answer_resolve, read_with, ProfilesBook, ProfilesErr, ProfilesResolveReq, Resolved,
};
use crate::wire::typed::{of_fail_with, Command, NoArgs};

/// `profiles-read`：读回配置文件整份（经这台的文件管理面读写 ⇒ 阻塞档，带文件面）。
pub(crate) struct ProfilesRead;

impl Command for ProfilesRead {
    const NAME: &'static str = "profiles-read";
    const TAKES_INPUT: bool = false;
    type Req = NoArgs;
    type Reply = ProfilesBook;
    type Err = ProfilesErr;
}

impl crate::wire::typed::Handle for ProfilesRead {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        _req: NoArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ProfilesBook, ProfilesErr>> + Send + 'static {
        let (cx, needs) = cx.split();
        async move {
            let cx = &cx;
            let files = needs;

            read_with(files, crate::control::terminals::tmux_here(), &cx.tz)
                .map_err(of_fail_with(ProfilesErr::Refused))
        }
    }
}

/// `profiles-resolve`：一段合下来的合并表与「等于」那一行（经这台的文件管理面读写 ⇒ 阻塞档，带文件面）。
pub(crate) struct ProfilesResolve;

impl Command for ProfilesResolve {
    const NAME: &'static str = "profiles-resolve";
    type Req = ProfilesResolveReq;
    type Reply = Resolved;
    type Err = ProfilesErr;
}

impl crate::wire::typed::Handle for ProfilesResolve {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        req: ProfilesResolveReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Resolved, ProfilesErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let files = needs;

            answer_resolve(files, &req, crate::faces::profiles_face::preview_resolved)
                .map_err(of_fail_with(ProfilesErr::Refused))
        }
    }
}

/// `ccm-print`（一条别名实际会执行什么）：计划与渲染住控制面（`control::ccm::answer_print`），这里交这台的账号清单成品。
impl crate::wire::typed::Handle for crate::control::ccm::CcmPrint {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: crate::control::ccm::CcmPrintReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<
        Output = Result<crate::control::ccm::CcmLine, crate::control::ccm::PrintErr>,
    > + Send
           + 'static {
        async move {
            crate::control::ccm::answer_print(&req, crate::faces::launch_account_face::list_at)
                .map_err(crate::wire::typed::of_fail_with(
                    crate::control::ccm::PrintErr::Refused,
                ))
        }
    }
}

/// 别名页「等于」那一行（`assets::aliases::page::RenderLine`）：同 `ccm @名 -- --ccm-print` 那一份计划，号照这台的账号清单成品判。
pub(crate) fn preview_resolved(
    p: crate::ccm_line::argv::Parsed,
    home: &str,
    at: Option<&str>,
) -> Result<(String, String), String> {
    crate::control::ccm::preview_resolved(p, home, at, crate::faces::launch_account_face::list_at)
}
