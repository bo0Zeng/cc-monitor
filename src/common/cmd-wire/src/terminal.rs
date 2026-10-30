//! 终端那几条有 Rust 客户端的命令：给一台远端开终端（`terminal-ssh`，本机后端答；主界面 · 壳问同一条）·
//! 退订终端画面（`terminal-unfollow`，壳替界面发）。

use serde::{Deserialize, Serialize};

/// `terminal-ssh` 的入参：哪一台（名字；怎么拨照本机后端的机器表组）＋ 要在那台跑什么。
/// `command`（主界面交成品命令）与 `cwd`（文件窗口「在此打开终端」只交意图，命令由后端拼）**至多给一个**；都不给 ⇒ 那台的登录 shell（命令面板「打开终端」）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TerminalSshReq {
    /// 那台的名字。
    pub machine: crate::Machine,
    /// 要在那台跑的命令。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub command: Option<String>,
    /// 在那台的这个目录开登录 shell（串，或非 UTF-8 的 `{"b16": "<hex>"}`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(optional, type = "string | { b16: string }")
    )]
    pub cwd: Option<serde_json::Value>,
    /// 这个终端显示的那条会话（只跟 `command`）：给了 ⇒ 那一行前面接一段，跑它的那个 shell 把自己的窗口标签记进本机后端的家，↗ 先照它在本机找。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub sid: Option<crate::Sid>,
}

/// `terminal-unfollow` 的入参：退订那张终端画面票（monitor 替界面向那台退订）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct UnfollowReq {
    /// 退不在册的也回 `ok`（记下它：订阅那一问若还在路上，到了也不起）。
    pub ticket: String,
}

/// `terminal-ssh` 的回包：前奏 ＋ 那一行。开窗跑两段接起来的那一串；开不了窗时复制出去的只有 `line`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct SshCommand {
    /// 只给 cc-monitor 自己开的窗、接在 `line` 前面跑的一段（跑它的 shell 把窗口标签记进本机后端的家；里面是本机数据目录的
    /// 绝对路径与这台终端方言的写法）。入参没给 `sid` · 记录目录不在 ⇒ 空串。
    pub prelude: String,
    /// 这台电脑终端里跑的那一行（Windows：PowerShell `& '<ssh 全路径>' -t … -- 'bash -lic …'`；别处：同一组参数的 POSIX shell 一行）。
    pub line: String,
}

/// `terminals-list` 的入参。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TerminalsListReq {
    /// 自报的前端（1–32 个 `[a-z0-9-]`）：决定每行的 `can` 与 `started_by.mine`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub client: Option<String>,
}

/// `terminal-preview` 的入参。目标恰给一个：`terminal`（名单里的句柄）· `sid`（会话 id）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TerminalPreviewReq {
    /// 目标：名单里的句柄（与 `sid` 恰给一个）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub terminal: Option<String>,
    /// 目标：会话 id（与 `terminal` 恰给一个）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub sid: Option<String>,
    /// 要不要颜色；缺省 `true`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub color: Option<bool>,
    /// 往回多要几行；缺省 0、上限 2000（超了截到上限、回包 `capped: true`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub scrollback: Option<u64>,
}

/// `terminal-input` 的入参。目标同 [`TerminalPreviewReq`]；送什么恰给一个：`text`（＋ `enter`）· `key`。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TerminalInputReq {
    /// 目标：名单里的不透明句柄（前端不拼、不解析）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub terminal: Option<String>,
    /// 目标：会话 id（与 `terminal` 恰给一个）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub sid: Option<String>,
    /// 送字：字面字，原样送、不解释成键名；多行按粘贴送（与 `key` 恰给一个）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub text: Option<String>,
    /// `text` 之后补一个回车；缺省 `true`（只跟 `text`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub enter: Option<bool>,
    /// 送键：`esc` · `ctrl-c` · `ctrl-d` · `up` · `down` · `left` · `right` · `tab` · `shift-tab` · `enter` · `backspace` · `page-up` · `page-down`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub key: Option<String>,
    /// 送之前看到的那一屏的指纹；画面已经变了 ⇒ 不送、回 `refused` ＋ `screen_changed`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub seen_screen: Option<String>,
    /// 自报的前端（过「哪个前端的会话」那一维）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub client: Option<String>,
    /// `text` 之前各单独粘一次的那几段（投递带附件那一句时的图片绝对路径，由那台按附件编号写好）。不上线：入参里写了它 ⇒ `bad_args`（`deny_unknown_fields`）。
    #[serde(skip)]
    #[cfg_attr(any(test, feature = "ts"), ts(skip))]
    pub pastes: Vec<String>,
}
