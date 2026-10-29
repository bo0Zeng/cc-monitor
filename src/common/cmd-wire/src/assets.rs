//! 资产目录那几条的入参与回包：
//! - `assets-sync`（本机常驻后端答）：壳连上一台时照它交那台的名字，界面经通道问同一条；
//! - `assets-catalog` · `assets-catalog-merge`：一台的整份目录 ＋「这台缺什么」（判定与盘上那一份在后端 `assets/asset_catalog.rs`）。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// `assets-sync` 的入参：缺 ⇒ 对机器表里「连这台」的每一台各一趟；给 `origin` ⇒ 先对那一台（机器表里要有）。
/// 怎么够到那台只看本机后端的机器表：线上不交拨号（带 `dial` ⇒ `bad_args`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct AssetsSyncReq {
    /// 那台的名字。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<String>"))]
    pub origin: Option<crate::Machine>,
}

/// 一趟对一台的结局（`synced[]` 的一行）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AssetsSyncRow {
    /// 那台的名字。
    pub origin: crate::Machine,
    /// 那台目录的 `self`（没拉成 ⇒ `null`）。
    pub peer: Option<String>,
    /// 本机目录因这一趟变了没有。
    pub changed: bool,
    /// 推过去几台快照。
    pub pushed: usize,
    /// 那一趟哪里没办成（`null` ＝ 全办成了）。
    pub error: Option<String>,
}

/// 「连这台」的那几台的一行：`machine` = 对面的资产目录 id，没拉过 ⇒ `null`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct ReachRow {
    /// 那台的名字。
    pub origin: crate::Machine,
    /// 对面的资产目录 id。
    pub machine: Option<String>,
}

/// `assets-sync` 的回包。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AssetsSynced {
    /// 本机目录的 id（开头那一次现扫拿到的）—— 界面据它把目录里本机那一格对回 `<local>`。
    #[serde(rename = "self")]
    #[cfg_attr(any(test, feature = "ts"), ts(rename = "self"))]
    pub self_id: String,
    pub synced: Vec<AssetsSyncRow>,
    /// 「连这台」的那几台 `[{origin, machine}]`：`machine` 是那台目录的 id（还没拉成过 ⇒ `null`）。
    pub reach: Vec<ReachRow>,
}

/// 条目种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "lowercase")]
pub enum ExtKind {
    Skill,
    Mcp,
    /// 插件（这一版只读：装 · 卸不经 cc-monitor）。
    Plugin,
}

impl ExtKind {
    /// 线上写法（与 serde 那一格同名）：条目键 · 备份槽名 · 摘要前缀要它。
    pub fn as_str(self) -> &'static str {
        match self {
            ExtKind::Skill => "skill",
            ExtKind::Mcp => "mcp",
            ExtKind::Plugin => "plugin",
        }
    }
}

/// 照线上写法排（目录 · 表的行序跟着它）。
impl Ord for ExtKind {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl PartialOrd for ExtKind {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// 缺 · `null` ⇒ 空（另一台的整份应答：旧版没有这一格）。
fn null_is_empty<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// 目录里的一条（盘上那一份与线上同一形）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "CatalogAsset",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub struct Asset {
    pub kind: ExtKind,
    pub name: String,
    /// 那台上的项目目录（项目级）；`None` = 用户级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub project: Option<String>,
    /// 它在那台上住哪（skill：那个目录；MCP：那份配置文件）。只给人看、给「在文件窗口里打开」用，不参与判定。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub dir: Option<String>,
    pub digest: String,
    /// 给人看的摘要（skill：description / 文件数 / 字节数；MCP：去掉密钥值之后的定义；插件：市场 · 版本 · 说明 · 组成 · 开关出处）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "unknown"))]
    pub summary: Value,
    /// 装了、关着（只有插件有这一态；缺 ＝ 开着）。
    #[serde(default, skip_serializing_if = "is_false")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub off: bool,
}

/// 一条备注。`rev` 是写它那一刻这台目录里同一条目各台备注的最大 `rev` ＋ 1（不比墙钟）；`text` 空 = 清掉了。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "CatalogNote",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub struct Note {
    pub text: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub rev: u64,
}

/// 别处那一条（[`CatalogRow::from`]）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct CatalogFrom {
    /// 那台目录的 id。
    pub machine: String,
    /// 那台上的项目目录；用户级 ⇒ `null`。
    pub project: Option<String>,
    pub digest: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "unknown"))]
    pub summary: Value,
}

/// 「别的机器有的，这台怎样」那一行：`state` 闭集 `missing` · `differs` · `same`（判定在后端 `asset_catalog::rows`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct CatalogRow {
    pub kind: ExtKind,
    pub name: String,
    pub state: String,
    pub from: Vec<CatalogFrom>,
}

/// 目录线上形里的一台（两条命令共用；`assets-catalog-merge` 收的 `catalog.machines` 也是这个形状）。
/// 收的那一份是另一台后端的应答：多出来的键照收（对面可能更新）；`projectDirs` · `notes` · `sharedMcp` 缺或 `null` ⇒ 空。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "camelCase")]
pub struct CatalogMachine {
    pub id: String,
    pub label: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub gen: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub seen_at: u64,
    pub assets: Vec<Asset>,
    #[serde(default, deserialize_with = "null_is_empty")]
    pub project_dirs: Vec<String>,
    #[serde(default, deserialize_with = "null_is_empty")]
    pub notes: BTreeMap<String, Note>,
    #[serde(default, deserialize_with = "null_is_empty")]
    pub shared_mcp: bool,
}

/// `assets-catalog` / `assets-catalog-merge` 的回包：整份目录 ＋「这台缺什么」。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct CatalogReply {
    /// 这台机器的 id。
    #[serde(rename = "self")]
    #[cfg_attr(any(test, feature = "ts"), ts(rename = "self"))]
    pub self_id: String,
    pub machines: Vec<CatalogMachine>,
    pub rows: Vec<CatalogRow>,
    /// 这一趟扫描读不出来的那几份（一句话一份）。
    pub problems: Vec<String>,
    /// 这一趟目录有没有变（变了才写盘）。
    pub changed: bool,
    /// 目录文件在这台上的路径。
    pub path: Option<String>,
}

/// 另一台的整份目录（`assets-catalog-merge` 的 `catalog`）：只读 `machines`，别的键照收。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct CatalogIn {
    pub machines: Vec<CatalogMachine>,
}

/// `assets-catalog` 的入参：不收参数（缺 · `null` · `{}` 都认，多一个键 ⇒ `bad_args`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct AssetsCatalogReq {}

/// `assets-catalog-merge` 的入参：另一台后端的整份目录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct AssetsCatalogMergeReq {
    pub catalog: CatalogIn,
}
