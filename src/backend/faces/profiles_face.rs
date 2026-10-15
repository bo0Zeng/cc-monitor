//! 配置文件那两条要问控制面的命令（typed）：`profiles-read`（这台有没有 tmux）· `profiles-resolve`（渲 ccm 那一行）。
//! 本体住 `assets::aliases::page`；探 tmux（`control::terminals::tmux_here`）与起会话那一整套计划与渲染
//! （`control::ccm::preview_resolved`）由这里交进去。两边只在这里接上：资产域不认识控制面（`layering_guard` 的 assets ↛ control）。

use crate::assets::aliases::page::{
    answer_resolve, read_with, ProfilesBook, ProfilesErr, ProfilesResolveReq, Resolved,
};
use crate::wire::typed::{of_pair, Command, Cx, Files, NoArgs, WithFiles};

/// `profiles-read`：读回配置文件整份（经这台的文件管理面读写 ⇒ 阻塞档，带文件面）。
pub(crate) struct ProfilesRead;

impl Command for ProfilesRead {
    const NAME: &'static str = "profiles-read";
    const TAKES_INPUT: bool = false;
    type Req = NoArgs;
    type Reply = ProfilesBook;
    type Err = ProfilesErr;
}

impl WithFiles for ProfilesRead {
    fn run(_req: NoArgs, cx: &Cx, files: Files) -> Result<ProfilesBook, ProfilesErr> {
        read_with(files, crate::control::terminals::tmux_here(), &cx.tz)
            .map_err(of_pair(ProfilesErr::Refused))
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

impl WithFiles for ProfilesResolve {
    fn run(req: ProfilesResolveReq, _cx: &Cx, files: Files) -> Result<Resolved, ProfilesErr> {
        answer_resolve(files, &req, crate::control::ccm::preview_resolved)
            .map_err(of_pair(ProfilesErr::Refused))
    }
}
