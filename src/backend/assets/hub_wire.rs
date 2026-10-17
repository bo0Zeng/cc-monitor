//! 枢纽问各台的那 8 条内层命令（来源那台读 · 被写那台判 / 写）的线上形：每条一个标记类型，名字 · 入参 · 回包连在它的 `Command` 上。
//!
//! 这是**两台后端之间**的线上形：远端那台可能更旧或更新。于是入参拒多出来的键（这台收到的是同版本枢纽发的），
//! 回包不拒（对面可能更新、多带一格）；回包解不出 ⇒ 枢纽说「那台后端版本不同」（`hub::ask_one`）。
//! 帧面那一侧（`faces/hub_inner_face.rs`）每条一行 `impl Command`（线上名 · 入参 · 回包 · 码），名字只写那一处。
//! 有 `null` 的那几格照出 `null`（不省略）：钉住的字节（`hub_inner_wire_tests`）就是这一形。

use super::ext::{ExtLoc, ExtSlot};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

// 标记类型住这里（枢纽按它们问）；帧面的 `impl Command`（线上名 · 入参 · 回包）住 `faces/hub_inner_face.rs`，线上名只写在那一处。

/// 来源那台读一个 skill 的全部文件（`skill-read`）。
pub(crate) struct SkillReadCmd;
/// 被写那台判 skill 装不装得过来（`skill-install-plan`）。
pub(crate) struct SkillPlanCmd;
/// 被写那台把勾的那几个 skill 文件写进去（`skill-install-apply`）。
pub(crate) struct SkillApplyCmd;
/// 来源那台交那一条 MCP，密钥值换成空位（`mcp-sync-source`）。
pub(crate) struct McpSourceCmd;
/// 被写那台看装上之后那一条（`mcp-sync-preview`）。
pub(crate) struct McpPreviewCmd;
/// 被写那台把那一条写进去（`mcp-sync-apply`）。
pub(crate) struct McpApplyCmd;
/// 装 cc-bus 之前看一眼（`cc-bus-install-state`）。
pub(crate) struct CcBusStateCmd;
/// 把这台二进制带着的 cc-bus 装到这台（`cc-bus-install`）。
pub(crate) struct CcBusInstallCmd;

/// 不收入参的那两条：缺 · `null` · `{}` 都认，多一个键 ⇒ `bad_args`。发出去是 `{}`。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct NoReq {}

// ───────────────────────── skill ─────────────────────────

/// `skill-read`：哪一个 skill；`project` 给了 ⇒ 那个项目里的（这台上的绝对路径），缺 / `null` ⇒ 用户级。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SkillReadReq {
    /// skill 的目录名（一段：不许分隔符 / `..` / 点开头）。
    pub name: String,
    #[serde(default)]
    pub project: Option<String>,
}

/// 来源那台读到的一个文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct SkillReadFile {
    /// skill 里的相对路径（`/` 分段）。
    pub path: String,
    /// 原文；不是文本 / 太大 / 读不出来 ⇒ `null`（这一个装不过去），原因在 `why`。
    pub text: Option<String>,
    pub bytes: u64,
    pub exec: bool,
    pub why: Option<String>,
}

/// `skill-read` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct SkillRead {
    /// 这台 skill 的根。
    pub root: String,
    /// 这个 skill 的目录（绝对路径）。
    pub dir: String,
    pub files: Vec<SkillReadFile>,
    /// 没读的那几处（指向目录的链接 / 特殊文件）。
    pub skipped: Vec<String>,
}

/// 装那一跳交给被写那台的一个文件（来源那台读到的那三格）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SkillSourceFile {
    pub path: String,
    /// `null` = 装不过去的那一个。
    pub text: Option<String>,
    #[serde(default)]
    pub exec: bool,
}

/// 被写那台上一个文件现有的原文（写的时候当 CAS 期望）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SkillTargetFile {
    pub path: String,
    pub text: String,
}

/// `skill-install-plan`：给了 `take` 才答 `write`；`differs` 的要在 `overwrite` 里点名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SkillPlanReq {
    pub name: String,
    pub source: Vec<SkillSourceFile>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub take: Option<Vec<String>>,
    #[serde(default)]
    pub overwrite: Option<Vec<String>>,
}

/// skill 一个文件里的一条可疑项。`kind` 闭集 `abs-path` · `binary` · `command-missing` · `executable`；
/// `there` = 它在被写那台上的样子（`present` · `absent` · `foreign` · `unknown`；不问那台的那几种 ⇒ `null`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct SkillSuspect {
    pub kind: String,
    pub value: String,
    pub there: Option<String>,
}

/// 判定表里一个路径一行。`state` 闭集 `new` · `same` · `differs` · `only-there`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct SkillPlanRow {
    pub path: String,
    pub state: String,
    pub suspects: Vec<SkillSuspect>,
    /// 这台那一份盖不了（读不出原文 / 不是普通文件）的原因。
    pub blocked: Option<String>,
}

/// 装记录里一个文件：装进去的那一份的摘要 ＋ 装之前在不在。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct SkillLedgerFile {
    pub digest: String,
    pub created: bool,
}

/// `skill-install-plan` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct SkillPlan {
    pub root: String,
    pub dir: String,
    /// 写的时候 `Door::put` 用的 `root`。
    pub base: String,
    /// 写的时候 `Door::put` 用的相对前缀：`rel` = `<prefix>/<path>`。
    pub prefix: String,
    pub rows: Vec<SkillPlanRow>,
    pub target: Vec<SkillTargetFile>,
    /// 没给 `take` ⇒ `null`；给了 ⇒ 真要写的路径（排序）。
    pub write: Option<Vec<String>>,
    /// 没给 `take` ⇒ `null`。
    pub ledger: Option<BTreeMap<String, SkillLedgerFile>>,
}

/// `skill-install-apply`：`source` = 来源那台读到的（原样）；`target` = 看差异时这台回的那几份原文（CAS 期望）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SkillApplyReq {
    pub name: String,
    #[serde(default)]
    pub project: Option<String>,
    pub source: Vec<SkillSourceFile>,
    pub target: Vec<SkillTargetFile>,
    #[serde(default)]
    pub take: Option<Vec<String>>,
    #[serde(default)]
    pub overwrite: Option<Vec<String>>,
}

/// `skill-install-apply` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillApplied {
    pub dir: String,
    /// 真写成了的那几个（按写的顺序）。
    pub written: Vec<String>,
    /// 写成了但执行位没置上的那几个。
    pub chmod_failed: Vec<String>,
    /// 装记录没记下来时那一句（`null` = 记下了）。
    pub record_failed: Option<String>,
}

// ───────────────────────── MCP ─────────────────────────

/// `mcp-sync-source`：哪一条 · 在这台上哪一级。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct McpSourceReq {
    pub name: String,
    pub at: ExtLoc,
}

/// 来源那台换成空位的一格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct McpSourceSlot {
    /// `env` · `headers`。
    pub field: String,
    pub key: String,
}

/// `mcp-sync-source` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct McpSource {
    /// 读的是哪一份。
    pub path: String,
    /// 那一条；`env` / `headers` 的值在这台就换成 `null`。
    #[cfg_attr(test, ts(type = "unknown"))]
    pub def: Value,
    pub slots: Vec<McpSourceSlot>,
    /// 按原样那一条（含密钥值）算的记号。
    pub token: String,
}

/// `mcp-sync-preview`：`def` = 来源那台交回的那一条，原样。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct McpPreviewReq {
    pub name: String,
    pub at: ExtLoc,
    #[cfg_attr(test, ts(type = "unknown"))]
    pub def: Value,
}

/// MCP 那一条里的一条可疑项：`kind` 闭集 `abs-path` · `command-missing` · `command-relative`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct McpSuspect {
    pub kind: String,
    pub field: String,
    pub value: String,
    pub there: Option<String>,
}

/// `mcp-sync-preview` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct McpPreview {
    /// 这台那份的路径。
    pub path: String,
    /// `new` · `same` · `differs`。
    pub state: String,
    #[cfg_attr(test, ts(type = "unknown"))]
    pub def: Value,
    pub slots: Vec<ExtSlot>,
    pub suspects: Vec<McpSuspect>,
    /// 这台那份的记号（不存在 ⇒ `null`）—— 写的时候原样交回。
    pub target: Option<String>,
}

/// `mcp-sync-apply`：`fill` = 用户在卡上填的 `{env: {键: 值}, headers: {…}}`；`target` = 看卡时这台那份的记号。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct McpApplyReq {
    pub name: String,
    pub at: ExtLoc,
    #[cfg_attr(test, ts(type = "unknown"))]
    pub def: Value,
    #[serde(default)]
    pub fill: Option<BTreeMap<String, BTreeMap<String, String>>>,
    #[serde(default)]
    pub target: Option<String>,
}

/// `mcp-sync-apply` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpApplied {
    pub path: String,
    /// 真写了吗（与原有那一条逐字相同 ⇒ `false`）。
    pub written: bool,
    pub record_failed: Option<String>,
}

// ───────────────────────── cc-bus ─────────────────────────

/// `cc-bus-install-state` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct CcBusState {
    /// 落点（`<skills 根>/cc-bus`）。
    pub dest: String,
    /// 内容会变的那几个（缺的 ＋ 不一样的）；空 ⇒ 已是这一版。
    pub writes: Vec<String>,
    /// 落点上已经有东西（要写时先整个改名留作备份）。
    pub existing: bool,
    /// 内嵌那一份的摘要。
    pub version: String,
}

/// `cc-bus-install` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct CcBusInstalled {
    pub dest: String,
    /// 写了的那几个相对路径（全一致 ⇒ 空）。
    pub written: Vec<String>,
    /// 覆盖前整个目录改名成的那一份（`null` = 之前没装过）。
    pub backup: Option<String>,
    pub record_failed: Option<String>,
}

// ───────────────────────── 枢纽自己那一对（界面读：`ext-hub-preview` / `-apply`） ─────────────────────────

/// 用户在卡上填的值 `{field: {key: 值}}`。
pub(crate) type Fill = BTreeMap<String, BTreeMap<String, String>>;

/// `ext-hub-preview`：装到勾上的几台之前那一张卡。`to` 里 `null` = 这台自己；`place` = 用户在卡上选的那一处。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HubReq {
    pub kind: super::ext::ExtKind,
    pub name: String,
    pub to: Vec<Option<String>>,
    #[serde(default)]
    pub place: Option<ExtLoc>,
}

/// `ext-hub-apply`：同 [`HubReq`]，再带每台那张卡上的记号（键 = 机器键，本机 = 空串）与填的值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct HubApplyReq {
    pub kind: super::ext::ExtKind,
    pub name: String,
    pub to: Vec<Option<String>>,
    #[serde(default)]
    pub place: Option<ExtLoc>,
    #[serde(default)]
    pub tokens: BTreeMap<String, super::ext::ExtTokens>,
    #[serde(default)]
    pub fill: Fill,
}

/// 几张卡的要填格并成一份：每格一次，`kept` = 那一格已经有值的几台（名字）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HubSlot {
    pub field: String,
    pub key: String,
    pub kept: Vec<String>,
}

/// 勾上的一台那张卡：拼不成 ⇒ `card` 为 `null`、`error` = 那台说的那一句、`detail` = 原话（进那一行的复制详情；没有 ⇒ `null`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HubMachineCard {
    pub to: Option<String>,
    pub name: String,
    pub card: Option<super::ext::ExtCard>,
    /// 会写的文件那一行（MCP：那份配置文件 ＋ 键；skill：目录 ＋ 要写的几个）。
    pub files: Vec<String>,
    pub error: Option<String>,
    pub detail: Option<String>,
}

/// `ext-hub-preview` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HubPreview {
    /// 各台能装的各处并起来：勾上的每台都能装才 `ok`。
    pub places: Vec<super::ext::ExtTarget>,
    /// 共用的那一处（没有每台都能装的 ⇒ `null`）。
    pub place: Option<ExtLoc>,
    pub slots: Vec<HubSlot>,
    pub machines: Vec<HubMachineCard>,
}

/// 一台的结局：没成 ⇒ `done` 为 `null`、`error` = 那台说的那一句、`detail` = 原话（同 [`HubMachineCard`]）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HubMachineDone {
    pub to: Option<String>,
    pub name: String,
    pub done: Option<super::ext::ExtDone>,
    pub error: Option<String>,
    pub detail: Option<String>,
}

/// `ext-hub-apply` 的回包：每台各自结局，一台没成不挡别台。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct HubApplied {
    pub machines: Vec<HubMachineDone>,
}
