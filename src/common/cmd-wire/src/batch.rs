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
    /// `size` 写好的字（`2.1 MB`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub size_text: Option<String>,
    /// 要放过去的那一项的修改时刻，后端按看的那一台的钟写好（当天 `14:02` · 当年 `10-12 14:02` · 别的年带年份）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub mtime_text: Option<String>,
    /// 已在那儿的那一项是什么。
    pub there: crate::files::FileKind,
    /// 已在那儿的那一项的大小（文件）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub there_size: Option<u64>,
    /// `there_size` 写好的字。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub there_size_text: Option<String>,
    /// 已在那儿的那一项的修改时刻（同 [`Conflict::mtime_text`] 的写法）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub there_mtime_text: Option<String>,
    /// 这一项能怎么答（后端定，界面照画、不另判）：两头有一头是目录 ⇒ 没有覆盖（目录不合并）。
    pub answers: Vec<Decide>,
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

/// 一批会话要做的那一样（`stop`；`start` 带放在哪；隐藏那三样 · 固定那两样写会话那一台的注解）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum SessionsOp {
    /// 停：结束带着它的那个终端。
    Stop,
    /// 起：恢复到 `run` 那一种宿主里。
    Start {
        run: crate::session::Launch,
        /// 别的台转来的这一组（本机合各台转出去时写，界面不写）：开窗的那台不是这台。缺 ⇒ `false`（开窗的就是这台）。
        #[serde(default)]
        #[cfg_attr(any(test, feature = "ts"), ts(skip))]
        relayed: bool,
    },
    /// 隐藏：单个那一格写 `on`。
    Hide,
    /// 取消隐藏：单个那一格写 `off`（规则隐藏的一样，之后不再按那一条隐藏）。
    Unhide,
    /// 摘掉单个那一格、回到规则（撤销「隐藏」时原本没有那一格的那几项）。
    ClearHide,
    /// 固定（会话那一台的注解；已固定的那台判跳过）。
    Pin,
    /// 取消固定（没固定的那台判跳过）。
    Unpin,
}

/// 一批里的一项：哪台的哪条会话。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct SessionItem {
    pub machine: crate::Machine,
    pub sid: crate::Sid,
}

/// 一批会话一次问的请求：只有 id 与刚选的那一样（跳不跳过由各台照会话脸判）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct SessionsBatchReq {
    pub op: SessionsOp,
    /// 那几项（1–64 个，照选中的先后）。
    pub items: Vec<SessionItem>,
    /// 自报的前端（「哪个前端的会话」那一维，同 `terminal-end`）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub client: Option<String>,
}

/// 一条会话的结局：做成了 · 不用做 / 做不了 · 做了没成。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Done,
    Skipped,
    Failed,
}

/// 一条会话的结局上线那一形：每台那一跳与本机合好的回包同一形。可缺的格一律出 `null`，不省键。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionOutcome {
    /// 哪台（每台那一跳答的是它自己 ⇒ 本机那一个；本机合好时换成那一项的那台）。
    pub machine: crate::Machine,
    pub sid: crate::Sid,
    pub outcome: SessionState,
    /// 跳过 / 失败的原因码（挑按钮用）；做成了 ⇒ `null`。
    pub why: Option<String>,
    /// 那台写好的那一句；做成了 ⇒ `null`。
    pub said: Option<String>,
    /// 失败那一项的复制详情（码 ＋ 原话；别的 ⇒ 空串）。
    pub copy_detail: String,
    /// 那台写的称呼（项目 ＋ 名字）。
    pub title: String,
    /// 落在哪个终端会话上。
    pub session: Option<String>,
}

/// 一批会话合好的回包：逐条结局与入参同序，汇总句 · 逐条行 · 复制详情都写好（界面只摆）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionsBatchReply {
    pub results: Vec<SessionOutcome>,
    /// 一句汇总（同文件批量那一句）。
    pub said: String,
    /// 有没成的 ⇒ 出错的样子。
    pub failed: bool,
    /// 没成 · 跳过的逐条（写好的字）。
    pub lines: Vec<String>,
    /// 没成的那几段（都成了 ⇒ 空串）。
    pub copy_detail: String,
    /// 起那一批里落在窗口里的那几项（照原序；别的批 ⇒ 空）：开窗是这台电脑上的事，界面照这几行开。
    pub windows: Vec<OpenWindow>,
    /// 撤销交的那几问（撤得回的那一样里做成了的那几项，后端照各自改之前那一格分好；原样交回本机）；撤不回 ⇒ 空。
    pub undo: Vec<SessionsBatchReq>,
}

/// 起那一批里落在窗口里的一项：哪台的哪条 ＋ 那台渲好的那一行与起始目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenWindow {
    pub machine: crate::Machine,
    pub sid: crate::Sid,
    /// 开终端跑的那一行。
    pub line: String,
    /// 起在哪个目录（那台展开过的；没有 ⇒ `null`）。
    pub cwd: Option<String>,
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/batch_tests.rs"]
mod tests;
