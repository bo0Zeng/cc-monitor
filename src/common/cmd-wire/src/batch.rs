//! 一组一条命令（复制 · 删除 · 改权限 · 下载 · 上传 · 跨机复制）共用的线上类型：同名怎么办 · 逐项结局 · 同名清单 · 进度 · 撤。
//!
//! 单个就是一组里的一项：没有一次一件的那一形。判法住后端一处（同台那三条在 `control/files_batch.rs`，传输那几条在 `control/transfer.rs`）。

use serde::{Deserialize, Serialize};

use crate::files::WirePath;

/// 目标那儿已经有同名的一项时怎么办（缺 ⇒ `ask`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum OnExists {
    /// 先把整组探一遍：有同名的 ⇒ 一项都不做，回码 `exists`、`data` 是同名清单（[`Conflicts`]）。
    #[default]
    Ask,
    /// 盖掉（只对文件；目录不合并 ⇒ 那一项跳过）。
    Overwrite,
    /// 跳过同名的那几项。
    Skip,
    /// 保留两份：新的那一份另起一个名（`名 (2).ext`，撞上往后试）。
    KeepBoth,
}

/// 冲突框逐项答的那一个（`decide` 的值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum Decide {
    Overwrite,
    Skip,
    KeepBoth,
}

/// 一项的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    /// 做完了。
    Done,
    /// 同名、照答的跳过了。
    Skipped,
    /// 探完之后才冒出来的同名：没盖，这一项没做。
    Exists,
    /// 没做成（`reason` 是原因词，原话进 `detail`）。
    Failed,
    /// 撤了：还没轮到它。
    Cancelled,
}

/// 一项的结局（`key` ＝ 这一项在 `items` 里的序号，十进制串）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct BatchOutcome {
    pub key: String,
    pub state: ItemState,
    /// 落在哪（做完的那几项；保留两份时是另起的那个名）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub path: Option<WirePath>,
    /// 保留两份时另起的那个名（只一段）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub new_name: Option<WirePath>,
    /// 这一项落了几个字节（复制 · 传输）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub bytes: Option<u64>,
    /// 这一项复制了几份文件 · 几个目录（复制：目录那一项是整棵的数）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub files: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub dirs: Option<u64>,
    /// 这一项删了几条（删除：整棵树按条数；撤在半路时是已删的那几条）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub removed: Option<u64>,
    /// 改权限之前的低 12 位（撤销 ＝ 拿它再改一次；读不到就缺席）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub before: Option<u32>,
    /// 没做成的那一项的码（同单条命令的码表：`refused` · `io_failed` · `stale` · `no_unix_mode` · `exists` …）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub code: Option<String>,
    /// 原因词（闭集，同界面「失败 · X」那一格）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub reason: Option<String>,
    /// 复制详情（下层那一句 ＋ 原话），不上界面那一行。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub detail: Option<String>,
}

impl BatchOutcome {
    /// 只有键与结局的那一份（别的格缺席）。
    pub fn bare(key: impl Into<String>, state: ItemState) -> BatchOutcome {
        BatchOutcome {
            key: key.into(),
            state,
            path: None,
            new_name: None,
            bytes: None,
            files: None,
            dirs: None,
            removed: None,
            before: None,
            code: None,
            reason: None,
            detail: None,
        }
    }
}

/// 一组的回包：逐项结局 ＋ 后端拼好的那一行（界面原样画）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct BatchReply {
    pub outcomes: Vec<BatchOutcome>,
    pub said: String,
}

/// 同名的那一项（码 `exists` 的 `data`：`{conflicts: [..]}`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct Conflict {
    /// 同 [`BatchOutcome::key`]。
    pub key: String,
    /// 落点那一段名字。
    pub name: WirePath,
    /// 要放过去的那一项是什么。
    pub kind: crate::files::FileKind,
    /// 要放过去的那一项的大小（文件）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub size: Option<u64>,
    /// 要放过去的那一项的修改时刻（Unix 秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub mtime: Option<i64>,
    /// 已在那儿的那一项是什么。
    pub there: crate::files::FileKind,
    /// 已在那儿的那一项的大小（文件）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub there_size: Option<u64>,
    /// 已在那儿的那一项的修改时刻（Unix 秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub there_mtime: Option<i64>,
    /// 两头有一头是目录：目录不合并，只能跳过或保留两份。
    pub dir_into: bool,
}

/// 码 `exists` 的 `data`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct Conflicts {
    pub conflicts: Vec<Conflict>,
}

/// `files-batch-stop`：撤一组（做完手上那一项就停，剩下的回 `cancelled`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesBatchStopReq {
    pub ticket: String,
}

/// `files-batch-stop` 的回包：`stopping` ＝ 那一组此刻还在做、撤的旗已经立上（`false` ＝ 没在做：已做完或没见过这张票）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesBatchStopReply {
    pub stopping: bool,
}

impl crate::files::Call for FilesBatchStopReq {
    const NAME: &'static str = "files-batch-stop";
    type Reply = FilesBatchStopReply;
}
