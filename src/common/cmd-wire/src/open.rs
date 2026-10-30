//! 起会话那两条帧命令的**入参**（`session-open` · `session-open-facts`）—— 两侧同一份：后端照这里的类型收，
//! 有 Rust 客户端的那几端照这里的类型发；回包住 [`crate::session`]（[`crate::session::Opened`] · [`crate::session::OpenFacts`]），
//! 名字与回包挂在 [`crate::Call`] 上。判定（用哪个号 · 放在哪 · 起不起）全在后端 `control/session_open`。

use crate::session::Launch;
use serde::{Deserialize, Serialize};

/// `session-open` 的入参（`deny_unknown_fields`：多送一格 ⇒ 拒）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct SessionOpenReq {
    /// 起新的 · 恢复某条 · 接回 · 开一个 shell。
    pub what: OpenWhat,
    /// 缺 ＝ 跟随：恢复 ⇒ 这条会话上次的号；新起 ⇒ 这台的默认号；分叉 ⇒ 源会话的号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub account: Option<AccountAsk>,
    /// 放在哪；缺 ＝ 这台定（新起：有 tmux 就 tmux、分叉照源会话那一种；恢复：照这台的偏好）。接回不收。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub run: Option<Launch>,
    /// 开窗的那台就是这台（本机 Windows 渲 PowerShell；本机直路不带工作目录）。
    pub local: bool,
    /// 这一趟的票（每次点一下一张；期限到了带同一张再问 ⇒ 认出同一趟，不起第二个）。缺 ⇒ 不认。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub ticket: Option<String>,
    /// 自报的前端（就地键入过身份门，同 `kill`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub client: Option<String>,
}

/// 意图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum OpenWhat {
    /// 起一个新会话（可分叉、可带第一句）。
    New(NewOpen),
    /// 恢复某条：哪一家 · 目录 · 模型 · 权限档这台从记录里读。
    Resume { sid: crate::Sid },
    /// 接回 / 看一眼，不起 agent：按会话（找挂着它的终端：tmux 会话 · 托管终端）或按终端名，两格给一格。
    Look {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        sid: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        session: Option<String>,
    },
    /// 开一个 shell，不起 agent：给了 `session` ⇒ 这台 tmux 里那个会话（不在就在 `cwd` 里建一个空的）；
    /// 只给 `cwd` ⇒ 这台 shell 里进那个目录那一行。`cwd` 只在建的那一刻生效（已在的会话不往里敲 `cd`）。
    Shell {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        session: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        cwd: Option<String>,
    },
}

/// 新起那一形的几格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct NewOpen {
    /// 哪一家（线上的 kind）。必填。
    pub agent: crate::AgentKind,
    /// 工作目录（开头的 `~` 按这台的家目录读）。
    pub cwd: String,
    /// 用户改过的终端名（只跟 tmux）；缺 ⇒ 这台铸（分叉不收：名字从源会话的终端名铸）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub tmux_name: Option<String>,
    /// 这一趟手填的启动命令（空 / 缺 ⇒ 这台偏好里那一家的启动命令）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub command: Option<String>,
    /// 分叉自哪一条（只在这一步写分支记录）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub fork: Option<ForkFrom>,
    /// 第一句：起的时候交给那一家；只跟 tmux。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub prompt: Option<String>,
    /// 这一趟用的模型（压过偏好表）；缺 ＝ 跟随。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub model: Option<String>,
    /// 起在哪一个权限档（那一家声明的档之一）；缺 ＝ 这台偏好里那一家的那一档，没设 ⇒ 那一家自己的默认。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub permission_mode: Option<String>,
    /// 轮换：缺 ＝ 用「新会话用」的那份策略 · `{strategy: id}` ＝ 起之前按定好的 sid 写成用那一份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub rotation: Option<RotationPick>,
    /// 经哪条别名起（一类 ＝ 一条别名：那一行 `ccm @别名`，别名合下来的几格在 ccm 那一侧合、这一趟给的盖过它）；缺 ⇒ 不经别名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub alias: Option<String>,
}

/// 分叉自：源会话 ＋ 从哪条消息处。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ForkFrom {
    /// 源会话 id。
    pub sid: String,
    /// 从哪条消息处分叉。
    pub uuid: String,
}

/// 轮换：用哪一份策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct RotationPick {
    /// 策略的 id（策略目录里的文件名去掉 `.rhai`，非空）；`null` ⇒ 这个会话不轮换（新会话用的那份不管它）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string | null"))]
    pub strategy: Option<String>,
}

/// 线上 `account` 那一格（三条起会话请求同一份）：跟随 · 账号 0 · 用户点名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub enum AccountAsk {
    /// 这条会话上次用的号 → 这台的默认号 → 不指定（新起的会话没有上次的号）。
    Follow,
    Base,
    /// 用户点名的那个号（按名字判；分叉沿用源会话的号也是名字，由这台从源会话推出）。
    Named {
        name: String,
    },
}

/// `session-open-facts` 的入参。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct OpenFactsReq {
    /// 可缺：分叉源会话 sid。
    #[serde(default, rename = "forkOf", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub fork_of: Option<String>,
    /// 可缺（随 `forkOf`）：从哪条消息处分叉（框顶说第几轮、几点）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub at: Option<String>,
}
