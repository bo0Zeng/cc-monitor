//! 「文件与数据」那一份成品（帧面 `data-report`）：同一份足迹（[`super::rows`]）按设置窗那一页要的三样重排 —— 只读。
//!
//! - `changedFiles`：cc-monitor 写进**你的**文件的那几处（今天在、cc-monitor 装口放的、不在 `~/.cc-monitor/` 里；Windows 那一台还有用户级 `PATH` 里那一段），
//!   每处说改了什么、撤回在哪一页（页 · 栏 · 锚点，界面照它跳）。
//! - `todo`：「待办」各件（[`super::chores`]）；其中「要装」那一类来自 [`needs_install`]：你自己装、cc-monitor 只查的那几样里，
//!   这台**确实缺**的（查不动的不算缺，不报；tmux 一律可选、不进这里）；`required` ＝ 缺了起不了会话。
//! - `tmux`：这台有没有 tmux（查不动 ⇒ `null`；与起新会话那一问同一个判法 `host::tmux::rows_here`）。
//! - `counts`：「待办」各类的件数（[`super::chores::counts`]）；跨台合计由本机后端 `chores-merge` 出，界面不自己数。
//! - `rank`：几台排先后的键（设置窗那一页按它排各台）。
//!
//! - `own`：cc-monitor 在这台自己家里（`~/.cc-monitor/`）放的每一样（[`own_rows`]）：在不在（不在 ⇒ 写好的那一句）· 文件多大 · 删了会丢还是能重建；
//!   本机那一问（带 monitor 的事实）后面接 monitor 数据目录里界面自己那几样与网页引擎的数据目录（[`monitor_rows`]），远端那一问不列。
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub(crate) struct UndoAt {
    pub(crate) page: Cow<'static, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub(crate) tab: Option<Cow<'static, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
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

    /// 页 · 栏（那一栏里没有单独的锚点）。
    pub(crate) const fn tab(page: &'static str, tab: &'static str) -> UndoAt {
        UndoAt {
            page: Cow::Borrowed(page),
            tab: Some(Cow::Borrowed(tab)),
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
        // 用户级 `PATH` 那一段：机器设置「配置」栏里那个开关（`ccm-user-path` 撤）。
        "ccm-user-path" => Some(UndoAt::tab("machine", "config")),
        _ => None,
    }
}

/// cc-monitor 写进用户文件的一处。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub(crate) struct ChangedFile {
    pub(crate) path: Cow<'static, str>,
    pub(crate) what: String,
    /// 撤回在哪（设置窗的页 · 栏 · 锚点）；没有 ⇒ `null`。
    pub(crate) undo: Option<UndoAt>,
    /// 这一行就地有「撤回」：cc-monitor 改过的项目级 MCP 配置，整份回到改之前的原文（帧命令 `mcp-project-revert {file: path}`）。
    /// 没有 ⇒ 线上不出这一格（足迹那几行的线上形不变）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub(crate) revert: bool,
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "lowercase")]
pub(crate) enum OwnClass {
    Truth,
    Cache,
}

/// cc-monitor 在这台放的一样（家里那几样；本机那一问另有界面自己那几样）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub(crate) struct OwnItem {
    /// 闭集同 `relay_route_core::OWN_HOME_ENTRIES` ＋ `MONITOR_DATA_ENTRIES` ＋ `webview`。
    pub(crate) id: Cow<'static, str>,
    /// 是什么（写好的那一句，界面照抄）。
    pub(crate) what: String,
    /// 在家目录底下 ⇒ `~/…` 写法；不在（数据目录挪到别处 · 网页引擎目录）⇒ 绝对路径。
    pub(crate) path: String,
    /// 显示名：`~/.cc-monitor/` 底下那一段（不在那底下 ⇒ 同 `path`），目录带尾 `/`。
    pub(crate) label: String,
    /// 落点的绝对路径（分隔符照那台家目录的写法），打开 · 悬停照它。
    pub(crate) abs: String,
    pub(crate) dir: bool,
    pub(crate) class: OwnClass,
    pub(crate) exists: bool,
    /// 不在盘上 ⇒ 写好的那一句（界面照抄）；在 ⇒ `null`。
    pub(crate) absent_said: Option<crate::common::cells::Words>,
    /// 文件的大小，写好的字（`copy_core::size_text`）；目录 / 不在 ⇒ `null`。
    pub(crate) size_text: Option<crate::common::cells::Words>,
}

/// `data-report` 的回包。也收（`Deserialize`）：本机后端替界面记的「上次值」按它核存档（`last_seen`）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct DataReport {
    pub(crate) changed_files: Vec<ChangedFile>,
    pub(crate) todo: Vec<super::chores::Chore>,
    /// 这台有没有 tmux（查不动 ⇒ `null`）。
    pub(crate) tmux: Option<bool>,
    /// 「待办」各类的件数。
    pub counts: super::chores::ChoreCounts,
    /// 几台排先后的键，小的在前：有进角标的件 0 · 只剩别的没做完的 1 · 没事 2。
    pub(crate) rank: u8,
    pub(crate) own: Vec<OwnItem>,
}

fn changed(r: &SurfaceRow) -> Option<ChangedFile> {
    if r.tier != EnvTier::AppInstalls || r.state != SurfaceState::Present {
        return None;
    }
    let p: &str = &r.path_declared;
    // 家目录底下的文件，或用户级 `PATH` 那一格（不是文件，申报写它在注册表里的位置）。
    let yours = p.starts_with("~/") || p == crate::agents::footprint_decl::USER_PATH_TOKEN;
    if !yours || p == OWN_HOME || p.starts_with(&format!("{OWN_HOME}/")) {
        return None;
    }
    Some(ChangedFile {
        path: r.path_declared.clone(),
        what: r.tool_name.clone(),
        undo: undo_of(r.tool_id),
        revert: false,
    })
}

/// 「改过的用户文件」后面接上装记录里记着「cc-monitor 改过」的那几份项目级 MCP 配置，一份一行（路径是那份的绝对路径；撤回在那一行上）。
pub(crate) fn with_touched(
    mut out: DataReport,
    touched: &std::collections::BTreeMap<String, crate::assets::skill_ledger::Touched>,
) -> DataReport {
    out.changed_files.extend(touched.iter().map(|(file, t)| {
        ChangedFile {
            path: Cow::Owned(file.clone()),
            what: copy_core::copy_text(
                "dataPage.changed.projectMcp",
                &[(
                    "names",
                    &t.names
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(&copy_core::copy_text("kit.text.sep", &[])),
                )],
            ),
            undo: None,
            revert: true,
        }
    }));
    out
}

fn missing(registry: &[crate::agents::Adapter], r: &SurfaceRow) -> Option<Missing> {
    if r.tier != EnvTier::UserInstallsWePrompt || r.state != SurfaceState::Absent {
        return None;
    }
    let decl = super::registry::env_decl(registry, r.tool_id);
    // 缺了劝不劝装照那一条的申报（`UnmanagedEnv::asks_install`）。
    if decl.is_some_and(|u| !u.asks_install) {
        return None;
    }
    // 要装的只是申报表里写死的那几样（命令名）。现算的路径只出在各号那几行（`rows::account_rows`），档是 `AppInstalls`，
    // 上面那道档已挡掉；这里取出那一形的 `&'static str`。
    let Cow::Borrowed(what) = r.path_declared else {
        return None;
    };
    Some(Missing {
        id: r.tool_id,
        name: r.tool_name.clone(),
        what,
        // 必装 · 怎么装照环境清单里那一条申报的（[`super::registry::env_decl`]）。
        required: decl.is_some_and(|u| u.required),
        how_url: decl.and_then(|u| u.how),
    })
}

/// 足迹里这台确实缺的那几样（「待办」里「要装」那一类的事实）。各项的申报照 `registry`（与建这份足迹的是同一份）。
pub(crate) fn needs_install(
    report: &ConfigSurfaceReport,
    registry: &[crate::agents::Adapter],
) -> Vec<Missing> {
    report
        .rows
        .iter()
        .filter_map(|r| missing(registry, r))
        .collect()
}

/// 一样东西 stat 一次成一行：`at` 是它在盘上的落点，`path` 是显示的那一形；目录不算大小（不递归，免得大目录卡住）。
fn probed(id: &'static str, at: &std::path::Path, path: String, dir: bool, truth: bool) -> OwnItem {
    let meta = Ro::metadata(at).ok();
    let exists = meta.as_ref().is_some_and(|m| m.is_dir() == dir);
    let size_text = meta
        .filter(|m| exists && m.is_file())
        .map(|m| crate::common::cells::Words(copy_core::size_text(m.len())));
    let tail = path
        .strip_prefix(OWN_HOME)
        .map_or(path.as_str(), |t| t.trim_start_matches('/'));
    let label = if dir {
        format!("{tail}/")
    } else {
        tail.to_string()
    };
    OwnItem {
        id: Cow::Borrowed(id),
        what: own_what(id),
        label,
        abs: at.display().to_string(),
        path,
        dir,
        class: if truth {
            OwnClass::Truth
        } else {
            OwnClass::Cache
        },
        exists,
        absent_said: (!exists).then(|| {
            crate::common::cells::Words(copy_core::copy_text("data.item.notCreated", &[]))
        }),
        size_text,
    }
}

/// 这台家里那几样（契约里那一份 `relay_route_core::OWN_HOME_ENTRIES`）各一行：`path` 写成 `~/…`。
pub(crate) fn own_rows(home: &std::path::Path) -> Vec<OwnItem> {
    relay_route_core::OWN_HOME_ENTRIES
        .iter()
        .map(|&(id, rel, dir, truth)| {
            let p = crate::platform::paths::home_join(home, rel);
            probed(id, &p, format!("~/{rel}"), dir, truth)
        })
        .collect()
}

/// 绝对路径的显示形：在家目录底下 ⇒ `~/…`（分隔符写 `/`），不在 ⇒ 原样。
fn shown(home: &std::path::Path, p: &std::path::Path) -> String {
    match p.strip_prefix(home) {
        Ok(rest) => format!("~/{}", rest.to_string_lossy().replace('\\', "/")),
        Err(_) => p.display().to_string(),
    }
}

/// monitor 数据目录里界面自己那几样（契约里那一份 `relay_route_core::MONITOR_DATA_ENTRIES`）＋ 网页引擎的数据目录（monitor 交的落点，没交 ⇒ 不列）各一行。
pub(crate) fn monitor_rows(
    home: &std::path::Path,
    data_dir: &std::path::Path,
    webview: Option<&std::path::Path>,
) -> Vec<OwnItem> {
    relay_route_core::MONITOR_DATA_ENTRIES
        .iter()
        .map(|&(id, rel, dir, truth)| {
            let p = crate::platform::paths::home_join(data_dir, rel);
            probed(id, &p, shown(home, &p), dir, truth)
        })
        // 里面有界面的本地存储（偏好）⇒ 删了会丢。
        .chain(webview.map(|w| probed("webview", w, shown(home, w), true, true)))
        .collect()
}

/// 「cc-monitor 的文件」那几行：自家那几样；那一问带了 monitor 的事实（本机那一问）⇒ 后面接界面自己那几样。
/// 数据目录推不出 ⇒ 界面那几样不列（不猜一个）；monitor 交的网页引擎落点不是绝对路径 ⇒ 那一行不列。
pub(crate) fn rows_for(
    home: &std::path::Path,
    data_dir: Option<&std::path::Path>,
    client: Option<&super::FootprintClient>,
) -> Vec<OwnItem> {
    let mut rows = own_rows(home);
    if let (Some(c), Some(d)) = (client, data_dir) {
        let web = c
            .webview
            .as_deref()
            .map(std::path::Path::new)
            .filter(|p| p.is_absolute());
        rows.extend(monitor_rows(home, d, web));
    }
    rows
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
        "profiles" => copy_text("rsDataPaths.backend.profiles", &[]),
        "profilesWritten" => copy_text("rsDataPaths.backend.profilesWritten", &[]),
        "aliasesPosix" => copy_text("rsDataPaths.backend.aliasesPosix", &[]),
        "aliasesPs" => copy_text("rsDataPaths.backend.aliasesPs", &[]),
        "skillLedger" => copy_text("rsDataPaths.backend.skillLedger", &[]),
        "chores" => copy_text("rsDataPaths.backend.chores", &[]),
        "planReview" => copy_text("rsDataPaths.backend.planReview", &[]),
        "lastSeen" => copy_text("rsDataPaths.backend.lastSeen", &[]),
        "bookmarks" => copy_text("rsDataPaths.backend.bookmarks", &[]),
        "assetCatalog" => copy_text("rsDataPaths.backend.assetCatalog", &[]),
        "quota" => copy_text("rsDataPaths.backend.quota", &[]),
        "rotation" => copy_text("rsDataPaths.backend.rotation", &[]),
        "lineage" => copy_text("rsDataPaths.backend.lineage", &[]),
        "sessionQueue" => copy_text("rsDataPaths.backend.sessionQueue", &[]),
        "ccBus" => copy_text("rsDataPaths.backend.ccBus", &[]),
        "launchAccounts" => copy_text("rsDataPaths.backend.launchAccounts", &[]),
        "historyMetadata" => copy_text("rsDataPaths.backend.historyMetadata", &[]),
        "apikey" => copy_text("rsDataPaths.backend.apikey", &[]),
        "launchNotes" => copy_text("rsDataPaths.backend.launchNotes", &[]),
        "answerPlugin" => copy_text("rsDataPaths.backend.answerPlugin", &[]),
        "knownHosts" => copy_text("rsDataPaths.backend.knownHosts", &[]),
        "sshConfig" => copy_text("rsDataPaths.backend.sshConfig", &[]),
        "accounts" => copy_text("rsDataPaths.backend.accounts", &[]),
        "accountsMcp" => copy_text("rsDataPaths.backend.accountsMcp", &[]),
        "extBackups" => copy_text("rsDataPaths.backend.extBackups", &[]),
        "skeleton" => copy_text("rsDataPaths.backend.skeleton", &[]),
        "listingCache" => copy_text("rsDataPaths.backend.listingCache", &[]),
        "remoteListing" => copy_text("rsDataPaths.backend.remoteListing", &[]),
        "setGroups" => copy_text("rsDataPaths.backend.setGroups", &[]),
        "groupsOwed" => copy_text("rsDataPaths.backend.groupsOwed", &[]),
        "strategies" => copy_text("rsDataPaths.backend.strategies", &[]),
        "hosts" => copy_text("rsDataPaths.backend.hosts", &[]),
        // 界面自己那几样（`relay_route_core::MONITOR_DATA_ENTRIES`）＋ 网页引擎那一行。
        "config" => copy_text("rsDataPaths.monitor.config", &[]),
        "autoLaunch" => copy_text("rsDataPaths.monitor.autoLaunch", &[]),
        "filewinView" => copy_text("rsDataPaths.monitor.filewinView", &[]),
        "psAwait" => copy_text("rsDataPaths.monitor.integrationSignal", &[]),
        "psRegistry" => copy_text("rsDataPaths.monitor.psRegistry", &[]),
        "logs" => copy_text("rsDataPaths.monitor.logs", &[]),
        "backendLogs" => copy_text("rsDataPaths.monitor.backendLogs", &[]),
        "webview" => copy_text("rsDataPaths.webview.dataDir", &[]),
        _ => copy_text("dataPage.own.unknownItem", &[("id", id)]),
    }
}

/// 整份足迹 ＋「待办」各件 ＋ 有没有 tmux ＋ 自己家里那几样 ⇒ 这一页的成品（cc-monitor 改过的项目级 MCP 配置由 [`with_touched`] 接在后面）。
pub(crate) fn shape(
    report: &ConfigSurfaceReport,
    todo: Vec<super::chores::Chore>,
    tmux: Option<bool>,
    own: Vec<OwnItem>,
) -> DataReport {
    let counts = super::chores::counts(&todo);
    DataReport {
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
