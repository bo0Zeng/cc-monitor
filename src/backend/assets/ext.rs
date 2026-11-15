//! 设置里「扩展」那一页的后端：skill 与 MCP 跨机器的一张表（`ext-list`）· 装到一台的确认卡（枢纽 `ext-hub-*`，住 [`super::hub`]）·
//! 从一台卸掉（`ext-uninstall-*`，在被卸的那一台上判、写）· 条目的备注（`ext-note-set`）。
//!
//! # 判定只在这里
//!
//! 界面只画：每格的点（`same` · `differs` · `missing` · `project`）、那台上的各处（全局一行 ＋ 每个装着它的项目一行，各自的态与「卸载」）、
//! 机器那一行的「装到…」（从哪台拿哪一版 · 建议装到哪 · 哪几处能选、不能选的为什么），全由 [`table`] 答；摘要不出后端（线上一个 `digest` 都没有）。
//!
//! - `same` / `differs` 只拿**用户级**那几份比：持有人最多的那一版算「这一版」（打平时本机那一份优先），和它一样 ⇒ `same`。
//!   cc-monitor 认得的（[`KNOWN`]，表顶一组）：嵌在后端里的（cc-bus）「这一版」= 本机后端二进制里那一份，装它用被写那台自己二进制里那一份；
//!   自己发版的（planned-build · code-picture）「这一版」= 各台用户级里版本最新的那一份，装它从持有那一份的那台拿。没装的那一格说「推荐 · 未装」。
//! - 用户级没有、只在项目里有 ⇒ `project`；哪儿都没有 ⇒ `missing`。
//! - 哪一处能写只问 [`writable`] / [`target_refused`]：用户级 MCP 住 agent 自己的热状态文件（它自己一直在重写）⇒ 只读；自带的只装全局。
//! - 「新见到」：这台目录第一次见到这个条目的时刻晚于上一次来看这一页（`asset_catalog::Visits`）。
//! - 备注：内置的（自带的扩展）＋ 用户写的（记在本机目录自己那一格，随目录同步；生效的是各台里最新的那一条，`asset_catalog::note_of`）。
//!
//! # 卸
//!
//! 装记录里有 ⇒ 只撤装时写进去的（skill：`skill_install` 那一份逐文件判；MCP：那一条的摘要对得上才不问）；
//! 没有 ⇒ 不是 cc-monitor 装的：skill 先挪进 `~/.cc-monitor/backups/` 再删；项目级 MCP 那一条删之前的原文记进装记录
//! （`skill_ledger` 的 `touched`，「改过的用户文件」那一行撤回），不另抄；预览里明说。

use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::asset_catalog::{self, Asset, Catalog, ExtKind};
use super::skill_ledger::{Record, RecordReq};
use crate::common::door::{self, Door, Refused};
use crate::common::said::Said;
use crate::common::time::now_ms;

// ───────────────────────── 线上形状（界面那一份由这里生成） ─────────────────────────

/// 一个扩展住哪一级。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "level", rename_all = "lowercase")]
pub enum ExtLoc {
    User,
    Project { dir: String },
}

/// 一格的态（闭集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "lowercase")]
pub enum ExtState {
    /// 用户级有，且是「这一版」。
    Same,
    /// 用户级有，但不是「这一版」。
    Differs,
    /// 哪儿都没有。
    Missing,
    /// 用户级没有（或关着），只在项目里有 / 开着。
    Project,
    /// 装了、关着（只有插件有这一态）。
    Off,
}

/// 装到一台时两头各在哪一级。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ExtScope {
    pub from: ExtLoc,
    pub to: ExtLoc,
}

/// 「装到哪」的一个选项。`ok = false` 的照样列出来（显示但不可选），`note` 说为什么。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ExtTarget {
    pub at: ExtLoc,
    pub ok: bool,
    pub note: Option<String>,
}

/// 机器那一行的「装到…」：从哪台拿哪一版（`from` = 来源那台的键，`null` = 本机后端自己，同枢纽的 `from` / `to`）·
/// 建议的落点（与来源同级）· 可选的各处。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct ExtBring {
    pub from: Option<String>,
    pub scope: ExtScope,
    pub targets: Vec<ExtTarget>,
}

/// 抽屉里那台机器的一处（全局一行 ＋ 每个装着它的项目一行）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ExtPlace {
    pub at: ExtLoc,
    /// 全局那一行：`same` · `differs` · `missing`；项目那几行：`same` · `differs`（与「这一版」比）。
    pub state: ExtState,
    /// 这一处那一句（后端写好）。
    pub said: String,
    /// skill：那一处的目录（「在文件窗口里打开」用）。
    pub dir: Option<String>,
    /// 这一处有「卸载」。
    pub uninstall: bool,
    /// 有它、却没有「卸载」时为什么。
    pub note: Option<String>,
    /// 这一处自己的一句（插件：版本 · 开着 / 关着 · 出处）；别的种类没有。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub line: Option<String>,
}

/// 表里一台机器（一列）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ExtMachine {
    /// 枢纽认它的键：本机后端自己 = `null`；别的台 = 可达表里的名字；没连上 = `null` 且 `reachable = false`。
    pub key: Option<String>,
    /// 就是本机后端这一台。
    pub here: bool,
    pub reachable: bool,
    /// 给人看的名字（本机 ⇒ `copy_core::machine_word`；别的台 ⇒ 可达表里的名字，没有就是那台自报的称呼）。
    pub name: String,
    /// 那台的 MCP 列表（那台 `mcp-read` 的成品原样：本机后端问到再交，`faces/ext_list_face.rs::with_mcp`）；
    /// 表里没有 MCP 行 · 那台没连上 · 问不到 ⇒ 缺（界面那台的 MCP 格不画小标）。这一层只转交，只读每条的家 · 名字 · 不一致 · 那一句
    /// 来拆同名不一致的行（[`split_mismatch`]）；形状是 `mcp-read` 的回包（金样 `tests/__fixtures__/mcp-read.golden.json` 钉），界面按那个生成类型收（`ext-reads.ts::mcpOf`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "unknown"))]
    pub mcp: Option<serde_json::Value>,
}

/// 一格（一台机器）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ExtCell {
    /// 表上那个点。
    pub state: ExtState,
    /// 那个点那一句（悬停 · 抽屉里那台那一行；后端写好）。
    pub said: String,
    /// 那台上的各处：全局一行在前（没有也列），再是每个装着它的项目。
    pub places: Vec<ExtPlace>,
    pub bring: Option<ExtBring>,
    /// 没有「装到…」时为什么（现状 ＋ 能做什么）。
    pub note: Option<String>,
}

/// 抽屉里「看内容」那几行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ExtDetail {
    pub label: String,
    pub value: String,
}

/// cc-monitor 认得的那一样（表顶那一组，[`KNOWN`]）：从哪来 ＋ 内置备注 ＋ 要不要列各台的钩子状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ExtKnown {
    /// 名字底下那一句：从哪来（cc-monitor 自带 · 发布处）。
    pub from: String,
    /// 内置备注（有才有）。
    pub note: Option<String>,
    /// 装上之后要在那台的 agent 配置里加钩子：抽屉里每台一行钩子状态（问那台的 `hooks-diag`）。
    pub hooks: bool,
}

/// 表里一行（一个条目）。`cells` 与 [`ExtList::machines`] 同序。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct ExtRow {
    /// 哪一家的（线上的 kind）；默认那一家 ⇒ 缺（同只有一家时那一形）。卸 · 装那一跳照它问回那一家。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent: Option<cmd_wire::AgentKind>,
    /// 那一家对人的叫法（后端写好）：表里不止一家时每行都给，只一家 ⇒ 缺（线上形同只有一家时）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent_said: Option<String>,
    pub kind: ExtKind,
    /// 种类那一格的字（后端写好）。
    pub kind_said: String,
    pub name: String,
    pub about: Option<String>,
    pub detail: Vec<ExtDetail>,
    pub new: bool,
    /// cc-monitor 认得的那几样才有（表顶一组，排在最前）。
    pub known: Option<ExtKnown>,
    /// 用户写的备注（随目录在后端之间同步）。
    pub note: Option<String>,
    pub cells: Vec<ExtCell>,
    /// MCP 同名、各号配置不一致、拆成几行的：这是拆出的第几行（从 0 数）；没拆 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub variant: Option<u32>,
    /// 拆出的那一行在各台挑哪一条（与 [`ExtList::machines`] 同序：那台 `mcp` 里 `entries` 的下标；那台没有这一份 ⇒ 空）；没拆 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub picks: Option<Vec<Option<u32>>>,
    /// 拆成的那一行名字后面那一句（说不一致的那一条写好的 `held.said`）；没拆 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub held_said: Option<String>,
    /// 插件那一行从哪来（同一光名字几个来源各是一行；条目上有那一家的写法就有，经不经插件命令管另由各格判）；
    /// 别的种类 · 自己发版的 · 没写法的（旧对端）⇒ 缺。
    /// 卸 · 装那一跳把 `key` 原样交回，后端照它找那一条。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub source: Option<ExtSource>,
}

/// 插件那一行的来源：那一家自己的写法（目录条目的 `summary.key`，装 · 删交它）＋ 给人看的那一句（照条目的市场写好）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ExtSource {
    pub key: String,
    pub said: String,
}

/// `ext-list` 的成品。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct ExtList {
    pub machines: Vec<ExtMachine>,
    pub rows: Vec<ExtRow>,
    pub problems: Vec<String>,
    /// 表下图例：列哪几态 ＋ 各一句（「关着」只在有一台装着插件时列）。
    pub legend: Vec<ExtLegend>,
    /// 表顶「认得」那一组的组头（带 `known` 的那几行排在最前，界面在它们前面放这一句）。
    #[serde(default)]
    pub known_title: String,
}

/// 图例里一项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ExtLegend {
    pub state: ExtState,
    pub said: String,
}

/// 一个点 / 一处那一句：「只在项目里」带上那几个项目。
fn state_said(state: ExtState, places: &[ExtPlace]) -> String {
    match state {
        ExtState::Same => copy_text("beExt.legend.same", &[]),
        ExtState::Differs => copy_text("beExt.state.differs", &[]),
        ExtState::Missing => copy_text("reason.spawn.notInstalled", &[]),
        ExtState::Off => copy_text("beExt.state.off", &[]),
        ExtState::Project => {
            let dirs: Vec<&str> = places.iter().filter_map(|p| p.at.project()).collect();
            copy_text(
                "beExt.state.project",
                &[("dirs", &dirs.join(&copy_text("kit.text.sep", &[])))],
            )
        }
    }
}

/// 种类那一格的字。
fn kind_said(kind: ExtKind) -> String {
    match kind {
        ExtKind::Skill => copy_text("mobPlus.row.skills", &[]),
        ExtKind::Mcp => copy_text("extPage.filter.mcp", &[]),
        ExtKind::Plugin => copy_text("beExt.kind.plugin", &[]),
    }
}

/// 表下图例：四态，有一台装着插件再加「关着」（认得的 planned-build 那一行哪台都没装也列，不按「有插件行」判）。
fn legend_of(rows: &[ExtRow]) -> Vec<ExtLegend> {
    let mut states = vec![
        ExtState::Same,
        ExtState::Differs,
        ExtState::Missing,
        ExtState::Project,
    ];
    let installed = |r: &ExtRow| r.cells.iter().any(|c| c.state != ExtState::Missing);
    if rows
        .iter()
        .any(|r| r.kind == ExtKind::Plugin && installed(r))
    {
        states.push(ExtState::Off);
    }
    states
        .into_iter()
        .map(|state| ExtLegend {
            state,
            said: match state {
                ExtState::Same => copy_text("beExt.legend.same", &[]),
                ExtState::Differs => copy_text("beExt.legend.differs", &[]),
                ExtState::Project => copy_text("beExt.legend.project", &[]),
                ExtState::Off => copy_text("beExt.state.off", &[]),
                ExtState::Missing => copy_text("reason.spawn.notInstalled", &[]),
            },
        })
        .collect()
}

/// 确认卡上一格待填（或沿用目标机已有的值）的值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ExtSlot {
    /// `env` 或 `headers`。
    pub field: String,
    pub key: String,
    /// 目标机那一条里已有这个键的值（可以沿用；不填就沿用）。
    pub kept: bool,
}

/// 看过的那一份的记号：应用时原样交回，枢纽拿它判「看过之后变了没有」（界面不解读）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ExtTokens {
    pub source: String,
    pub target: Option<String>,
}

/// `ext-hub-preview` 的成品：确认卡。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ExtCard {
    pub kind: ExtKind,
    pub name: String,
    /// 被写那台上的落点（skill 目录 / MCP 配置文件）。
    pub path: String,
    /// 要写的那几个（skill：目录里的相对路径；MCP：那份配置文件）。
    pub writes: Vec<String>,
    /// 装上之后和现在一样（没什么可写）。
    pub unchanged: bool,
    /// 要留意的几件（说人话）。
    pub suspects: Vec<String>,
    /// 装不了的原因（有它就不给确认）。
    pub stop: Option<String>,
    /// MCP：装上之后那一条（待填的值是 `null`）。
    pub config: Option<String>,
    pub slots: Vec<ExtSlot>,
    pub tokens: ExtTokens,
}

/// `ext-uninstall-preview` 的成品：卸之前那张卡。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ExtUninstallCard {
    pub kind: ExtKind,
    pub name: String,
    /// 卸的是哪一家的（照请求；默认那一家 ⇒ 缺）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent: Option<cmd_wire::AgentKind>,
    pub path: String,
    /// cc-monitor 装的（装记录里有）。
    pub recorded: bool,
    /// 要删的那几个（skill：相对路径；MCP：那份配置文件里的那一条；插件：那个插件的名字）。
    pub files: Vec<String>,
    /// 删之前先放到哪（不是 cc-monitor 装的才有）。
    pub backup: Option<String>,
    /// 这一趟会做什么（说人话：装的 / 不是装的 · 改过没有 · 删了回不回得去）。
    pub said: String,
    pub token: String,
}

/// 应用 / 卸完之后的成品。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ExtDone {
    pub path: String,
    /// 写了 / 删了的那几个。
    pub changed: Vec<String>,
    /// 做成了，但有一件要知道的（执行位没改成 · 没记下来 · 空目录没收掉 …）。
    pub note: Option<String>,
}

impl ExtLoc {
    /// 已解出来的位置按**这台**的路径写法再判一次（项目目录须是这台上的绝对路径）：内层命令收到 typed 入参之后用。
    pub(crate) fn here(&self) -> Result<ExtLoc, Said> {
        match self {
            ExtLoc::User => Ok(ExtLoc::User),
            ExtLoc::Project { dir } => Ok(ExtLoc::Project {
                dir: super::mcp_edit::project_root_as(dir, super::mcp_edit::PathForm::HERE)?,
            }),
        }
    }

    /// 只认形状（`{level: user}` · `{level: project, dir}` 且 dir 非空）：枢纽用，别台的路径由那台自己判。
    pub(crate) fn shape_of(v: Option<&Value>, key: &str) -> Result<ExtLoc, Said> {
        Self::judged(v, key, None)
    }

    /// 两者的本体：`form` 有 ⇒ 按那一形判项目目录是不是绝对路径；没有 ⇒ 只认形状。
    pub(crate) fn judged(
        v: Option<&Value>,
        key: &str,
        form: Option<super::mcp_edit::PathForm>,
    ) -> Result<ExtLoc, Said> {
        let v = v.ok_or_else(|| {
            crate::wire::spec::fail(
                "bad_args",
                crate::common::contract::malformed(&format!("missing `{key}`")),
            )
        })?;
        let loc: ExtLoc = serde_json::from_value(v.clone()).map_err(|e| {
            let raw = e.to_string();
            tracing::warn!("contract: bad_args deserialization failed: {raw}");
            let detail = crate::wire::typed::sanitize_de_error::<ExtLoc>(v, &raw);
            Said::new(
                "bad_args",
                crate::common::contract::malformed(&format!(
                    "`{key}` must be {{level: user}} or {{level: project, dir}}: {detail}"
                )),
            )
        })?;
        match (loc, form) {
            (ExtLoc::User, _) => Ok(ExtLoc::User),
            (ExtLoc::Project { dir }, Some(form)) => Ok(ExtLoc::Project {
                dir: super::mcp_edit::project_root_as(&dir, form)?,
            }),
            (ExtLoc::Project { dir }, None) if dir.trim().is_empty() => Err(
                crate::wire::spec::fail("bad_args", copy_text("beMcpEdit.path.emptyDir", &[])),
            ),
            (ExtLoc::Project { dir }, None) => Ok(ExtLoc::Project {
                dir: dir.trim().to_string(),
            }),
        }
    }

    pub(crate) fn project(&self) -> Option<&str> {
        match self {
            ExtLoc::User => None,
            ExtLoc::Project { dir } => Some(dir),
        }
    }

    fn of(asset: &Asset) -> ExtLoc {
        match &asset.project {
            None => ExtLoc::User,
            Some(d) => ExtLoc::Project { dir: d.clone() },
        }
    }
}

/// 入参里那一格 `kind` ⇒ 条目种类（不认 ⇒ `bad_args`）。
pub(crate) fn kind_from_arg(args: &Value) -> Result<ExtKind, Said> {
    serde_json::from_value(args.get("kind").cloned().unwrap_or(Value::Null)).map_err(|_| {
        crate::wire::spec::fail(
            "bad_args",
            crate::common::contract::malformed("`kind` must be skill, mcp or plugin"),
        )
    })
}

// ───────────────────────── 表（纯） ─────────────────────────

/// 一台机器在表里的样子（内部：带目录里的 id）。
struct Column<'a> {
    id: &'a str,
    m: ExtMachine,
    /// 那台上开过会话的项目目录（「装到…」的项目落点；不上线，界面只看落点）。
    projects: &'a [String],
    /// 那台有账号库（用户级 MCP 是各账号共用的那一份）。
    shared: bool,
}

fn columns<'a>(cat: &'a Catalog, reach: &[(String, Option<String>)]) -> Vec<Column<'a>> {
    let mut cols: Vec<Column> = cat
        .machines
        .iter()
        .map(|(id, snap)| {
            let here = *id == cat.self_id;
            let key = if here {
                None
            } else {
                reach
                    .iter()
                    .find(|(_, peer)| peer.as_deref() == Some(id.as_str()))
                    .map(|(o, _)| o.clone())
            };
            Column {
                id: id.as_str(),
                m: ExtMachine {
                    reachable: here || key.is_some(),
                    name: if here {
                        copy_core::machine_word(&cmd_wire::Machine::local())
                    } else {
                        key.clone().unwrap_or_else(|| snap.label.clone())
                    },
                    key,
                    here,
                    mcp: None,
                },
                projects: &snap.project_dirs,
                shared: snap.shared_mcp,
            }
        })
        .collect();
    cols.sort_by(|a, b| (!a.m.here, &a.m.name, a.id).cmp(&(!b.m.here, &b.m.name, b.id)));
    cols
}

/// 「这一版」：用户级那几份里持有人最多的摘要；打平时本机那一份优先，再按列序。
fn canonical<'a>(held: &[Option<&'a Asset>], cols: &[Column]) -> Option<&'a str> {
    let mut count: BTreeMap<&str, usize> = BTreeMap::new();
    for a in held.iter().flatten() {
        *count.entry(a.digest.as_str()).or_default() += 1;
    }
    let best = *count.values().max()?;
    let tied: Vec<&str> = count
        .iter()
        .filter(|(_, n)| **n == best)
        .map(|(d, _)| *d)
        .collect();
    cols.iter()
        .zip(held)
        .filter(|(c, _)| c.m.here)
        .chain(cols.iter().zip(held))
        .find_map(|(_, a)| a.map(|a| a.digest.as_str()).filter(|d| tied.contains(d)))
}

/// 装到项目时替那台先挑一个：和来源同一个目录（那台也有这个项目）⇒ 它；否则那台的第一个；那台没有项目 ⇒ `None`。
fn pick_project(projects: &[String], like: Option<&str>) -> Option<String> {
    like.filter(|d| projects.iter().any(|p| p == d))
        .map(str::to_string)
        .or_else(|| projects.first().cloned())
}

fn about_of(a: &Asset) -> Option<String> {
    let s = &a.summary;
    if matches!(a.kind, ExtKind::Skill | ExtKind::Plugin) {
        return s
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    if let Some(cmd) = s.get("command").and_then(Value::as_str) {
        let args: Vec<&str> = s
            .get("args")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        return Some(
            std::iter::once(cmd)
                .chain(args)
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    s.get("url").and_then(Value::as_str).map(str::to_string)
}

/// 插件组成那几样（`summary.parts` 的键），按抽屉里的次序。
const PLUGIN_PARTS: &[&str] = &["skills", "commands", "agents", "hooks", "mcp"];

/// 组成那一样在抽屉里的标签。
fn part_label(key: &str) -> String {
    match key {
        "skills" => copy_text("beExt.part.skills", &[]),
        "commands" => copy_text("detail.label.command", &[]),
        "agents" => copy_text("beProfile.slot.agent", &[]),
        "hooks" => copy_text("beExt.part.hooks", &[]),
        _ => copy_text("extPage.filter.mcp", &[]),
    }
}

/// 插件：市场 · 版本 · 组成（每样一行：几个 ＋ 名字）。
fn plugin_detail(s: &Value) -> Vec<ExtDetail> {
    let line = |label: String, value: String| ExtDetail { label, value };
    let mut out = Vec::new();
    if let Some(m) = s.get("market").and_then(Value::as_str) {
        out.push(line(copy_text("beExt.detail.market", &[]), m.to_string()));
    }
    if let Some(v) = s.get("version").and_then(Value::as_str) {
        out.push(line(copy_text("beExt.detail.version", &[]), v.to_string()));
    }
    for key in PLUGIN_PARTS {
        let names: Vec<&str> = s["parts"][*key]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if names.is_empty() {
            continue;
        }
        out.push(line(
            part_label(key),
            copy_text(
                "beExt.detail.partValue",
                &[
                    ("n", &names.len().to_string()),
                    ("names", &names.join(&copy_text("kit.text.sep", &[]))),
                ],
            ),
        ));
    }
    out
}

/// 插件那一处自己的一句：版本 · 开着 / 关着 · 出处（`summary.from` 的码在这里成字）。不是插件 ⇒ `None`。
fn plugin_line(a: &Asset) -> Option<String> {
    if a.kind != ExtKind::Plugin {
        return None;
    }
    let s = &a.summary;
    let on = if a.off {
        copy_text("machinePage.state.disabled", &[])
    } else {
        copy_text("beExt.plugin.on", &[])
    };
    let from = match s.get("from").and_then(Value::as_str) {
        Some("managed") => copy_text("beExt.plugin.fromManaged", &[]),
        Some("local") => copy_text("beExt.plugin.fromLocal", &[]),
        Some("project") => copy_text("beExt.plugin.fromProject", &[]),
        Some("user") => copy_text("beExt.plugin.fromUser", &[]),
        _ => copy_text("beExt.plugin.fromManifest", &[]),
    };
    Some(match s.get("version").and_then(Value::as_str) {
        Some(v) => copy_text(
            "beExt.plugin.line",
            &[("version", v), ("on", &on), ("from", &from)],
        ),
        None => copy_text(
            "beExt.plugin.lineNoVersion",
            &[("on", &on), ("from", &from)],
        ),
    })
}

fn detail_of(a: &Asset) -> Vec<ExtDetail> {
    let s = &a.summary;
    let line = |label: String, value: String| ExtDetail { label, value };
    if a.kind == ExtKind::Skill {
        let n = |k: &str| s.get(k).and_then(Value::as_u64).unwrap_or(0).to_string();
        let mut out = vec![line(
            copy_text("mobDrawer.item.files", &[]),
            copy_text(
                "beExt.detail.filesValue",
                &[("files", &n("files")), ("bytes", &n("bytes"))],
            ),
        )];
        if s.get("binary").and_then(Value::as_bool) == Some(true) {
            out.push(line(
                copy_text("beExt.detail.binary", &[]),
                copy_text("beExt.detail.binaryValue", &[]),
            ));
        }
        return out;
    }
    if a.kind == ExtKind::Plugin {
        return plugin_detail(s);
    }
    // MCP：目录里那一份本来就不带密钥值（`asset_catalog::mcp_summary`），字段名是用户自己写在配置里的那几个。
    let mut out = Vec::new();
    if let Some(o) = s.as_object() {
        for (k, v) in o {
            let value = match v {
                Value::String(t) => t.clone(),
                Value::Array(items) => items
                    .iter()
                    .map(|x| {
                        x.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| x.to_string())
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
                other => other.to_string(),
            };
            out.push(line(k.clone(), value));
        }
    }
    out
}

// ───────────────────────── 哪一处能写（全仓只此一处判） ─────────────────────────

/// 这一级能不能往里写 / 从里删（`agent` 是规整后的家，默认那一家 ⇒ 缺）。用户级 MCP：别的家 ⇒ 不经这里写
/// （各账号共用的那一份是默认那一家的，别的家那一格读的是它自己那一份）；默认那一家、那台有账号库（`shared` = 各账号共用一份）⇒
/// 写进共用的那一份、同步到所有号；没有 ⇒ 它住 agent 自己的热状态文件（agent 自己一直在重写）⇒ 只读。
pub(crate) fn writable(
    kind: ExtKind,
    at: &ExtLoc,
    shared: bool,
    agent: Option<&cmd_wire::AgentKind>,
) -> Result<(), String> {
    writable_among(crate::agents::REGISTRY, kind, at, shared, agent)
}

/// [`writable`] 的注册表由调用方交那一半（枢纽 · 卸载照它们手上那一张判）。
pub(crate) fn writable_among(
    registry: &[crate::agents::Adapter],
    kind: ExtKind,
    at: &ExtLoc,
    shared: bool,
    agent: Option<&cmd_wire::AgentKind>,
) -> Result<(), String> {
    if let Some(why) = read_only_among(registry, kind, agent)
        .or_else(|| family_refused(kind, agent, at))
        .or_else(|| plugin_user_only(kind, at))
    {
        return Err(why);
    }
    match (kind, at) {
        (ExtKind::Mcp, ExtLoc::User) if !shared => {
            Err(copy_text("beExt.note.userMcpReadOnly", &[]))
        }
        _ => Ok(()),
    }
}

/// 整种只读（哪一级都不经 cc-monitor 写）⇒ 为什么：插件只在那一家没声明插件那一格（`AssetFace::plugin_cells`）时只读。
/// 注册表由调用方交。
pub(crate) fn read_only_among(
    registry: &[crate::agents::Adapter],
    kind: ExtKind,
    agent: Option<&cmd_wire::AgentKind>,
) -> Option<String> {
    (kind == ExtKind::Plugin && !super::plugin_exec::declared(registry, agent))
        .then(|| plugin_read_only_said(registry, agent))
}

/// 那一家插件只读那一句（带那一家对人的叫法）。
pub(crate) fn plugin_read_only_said(
    registry: &[crate::agents::Adapter],
    agent: Option<&cmd_wire::AgentKind>,
) -> String {
    let kind = super::skill_install::family_among(registry, agent.map(|a| a.as_str()))
        .ok()
        .flatten()
        .or(agent.map(|a| a.as_str()))
        .unwrap_or_default();
    copy_text(
        "beExt.note.pluginReadOnly",
        &[("family", &crate::agents::family_said_among(registry, kind))],
    )
}

/// 用户级 MCP 配置文件在这台上的（根, 相对段）：各账号共用的那一份（家照 `agent`，规整后不是默认那一家 · 没有账号库 ⇒ 拒）。
/// 项目级的那一份照那一家取（[`super::mcp_edit::project_mcp_of`]）。
pub(crate) fn user_mcp_spot(
    d: &dyn Door,
    reg: &[crate::agents::Adapter],
    agent: Option<&str>,
) -> Result<(String, String), Said> {
    let family =
        super::skill_install::family_of(reg, agent.map(cmd_wire::AgentKind::from).as_ref())?;
    let store = shared_mcp_file(d, family.as_ref())?;
    let (Some(dir), Some(file)) = (store.parent(), store.file_name()) else {
        return Err(crate::wire::spec::fail(
            "io_failed",
            copy_text("beExt.uninstall.noHome", &[]),
        ));
    };
    Ok((path_text(dir)?, path_text(Path::new(file))?))
}

/// 这台的用户级 MCP 落在哪（被写的那一台自己判）：各账号共用的那一份（默认那一家 · 有账号库）；别的家 · 没有 ⇒ 按 [`writable`] 拒。
pub(crate) fn shared_mcp_file(
    d: &dyn Door,
    agent: Option<&cmd_wire::AgentKind>,
) -> Result<PathBuf, Said> {
    let home = door::home(d).map_err(|e| Said::new("io_failed", e))?;
    let store = crate::accounts::manage::mcp_share_exec::store_path_in(Path::new(&home));
    writable(ExtKind::Mcp, &ExtLoc::User, store.is_some(), agent)
        .map_err(|why| Said::new("refused", why))?;
    Ok(store.unwrap_or_default())
}

/// 路径写成字（门只收字，线上也是字）：不是合法 UTF-8 ⇒ 照实拒，不换成替换字符拼出一个指到别处的路径。
fn path_text(p: &Path) -> Result<String, Said> {
    p.to_str().map(str::to_string).ok_or_else(|| {
        Said::new(
            "refused",
            copy_text("beExt.path.notUtf8", &[("path", &p.display().to_string())]),
        )
    })
}

/// 认得的那一样从哪来（闭集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// 嵌在 cc-monitor 后端二进制里：「这一版」= 本机后端那一份，装它用被写那台自己二进制里那一份。
    Embedded,
    /// 它自己发版（发版即装进那台的用户级 skills 目录）：「这一版」= 各台用户级里版本最新的那一份，装它从持有那一份的那台拿。
    Release,
}

/// cc-monitor 认得的一样（表顶那一组的一行）。
struct Known {
    kind: ExtKind,
    name: &'static str,
    origin: Origin,
    note: Option<fn() -> String>,
    /// 装上之后要在那台的 agent 配置里加钩子（抽屉里每台一行钩子状态）。
    hooks: bool,
}

/// cc-monitor 认得的那几样，按表顶的次序。哪台都没装也列；没装的那一格说「推荐 · 未装」，不替人默认装。
const KNOWN: &[Known] = &[
    Known {
        kind: ExtKind::Plugin,
        // 与 `plan::locate::PLUGIN_NAME` 同一个名字（不从那里引：assets → plan 会让顶层模块成环）。
        name: "planned-build",
        origin: Origin::Release,
        note: None,
        hooks: false,
    },
    Known {
        kind: ExtKind::Skill,
        name: "code-picture",
        origin: Origin::Release,
        note: None,
        hooks: false,
    },
    Known {
        kind: ExtKind::Skill,
        name: super::cc_bus_install::NAME,
        origin: Origin::Embedded,
        note: Some(cc_bus_note),
        hooks: true,
    },
];

fn cc_bus_note() -> String {
    copy_text("beExt.builtin.ccBus", &[])
}

/// 认得的那一样（照带家的行认：只属默认那一家，`agent` 是规整后的家，别的家同名那一样不算）。
fn known_of(
    kind: ExtKind,
    name: &str,
    agent: Option<&cmd_wire::AgentKind>,
) -> Option<&'static Known> {
    KNOWN
        .iter()
        .find(|k| agent.is_none() && k.kind == kind && k.name == name)
}

/// 是不是嵌在 cc-monitor 里的那一个（装它用被写那台二进制里那一份，不从别的机器拿）。
pub(crate) fn is_builtin(kind: ExtKind, name: &str, agent: Option<&cmd_wire::AgentKind>) -> bool {
    known_of(kind, name, agent).is_some_and(|k| k.origin == Origin::Embedded)
}

/// 自己发版的那几样（[`Origin::Release`]）：装它照 skill 那条路，从持有最新那一版的那台拷进被写那台的用户级 skills 目录
/// （插件也一样：它发版就是整个目录拷进 skills 目录）。
pub(crate) fn is_release(kind: ExtKind, name: &str, agent: Option<&cmd_wire::AgentKind>) -> bool {
    known_of(kind, name, agent).is_some_and(|k| k.origin == Origin::Release)
}

/// 这一家的这一种在这一级经这里装不了（来源或落点各问一次）：别的家的 MCP 在全局那一级（全局那一份只读默认那一家，
/// 不拿它冒充别家那一份）；项目级照那一家的项目级配置走，放行。
pub(crate) fn family_refused(
    kind: ExtKind,
    agent: Option<&cmd_wire::AgentKind>,
    at: &ExtLoc,
) -> Option<String> {
    (kind == ExtKind::Mcp && agent.is_some() && *at == ExtLoc::User)
        .then(|| copy_text("beExt.note.userMcpFamily", &[]))
}

/// 版本号逐段比：数字段按数比，别的按字比（「0.5.10」比「0.5.9」新）。
fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let seg = |s: &str| -> Vec<(u64, String)> {
        s.split(['.', '-', '+'])
            .map(|p| (p.parse::<u64>().unwrap_or(0), p.to_string()))
            .collect()
    };
    seg(a).cmp(&seg(b))
}

fn version_of(a: &Asset) -> Option<&str> {
    a.summary.get("version").and_then(Value::as_str)
}

/// 自己发版的那几样的「这一版」：用户级那几份里版本最新的；没有一份带版本 ⇒ `None`（照持有人最多的判）。
fn newest<'a>(held: &[Option<&'a Asset>]) -> Option<&'a str> {
    held.iter()
        .flatten()
        .filter_map(|a| version_of(a).map(|v| (v, a.digest.as_str())))
        .max_by(|x, y| version_cmp(x.0, y.0))
        .map(|(_, d)| d)
}

/// cc-monitor 自带的那一样只装全局（枢纽收到的落点先过这一道；插件只到用户级由 [`plugin_user_only`] 判；用户级 MCP 能不能写由被写那台自己判，见 [`shared_mcp_file`]）。
/// `agent` 是规整后的家（自带的 · 自己发版的只属默认那一家）。
pub(crate) fn builtin_refused(
    kind: ExtKind,
    name: &str,
    agent: Option<&cmd_wire::AgentKind>,
    at: &ExtLoc,
) -> Option<String> {
    if *at == ExtLoc::User {
        return None;
    }
    is_builtin(kind, name, agent).then(|| copy_text("beExt.target.builtinUserOnly", &[]))
}

/// 插件只到用户级（自己发版的拷进用户级 skills 目录 · 别的经那一家的插件命令装删）：落点是项目 ⇒ 为什么。全仓只此一处判。
fn plugin_user_only(kind: ExtKind, at: &ExtLoc) -> Option<String> {
    (kind == ExtKind::Plugin && *at != ExtLoc::User)
        .then(|| copy_text("beExt.target.pluginUserOnly", &[]))
}

/// 装到这一处行不行（表上「装到哪」的选项 · 枢纽收到的落点）：不能写的那一级（[`writable_among`]）· 自带的只装全局。
/// 行 ⇒ `None`。照带家的行认（`agent` 是那一行的家）；注册表由调用方交。
pub(crate) fn target_refused_among(
    registry: &[crate::agents::Adapter],
    kind: ExtKind,
    name: &str,
    agent: Option<&cmd_wire::AgentKind>,
    at: &ExtLoc,
    shared: bool,
) -> Option<String> {
    // 自己发版的插件装进那台的用户级 skills 目录（各号共用那一份），不经那一家的插件命令（那一家声明没声明都照装）。
    if kind == ExtKind::Plugin && is_release(kind, name, agent) {
        return plugin_user_only(kind, at);
    }
    writable_among(registry, kind, at, shared, agent)
        .err()
        .or_else(|| builtin_refused(kind, name, agent, at))
}

/// 来源与目标是同一台的同一处（枢纽拒、表上那一项不可选）。
pub(crate) fn same_place(
    from: Option<&str>,
    at_from: &ExtLoc,
    to: Option<&str>,
    at_to: &ExtLoc,
) -> bool {
    from == to && at_from == at_to
}

/// 表里一行的键：（种类, 名字, 家, 来源键）—— 同名的两家各是一行（默认那一家 = `None`，排在同名那几行最前）；
/// 同一家同名的插件几个来源各是一行（来源键见 [`source_key`]）。
type RowKey = (ExtKind, String, Option<cmd_wire::AgentKind>, Option<String>);

/// 一条的来源键：插件 ⇒ 那一家的写法（`summary.key`）；别的种类 · 自己发版的 · 没写法的（旧对端）⇒ 缺。
fn source_key(a: &Asset) -> Option<String> {
    (a.kind == ExtKind::Plugin && known_of(a.kind, &a.name, a.agent.as_ref()).is_none())
        .then(|| super::plugin_exec::key_of(&a.summary).map(str::to_string))
        .flatten()
}

/// 各台目录 ＋ 可达表（`origin` → 那台目录的 id）⇒ 条目 × 机器一张表。**判定只在这里。**
pub(crate) fn table(cat: &Catalog, reach: &[(String, Option<String>)]) -> ExtList {
    table_among(crate::agents::REGISTRY, cat, reach)
}

/// [`table`] 的注册表由调用方交那一半（判据交替身家）。
pub(crate) fn table_among(
    reg: &[crate::agents::Adapter],
    cat: &Catalog,
    reach: &[(String, Option<String>)],
) -> ExtList {
    let cols = columns(cat, reach);
    let mut by_key: BTreeMap<RowKey, Vec<Vec<&Asset>>> = BTreeMap::new();
    for k in KNOWN {
        by_key
            .entry((k.kind, k.name.to_string(), None, None))
            .or_insert_with(|| vec![Vec::new(); cols.len()]);
    }
    for (i, c) in cols.iter().enumerate() {
        for a in &cat.machines[c.id].assets {
            by_key
                .entry((a.kind, a.name.clone(), a.agent.clone(), source_key(a)))
                .or_insert_with(|| vec![Vec::new(); cols.len()])[i]
                .push(a);
        }
    }
    // 认得的那几样（默认那一家的）按 [`KNOWN`] 的次序排在最前，其余按（种类, 名字, 家）。
    let rank = |(kind, name, agent, _): &RowKey| match agent {
        Some(_) => KNOWN.len(),
        None => KNOWN
            .iter()
            .position(|k| k.kind == *kind && k.name == name)
            .unwrap_or(KNOWN.len()),
    };
    let mut keyed: Vec<_> = by_key.into_iter().collect();
    keyed.sort_by_key(|(key, _)| rank(key));
    let mut rows: Vec<ExtRow> = keyed
        .into_iter()
        .map(|((kind, name, agent, source), per)| ExtRow {
            source: source.map(|key| source_of(key, &per)),
            ..row(reg, cat, &cols, kind, &name, agent, &per)
        })
        .collect();
    // 不止一家 ⇒ 每行写上那一家的叫法（行上没写家的是资产默认那一家，打标签时省掉的就是它；判在 `said_when_several`）。
    let agents: Vec<Option<&str>> = rows.iter().map(|r| r.agent.as_deref()).collect();
    let said =
        crate::agents::said_when_several(reg, crate::agents::asset_default_among(reg), &agents);
    for (r, s) in rows.iter_mut().zip(said) {
        r.agent_said = s;
    }
    let legend = legend_of(&rows);
    ExtList {
        machines: cols.into_iter().map(|c| c.m).collect(),
        rows,
        problems: Vec::new(),
        legend,
        known_title: copy_text("beExt.list.known", &[]),
    }
}

/// 一行的来源：来源键 ＋ 那一句（照这一行各条记的市场写；同一写法的几条市场同一个，取第一条记了的；都没记 ⇒ 照写法写）。
fn source_of(key: String, per: &[Vec<&Asset>]) -> ExtSource {
    let market = per
        .iter()
        .flatten()
        .find_map(|a| a.summary.get("market").and_then(Value::as_str))
        .map(str::to_string);
    let said = copy_text(
        "beExt.row.source",
        &[("market", market.as_deref().unwrap_or(&key))],
    );
    ExtSource { key, said }
}

/// 「装到…」从哪来：自带的 ⇒ 被写那台二进制里那一份；否则某一台上的那一份。
enum Source<'c, 'a> {
    Builtin,
    Machine(&'c Column<'a>, &'a Asset),
}

fn row(
    reg: &[crate::agents::Adapter],
    cat: &Catalog,
    cols: &[Column],
    kind: ExtKind,
    name: &str,
    agent: Option<cmd_wire::AgentKind>,
    per: &[Vec<&Asset>],
) -> ExtRow {
    // 认得的那几样是默认那一家的（别的家同名那一行不算）；条目键带家（与目录里记的同一个）。
    let key = asset_catalog::entry_key(agent.as_deref(), kind, name);
    let known = known_of(kind, name, agent.as_ref());
    let user: Vec<Option<&Asset>> = per
        .iter()
        .map(|v| v.iter().copied().find(|a| a.project.is_none()))
        .collect();
    // 「这一版」：嵌在 cc-monitor 里的 ⇒ 本机后端二进制里那一份；自己发版的 ⇒ 用户级里版本最新的那一份；
    // 否则（或自己发版的却没有一份带版本）用户级持有人最多的那一版。
    let canon: Option<String> = match known.map(|k| k.origin) {
        Some(Origin::Embedded) => Some(super::cc_bus_install::embedded_digest()),
        Some(Origin::Release) => newest(&user)
            .or_else(|| canonical(&user, cols))
            .map(str::to_string),
        None => canonical(&user, cols).map(str::to_string),
    };
    // 来源：持有「这一版」的那几台里本机优先、再按列序、要连得上；用户级谁都没有 ⇒ 项目里有的那几台里同样挑。
    let source = match known.map(|k| k.origin) {
        Some(Origin::Embedded) => Some(Source::Builtin),
        _ => {
            let held: Vec<Option<&Asset>> = match canon.as_deref() {
                Some(d) => user.iter().map(|a| a.filter(|a| a.digest == d)).collect(),
                None => per.iter().map(|v| v.first().copied()).collect(),
            };
            pick_source(cols, &held).map(|(c, a)| Source::Machine(c, a))
        }
    };
    // 项目里那几处与谁比：「这一版」；用户级谁都没有 ⇒ 「装到…」会拿过去的那一版。
    let reference: Option<String> = canon.clone().or_else(|| match &source {
        Some(Source::Machine(_, a)) => Some(a.digest.clone()),
        _ => None,
    });
    let shown = match &source {
        Some(Source::Machine(_, a)) => Some(*a),
        _ => None,
    }
    .or_else(|| per.iter().flatten().next().copied());
    let first_seen = cat.known.get(&key);
    let new = cat.visits.prev > 0 && first_seen.is_some_and(|t| *t > cat.visits.prev);
    let cells = cols
        .iter()
        .enumerate()
        .map(|(i, c)| {
            cell(
                reg,
                kind,
                name,
                agent.as_ref(),
                known,
                c,
                &per[i],
                user[i],
                canon.as_deref(),
                reference.as_deref(),
                source.as_ref(),
            )
        })
        .collect();
    ExtRow {
        agent,
        agent_said: None,
        kind,
        kind_said: kind_said(kind),
        name: name.to_string(),
        about: shown.and_then(about_of),
        detail: shown.map(detail_of).unwrap_or_default(),
        new,
        known: known.map(|k| ExtKnown {
            from: match k.origin {
                Origin::Embedded => copy_text("beExt.from.builtin", &[]),
                Origin::Release => copy_text("beExt.from.release", &[]),
            },
            note: k.note.map(|f| f()),
            hooks: k.hooks,
        }),
        note: asset_catalog::note_of(cat, &key),
        cells,
        variant: None,
        picks: None,
        held_said: None,
        source: None,
    }
}

/// 同名 MCP 各号配置不一致的那几行拆开（各台 `mcp-read` 回来之后，`faces/ext_list_face.rs::with_mcp` 调）：
/// 有一台说那个名字（同一家 · 用户级或本地级）各号不一致 ⇒ 那一行照说不一致的那几份（哪一级, 配置原文）拆：每种一行（先后照台序、台里的先后），
/// 各带各台挑哪一条（`picks`：那台同一级同一份配置的那一条，没有 ⇒ 空）与说不一致的那一条写好的那一句；格子照抄（目录只认名字）。
/// 别台同一级同一份配置、它那台一致的那一条也挑进那一行；对不上哪一份的（项目级 · 一致的那一级）不进拆出的行。没有一台说不一致 ⇒ 不拆。
pub fn split_mismatch(list: &mut ExtList) {
    /// `mcp-read` 一条里拆行要的那几格（形状同 `faces::feature_face::McpReadEntry`；读不出的那一条略过）。
    #[derive(Deserialize)]
    struct Seen {
        name: String,
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        scope: String,
        #[serde(default)]
        server: serde_json::Value,
        #[serde(default)]
        mismatch: bool,
        #[serde(default)]
        held: Option<SeenHeld>,
    }
    #[derive(Deserialize)]
    struct SeenHeld {
        said: String,
    }
    // 各台那一格 `mcp` 的每一条（下标照原样留着：界面照下标挑）。
    let seen: Vec<Vec<Option<Seen>>> = list
        .machines
        .iter()
        .map(|m| {
            m.mcp
                .as_ref()
                .and_then(|v| v.get("entries"))
                .and_then(|v| v.as_array())
                .map(|es| {
                    es.iter()
                        .map(|e| serde_json::from_value(e.clone()).ok())
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect();
    let mut rows = Vec::with_capacity(list.rows.len());
    for r in std::mem::take(&mut list.rows) {
        // 各台这一行的那几条：(下标, 那一条)。
        let per: Vec<Vec<(usize, &Seen)>> = seen
            .iter()
            .map(|es| {
                es.iter()
                    .enumerate()
                    .filter_map(|(i, e)| e.as_ref().map(|e| (i, e)))
                    .filter(|(_, e)| {
                        r.kind == ExtKind::Mcp
                            && e.name == r.name
                            && e.agent.as_deref() == r.agent.as_deref()
                            && e.scope != "project"
                    })
                    .collect()
            })
            .collect();
        if !per.iter().flatten().any(|(_, e)| e.mismatch) {
            rows.push(r);
            continue;
        }
        // 说不一致的每种（哪一级, 配置原文）一行：不一致本来就在一级之内判，这里只照各台说的起行、不另判。
        let mut configs: Vec<(&str, &serde_json::Value)> = Vec::new();
        for (_, e) in per.iter().flatten().filter(|(_, e)| e.mismatch) {
            let key = (e.scope.as_str(), &e.server);
            if !configs.contains(&key) {
                configs.push(key);
            }
        }
        for (i, (scope, cfg)) in configs.into_iter().enumerate() {
            let hit: Vec<Option<&(usize, &Seen)>> = per
                .iter()
                .map(|es| {
                    es.iter()
                        .find(|(_, e)| e.scope == scope && e.server == *cfg)
                })
                .collect();
            // 那一句取说不一致的那一条写好的（每行至少有一条）。
            let said = hit
                .iter()
                .flatten()
                .filter(|(_, e)| e.mismatch)
                .find_map(|(_, e)| e.held.as_ref().map(|h| h.said.clone()));
            rows.push(ExtRow {
                variant: Some(u32::try_from(i).unwrap_or(u32::MAX)),
                picks: Some(
                    hit.iter()
                        .map(|h| h.map(|(at, _)| u32::try_from(*at).unwrap_or(u32::MAX)))
                        .collect(),
                ),
                held_said: said,
                ..r.clone()
            });
        }
    }
    list.rows = rows;
}

#[expect(
    clippy::too_many_arguments,
    reason = "一格要的几样（种类 · 名字 · 家 · 认得的那一样 · 那一列 · 本机的 · 用户的 · 规范名 · 参照 · 来源）由调用方逐列逐行现拿"
)]
fn cell(
    reg: &[crate::agents::Adapter],
    kind: ExtKind,
    name: &str,
    agent: Option<&cmd_wire::AgentKind>,
    known: Option<&Known>,
    c: &Column,
    mine: &[&Asset],
    user: Option<&Asset>,
    canon: Option<&str>,
    reference: Option<&str>,
    source: Option<&Source>,
) -> ExtCell {
    let is_this = |a: &Asset, d: Option<&str>| {
        if a.off {
            ExtState::Off
        } else if Some(a.digest.as_str()) == d {
            ExtState::Same
        } else {
            ExtState::Differs
        }
    };
    // 用户级开着 ⇒ 与「这一版」比；否则项目里有开着的 ⇒ 只在项目里；否则装着却都关着 ⇒ 关着；都没有 ⇒ 没装。
    let state = match user {
        Some(a) if !a.off => is_this(a, canon),
        _ if mine.iter().any(|a| a.project.is_some() && !a.off) => ExtState::Project,
        _ if !mine.is_empty() => ExtState::Off,
        _ => ExtState::Missing,
    };
    let live = c.m.reachable;
    let mut places = vec![place(
        reg,
        kind,
        agent,
        ExtLoc::User,
        user,
        user.map_or(ExtState::Missing, |a| is_this(a, canon)),
        live,
        c.shared,
    )];
    let mut in_projects: Vec<&Asset> = mine
        .iter()
        .copied()
        .filter(|a| a.project.is_some())
        .collect();
    in_projects.sort_by(|a, b| a.project.cmp(&b.project));
    for a in in_projects {
        places.push(place(
            reg,
            kind,
            agent,
            ExtLoc::of(a),
            Some(a),
            is_this(a, reference),
            live,
            c.shared,
        ));
    }
    let (bring, note) = if !live {
        (None, Some(copy_text("kit.offline.later", &[])))
    } else if let Some(why) = read_only_among(reg, kind, agent)
        .filter(|_| !known.is_some_and(|k| k.origin == Origin::Release))
    {
        // 整种只读（插件）：没有「装到…」，说为什么。自己发版的那几样例外：从持有最新那一版的那台装进用户级。
        (None, Some(why))
    } else {
        match source {
            None => (None, Some(copy_text("beExt.note.noSource", &[]))),
            Some(src) => bring_for(reg, kind, name, agent, c, src),
        }
    };
    // 认得的那几样：没装 ⇒ 「推荐 · 未装」；装着最新那一版、带版本 ⇒ 「已装 · 版本」。
    let said = match (known, state, user.and_then(version_of)) {
        (Some(_), ExtState::Missing, _) => copy_text("beExt.state.recommended", &[]),
        (Some(_), ExtState::Same, Some(v)) => {
            copy_text("beExt.state.sameVersion", &[("version", v)])
        }
        _ => state_said(state, &places),
    };
    ExtCell {
        said,
        state,
        places,
        bring,
        note,
    }
}

/// 一处：有它 ⇒ 能写的那一级给「卸载」（那台连着才给），不能写的说为什么；没有 ⇒ 只给态。照带家的行判（`agent` 是那一行的家）。
fn place(
    reg: &[crate::agents::Adapter],
    kind: ExtKind,
    agent: Option<&cmd_wire::AgentKind>,
    at: ExtLoc,
    held: Option<&Asset>,
    state: ExtState,
    live: bool,
    shared: bool,
) -> ExtPlace {
    let dir = match kind {
        ExtKind::Mcp => None,
        ExtKind::Skill | ExtKind::Plugin => held.and_then(|a| a.dir.clone()),
    };
    let (uninstall, note) = match (held, writable_among(reg, kind, &at, shared, agent)) {
        (None, _) => (false, None),
        (Some(a), _) if release_by_copy(kind, &a.name, agent).is_some() => (false, None),
        (Some(a), Ok(())) => match not_by_command(kind, a, agent) {
            Some(why) => (false, Some(why)),
            None => (live, None),
        },
        // 整种只读的那一句已在那一格上说过（[`cell`]），每处不再重复。
        (Some(_), Err(why)) => (
            false,
            read_only_among(reg, kind, agent).is_none().then_some(why),
        ),
    };
    ExtPlace {
        at,
        said: state_said(state, &[]),
        state,
        dir,
        uninstall,
        note,
        line: held.and_then(plugin_line),
    }
}

/// 机器那一行的「装到…」：可选的各处（全局 ＋ 那台每个开过会话的项目，不行的照列、说为什么）＋ 建议的那一处（与来源同级）。
fn bring_for(
    reg: &[crate::agents::Adapter],
    kind: ExtKind,
    name: &str,
    agent: Option<&cmd_wire::AgentKind>,
    c: &Column,
    src: &Source,
) -> (Option<ExtBring>, Option<String>) {
    let (from, from_loc) = match src {
        Source::Builtin => (None, ExtLoc::User),
        Source::Machine(col, a) => (col.m.key.clone(), ExtLoc::of(a)),
    };
    // 来源那一级装不了（别家全局那一级的 MCP · 来源那台那一家不经插件命令管的插件）：没有「装到…」，说为什么（不逐处列同一句）。
    let not_managed = match src {
        Source::Machine(_, a) => not_by_command(kind, a, agent),
        Source::Builtin => None,
    };
    if let Some(why) = family_refused(kind, agent, &from_loc).or(not_managed) {
        return (None, Some(why));
    }
    let from_machine = matches!(src, Source::Machine(..));
    // 插件只有用户级那一处（自己发版的拷进 skills 目录，别的经那一家自己的插件命令装），不列项目。
    let projects: &[String] = if kind == ExtKind::Plugin {
        &[]
    } else {
        c.projects
    };
    let targets: Vec<ExtTarget> = std::iter::once(ExtLoc::User)
        .chain(projects.iter().map(|d| ExtLoc::Project { dir: d.clone() }))
        .map(|at| {
            let why = target_refused_among(reg, kind, name, agent, &at, c.shared).or_else(|| {
                (from_machine && same_place(from.as_deref(), &from_loc, c.m.key.as_deref(), &at))
                    .then(|| copy_text("beExt.card.sameMachine", &[]))
            });
            ExtTarget {
                ok: why.is_none(),
                note: why,
                at,
            }
        })
        .collect();
    let like = match &from_loc {
        ExtLoc::User => ExtLoc::User,
        ExtLoc::Project { dir } => pick_project(c.projects, Some(dir))
            .map(|dir| ExtLoc::Project { dir })
            .unwrap_or(ExtLoc::User),
    };
    let to = targets
        .iter()
        .find(|t| t.ok && t.at == like)
        .or_else(|| targets.iter().find(|t| t.ok))
        .map(|t| t.at.clone());
    let Some(to) = to else {
        let why = if kind == ExtKind::Mcp && c.projects.is_empty() {
            copy_text("beExt.note.noProject", &[])
        } else {
            copy_text("beExt.card.sameMachine", &[])
        };
        return (None, Some(why));
    };
    (
        Some(ExtBring {
            from,
            scope: ExtScope { from: from_loc, to },
            targets,
        }),
        None,
    )
}

/// 有这一版、连得上的那几台里挑一台：本机优先，再按列序。
fn pick_source<'c, 'a>(
    cols: &'c [Column<'a>],
    held: &[Option<&'a Asset>],
) -> Option<(&'c Column<'a>, &'a Asset)> {
    cols.iter()
        .zip(held)
        .filter_map(|(c, a)| a.filter(|_| c.m.reachable).map(|a| (c, a)))
        .min_by_key(|(c, _)| !c.m.here)
}

/// 目录的写口（现扫 ＋ 记下 ＋ 交回整份与读不出来的那几份）：生产 = `asset_catalog::answer_current`。
pub(crate) type Current<'a> = &'a dyn Fn(bool) -> Result<(Catalog, Vec<String>), Said>;

/// `ext-list` / `ext-list-here` 的入参。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct ExtListReq {
    /// `true` = 这一问算「来看了一次」（扩展页每次变可见时的第一问）—— 「新见到」按上一次来看算。缺 · `null` ⇒ 不算。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub visit: Option<bool>,
}

/// `ext-list`：这台现扫一次、记下（`current` = 目录的写口，由 `stream/inbound/` 递进来），出表。
pub fn answer_list(
    req: &ExtListReq,
    current: Current,
    reach: &[(String, Option<String>)],
) -> Result<ExtList, Said> {
    let (cat, problems) = current(req.visit.unwrap_or(false))?;
    let mut list = table(&cat, reach);
    list.problems = problems;
    Ok(list)
}

/// 备注最长多少个字（超了拒，不截断）。
pub(crate) const NOTE_MAX_CHARS: usize = 2000;

/// 备注的写口（现扫 ＋ 记进本机目录自己那一格 ＋ 交回整份）：生产 = `asset_catalog::answer_note`。
pub(crate) type NoteWrite<'a> = &'a dyn Fn(&str, &str) -> Result<Catalog, Said>;

/// `ext-note-set` 的入参。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct ExtNoteReq {
    pub kind: ExtKind,
    pub name: String,
    /// 哪一家那一行的（线上的 kind）；缺 ⇒ 默认那一家。先规整（指名默认那一家 ⇒ 同缺；认不出 ⇒ `bad_args`），
    /// 条目键照规整后的家带（与表里那一行读的同一个）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent: Option<cmd_wire::AgentKind>,
    /// 备注正文（首尾空白去掉；空串 = 清掉；最长 [`NOTE_MAX_CHARS`] 字，超了拒、不截断）。
    pub text: String,
}

/// `ext-note-set` 的回包：现在生效的那一份（清掉了 ⇒ `null`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ExtNote {
    pub note: Option<String>,
}

/// `ext-note-set`：用户写 / 改 / 清（空串）一个条目的备注。记在本机目录自己那一格，随目录同步到别的后端。
pub fn answer_note(req: &ExtNoteReq, write: NoteWrite) -> Result<ExtNote, Said> {
    answer_note_among(crate::agents::REGISTRY, req, write)
}

/// [`answer_note`] 的本体：认家的注册表可喂（判据拿替身家喂）。
pub(crate) fn answer_note_among(
    reg: &[crate::agents::Adapter],
    req: &ExtNoteReq,
    write: NoteWrite,
) -> Result<ExtNote, Said> {
    let agent = super::skill_install::family_of(reg, req.agent.as_ref())?;
    if req.name.trim().is_empty() {
        return Err(crate::wire::spec::fail(
            "bad_args",
            crate::common::contract::malformed("`name` is empty"),
        ));
    }
    if req.text.chars().count() > NOTE_MAX_CHARS {
        return Err(crate::wire::spec::fail(
            "bad_args",
            copy_text(
                "beExt.note.tooLong",
                &[("max", &NOTE_MAX_CHARS.to_string())],
            ),
        ));
    }
    let key = asset_catalog::entry_key(agent.as_deref(), req.kind, &req.name);
    let cat = write(&key, &req.text)?;
    Ok(ExtNote {
        note: asset_catalog::note_of(&cat, &key),
    })
}

/// 「连这台」的那几台此刻：`(名字, 那台目录的 id)`。
pub fn reach_of(table: &crate::dial::remote_ask::Table) -> Vec<(String, Option<String>)> {
    table
        .connected()
        .into_iter()
        .map(|o| {
            let peer = table.peer(&o);
            (o, peer)
        })
        .collect()
}

// ───────────────────────── 这台上的那几处（可喂夹具） ─────────────────────────

/// 一台机器上一家的扩展住哪（生产 = 这台的环境照请求里的家现解；判据交临时目录与替身家）。
pub struct Env<'a> {
    /// 认家的注册表 ＋ 用户级根的覆盖（生产 = [`Roots::HERE`](super::skill_install::Roots::HERE)）：skill 根都经它现算。
    pub(crate) roots: super::skill_install::Roots<'a>,
    /// 这一趟是哪一家（规整后的：默认那一家 ⇒ 缺）。
    pub agent: Option<cmd_wire::AgentKind>,
    /// 装记录那份文件。
    pub ledger: Option<PathBuf>,
    /// 家目录（备份落在它下面的 `.cc-monitor/backups`）。
    pub home: Option<PathBuf>,
    /// 插件那几行怎么跑（生产 = [`run_here`](super::plugin_exec::run_here)；判据交记账的替身）。
    pub(crate) run: super::plugin_exec::Run<'a>,
}

impl Env<'static> {
    /// 这台上请求里那一家的那几处。家先规整（指名默认那一家 ⇒ 缺）；认不出 / 没有资产面 ⇒ `bad_args`（不退回默认那一家）。
    pub fn here(agent: Option<&cmd_wire::AgentKind>) -> Result<Env<'static>, Said> {
        let roots = super::skill_install::Roots::HERE;
        Ok(Env {
            agent: super::skill_install::family_of(roots.registry, agent)?,
            roots,
            ledger: super::skill_ledger::ledger_path(),
            home: crate::platform::paths::home_dir(),
            run: &super::plugin_exec::run_here,
        })
    }
}

impl Env<'_> {
    /// 那一级 skill 的根（这一家的）。
    pub(crate) fn skill_root(&self, at: &ExtLoc) -> Result<Option<PathBuf>, Said> {
        super::skill_install::skill_root_of(self.roots, self.agent.as_deref(), at.project())
    }
}

/// 这台上缺了家目录那一格（装记录 · 备份 · skill 根都从它来）。
fn need<T>(v: Option<T>) -> Result<T, Said> {
    v.ok_or_else(|| crate::wire::spec::fail("io_failed", copy_text("beExt.uninstall.noHome", &[])))
}

/// 一段值的记号（看过的那一份 · 应用时再算一次比）。
pub(crate) fn token_of(v: &Value) -> String {
    crate::common::digest::fnv_digest(&asset_catalog::canonical(v))
}

/// 备份落点：`<家>/.cc-monitor/backups/<毫秒>-<种类>-<名字>`（名字里别的字符换成 `_`）。回（家下相对段, 绝对路径）。
fn backup_slot(home: &str, kind: ExtKind, name: &str, now_ms: u64) -> (String, String) {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let rel = format!(
        "{}/{now_ms}-{}-{safe}",
        relay_route_core::EXT_BACKUPS_DIR_REL,
        kind.as_str()
    );
    let abs = door::join_under(home, &rel);
    (rel, abs)
}

// ───────────────────────── 卸（在被卸的那一台上） ─────────────────────────

// ───────────────────────── 撤回（卸 · 装改过的项目级 MCP 配置，在那份所在的那一台上） ─────────────────────────

/// `mcp-project-revert` 的入参：那一份项目级 MCP 配置的绝对路径（`data-report` 里带 `revert` 的那一行的 `path`）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct McpRevertReq {
    pub file: String,
}

/// `mcp-project-revert` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct McpReverted {
    /// 那份配置文件。
    pub path: String,
    /// 改之前没有那份文件 ⇒ 撤回是删掉它。
    pub removed: bool,
}

crate::wire::typed::codes! {
    /// `mcp-project-revert` 的码。
    pub enum McpRevertErr {
        /// 装记录里没记过这一份（没改过 · 已撤过）。
        NotFound = "not_found",
        /// cc-monitor 改过之后这份又被改过 · 已不在：一个字节不动。
        Stale = "stale",
        /// 读 · 写那份没成。
        Refused = "refused",
        /// 装记录读不懂（不覆盖）。
        LedgerUnreadable = "ledger_unreadable",
        /// 这台的家目录解不出来（装记录无从找）。
        IoFailed = "io_failed",
    }
}

/// `mcp-project-revert`：「文件与数据 → 改过的用户文件」那一行的「撤回」，在那份文件所在的那台上判、写（本体 `mcp_sync_flow::revert_project`）。
pub struct McpRevertCmd;

impl crate::wire::typed::Command for McpRevertCmd {
    const NAME: &'static str = "mcp-project-revert";
    type Req = McpRevertReq;
    type Reply = McpReverted;
    type Err = McpRevertErr;
}

impl crate::wire::typed::Handle for McpRevertCmd {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: McpRevertReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<McpReverted, McpRevertErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let (files, own) = (needs.0, &needs.1);
            let ledger = super::skill_ledger::ledger_path().ok_or_else(|| {
                McpRevertErr::IoFailed(copy_text("beSkillLedger.path.noHome", &[]))
            })?;
            answer_revert(
                files,
                &ledger,
                &|a| super::skill_ledger::answer_record(a, own),
                &req,
            )
        }
    }
}

/// [`McpRevertCmd`] 的本体（装记录的路径与写口是参数，判据拿沙箱喂）。
pub(crate) fn answer_revert(
    d: &dyn Door,
    ledger: &Path,
    record: Record,
    req: &McpRevertReq,
) -> Result<McpReverted, McpRevertErr> {
    super::mcp_sync_flow::revert_project(d, ledger, record, &req.file)
        .map_err(|f| crate::wire::typed::of_fail(f, McpRevertErr::Refused))
}

/// 一个 skill 的卸法：装记录里有 ⇒ 那一份逐文件判（`seen` 当 CAS 期望）；没有 ⇒ 整个目录的摘要当记号。
enum SkillPlan {
    Recorded { plan: Value },
    Foreign { digest: String, files: Vec<String> },
}

fn skill_plan(env: &Env<'_>, name: &str, dir: &Path) -> Result<SkillPlan, Said> {
    let ledger = need(env.ledger.clone())?;
    let key = path_text(dir)?;
    let recorded = super::skill_ledger::load_at(&ledger)?
        .installs
        .contains_key(&key);
    if recorded {
        let plan = super::skill_install::answer_uninstall_plan_at(&ledger, &json!({ "dir": key }))?;
        return Ok(SkillPlan::Recorded { plan });
    }
    let files: Vec<String> = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .min_depth(1)
        .sort_by_file_name()
        .into_iter()
        .flatten()
        .filter(|e| !e.file_type().is_dir())
        .map(|e| {
            path_text(e.path().strip_prefix(dir).unwrap_or(e.path())).map(|t| t.replace('\\', "/"))
        })
        .collect::<Result<_, _>>()?;
    Ok(SkillPlan::Foreign {
        digest: asset_catalog::skill_asset(None, name, dir, None).digest,
        files,
    })
}

fn skill_dir_of(env: &Env<'_>, name: &str, at: &ExtLoc) -> Result<PathBuf, Said> {
    if !super::skill_install::valid_name(name) {
        return Err(crate::wire::spec::fail(
            "bad_args",
            copy_text("beSkillInstall.read.badName", &[("name", name)]),
        ));
    }
    let dir = need(env.skill_root(at)?)?.join(name);
    if !dir.is_dir() {
        return Err(crate::wire::spec::fail(
            "not_found",
            copy_text(
                "beExt.uninstall.notThere",
                &[("path", &dir.display().to_string())],
            ),
        ));
    }
    Ok(dir)
}

/// MCP 那一条在这台上：（配置文件, 原文, 那一条, 装记录里记的摘要）；项目级的照那一家的配置格式读。
struct McpHere {
    root: String,
    rel: String,
    path: String,
    text: String,
    def: Value,
    recorded: Option<String>,
    /// 项目级那一份的配置格式（那一家的）；用户级 ⇒ `None`（各号共用那一份另有写口）。
    cells: Option<crate::agents::McpCells>,
}

fn mcp_here(d: &dyn Door, env: &Env<'_>, name: &str, at: &ExtLoc) -> Result<McpHere, Said> {
    let agent = env.agent.as_deref();
    let reg = env.roots.registry;
    let (root, rel, cells) = match at {
        ExtLoc::Project { dir } => {
            let (file, cells) = super::mcp_edit::project_mcp_of(reg, agent)?;
            (dir.clone(), file.to_string(), Some(cells))
        }
        ExtLoc::User => {
            let (root, rel) = user_mcp_spot(d, reg, agent)?;
            (root, rel, None)
        }
    };
    let got = door::peek(d, &root, &rel).map_err(|m| Said::new("refused", m))?;
    let text = got.text.ok_or_else(|| {
        crate::wire::spec::fail(
            "not_found",
            copy_text("beExt.uninstall.notThere", &[("path", &got.path)]),
        )
    })?;
    let servers = match cells {
        Some(c) => super::mcp_edit::project_servers(&c, &got.path, Some(&text))?,
        None => super::mcp_sync::servers_of(
            Some(&text),
            &copy_text("beMcpSync.answerWith.targetSide", &[]),
        )?,
    };
    let def = servers.get(name).cloned().ok_or_else(|| {
        crate::wire::spec::fail(
            "not_found",
            copy_text(
                "beExt.uninstall.noEntry",
                &[("name", name), ("path", &got.path)],
            ),
        )
    })?;
    // 全局那一份不记装记录：从它删只有一条路（各账号一起撤）。
    let recorded = match at {
        ExtLoc::User => None,
        ExtLoc::Project { .. } => {
            let ledger = need(env.ledger.clone())?;
            super::skill_ledger::load_at(&ledger)?
                .mcp
                .get(&got.path)
                .and_then(|m| m.get(name))
                .cloned()
        }
    };
    Ok(McpHere {
        root,
        rel,
        path: got.path,
        text,
        def,
        recorded,
        cells,
    })
}

/// `ext-uninstall-preview {kind, name, at}`：卸之前那张卡（只读）。
/// `ext-uninstall-preview` 的入参。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct ExtUninstallReq {
    pub kind: ExtKind,
    pub name: String,
    /// 卸哪一家的（线上的 kind）；缺 ⇒ 默认那一家（指名默认那一家同缺）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent: Option<cmd_wire::AgentKind>,
    /// 插件：那一行的来源键（[`ExtSource::key`]），照它找那一条；插件不带 ⇒ `bad_args`。别的种类不看它。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub source: Option<String>,
    /// 在这台哪一级（项目目录须是这台上的绝对路径）。
    pub at: ExtLoc,
}

/// `ext-uninstall-apply` 的入参：卡上那一份记号原样交回（现在对不上 ⇒ `stale`、一个字节不动）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct ExtUninstallApplyReq {
    pub kind: ExtKind,
    pub name: String,
    /// 卸哪一家的（线上的 kind）；缺 ⇒ 默认那一家（指名默认那一家同缺）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub agent: Option<cmd_wire::AgentKind>,
    /// 同 [`ExtUninstallReq::source`]。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub source: Option<String>,
    pub at: ExtLoc,
    pub token: String,
}

/// 自己发版的插件是拷进 skills 目录的那一份，不经那一家的插件命令卸 ⇒ 为什么（表上不给「卸载」、卸载那一对拒，同这一处判）。
fn release_by_copy(
    kind: ExtKind,
    name: &str,
    agent: Option<&cmd_wire::AgentKind>,
) -> Option<String> {
    (kind == ExtKind::Plugin && is_release(kind, name, agent))
        .then(|| copy_text("beExt.uninstall.releasePlugin", &[]))
}

/// 目录里那一条插件不经那一家的插件命令管（扫完照那一家列出来的结果标的 `byCommand` 不为真：不在它的列表里 · 列不出 ·
/// 旧对端没这一格 · 条目上没有那一家的写法）⇒ 为什么（表上不给「卸载」「装到…」）。自己发版的那几样另走 [`release_by_copy`]，不在这里判。
fn not_by_command(kind: ExtKind, a: &Asset, agent: Option<&cmd_wire::AgentKind>) -> Option<String> {
    if kind != ExtKind::Plugin || release_by_copy(kind, &a.name, agent).is_some() {
        return None;
    }
    let keyed = super::plugin_exec::key_of(&a.summary).is_some();
    match a
        .summary
        .get(super::plugin_exec::BY_COMMAND)
        .and_then(Value::as_bool)
    {
        Some(true) if keyed => None,
        Some(false) => Some(copy_text("beExt.plugin.notByCommand", &[])),
        // 列不出（原因在目录的 problems）· 旧对端没这一格 · 没有写法（装 · 删交不出）：照实说列不出，不说「不经插件命令管」。
        _ => Some(copy_text("beExt.plugin.unlisted", &[])),
    }
}

/// 卸一个插件之前看一眼：这一级写不了（[`writable_among`]：那一家没声明插件那一格 · 不在用户级）⇒ 拒；
/// 自己发版的那几样（拷进 skills 目录的那一份，不经那一家的插件命令卸）⇒ 拒；列出来没有它 ⇒ `not_found`。
/// 界面交那一行的来源键（[`ExtSource::key`]，那一家的写法）：照它在那一家列出来的里找那一条，删交它（通用层不拼）；
/// 不带 ⇒ `bad_args`（不照光名字猜）。回（删那一行给人看的样子, 记号：列出来的那一条, 那一家的写法）。
fn plugin_here(
    env: &Env<'_>,
    name: &str,
    source: Option<&str>,
    at: &ExtLoc,
) -> Result<(String, String, String), Said> {
    let registry = env.roots.registry;
    let agent = env.agent.as_ref();
    if let Some(why) = writable_among(registry, ExtKind::Plugin, at, true, agent)
        .err()
        .or_else(|| release_by_copy(ExtKind::Plugin, name, agent))
    {
        return Err(crate::wire::spec::fail("refused", why));
    }
    let Some(source) = source else {
        return Err(crate::wire::spec::fail(
            "bad_args",
            crate::common::contract::malformed("plugin uninstall without `source`"),
        ));
    };
    let listed = super::plugin_exec::listed(registry, agent, env.run)?;
    let Some(held) = listed.iter().find(|p| p.key == source && p.name == name) else {
        return Err(crate::wire::spec::fail(
            "not_found",
            copy_text("beExt.uninstall.notThere", &[("path", name)]),
        ));
    };
    let line = super::plugin_exec::remove_line(registry, agent, &held.key)?;
    Ok((
        line,
        token_of(
            &json!({ "key": held.key, "name": held.name, "version": held.version, "on": held.on }),
        ),
        held.key.clone(),
    ))
}

pub fn answer_uninstall_preview(
    d: &dyn Door,
    env: &Env<'_>,
    req: &ExtUninstallReq,
) -> Result<ExtUninstallCard, Said> {
    let (kind, name) = (req.kind, req.name.as_str());
    let at = req.at.here()?;
    let home = path_text(&need(env.home.clone())?)?;
    let card = match kind {
        ExtKind::Skill => {
            let dir = skill_dir_of(env, name, &at)?;
            let dir_s = path_text(&dir)?;
            match skill_plan(env, name, &dir)? {
                SkillPlan::Recorded { plan } => {
                    let rows = plan["rows"].as_array().cloned().unwrap_or_default();
                    let files: Vec<String> = rows
                        .iter()
                        .filter(|r| r["deletable"] == true)
                        .filter_map(|r| r["path"].as_str().map(str::to_string))
                        .collect();
                    let ask = rows.iter().any(|r| r["ask"] == true);
                    ExtUninstallCard {
                        agent: env.agent.clone(),
                        kind,
                        name: name.to_string(),
                        path: dir_s.clone(),
                        recorded: true,
                        said: if ask {
                            copy_text(
                                "beExt.uninstall.ownedAsk",
                                &[("n", &files.len().to_string())],
                            )
                        } else {
                            copy_text("beExt.uninstall.owned", &[("n", &files.len().to_string())])
                        },
                        files,
                        backup: None,
                        token: token_of(&json!({ "rows": plan["rows"], "seen": plan["seen"] })),
                    }
                }
                SkillPlan::Foreign { digest, files } => {
                    let (_, abs) = backup_slot(&home, ExtKind::Skill, name, 0);
                    let at_dir = Path::new(&abs)
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or(abs);
                    ExtUninstallCard {
                        agent: env.agent.clone(),
                        kind,
                        name: name.to_string(),
                        path: dir_s.clone(),
                        recorded: false,
                        said: copy_text("beExt.uninstall.foreign", &[("backup", &at_dir)]),
                        files,
                        backup: Some(at_dir),
                        token: digest,
                    }
                }
            }
        }
        ExtKind::Mcp if at == ExtLoc::User => {
            let m = mcp_here(d, env, name, &at)?;
            ExtUninstallCard {
                agent: env.agent.clone(),
                kind,
                name: name.to_string(),
                path: m.path.clone(),
                recorded: false,
                files: vec![name.to_string()],
                backup: None,
                said: copy_text("beExt.uninstall.mcpShared", &[("name", name)]),
                token: crate::common::digest::fnv_digest(&m.text),
            }
        }
        ExtKind::Plugin => {
            let (line, token, _) = plugin_here(env, name, req.source.as_deref(), &at)?;
            ExtUninstallCard {
                agent: env.agent.clone(),
                kind,
                name: name.to_string(),
                path: line.clone(),
                recorded: false,
                files: vec![name.to_string()],
                backup: None,
                said: copy_text("beExt.uninstall.pluginByCommand", &[("cmd", &line)]),
                token,
            }
        }
        ExtKind::Mcp => {
            let m = mcp_here(d, env, name, &at)?;
            let intact = m.recorded.as_deref() == Some(&super::skill_ledger::mcp_digest(&m.def));
            // 不是 cc-monitor 装的那一条：删之前的原文记进装记录（撤回在「改过的用户文件」那一行），不另抄一份。
            let said = match (&m.recorded, intact) {
                (Some(_), true) => copy_text("beExt.uninstall.mcpOwned", &[]),
                (Some(_), false) => copy_text("beExt.uninstall.mcpModified", &[]),
                (None, _) => copy_text("beExt.uninstall.mcpForeign", &[]),
            };
            ExtUninstallCard {
                agent: env.agent.clone(),
                kind,
                name: name.to_string(),
                path: m.path.clone(),
                recorded: m.recorded.is_some(),
                files: vec![name.to_string()],
                backup: None,
                said,
                token: crate::common::digest::fnv_digest(&m.text),
            }
        }
    };
    Ok(card)
}

fn stale() -> Said {
    crate::wire::spec::fail("stale", copy_text("beExt.uninstall.changed", &[]))
}

/// `ext-uninstall-apply {kind, name, at, token}`：看卡时那一份（`token`）与现在不同 ⇒ `stale`、一个字节不动。
/// 用户在卡上点了确认 = 同意（装完改过的 · 装时盖掉原有的 · 不是装的，卡上都说过）。
pub fn answer_uninstall_apply(
    d: &dyn Door,
    env: &Env<'_>,
    record: Record,
    req: &ExtUninstallApplyReq,
) -> Result<ExtDone, Said> {
    let (kind, name, token) = (req.kind, req.name.as_str(), req.token.as_str());
    let at = req.at.here()?;
    let home = path_text(&need(env.home.clone())?)?;
    let done = match kind {
        ExtKind::Skill => {
            let dir = skill_dir_of(env, name, &at)?;
            let dir_s = path_text(&dir)?;
            match skill_plan(env, name, &dir)? {
                SkillPlan::Recorded { plan } => {
                    if token_of(&json!({ "rows": plan["rows"], "seen": plan["seen"] })) != token {
                        return Err(stale());
                    }
                    let rows = plan["rows"].as_array().cloned().unwrap_or_default();
                    let pick = |k: &str| -> Vec<Value> {
                        rows.iter()
                            .filter(|r| r["deletable"] == true && (k == "take" || r["ask"] == true))
                            .map(|r| r["path"].clone())
                            .collect()
                    };
                    let ledger = need(env.ledger.clone())?;
                    let out = super::skill_flow::answer_uninstall(
                        d,
                        &ledger,
                        record,
                        &json!({ "dir": dir_s, "seen": plan["seen"], "take": pick("take"), "confirm": pick("confirm") }),
                    )?;
                    let changed: Vec<String> = out["deleted"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|p| p.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    let note = [&out["recordFailed"], &out["dirFailed"]]
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect::<Vec<_>>();
                    ExtDone {
                        path: dir_s.clone(),
                        changed,
                        note: (!note.is_empty()).then(|| note.join(" ")),
                    }
                }
                SkillPlan::Foreign { digest, files } => {
                    if digest != token {
                        return Err(stale());
                    }
                    let from = door::rel_under(&home, &dir_s).map_err(|_| {
                        Said::new(
                            "refused",
                            copy_text("beExt.uninstall.backupOutsideHome", &[("path", &dir_s)]),
                        )
                    })?;
                    let (rel, abs) = backup_slot(&home, ExtKind::Skill, name, now_ms());
                    ensure_backups_dir(d, &home)?;
                    door::rename(d, &home, &from, &rel).map_err(|e| {
                        Said::new(
                            "refused",
                            copy_text("beExt.uninstall.backupFailed", &[("why", &e)]),
                        )
                    })?;
                    ExtDone {
                        path: dir_s.clone(),
                        changed: files,
                        note: Some(copy_text("beExt.uninstall.movedTo", &[("path", &abs)])),
                    }
                }
            }
        }
        ExtKind::Mcp if at == ExtLoc::User => {
            let m = mcp_here(d, env, name, &at)?;
            if crate::common::digest::fnv_digest(&m.text) != token {
                return Err(stale());
            }
            // 卸的是行上那一家那一格（规整过的家；别家那一格前面 [`writable`] 已拒）。
            let view =
                crate::accounts::manage::mcp_share_exec::remove(d, name, env.agent.as_deref())?;
            ExtDone {
                path: m.path,
                changed: vec![name.to_string()],
                note: (!view.notes.is_empty()).then(|| view.notes.join(" ")),
            }
        }
        ExtKind::Plugin => {
            let (_, now, key) = plugin_here(env, name, req.source.as_deref(), &at)?;
            if now != token {
                return Err(stale());
            }
            super::plugin_exec::remove(env.roots.registry, env.agent.as_ref(), &key, env.run)?
        }
        ExtKind::Mcp => {
            let m = mcp_here(d, env, name, &at)?;
            if crate::common::digest::fnv_digest(&m.text) != token {
                return Err(stale());
            }
            // 项目级那一份照那一家的配置格式改：只删那一个服务器那一节，别的键与写法原样。
            // 项目级那一份恒带格式（[`mcp_here`] 照那一家取过一次，这里不再取）。
            let Some(cells) = m.cells else {
                return Err(crate::wire::spec::fail(
                    "bad_args",
                    crate::common::contract::malformed("project MCP without a config format"),
                ));
            };
            let next = super::mcp_edit::remove_mcp_server(&cells, &m.path, &m.text, name)
                .map_err(|e| Said::new("refused", e))?;
            // 先记账、再写文件：不是 cc-monitor 装的那一条也记（改过这份文件 ⇒ 进「改过的用户文件」、撤得回改之前的原文），
            // 装记录是原文唯一的留底 ⇒ 记不上（读不懂 · 写不进）就一个字节不删。
            let after = crate::common::digest::fnv_digest(&next);
            let noted = record(&RecordReq::McpDrop {
                file: m.path.clone(),
                name: name.to_string(),
                before: Some(m.text.clone()),
                after: after.clone(),
            })?;
            if let Err(e) = door::put(d, &m.root, &m.rel, &next, Some(&m.text), false, false) {
                super::mcp_sync_flow::undo_noted(record, &m.path, Some(&after), noted.prior);
                return Err(match e {
                    Refused::Stale(_) => stale(),
                    e => crate::wire::spec::fail("refused", e.said()),
                });
            }
            ExtDone {
                path: m.path,
                changed: vec![name.to_string()],
                note: None,
            }
        }
    };
    Ok(done)
}

/// `~/.cc-monitor/backups` 不在就建（它的上一层是后端自己的家，一定在）。
fn ensure_backups_dir(d: &dyn Door, home: &str) -> Result<(), Said> {
    let abs = door::join_under(home, relay_route_core::EXT_BACKUPS_DIR_REL);
    match door::stat_kind(d, &abs) {
        Ok(Some(_)) => Ok(()),
        Ok(None) => door::mkdir(d, home, relay_route_core::EXT_BACKUPS_DIR_REL).map_err(|e| {
            crate::wire::spec::fail(
                "refused",
                copy_text("beExt.uninstall.backupFailed", &[("why", &e)]),
            )
        }),
        Err(e) => Err(crate::wire::spec::fail(
            "refused",
            copy_text("beExt.uninstall.backupFailed", &[("why", &e)]),
        )),
    }
}

// ───────────────────────── 可疑项的那句话（枢纽拼卡时用） ─────────────────────────

/// 一条可疑项 → 那句话（`machine` = 被写那台给人看的名字）。种类闭集：MCP 与 skill 两边的并集。
/// skill 那一边没有 `field`（交空串）；`there` 缺 = 不问那台的那几种。
pub(crate) fn suspect_said(
    kind: &str,
    field: &str,
    value: &str,
    there: Option<&str>,
    machine: &str,
) -> String {
    match (kind, there.unwrap_or_default()) {
        ("abs-path", "present") => copy_text(
            "beExt.suspect.absPresent",
            &[("field", field), ("value", value), ("machine", machine)],
        ),
        ("abs-path", "absent") => copy_text(
            "beExt.suspect.absAbsent",
            &[("field", field), ("value", value), ("machine", machine)],
        ),
        ("abs-path", "foreign") => copy_text(
            "beExt.suspect.absForeign",
            &[("field", field), ("value", value), ("machine", machine)],
        ),
        ("abs-path", _) => copy_text(
            "beExt.suspect.absUnknown",
            &[("field", field), ("value", value), ("machine", machine)],
        ),
        ("command-missing", "unknown") => copy_text(
            "beExt.suspect.commandUnknown",
            &[("value", value), ("machine", machine)],
        ),
        ("command-missing", _) => copy_text(
            "beExt.suspect.commandMissing",
            &[("value", value), ("machine", machine)],
        ),
        ("command-relative", _) => copy_text(
            "beExt.suspect.commandRelative",
            &[("value", value), ("machine", machine)],
        ),
        ("executable", _) => copy_text("beExt.suspect.executable", &[]),
        ("binary", _) => copy_text("beExt.suspect.binary", &[]),
        _ => copy_text("beExt.suspect.other", &[("field", field), ("value", value)]),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/ext_tests.rs"]
mod tests;
