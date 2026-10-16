//! 足迹那一族的五条小命令（typed）：`agent-home-check` · `chores-mark` · `chores-merge` · `last-seen-read` · `last-seen-write`。
//! 本体在 `footprint/`（判定与写都在那里），这里只接上名字 · 入参 · 回包 · 码表与写句柄档：
//! 只读的两条 ⇒ 阻塞档；写后端自家状态（`~/.cc-monitor/`）的两条 ⇒ 只要写句柄那一档；纯计算的 `chores-merge` ⇒ 异步档。
//! `first-run` 不在这份文件里（`first_run_face.rs`）：它读启动文件那一路够得到别名那一族，引用图按文件算，放一起会把这五条也判成「够得着 tmux」。

use crate::footprint::agent_home_check;
use crate::footprint::chores::{marks, ChoreCounts};
use crate::footprint::last_seen;
use crate::wire::typed::{self, codes, Command};

codes! {
    /// `agent-home-check` 自己的码（只有入参不对那一种：判出来的四态都是回包）。
    pub(crate) enum AgentHomeCheckErr {}
}

/// `agent-home-check`：那个目录能不能当 agent 家目录。
pub(crate) struct AgentHomeCheckCmd;

impl Command for AgentHomeCheckCmd {
    const NAME: &'static str = "agent-home-check";
    type Req = agent_home_check::AgentHomeCheckReq;
    type Reply = agent_home_check::AgentHomeCheck;
    type Err = AgentHomeCheckErr;
}

impl crate::wire::typed::Handle for AgentHomeCheckCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: agent_home_check::AgentHomeCheckReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<agent_home_check::AgentHomeCheck, AgentHomeCheckErr>>
           + Send
           + 'static {
        async move {
            agent_home_check::answer_check(&req).map_err(typed::of_pair(AgentHomeCheckErr::BadArgs))
        }
    }
}

codes! {
    /// `chores-mark` 的码。
    pub(crate) enum ChoresMarkErr {
        /// 建那一层目录 · 拿锁 · 写没成。
        IoFailed = "io_failed",
        /// 盘上那一份读不懂（不覆盖）。
        MarksUnreadable = "marks_unreadable",
    }
}

/// `chores-mark`：记下「待办」里的一个选择。
pub(crate) struct ChoresMarkCmd;

impl Command for ChoresMarkCmd {
    const NAME: &'static str = "chores-mark";
    type Req = marks::ChoresMarkReq;
    type Reply = marks::Marks;
    type Err = ChoresMarkErr;
}

impl crate::wire::typed::Handle for ChoresMarkCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: marks::ChoresMarkReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<marks::Marks, ChoresMarkErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let own = &needs;

            marks::answer_mark(&req, own).map_err(|f| typed::of_fail(f, ChoresMarkErr::IoFailed))
        }
    }
}

/// `chores-merge` 的入参：读成的那几台各自的件数（`data-report` 的 `counts` 原样）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct ChoresMergeReq {
    pub(crate) machines: Vec<ChoreCounts>,
}

codes! {
    /// `chores-merge` 自己没有码（只有入参形状不对的 `bad_args`）。
    pub(crate) enum ChoresMergeErr {}
}

/// `chores-merge`：几台的「待办」件数合成一份（设置窗角标 · 「文件与数据」顶上 · 主窗口状态栏读它；纯计算，不读盘）。
pub(crate) struct ChoresMergeCmd;

impl Command for ChoresMergeCmd {
    const NAME: &'static str = "chores-merge";
    type Req = ChoresMergeReq;
    type Reply = ChoreCounts;
    type Err = ChoresMergeErr;
}

impl crate::wire::typed::Handle for ChoresMergeCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: ChoresMergeReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<ChoreCounts, ChoresMergeErr>> + Send + 'static
    {
        std::future::ready(Ok(crate::footprint::chores::merge_counts(req.machines)))
    }
}

codes! {
    /// `last-seen-read` 的码。
    pub(crate) enum LastSeenReadErr {
        /// 这台的家目录解析不出来。
        IoFailed = "io_failed",
    }
}

/// `last-seen-read`：读离线那台的上次值。
pub(crate) struct LastSeenReadCmd;

impl Command for LastSeenReadCmd {
    const NAME: &'static str = "last-seen-read";
    type Req = last_seen::LastSeenReadReq;
    type Reply = last_seen::LastSeen;
    type Err = LastSeenReadErr;
}

impl crate::wire::typed::Handle for LastSeenReadCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: last_seen::LastSeenReadReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<last_seen::LastSeen, LastSeenReadErr>> + Send + 'static
    {
        async move {
            last_seen::answer_read(&req).map_err(|f| typed::of_fail(f, LastSeenReadErr::IoFailed))
        }
    }
}

codes! {
    /// `last-seen-write` 的码。
    pub(crate) enum LastSeenWriteErr {
        /// 建那一层目录 · 拿锁 · 写没成。
        IoFailed = "io_failed",
        /// 那一份序列化后超过 256 KiB。
        TooLarge = "too_large",
    }
}

/// `last-seen-write`：记下一台这一次读成的那一份（或清掉那台）。
pub(crate) struct LastSeenWriteCmd;

impl Command for LastSeenWriteCmd {
    const NAME: &'static str = "last-seen-write";
    type Req = last_seen::LastSeenWriteReq;
    type Reply = last_seen::LastSeenWritten;
    type Err = LastSeenWriteErr;
}

impl crate::wire::typed::Handle for LastSeenWriteCmd {
    type Needs = crate::wire::typed::needs::Own;
    fn run(
        req: last_seen::LastSeenWriteReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<last_seen::LastSeenWritten, LastSeenWriteErr>>
           + Send
           + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let own = &needs;

            last_seen::answer_write(&req, own)
                .map_err(|f| typed::of_fail(f, LastSeenWriteErr::IoFailed))
        }
    }
}
