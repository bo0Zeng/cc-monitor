//! 聊天里随一句发的附件：`session-attachment-check` · `session-attachment-commit` · `session-attachment-drop` 三条的入参与回包，
//! 和记录里认出来的那一项（[`Attachment`]，随 `said` 记录带出）。两端同一份：后端按这里的类型回，界面用导出的类型，手机按生成的类解。
//! 落点在会话那台的形状（`.ccm-attach/<编号>/<原名>`）住后端（`common/attachment_dir.rs`），线上只过编号与那台写好的路径。

use serde::{Deserialize, Serialize};

use crate::files::{FileClass, Sha256Expect, Sha256Hex};

/// 开传前问一次的一份：名字 · 大小 · 是不是目录（本机那一份的事实）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct AttachItem {
    pub name: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub bytes: u64,
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub dir: bool,
}

/// `session-attachment-check`：这一条草稿里的全部附件（已传完的也列进来，按草稿里的先后），开传前问那台收不收。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct SessionAttachmentCheckReq {
    pub sid: crate::Sid,
    pub items: Vec<AttachItem>,
}

/// 一份不收的原因（闭集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum AttachRefused {
    /// 是目录（只收文件）。
    IsDir,
    /// 这一份超过单份上限。
    TooBig,
    /// 这一条里排在它前面的已经够数了。
    TooMany,
    /// 这一条加上它超过一条的总量上限。
    TooMuch,
    /// 会话那台的附件目录装不下了。
    DirFull,
    /// 会话的工作目录不可写（不回落别处）。
    NotWritable,
    /// 那台找不到这条会话的工作目录。
    NoSession,
}

/// 一份的答：收 ⇒ `ok: true`；不收 ⇒ 原因 ＋ 那台写好的一句。收不收都带它是哪一种（chip 上的类别字）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AttachVerdict {
    pub ok: bool,
    /// 照名字判的那一种（目录 ⇒ 文件夹；[`crate::file_class::class_of_name`]）。
    pub class: FileClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub why: Option<AttachRefused>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub said: Option<String>,
}

/// `session-attachment-check` 的回包：逐份，与入参同序同数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct SessionAttachmentCheckReply {
    pub items: Vec<AttachVerdict>,
}

/// `session-attachment-commit`：暂存区里传完的那一份（`<key>.part`；给了 `chunks` ⇒ 先把 `<key>.<seq>.chunk` 拼起来）落进会话工作目录下一个新的附件目录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct SessionAttachmentCommitReq {
    pub sid: crate::Sid,
    pub key: String,
    /// 原名（一段，过那台的名字规则）。
    pub name: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub chunks: Option<u64>,
    pub expect: Sha256Expect,
}

/// 落好的那一份：编号（送那一句时只交它）· 原名 · 那台的绝对路径 · 相对工作目录的路径 · 大小 · 摘要 · 哪一种。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct SessionAttachmentCommitReply {
    pub id: String,
    pub name: String,
    pub path: String,
    pub rel: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub bytes: u64,
    pub sha256: Sha256Hex,
    pub class: FileClass,
}

/// `session-attachment-drop`：撤一份（草稿上 ✕ 掉）：那台删掉那个附件目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct SessionAttachmentDropReq {
    pub sid: crate::Sid,
    pub id: String,
}

/// `session-attachment-drop` 的回包：删了 ⇒ `true`；本来就不在 ⇒ `false`（不是错）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct SessionAttachmentDropReply {
    pub removed: bool,
}

/// 记录里认出来的一份附件（随 `said` 记录带出；正文里那一行引用已去掉）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Attachment {
    pub name: String,
    /// 相对会话工作目录的路径。
    pub rel: String,
    /// 哪一种（照名字判，[`crate::file_class::class_of_name`]）：chip 上的类别字。
    pub class: FileClass,
    /// 那一份此刻多大；不在了 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub bytes: Option<u64>,
    /// 大小写给人看的那一格（那台写好）；不在了 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub size_text: Option<String>,
    /// 那台已清掉（过了七天 · 草稿上撤过）⇒ `true`：画「已清理」、不可点。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub gone: bool,
}
