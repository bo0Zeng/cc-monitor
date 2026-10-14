//! 首次运行「开始用 · 剩 N 步」那份数（帧面 `first-run`）：三步各自由这台的事实打勾，判定只在这里。
//!
//! - `terminal`（让终端认得 ccm 和别名）：这台某一份启动文件里有别名块（与「别名与配置文件」那一行同一份候选）。
//! - `named`（给现在登录的号起个名字，可选）：这台启用了多账号（账号库清单读得出，与账号页 `meta.enabled` 同一个判法）。
//! - `remote`（加一台远端机器，可选）：机器表里至少有一台。机器表住 monitor 那一侧，台数由问的那一方带上（`remotes`），这里只判。
//!
//! 哪步必做也在这里定（`required`：今天只有 `terminal`；另两步可选）。`left` ＝ 必做而还没打勾的几步（主窗口状态栏那一枚只数它，
//! 可选的不为它一直催）；`skipped` ＝ 「开始用」那一块点过「跳过」（记在 `chores.json`，经 `chores-mark` 的 `skipStart` 写）。只读。

use crate::common::door::Door;
use crate::platform::shell::dialect::Shell;
use serde::{Deserialize, Serialize};

/// `first-run` 的入参。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct FirstRunReq {
    /// 机器表里有几台远端（机器表住 monitor 那一侧，问的那一方带上）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) remotes: u64,
}

/// 三步里的哪一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "FirstRunStepId"
    )
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StepId {
    /// 让终端认得 ccm 和别名：某份启动文件里有别名块。
    Terminal,
    /// 给现在登录的号起名字：启用了多账号。
    Named,
    /// 加一台远端：`remotes` > 0。
    Remote,
}

/// 一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct FirstRunStep {
    pub(crate) id: StepId,
    pub(crate) done: bool,
    /// 必做（今天只有 `terminal`）。
    pub(crate) required: bool,
}

/// `first-run` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct FirstRun {
    pub(crate) steps: Vec<FirstRunStep>,
    /// 必做而还没打勾的几步（主窗口状态栏那一枚只数它）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) left: usize,
    /// 「开始用」那一块点过「跳过」（`chores-mark` 的 `skipStart` 写）。
    pub(crate) skipped: bool,
}

/// 三步的事实 ⇒ 成品。**纯函数**。
pub(crate) fn product(terminal: bool, named: bool, remotes: u64, skipped: bool) -> FirstRun {
    let steps = vec![
        FirstRunStep {
            id: StepId::Terminal,
            done: terminal,
            required: true,
        },
        FirstRunStep {
            id: StepId::Named,
            done: named,
            required: false,
        },
        FirstRunStep {
            id: StepId::Remote,
            done: remotes > 0,
            required: false,
        },
    ];
    let left = steps.iter().filter(|s| s.required && !s.done).count();
    FirstRun {
        steps,
        left,
        skipped,
    }
}

fn terminal_here(door: &dyn Door) -> bool {
    let shell = if cfg!(windows) {
        Shell::PowerShell
    } else {
        Shell::Posix
    };
    let marks = super::chores::marks::current(super::chores::marks::marks_path().as_deref());
    crate::assets::aliases::read_via(door, shell, marks.self_paste.as_deref())
        .map(|l| l.rc_candidates.iter().any(|c| c.block.present))
        .unwrap_or(false)
}

/// `first-run {remotes}`：帧面入口（只读）。
pub(crate) fn answer(door: &dyn Door, req: &FirstRunReq) -> FirstRun {
    let remotes = req.remotes;
    let skipped =
        super::chores::marks::current(super::chores::marks::marks_path().as_deref()).start_skipped;
    product(
        terminal_here(door),
        crate::observe::accounts_query::accounts_enabled_here(),
        remotes,
        skipped,
    )
}

#[cfg(test)]
#[path = "../../../tests/backend/footprint/readiness_tests.rs"]
mod tests;
