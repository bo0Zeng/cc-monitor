//! 聊天里随一句发的附件：`session-attachment-check` · `session-attachment-commit` · `session-attachment-drop` 三条的入参与回包，
//! 和记录里认出来的那一项（[`Attachment`]，随 `said` 记录带出）。两端同一份：后端按这里的类型回，界面用导出的类型，手机按生成的类解。
//! 输入框上方那一排附件那几形也住这里：`chunk_core::attach` 照此刻写的 [`AttachWant`] ⇒ `face_core::attach` 写好的 [`AttachFace`]；手机核心与 Kotlin 之间那一口的形住 `phone`。
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
    /// 照名字判的那一种（目录 ⇒ 文件夹；后端 `common/file_class.rs` 的 `class_of_name`）。
    pub class: FileClass,
    /// 那一种的词（那台写好，同文件管理「类型」那一列的词）：chip 上的类别字。
    pub class_text: String,
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
    /// 哪一种（照名字判，后端 `common/file_class.rs` 的 `class_of_name`）。
    pub class: FileClass,
    /// 那一种的词（那台写好）：chip 上的类别字。
    pub class_text: String,
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

// ───── 输入框上方那一排附件：`chunk_core::attach::Tray` 照此刻写成 [`AttachWant`]，`face_core::attach::face` 写成 [`AttachFace`] ─────

/// 交写字那一侧的那一份：每张卡走到哪 ＋ 断着时那台离线那一句 ＋ 手机这边的错。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct AttachWant {
    pub cards: Vec<AttachWantCard>,
    /// 那台离线那一句：只在有排队的、又断着时给。
    pub offline: Option<String>,
    /// 手机这边的错（那一条最先说它）。
    pub local: Option<AttachLocal>,
}

/// 一张卡：名字 ＋ 走到哪。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct AttachWantCard {
    pub name: String,
    pub phase: AttachCardPhase,
}

/// 一张卡走到哪（传输中不带进度数字：卡上只转环）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttachCardPhase {
    /// 还没轮到 · 断着等连上。
    Queued,
    Moving,
    Landed,
    /// 开传前那台就不收：那台写好的那一句（写在卡上）。
    Refused {
        said: String,
    },
    /// 传 · 落没成：那台那一句（进卡下那一条）。
    Failed {
        said: String,
    },
    /// 本机那一份副本开不了。
    Lost,
}

/// 手机这边的错。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum AttachLocal {
    /// 拷不进本机（空间满 …）。
    NoRoom,
    /// 没有能拍照的系统相机。
    NoCamera,
}

/// 写好的那一排的字：每张卡（与交进来的同序）＋ 卡下那一条。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AttachFace {
    pub cards: Vec<AttachFaceCard>,
    pub note: Option<AttachFaceNote>,
}

/// 一张卡上的字：第二行（传输中 · 落好了 ⇒ 不写）＋ ✕ 的读屏名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AttachFaceCard {
    pub line: Option<String>,
    pub remove: String,
}

/// 卡下那一条：那一句 ＋ 按钮（没有 ⇒ `None`）＋ 是不是断着那一档（虚线）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AttachFaceNote {
    pub text: String,
    pub act: Option<AttachAct>,
    pub dashed: bool,
}

/// 卡下那一条上的按钮。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum AttachAct {
    /// 那台的错：重传失败的那几份。
    Retry,
    /// 手机这边存不下：再挑一份。
    Repick,
}

// ───── 桌面那一排：本机后端替界面编排（`backend/control/attach_upload.rs`），界面只交草稿名单、照推来的那一排画 ─────

/// 加进草稿的一份：键（界面编，这一排里不重）· 名字 · 字节 · 壳放进本机暂存区的那一份的路径 · 是不是目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct AttachDraftCard {
    pub key: String,
    pub name: String,
    /// 壳写好的那一份（本机暂存区 `~/.cc-monitor/staging/<32 位十六进制>.part`；别处 ⇒ 落「本机副本开不了」）。目录 ⇒ 空。
    pub path: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub bytes: u64,
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub dir: bool,
}

/// `attach-draft-put`：往 `draft` 那一排加几份（那一排还没有 ⇒ 照 `machine` · `sid` 开一排）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct AttachDraftPutReq {
    /// 草稿号（界面编，一扇窗一条会话一个）。
    pub draft: String,
    /// 会话所在那台（本机 ⇒ `<local>`）。
    pub machine: crate::Machine,
    pub sid: crate::Sid,
    pub cards: Vec<AttachDraftCard>,
}

/// `attach-draft-drop`：✕ 一份（在传 ⇒ 撤那一趟；落好了 ⇒ 那台删掉那个编号）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct AttachDraftDropReq {
    pub draft: String,
    pub key: String,
}

/// `attach-draft-retry`：［重试］—— 失败 · 本机副本开不了的回排队，断着的也再试一次。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct AttachDraftRetryReq {
    pub draft: String,
}

/// `attach-draft-take`：发出去那一下要编号。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct AttachDraftTakeReq {
    pub draft: String,
}

/// `attach-draft-take` 的回包：全落好了 ⇒ 编号（按先后），那一排清空；还有没落好的 ⇒ `null`（那一排不动）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AttachDraftTaken {
    pub ids: Option<Vec<String>>,
}

/// `attach-draft-view`：此刻那一排（推的那一帧丢了 · 没带体时重问）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct AttachDraftViewReq {
    pub draft: String,
}

/// 本机后端写好的那一排（`changed {attach}` 的体 · put / drop / retry / view 的回包）：
/// 同手机核心交 Kotlin 的那一排（`phone::AttachShown`），`tray` 换成草稿号，每张卡多那一种与它的词。没有这一排 ⇒ 没有卡、`ready`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AttachDraftShown {
    pub draft: String,
    pub cards: Vec<AttachDraftShownCard>,
    pub note: Option<AttachFaceNote>,
    /// 断着、有排队的在等连上。
    pub offline: bool,
    /// 全落好了（没有卡也算）：➤ 才亮。
    pub ready: bool,
}

/// 一张卡：键 · 走到哪 · 已送字节 · 落好的编号 · 写好的字（第二行 · ✕ 的读屏名）· 那一种与它的词。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct AttachDraftShownCard {
    pub key: String,
    pub name: String,
    pub phase: AttachCardPhase,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub done: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub bytes: u64,
    pub id: Option<String>,
    pub line: Option<String>,
    pub remove: String,
    pub class: FileClass,
    pub class_text: String,
}
