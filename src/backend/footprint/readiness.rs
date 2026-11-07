//! 首次运行「开始用」那一份（帧面 `first-run`）：两步各自由这台的事实打勾，每步的名字 · 那一句 · 按钮的字与去处都在这里写好，判定只在这里。
//!
//! - `terminal`（终端认得 cc 那几条别名）：这台某一份启动文件里有别名块（与「别名与配置文件」那一行同一份候选）。
//! - `remote`（加一台远端机器，可选）：机器表里至少有一台。机器表住 monitor 那一侧，台数由问的那一方带上（`remotes`），这里只判。
//!
//! `skipped` ＝ 「开始用」那一块点过「跳过」（记在 `chores.json`，经 `chores-mark` 的 `skipStart` 写）。只读。

use crate::common::cells::Words;
use crate::common::door::Door;
use crate::platform::shell::dialect::Shell;
use copy_core::copy_text;
use serde::{Deserialize, Serialize};

/// `first-run` 的入参。
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct FirstRunReq {
    /// 机器表里有几台远端（机器表住 monitor 那一侧，问的那一方带上）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub(crate) remotes: u64,
}

/// 哪一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "FirstRunStepId",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StepId {
    /// 终端认得 cc 那几条别名：某份启动文件里有别名块。
    Terminal,
    /// 加一台远端：`remotes` > 0。
    Remote,
}

/// 按钮开哪一处（闭集）：别名那一节 · 加机器那个框。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "snake_case")]
pub(crate) enum FirstRunGo {
    Aliases,
    AddMachine,
}

/// 一步的按钮：写好的字 ＋ 开哪一处。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub(crate) struct FirstRunAct {
    pub(crate) label: Words,
    pub(crate) go: FirstRunGo,
}

/// 一步：打没打勾 ＋ 写好的名字 · 那一句 · 按钮。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub(crate) struct FirstRunStep {
    pub(crate) id: StepId,
    pub(crate) done: bool,
    pub(crate) name: Words,
    pub(crate) hint: Words,
    pub(crate) act: FirstRunAct,
}

/// `first-run` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct FirstRun {
    pub(crate) steps: Vec<FirstRunStep>,
    /// 「开始用」那一块点过「跳过」（`chores-mark` 的 `skipStart` 写）。
    pub(crate) skipped: bool,
}

fn step(
    id: StepId,
    done: bool,
    name: String,
    hint: String,
    label: String,
    go: FirstRunGo,
) -> FirstRunStep {
    FirstRunStep {
        id,
        done,
        name: Words(name),
        hint: Words(hint),
        act: FirstRunAct {
            label: Words(label),
            go,
        },
    }
}

/// 两步的事实 ⇒ 成品。**纯函数**。
pub(crate) fn product(terminal: bool, remotes: u64, skipped: bool) -> FirstRun {
    FirstRun {
        steps: vec![
            step(
                StepId::Terminal,
                terminal,
                copy_text("beFirstRun.terminal.name", &[]),
                copy_text("beFirstRun.terminal.hint", &[]),
                copy_text("beFirstRun.terminal.act", &[]),
                FirstRunGo::Aliases,
            ),
            step(
                StepId::Remote,
                remotes > 0,
                copy_text("beFirstRun.remote.name", &[]),
                copy_text("beFirstRun.remote.hint", &[]),
                copy_text("mobHosts.fab.aria", &[]),
                FirstRunGo::AddMachine,
            ),
        ],
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
pub fn answer(door: &dyn Door, req: &FirstRunReq) -> FirstRun {
    let remotes = req.remotes;
    let skipped =
        super::chores::marks::current(super::chores::marks::marks_path().as_deref()).start_skipped;
    product(terminal_here(door), remotes, skipped)
}

#[cfg(test)]
#[path = "../../../tests/backend/footprint/readiness_tests.rs"]
mod tests;
