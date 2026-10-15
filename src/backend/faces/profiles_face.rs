//! 配置文件那两条要问控制面的命令（typed）：`profiles-read`（这台有没有 tmux）· `profiles-resolve`（渲 ccm 那一行）。
//! 本体住 `assets::aliases::page`；探 tmux（`control::terminals::tmux_here`）与起会话那一整套计划与渲染
//! （`control::ccm::preview_resolved`）由这里交进去。两边只在这里接上：资产域不认识控制面（`layering_guard` 的 assets ↛ control）。

use crate::assets::aliases::page::{
    answer_resolve, form_of_line, read_with, ProfileForm, ProfilesBook, ProfilesErr,
    ProfilesFromLineReq, ProfilesResolveReq, Resolved,
};
use crate::wire::typed::{of_pair, Command, NoArgs};

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
                .map_err(of_pair(ProfilesErr::Refused))
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

            answer_resolve(files, &req, crate::control::ccm::preview_resolved)
                .map_err(of_pair(ProfilesErr::Refused))
        }
    }
}

/// `profiles-from-line`：一条自由命令行 ⇒ 别名表单（判法住 `assets::aliases::page::form_of_line`；不读不写盘）。
pub(crate) struct ProfilesFromLine;

impl Command for ProfilesFromLine {
    const NAME: &'static str = "profiles-from-line";
    type Req = ProfilesFromLineReq;
    type Reply = ProfileForm;
    type Err = ProfilesErr;
}

impl crate::wire::typed::Handle for ProfilesFromLine {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: ProfilesFromLineReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ProfileForm, ProfilesErr>> + Send + 'static {
        async move { form_of_line(&req).map_err(of_pair(ProfilesErr::Refused)) }
    }
}
