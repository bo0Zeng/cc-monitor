//! 传输的线上类型：`transfer-copy` 那一条（一组从这一台搬到另一台；两台都可以是本机，编排都在本机后端）与手机内嵌核心分块传那两样回值。

use serde::{Deserialize, Serialize};

use crate::files::WirePath;

/// `transfer-copy`：一组路径从这一台搬到另一台的一个目录；两台都可以是本机（上传 · 下载 · 两台之间同这一条，编排都在本机后端）。
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
    /// 源那台的名字。
    pub from: crate::Machine,
    /// 落点那台的名字。
    pub to: crate::Machine,
    pub items: Vec<CopyFrom>,
    /// 落点那台上的目录（绝对路径；原始字节）。
    pub to_dir: WirePath,
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

/// 跨机复制的一项：源那台上的路径（原始字节）＋ 落到那边叫什么（另存为；缺 ⇒ 源的末段名）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct CopyFrom {
    pub path: WirePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub name: Option<WirePath>,
}

/// 手机内嵌核心分块上传那一半送完的那一份（`reply` 那一件成了的 `data`）：提交时照交（`chunks` · `bytes` · `expect.sha256`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct Staged {
    /// 暂存区里那一份的键。
    pub key: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub chunks: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub bytes: u64,
    /// 整份 SHA-256，小写十六进制。
    pub sha256: String,
}

/// 手机内嵌核心分块下载那一半读回的那一份（`reply` 那一件成了的 `data`）：写进本机几个字节。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct Pulled {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub bytes: u64,
}
