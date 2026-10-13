//! 枢纽问各台的那 8 条内层命令（typed）：每条 `impl Command`（线上名只写这一处）＋ `impl Inner`（枢纽问的那一侧取前者的三样），
//! 本体在 `skill_install` · `skill_flow` · `mcp_sync_flow` · `cc_bus_install`。写的那三条（两条装 · cc-bus 装）走「文件管理面 ＋ 写句柄」那一档：
//! 写用户的文件经文件管理面、写装记录经写句柄；其余几条只读（要读 MCP 配置的那两条经文件管理面读）。

use crate::assets::hub_wire::{self as wire, Inner};
// 标记类型住 `assets::hub_wire`（枢纽按它们问）；命令表从这里认它们（本体在这份文件里）。
pub(crate) use crate::assets::hub_wire::{
    CcBusInstallCmd, CcBusStateCmd, McpApplyCmd, McpPreviewCmd, McpSourceCmd, SkillApplyCmd,
    SkillPlanCmd, SkillReadCmd,
};
use crate::stream::inbound::spec::Fail;
use crate::stream::inbound::typed::{
    self, codes, Blocking, Codes, Command, Cx, Files, WithFiles, WithOwn,
};

/// 下层那一份失败收进这一条的码表（码不在表里 ⇒ `fallback`）。
fn own_err<E: Codes>(fallback: fn(String) -> E) -> impl Fn(Fail) -> E {
    move |f| typed::of_fail(f, fallback)
}

/// 装记录的写口（`skill-install-record` 那一份本体，写句柄由分派那一层递）。
fn record(
    own: &crate::platform::Own,
) -> impl Fn(
    &crate::assets::skill_ledger::RecordReq,
) -> Result<crate::assets::skill_ledger::RecordReply, Fail>
       + '_ {
    move |a| crate::assets::skill_ledger::answer_record(a, own)
}

codes! {
    /// `skill-read` 的码。
    pub(crate) enum SkillReadErr {
        /// 这台找不到 skill 的根 · 走目录没成。
        IoFailed = "io_failed",
        /// 这台没有这个 skill。
        NotFound = "not_found",
        /// 文件数超了上限。
        TooLarge = "too_large",
    }
}

impl Command for SkillReadCmd {
    const NAME: &'static str = "skill-read";
    type Req = wire::SkillReadReq;
    type Reply = wire::SkillRead;
    type Err = SkillReadErr;
}

impl Inner for SkillReadCmd {
    const NAME: &'static str = <Self as Command>::NAME;
    type Req = <Self as Command>::Req;
    type Reply = <Self as Command>::Reply;
}

impl Blocking for SkillReadCmd {
    fn run(req: wire::SkillReadReq, _cx: &Cx) -> Result<wire::SkillRead, SkillReadErr> {
        crate::assets::skill_install::answer_read(&req).map_err(own_err(SkillReadErr::IoFailed))
    }
}

codes! {
    /// `skill-install-plan` 的码。
    pub(crate) enum SkillPlanErr {
        /// 这台那一份盖不了（不是文本）却勾了。
        BadFile = "bad_file",
        /// 这台找不到 skill 的根 · 走目录没成。
        IoFailed = "io_failed",
        /// 不同的那几个没在 `overwrite` 里点名。
        NeedsConsent = "needs_consent",
        /// 文件数超了上限。
        TooLarge = "too_large",
    }
}

impl Command for SkillPlanCmd {
    const NAME: &'static str = "skill-install-plan";
    type Req = wire::SkillPlanReq;
    type Reply = wire::SkillPlan;
    type Err = SkillPlanErr;
}

impl Inner for SkillPlanCmd {
    const NAME: &'static str = <Self as Command>::NAME;
    type Req = <Self as Command>::Req;
    type Reply = <Self as Command>::Reply;
}

impl Blocking for SkillPlanCmd {
    fn run(req: wire::SkillPlanReq, _cx: &Cx) -> Result<wire::SkillPlan, SkillPlanErr> {
        crate::assets::skill_install::answer_plan(&req).map_err(own_err(SkillPlanErr::IoFailed))
    }
}

codes! {
    /// `skill-install-apply` 的码。
    pub(crate) enum SkillApplyErr {
        /// 这台那一份盖不了（不是文本）却勾了。
        BadFile = "bad_file",
        /// 这台找不到 skill 的根 · 走目录没成。
        IoFailed = "io_failed",
        /// 不同的那几个没在 `overwrite` 里点名。
        NeedsConsent = "needs_consent",
        /// 看过之后这台变了 · 写到一半停了。
        Stale = "stale",
        /// 文件数超了上限。
        TooLarge = "too_large",
    }
}

impl Command for SkillApplyCmd {
    const NAME: &'static str = "skill-install-apply";
    type Req = wire::SkillApplyReq;
    type Reply = wire::SkillApplied;
    type Err = SkillApplyErr;
}

impl Inner for SkillApplyCmd {
    const NAME: &'static str = <Self as Command>::NAME;
    type Req = <Self as Command>::Req;
    type Reply = <Self as Command>::Reply;
}

impl WithOwn for SkillApplyCmd {
    fn run(
        req: wire::SkillApplyReq,
        _cx: &Cx,
        files: Files,
        own: &crate::platform::Own,
    ) -> Result<wire::SkillApplied, SkillApplyErr> {
        crate::assets::skill_flow::answer_install(
            files,
            &crate::assets::mcp_sync::Live::from_env(),
            None,
            &record(own),
            &req,
        )
        .map_err(own_err(SkillApplyErr::IoFailed))
    }
}

codes! {
    /// `mcp-sync-source` 的码。
    pub(crate) enum McpSourceErr {
        /// 那份配置不是 JSON / 形状不对。
        BadFile = "bad_file",
        /// 项目目录不是这台上的绝对路径。
        BadPath = "bad_path",
        /// 这台找不到用户级那一份。
        IoFailed = "io_failed",
        /// 那份文件 · 那一条不在。
        Missing = "missing",
        /// 读那份文件没成。
        Refused = "refused",
    }
}

impl Command for McpSourceCmd {
    const NAME: &'static str = "mcp-sync-source";
    type Req = wire::McpSourceReq;
    type Reply = wire::McpSource;
    type Err = McpSourceErr;
}

impl Inner for McpSourceCmd {
    const NAME: &'static str = <Self as Command>::NAME;
    type Req = <Self as Command>::Req;
    type Reply = <Self as Command>::Reply;
}

impl WithFiles for McpSourceCmd {
    fn run(
        req: wire::McpSourceReq,
        _cx: &Cx,
        files: Files,
    ) -> Result<wire::McpSource, McpSourceErr> {
        crate::assets::mcp_sync_flow::answer_source(
            files,
            crate::assets::ext::Env::here().user_mcp.as_deref(),
            &req,
        )
        .map_err(own_err(McpSourceErr::Refused))
    }
}

codes! {
    /// `mcp-sync-preview` 的码。
    pub(crate) enum McpPreviewErr {
        /// 这台那份配置不是 JSON / 形状不对。
        BadFile = "bad_file",
        /// 项目目录不是这台上的绝对路径。
        BadPath = "bad_path",
        /// 用户级只读 · 读那份文件没成。
        Refused = "refused",
    }
}

impl Command for McpPreviewCmd {
    const NAME: &'static str = "mcp-sync-preview";
    type Req = wire::McpPreviewReq;
    type Reply = wire::McpPreview;
    type Err = McpPreviewErr;
}

impl Inner for McpPreviewCmd {
    const NAME: &'static str = <Self as Command>::NAME;
    type Req = <Self as Command>::Req;
    type Reply = <Self as Command>::Reply;
}

impl WithFiles for McpPreviewCmd {
    fn run(
        req: wire::McpPreviewReq,
        _cx: &Cx,
        files: Files,
    ) -> Result<wire::McpPreview, McpPreviewErr> {
        crate::assets::mcp_sync_flow::answer_preview(
            files,
            &crate::assets::mcp_sync::Live::from_env(),
            &req,
        )
        .map_err(own_err(McpPreviewErr::Refused))
    }
}

codes! {
    /// `mcp-sync-apply` 的码。
    pub(crate) enum McpApplyErr {
        /// 这台那份配置不是 JSON / 形状不对。
        BadFile = "bad_file",
        /// 项目目录不是这台上的绝对路径。
        BadPath = "bad_path",
        /// 有空位既没填、这台也没有原值。
        NeedsInput = "needs_input",
        /// 用户级只读 · 写没成。
        Refused = "refused",
        /// 看卡之后这台那份变了。
        Stale = "stale",
    }
}

impl Command for McpApplyCmd {
    const NAME: &'static str = "mcp-sync-apply";
    type Req = wire::McpApplyReq;
    type Reply = wire::McpApplied;
    type Err = McpApplyErr;
}

impl Inner for McpApplyCmd {
    const NAME: &'static str = <Self as Command>::NAME;
    type Req = <Self as Command>::Req;
    type Reply = <Self as Command>::Reply;
}

impl WithOwn for McpApplyCmd {
    fn run(
        req: wire::McpApplyReq,
        _cx: &Cx,
        files: Files,
        own: &crate::platform::Own,
    ) -> Result<wire::McpApplied, McpApplyErr> {
        crate::assets::mcp_sync_flow::answer_apply(files, &record(own), &req)
            .map_err(own_err(McpApplyErr::Refused))
    }
}

codes! {
    /// `cc-bus-install-state` 的码。
    pub(crate) enum CcBusStateErr {
        /// 这台找不到 skill 的根。
        Refused = "refused",
    }
}

impl Command for CcBusStateCmd {
    const NAME: &'static str = "cc-bus-install-state";
    const TAKES_INPUT: bool = false;
    type Req = wire::NoReq;
    type Reply = wire::CcBusState;
    type Err = CcBusStateErr;
}

impl Inner for CcBusStateCmd {
    const NAME: &'static str = <Self as Command>::NAME;
    type Req = <Self as Command>::Req;
    type Reply = <Self as Command>::Reply;
}

impl Blocking for CcBusStateCmd {
    fn run(_req: wire::NoReq, _cx: &Cx) -> Result<wire::CcBusState, CcBusStateErr> {
        crate::assets::cc_bus_install::answer_state().map_err(own_err(CcBusStateErr::Refused))
    }
}

codes! {
    /// `cc-bus-install` 的码。
    pub(crate) enum CcBusInstallErr {
        /// 内嵌那一份不是 UTF-8。
        BadFile = "bad_file",
        /// 找不到根 · 围栏不过 · 写没成。
        Refused = "refused",
    }
}

impl Command for CcBusInstallCmd {
    const NAME: &'static str = "cc-bus-install";
    const TAKES_INPUT: bool = false;
    type Req = wire::NoReq;
    type Reply = wire::CcBusInstalled;
    type Err = CcBusInstallErr;
}

impl Inner for CcBusInstallCmd {
    const NAME: &'static str = <Self as Command>::NAME;
    type Req = <Self as Command>::Req;
    type Reply = <Self as Command>::Reply;
}

impl WithOwn for CcBusInstallCmd {
    fn run(
        _req: wire::NoReq,
        _cx: &Cx,
        files: Files,
        own: &crate::platform::Own,
    ) -> Result<wire::CcBusInstalled, CcBusInstallErr> {
        crate::assets::cc_bus_install::answer_install(files, &record(own))
            .map_err(own_err(CcBusInstallErr::Refused))
    }
}
