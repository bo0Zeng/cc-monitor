//! 传输台开单那两条（`transfer-upload` · `transfer-download`）的线上类型。
//!
//! 入参是两层：文件窗口经通道说给 monitor 的那一份（`*Ask`），monitor 补上那台远端的拨号请求（`dial`）
//! 再转给本机常驻后端的那一份（`*Req`）。两层各是一个类型，多一个键都拒；回包两层同一份。
//! 起跑 · 撤（`transfer-start` / `-stop`）不经窗口（订阅即起跑、停订即撤），不在这里。

use serde::{Deserialize, Serialize};

use crate::files::{Call, WirePath};

/// 窗口说的 `transfer-upload`：本机那一份文件 ＋ 问得到的那台后端的 `$HOME`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TransferUploadAsk {
    /// 本机一份普通文件的路径。
    pub local_path: String,
    /// 那台后端的 `$HOME`（问得到才带）：传输台连上之后与 SFTP 起始目录比，不一致 ⇒ 一个字节不写。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub home: Option<String>,
}

/// 窗口说的 `transfer-download`：远端路径 ＋ 本机落点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TransferDownloadAsk {
    /// 远端路径。
    pub remote_path: String,
    /// 本机落点（绝对路径；名字不是 UTF-8 时走原始字节，Linux 上按原字节落名）。
    pub local_path: WirePath,
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
    /// 本机一份普通文件的路径。
    pub local_path: String,
    /// 那台后端的 `$HOME`（同 [`TransferUploadAsk::home`]）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub home: Option<String>,
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
    /// 拨号请求（同 [`TransferUploadReq::dial`]）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "Record<string, unknown>"))]
    pub dial: serde_json::Value,
    /// 远端路径。
    pub remote_path: String,
    /// 本机落点（同 [`TransferDownloadAsk::local_path`]）：开单时当场过路径解析，起跑时再过一次。
    pub local_path: WirePath,
}

/// 上传开单的回包。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TransferUploaded {
    /// 传输单 id（`xfer-<n>`）：订阅 `transfer/<id>` 即起跑。
    pub id: String,
    /// 暂存件的键（32 位十六进制）：同一份文件重拖同一个键 ⇒ 续传；提交时交给远端后端。
    pub key: String,
}

/// 下载开单的回包。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TransferOpened {
    /// 传输单 id：订阅 `transfer/<id>` 即起跑。
    pub id: String,
}

impl TransferUploadAsk {
    /// 补上拨号请求（monitor 转给本机后端的那一份）。
    pub fn dialed(self, dial: serde_json::Value) -> TransferUploadReq {
        TransferUploadReq {
            dial,
            local_path: self.local_path,
            home: self.home,
        }
    }
}

impl TransferDownloadAsk {
    /// 补上拨号请求（同 [`TransferUploadAsk::dialed`]）。
    pub fn dialed(self, dial: serde_json::Value) -> TransferDownloadReq {
        TransferDownloadReq {
            dial,
            remote_path: self.remote_path,
            local_path: self.local_path,
        }
    }
}

impl Call for TransferUploadAsk {
    const NAME: &'static str = "transfer-upload";
    type Reply = TransferUploaded;
}
impl Call for TransferUploadReq {
    const NAME: &'static str = "transfer-upload";
    type Reply = TransferUploaded;
}
impl Call for TransferDownloadAsk {
    const NAME: &'static str = "transfer-download";
    type Reply = TransferOpened;
}
impl Call for TransferDownloadReq {
    const NAME: &'static str = "transfer-download";
    type Reply = TransferOpened;
}
