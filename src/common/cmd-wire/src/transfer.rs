//! 传输台开单那两条（`transfer-upload` · `transfer-download`）的线上类型。
//!
//! 入参是两层：文件窗口经通道说给 monitor 的那一份（`*Ask`），monitor 补上那台远端的拨号请求（`dial`）
//! 再转给本机常驻后端的那一份（`*Req`）。两层各是一个类型，多一个键都拒；回包两层同一份。
//! 起跑 · 撤（`transfer-start` / `-stop`）不经窗口（订阅即起跑、停订即撤），不在这里。

use serde::{Deserialize, Serialize};

use crate::files::{Call, WirePath};

/// 上传的一项：本机一份普通文件的路径。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct UploadItem {
    pub local_path: String,
}

/// 下载的一项：远端路径 ＋ 本机落名（缺 ⇒ 远端那一段名字；名字不是 UTF-8 时走原始字节）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct DownloadItem {
    pub remote_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub name: Option<WirePath>,
}

/// 窗口说的 `transfer-upload`：一组本机文件放进远端一个目录 ＋ 问得到的那台后端的 `$HOME`（传输台连上之后与 SFTP 起始目录比，不一致 ⇒ 一个字节不写）；
/// 同名照 `on_exists`（缺 ⇒ `ask`：先在远端落点探整组，有同名一单不开、回 `exists` 带同名清单）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TransferUploadAsk {
    pub items: Vec<UploadItem>,
    /// 远端落点目录（绝对路径；不是 UTF-8 时走原始字节）：同名在这一处探。
    pub remote_dir: WirePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub home: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub on_exists: Option<crate::batch::OnExists>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub decide: Option<std::collections::BTreeMap<String, crate::batch::Decide>>,
}

/// 窗口说的 `transfer-download`：一组远端路径落到本机一个目录；同名照 `on_exists`（缺 ⇒ `ask`：本机落点那一侧先探，有同名一单不开、回 `exists` 带同名清单）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TransferDownloadAsk {
    pub items: Vec<DownloadItem>,
    /// 本机落点目录（绝对路径；不是 UTF-8 时走原始字节）。
    pub local_dir: WirePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub on_exists: Option<crate::batch::OnExists>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub decide: Option<std::collections::BTreeMap<String, crate::batch::Decide>>,
}

/// monitor 转给本机后端的 `transfer-upload`：窗口那一份 ＋ 那台远端的拨号请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TransferUploadReq {
    /// 拨号请求（同 `link-open` 的 `dial`）：后端拨号核心读它。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "Record<string, unknown>"))]
    pub dial: serde_json::Value,
    pub items: Vec<UploadItem>,
    pub remote_dir: WirePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub home: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub on_exists: Option<crate::batch::OnExists>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub decide: Option<std::collections::BTreeMap<String, crate::batch::Decide>>,
}

/// monitor 转给本机后端的 `transfer-download`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TransferDownloadReq {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "Record<string, unknown>"))]
    pub dial: serde_json::Value,
    pub items: Vec<DownloadItem>,
    pub local_dir: WirePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub on_exists: Option<crate::batch::OnExists>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub decide: Option<std::collections::BTreeMap<String, crate::batch::Decide>>,
}

/// 开单那一项怎样了：开了单 · 同名照答跳过 · 没开成。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum OpenState {
    Opened,
    Skipped,
    Failed,
}

/// 开单的一项（`key` ＝ 它在 `items` 里的序号）。开了单的那一项订 `transfer/<id>` 即起跑，进度与终局照 `transfer` 帧。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct OpenedItem {
    pub key: String,
    pub state: OpenState,
    /// 传输单 id（`xfer-<n>`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub id: Option<String>,
    /// 上传：暂存件的键（32 位十六进制）：同一份文件重拖同一个键 ⇒ 续传；提交时交给远端后端。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub staged: Option<String>,
    /// 下载：本机落点。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub path: Option<WirePath>,
    /// 保留两份时另起的那个名（下载：本机落名；上传：提交时远端那一段名字）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub new_name: Option<WirePath>,
    /// 上传：提交时盖掉远端已有的同名（同名照答覆盖的那几项）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub overwrite: bool,
    /// 没开成的那一项的码 · 原因词 · 复制详情（同一组那几条的结局）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub detail: Option<String>,
}

/// 开单的回包：逐项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TransferOpened {
    pub items: Vec<OpenedItem>,
}

/// `transfer-copy`：一组远端路径从这一台复制到另一台的一个目录（经本机中转：A → 本机暂存 → B 暂存 → B 提交）。
/// 目录整棵：建目录、逐个文件传，逐项结局照实回（目录里的文件键是 `序号/相对段`）。同名照 `on_exists`（目录同名只许跳过或保留两份）。
/// 带票 ⇒ 进度走 `files_batch` 帧，`files-batch-stop` 撤；断开照常做完，同一张票再问回同一份结局。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TransferCopyReq {
    /// 源那台的拨号请求。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "Record<string, unknown>"))]
    pub from: serde_json::Value,
    /// 落点那台的拨号请求。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "Record<string, unknown>"))]
    pub to: serde_json::Value,
    pub items: Vec<CopyFrom>,
    /// 落点那台上的目录（绝对路径）。
    pub to_dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub on_exists: Option<crate::batch::OnExists>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub decide: Option<std::collections::BTreeMap<String, crate::batch::Decide>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub ticket: Option<String>,
}

/// 窗口说的 `transfer-copy`：源那台 ＝ 寻址的那台（窗口看着的这一台），`to` ＝ 落点那台的名字；两台的拨号请求由 monitor 补上
/// （[`TransferCopyAsk::dialed`]），其余各格原样转给本机后端（同 [`TransferCopyReq`]）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TransferCopyAsk {
    /// 落点那台的名字（同窗口寻址用的那个名字）。
    pub to: String,
    pub items: Vec<CopyFrom>,
    pub to_dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub on_exists: Option<crate::batch::OnExists>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub decide: Option<std::collections::BTreeMap<String, crate::batch::Decide>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub ticket: Option<String>,
}

impl TransferCopyAsk {
    /// 补上两台的拨号请求 ⇒ 转给本机后端的那一份。
    pub fn dialed(self, from: serde_json::Value, to: serde_json::Value) -> TransferCopyReq {
        TransferCopyReq {
            from,
            to,
            items: self.items,
            to_dir: self.to_dir,
            on_exists: self.on_exists,
            decide: self.decide,
            ticket: self.ticket,
        }
    }
}

impl Call for TransferCopyAsk {
    const NAME: &'static str = "transfer-copy";
    type Reply = crate::batch::BatchReply;
}

/// 跨机复制的一项：源那台上的路径。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct CopyFrom {
    pub remote_path: String,
}

impl TransferUploadAsk {
    /// 补上拨号请求（monitor 转给本机后端的那一份）。
    pub fn dialed(self, dial: serde_json::Value) -> TransferUploadReq {
        TransferUploadReq {
            dial,
            items: self.items,
            remote_dir: self.remote_dir,
            home: self.home,
            on_exists: self.on_exists,
            decide: self.decide,
        }
    }
}

impl TransferDownloadAsk {
    /// 补上拨号请求（同 [`TransferUploadAsk::dialed`]）。
    pub fn dialed(self, dial: serde_json::Value) -> TransferDownloadReq {
        TransferDownloadReq {
            dial,
            items: self.items,
            local_dir: self.local_dir,
            on_exists: self.on_exists,
            decide: self.decide,
        }
    }
}

impl Call for TransferUploadAsk {
    const NAME: &'static str = "transfer-upload";
    type Reply = TransferOpened;
}
impl Call for TransferUploadReq {
    const NAME: &'static str = "transfer-upload";
    type Reply = TransferOpened;
}
impl Call for TransferDownloadAsk {
    const NAME: &'static str = "transfer-download";
    type Reply = TransferOpened;
}
impl Call for TransferDownloadReq {
    const NAME: &'static str = "transfer-download";
    type Reply = TransferOpened;
}
impl Call for TransferCopyReq {
    const NAME: &'static str = "transfer-copy";
    type Reply = crate::batch::BatchReply;
}
