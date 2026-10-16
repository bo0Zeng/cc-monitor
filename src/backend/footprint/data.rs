//! 「文件与数据」那一份成品（帧面 `data-report`）：同一份足迹（[`super::rows`]）按设置窗那一页要的三样重排 —— 只读。
//!
//! - `changedFiles`：cc-monitor 写进**你的**文件的那几处（今天在、cc-monitor 装口放的、不在 `~/.cc-monitor/` 里），
//!   每处说改了什么、撤回在哪一页（页 · 栏 · 锚点，界面照它跳）。
//! - `todo`：「待办」各件（[`super::chores`]）；其中「要装」那一类来自 [`needs_install`]：你自己装、cc-monitor 只查的那几样里，
//!   这台**确实缺**的（查不动的不算缺，不报；tmux 一律可选、不进这里）；`required` ＝ 缺了起不了会话。
//! - `tmux`：这台有没有 tmux（查不动 ⇒ `null`；与起新会话那一问同一个判法 `control::terminals::rows_here`）。
//! - `counts`：「待办」各类的件数（[`super::chores::counts`]）；跨台合计由本机后端 `chores-merge` 出，界面不自己数。
//! - `rank`：几台排先后的键（设置窗那一页按它排各台）。
//!
//! - `own`：cc-monitor 在这台自己家里（`~/.cc-monitor/`）放的每一样（[`own_rows`]）：在不在 · 文件多大 · 删了会丢还是能重建。
//!
//! 另带 `home`（显示用：路径的 `~` 缩写按它）。
//!
//! 判定只在这里：哪一行算「改过你的文件」、撤回去哪、哪一样缺了起不了会话、怎么装的链接。

use super::registry::EnvTier;
use super::rows::{ConfigSurfaceReport, SurfaceRow, SurfaceState};
use crate::platform::fs::Ro;

/// cc-monitor 自己的家（相对家目录）：这下面的不是「你的文件」。
use std::borrow::Cow;

const OWN_HOME: &str = "~/.cc-monitor";

/// 设置窗的一处：页 · 栏 · 锚点（与设置窗「带目的地打开」同一种形状；没有的那一格不出）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct UndoAt {
    pub(crate) page: Cow<'static, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) tab: Option<Cow<'static, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub(crate) anchor: Option<Cow<'static, str>>,
}

impl UndoAt {
    /// 只到页。
    pub(crate) const fn page(page: &'static str) -> UndoAt {
        UndoAt {
            page: Cow::Borrowed(page),
            tab: None,
            anchor: None,
        }
    }

    /// 页 · 栏 · 锚点。
    pub(crate) const fn at(page: &'static str, tab: &'static str, anchor: &'static str) -> UndoAt {
        UndoAt {
            page: Cow::Borrowed(page),
            tab: Some(Cow::Borrowed(tab)),
            anchor: Some(Cow::Borrowed(anchor)),
        }
    }
}

/// 撤回在哪：设置窗的页 · 栏 · 锚点。
fn undo_of(tool_id: &str) -> Option<UndoAt> {
    match tool_id {
        // 接上终端那几行：别名与配置文件那一栏里「接上终端」那一行（卸载在那里）。
        "ccm" => Some(UndoAt::at("machine", "config", "connect-terminal")),
        // 扩展装进 `~/.claude/skills/` 的：扩展页卸载。
        "cc-bus" | "skill-install" => Some(UndoAt::page("ext")),
        _ => None,
    }
}

/// cc-monitor 写进用户文件的一处。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct ChangedFile {
    pub(crate) path: Cow<'static, str>,
    pub(crate) what: String,
    /// 撤回在哪（设置窗的页 · 栏 · 锚点）；没有 ⇒ `null`。
    pub(crate) undo: Option<UndoAt>,
}

/// 足迹里这台确实缺的一样（「待办」里「要装」那一类的事实；不上线）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Missing {
    pub(crate) id: &'static str,
    pub(crate) name: String,
    pub(crate) what: &'static str,
    /// 缺了起不了会话。
    pub(crate) required: bool,
    pub(crate) how_url: Option<&'static str>,
}

/// 删了会丢（`truth`）还是能重建（`cache`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub(crate) enum OwnClass {
    Truth,
    Cache,
}

/// cc-monitor 在这台自己家里放的一样。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct OwnItem {
    /// 闭集同 `relay_route_core::OWN_HOME_ENTRIES`。
    pub(crate) id: Cow<'static, str>,
    /// 是什么（写好的那一句，界面照抄）。
    pub(crate) what: String,
    /// `~/.cc-monitor/…` 写法。
    pub(crate) path: String,
    pub(crate) dir: bool,
    pub(crate) class: OwnClass,
    pub(crate) exists: bool,
    /// 文件的字节数；目录 / 不在 ⇒ `null`。
    #[cfg_attr(test, ts(type = "number | null"))]
    pub(crate) size: Option<u64>,
}

/// `data-report` 的回包。也收（`Deserialize`）：本机后端替界面记的「上次值」按它核存档（`last_seen`）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct DataReport {
    /// 这台家目录（显示时 `~` 缩写按它）。
    pub(crate) home: String,
    pub(crate) changed_files: Vec<ChangedFile>,
    pub(crate) todo: Vec<super::chores::Chore>,
    /// 这台有没有 tmux（查不动 ⇒ `null`）。
    pub(crate) tmux: Option<bool>,
    /// 「待办」各类的件数。
    pub(crate) counts: super::chores::ChoreCounts,
    /// 几台排先后的键，小的在前：有进角标的件 0 · 只剩别的没做完的 1 · 没事 2。
    pub(crate) rank: u8,
    pub(crate) own: Vec<OwnItem>,
}

/// 缺了起不了会话的那几样（其余缺了只少一个功能）。
fn required(tool_id: &str) -> bool {
    matches!(tool_id, "claude-cli")
}

/// 怎么装：那个工具自己的安装说明（外链）；没有通用的 ⇒ `None`。
fn how_url(tool_id: &str) -> Option<&'static str> {
    match tool_id {
        "claude-cli" => Some("https://docs.anthropic.com/en/docs/claude-code/setup"),
        "login-shell" => Some("https://www.gnu.org/software/bash/"),
        _ => None,
    }
}

fn changed(r: &SurfaceRow) -> Option<ChangedFile> {
    if r.tier != EnvTier::AppInstalls || !matches!(r.state, SurfaceState::Present { .. }) {
        return None;
    }
    let p = r.path_declared;
    if !p.starts_with("~/") || p == OWN_HOME || p.starts_with(&format!("{OWN_HOME}/")) {
        return None;
    }
    Some(ChangedFile {
        path: Cow::Borrowed(p),
        what: r.tool_name.clone(),
        undo: undo_of(r.tool_id),
    })
}

fn missing(r: &SurfaceRow) -> Option<Missing> {
    if r.tier != EnvTier::UserInstallsWePrompt || r.state != SurfaceState::Absent {
        return None;
    }
    // tmux 一律可选：缺了不进「要装」（有没有它只走 `tmux` 那一格，没有的那台不出 tmux 相关项）。
    if r.tool_id == "tmux" {
        return None;
    }
    Some(Missing {
        id: r.tool_id,
        name: r.tool_name.clone(),
        what: r.path_declared,
        required: required(r.tool_id),
        how_url: how_url(r.tool_id),
    })
}

/// 足迹里这台确实缺的那几样（「待办」里「要装」那一类的事实）。
pub(crate) fn needs_install(report: &ConfigSurfaceReport) -> Vec<Missing> {
    report.rows.iter().filter_map(missing).collect()
}

/// 这台家里那几样（契约里那一份 `relay_route_core::OWN_HOME_ENTRIES`）各一行 `{id, path, dir, class, exists, size}`：`path` 写成 `~/…`；目录不算大小（不递归，免得大目录卡住）。
pub(crate) fn own_rows(home: &std::path::Path) -> Vec<OwnItem> {
    relay_route_core::OWN_HOME_ENTRIES
        .iter()
        .map(|&(id, rel, dir, truth)| {
            let p = home.join(rel);
            let meta = Ro::metadata(&p).ok();
            let exists = meta.as_ref().is_some_and(|m| m.is_dir() == dir);
            let size = meta.filter(|m| exists && m.is_file()).map(|m| m.len());
            OwnItem {
                id: Cow::Borrowed(id),
                what: own_what(id),
                path: format!("~/{rel}"),
                dir,
                class: if truth {
                    OwnClass::Truth
                } else {
                    OwnClass::Cache
                },
                exists,
                size,
            }
        })
        .collect()
}

/// 自家那一样是什么（界面照抄这一句，不按 id 取字）。`id` 闭集同 `relay_route_core::OWN_HOME_ENTRIES`；认不出 ⇒ 「未识别」那一句。
fn own_what(id: &str) -> String {
    use copy_core::copy_text;
    match id {
        "bin" => copy_text("rsDataPaths.backend.bin", &[]),
        "staging" => copy_text("rsDataPaths.backend.staging", &[]),
        "relayKey" => copy_text("rsDataPaths.backend.relayKey", &[]),
        "listenDir" => copy_text("rsDataPaths.backend.listenDir", &[]),
        "policy" => copy_text("rsDataPaths.backend.policy", &[]),
        "prefs" => copy_text("rsDataPaths.backend.prefs", &[]),
        "machines" => copy_text("rsDataPaths.backend.machines", &[]),
        "prefsHandoff" => copy_text("rsDataPaths.backend.prefsHandoff", &[]),
        "profiles" => copy_text("rsDataPaths.backend.profiles", &[]),
        "profilesMigrated" => copy_text("rsDataPaths.backend.profilesMigrated", &[]),
        "profilesWritten" => copy_text("rsDataPaths.backend.profilesWritten", &[]),
        "aliasesPosix" => copy_text("rsDataPaths.backend.aliasesPosix", &[]),
        "aliasesPs" => copy_text("rsDataPaths.backend.aliasesPs", &[]),
        "skillLedger" => copy_text("rsDataPaths.backend.skillLedger", &[]),
        "chores" => copy_text("rsDataPaths.backend.chores", &[]),
        "planReview" => copy_text("rsDataPaths.backend.planReview", &[]),
        "lastSeen" => copy_text("rsDataPaths.backend.lastSeen", &[]),
        "assetCatalog" => copy_text("rsDataPaths.backend.assetCatalog", &[]),
        "quota" => copy_text("rsDataPaths.backend.quota", &[]),
        "rotation" => copy_text("rsDataPaths.backend.rotation", &[]),
        "lineage" => copy_text("rsDataPaths.backend.lineage", &[]),
        "launchAccounts" => copy_text("rsDataPaths.backend.launchAccounts", &[]),
        "launchNotes" => copy_text("rsDataPaths.backend.launchNotes", &[]),
        "answerPlugin" => copy_text("rsDataPaths.backend.answerPlugin", &[]),
        "knownHosts" => copy_text("rsDataPaths.backend.knownHosts", &[]),
        "accounts" => copy_text("rsDataPaths.backend.accounts", &[]),
        "accountsMcp" => copy_text("rsDataPaths.backend.accountsMcp", &[]),
        "extBackups" => copy_text("rsDataPaths.backend.extBackups", &[]),
        "skeleton" => copy_text("rsDataPaths.backend.skeleton", &[]),
        _ => copy_text("dataPage.own.unknownItem", &[("id", id)]),
    }
}

/// 整份足迹 ＋「待办」各件 ＋ 有没有 tmux ＋ 自己家里那几样 ⇒ 这一页的成品。
pub(crate) fn shape(
    report: &ConfigSurfaceReport,
    todo: Vec<super::chores::Chore>,
    tmux: Option<bool>,
    own: Vec<OwnItem>,
) -> DataReport {
    let counts = super::chores::counts(&todo);
    DataReport {
        home: report.home.clone(),
        changed_files: report.rows.iter().filter_map(changed).collect(),
        counts,
        rank: match (counts.badge, counts.open) {
            (1.., _) => 0,
            (0, 1..) => 1,
            _ => 2,
        },
        todo,
        tmux,
        own,
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/footprint/data_tests.rs"]
mod tests;
