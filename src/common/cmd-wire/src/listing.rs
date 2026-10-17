//! `history-list` 的回包线上形（并好 · 筛好 · 排好的成品）与 `history-listing` 那一形（一台自己的清单，按带键集合同步：`crate::sync`）
//! 连同它们引到的几样 —— 两侧同一份：后端按这里的类型写（扫 · 并 · 筛 · 排在后端 `history/`），手机嵌的 Rust 核心按这里的类型解。

use crate::cells::Words;
use crate::session::{Hidden, Hide, LaunchTab, Rules};
use crate::Machine;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// `history-list` 成品的头（行之外的那几格；总数在 `cmd_wire::sync::Head`）：按项目看时的分组 · 注解没并上的那一句。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct HistoryHead {
    pub groups: Vec<HistoryGroup>,
    /// 注解没并上的那一句；并上了 ⇒ `null`。
    pub notice: Option<String>,
}

/// 成品的一行。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct HistoryRow {
    pub agent: String,
    pub session_id: String,
    pub project_path: String,
    pub project_name: String,
    pub ai_title: Option<String>,
    pub first_user_excerpt: String,
    pub title: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub started_at: i64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub updated_at: i64,
    pub jsonl_path: String,
    pub message_count_approx: u32,
    pub is_bg: bool,
    pub starred: bool,
    pub custom_title: Option<String>,
    /// 隐藏那一格（判定只住后端 `control/session_hidden.rs`）；不隐藏 ⇒ `null`（格恒在）。
    pub hidden: Option<Hidden>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub forked_from_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub forked_from_message_uuid: Option<String>,
    /// 分叉来的：父会话那一行（显示标题）；不是分叉来的 · 父会话不在这台的清单里 ⇒ 缺（界面写「分叉自其他会话」）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub forked_from: Option<ForkedFrom>,
    pub project_dir: String,
    pub group: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub last_account: Option<String>,
    /// 显示的标题：用户改过的 ＞ 标题 ＞ 第一句。
    pub label: Words,
    /// 改过的标题 · 标题 · 第一句都没有（界面写「没有说过话的会话」）。
    pub untitled: bool,
    /// 那台此刻对这一行的说法（活着的照它的进程状态文件判在干什么；判不了活 ⇒ 状态不明）。
    pub face: crate::frame::SessionFace,
    pub can: Can,
    /// 行上那一家的小牌（对用户的叫法）；默认那一家 ⇒ `null`。
    pub agent_tag: Option<Words>,
    /// 被筛掉、只为挂住分叉子会话才带上的父会话（界面淡显）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "true"))]
    pub context: Option<bool>,
    /// 排序键（`activity` ⇒ 最后活动，`created` ⇒ 开始；毫秒）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub at: i64,
    pub at_text: Words,
    pub section_text: Words,
    pub span_text: Words,
    /// 问的是哪一台（与入参 `origin` 同一个机器名；这台 ⇒ 缺）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<String>"))]
    pub origin: Option<Machine>,
}

/// 分叉来的那一行的父会话：显示标题（同父会话自己那一行的 `label`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct ForkedFrom {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string"))]
    pub label: Words,
}

/// 按项目看时的一组。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct HistoryGroup {
    pub key: String,
    pub agent: String,
    pub project_name: String,
    pub project_path: String,
    pub project_dir: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub count: u64,
    /// 有在跑的 ⇒ `true`；有答不了的 ⇒ `null`；都没在跑 ⇒ `false`。
    pub has_live: Option<bool>,
    pub starred: bool,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub last_activity: i64,
    /// `last_activity` 写好的字（同行尾那一格的写法）；读不了的那一组 ⇒ `null`。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "string | null"))]
    pub at_text: Option<Words>,
    /// 几台的组并成一列时按它（大的在前）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub order: i64,
    /// 读不了的那个记录目录：原因。
    pub failed: Option<String>,
    /// 问的是哪一台（与入参 `origin` 同一个机器名；这台 ⇒ 缺）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<String>"))]
    pub origin: Option<Machine>,
}

/// 一台自己的清单（`raw` 那一形，也是远端那一支带回来的样子）：行 · 读不了的记录目录 · 那台自己的注解（或读不到它的那一句）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub rows: Vec<RawRow>,
    pub failed: Vec<FailedDir>,
    /// 那台自己那份注解（sid ⇒ 注解）；读不到 ⇒ 缺，`annotationsNotice` 说为什么。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Table>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations_notice: Option<String>,
    /// 那台偏好里子会话 · 分身会话进不进标签页（隐藏的判定照那一台的）。
    #[serde(default)]
    pub rules: Rules,
}

/// 一台清单里除了行的那几格（`history-listing` 那一问的「头」）：读不了的目录 · 注解 · 隐藏规则。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "HistoryListingHead",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "camelCase")]
pub struct ListingHead {
    pub failed: Vec<FailedDir>,
    pub annotations: Option<Table>,
    pub annotations_notice: Option<String>,
    pub rules: Rules,
}

impl Listing {
    /// 拆成头与行（行按清单里的先后）。
    pub fn split(self) -> (ListingHead, Vec<RawRow>) {
        let head = ListingHead {
            failed: self.failed,
            annotations: self.annotations,
            annotations_notice: self.annotations_notice,
            rules: self.rules,
        };
        (head, self.rows)
    }

    /// 头与行拼回一份。
    pub fn join(head: ListingHead, rows: Vec<RawRow>) -> Listing {
        Listing {
            rows,
            failed: head.failed,
            annotations: head.annotations,
            annotations_notice: head.annotations_notice,
            rules: head.rules,
        }
    }
}

/// 读不了的一个记录目录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "HistoryFailedDir",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "camelCase")]
pub struct FailedDir {
    pub project_dir: String,
    pub error: String,
}

/// 清单那一行这台判的活不活：在跑（那一家说在干什么，判不出 ⇒ 缺）· 可重连（claude 退了、tmux 里还挂着它，会话账本说的）· 已结束。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "HistoryRowLife",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(tag = "is", rename_all = "snake_case")]
pub enum RowLife {
    Live {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        activity: Option<crate::frame::SessionActivity>,
    },
    Reconnectable,
    Ended,
}

/// 清单里的一行（这台判过活、注解格先按没有填）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        rename = "HistoryRawRow",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
#[serde(rename_all = "camelCase")]
pub struct RawRow {
    pub agent: String,
    pub session_id: String,
    pub project_path: String,
    pub project_name: String,
    pub ai_title: Option<String>,
    pub first_user_excerpt: String,
    /// 显示标题（用户改过的另在 `customTitle`）：与全文搜索同一条规则。
    pub title: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub started_at: i64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub updated_at: i64,
    pub jsonl_path: String,
    /// 这台判的活不活（判不了：合成历史没有 pidfile ⇒ `null`）。
    pub life: Option<RowLife>,
    pub message_count_approx: u32,
    pub is_bg: bool,
    pub starred: bool,
    pub custom_title: Option<String>,
    /// 起会话时给了 `--tab` 的那一格（那台的启动台账）；没给 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub launch_tab: Option<LaunchTab>,
    /// 主会话（那台的会话血缘）；没有 ⇒ 缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub forked_from_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub forked_from_message_uuid: Option<String>,
    /// 读它那一组用的键（记录目录名 · 合成历史 `<kind>:<cwd>`）。
    pub project_dir: String,
    /// 分组键：哪一家 ＋ 真实目录。
    pub group: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub last_account: Option<String>,
}

/// `history-list` 每行（与全文搜索每个会话）的「这一行能做什么」（[`can_of`] 的成品）：闭集的词与两个开关。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct Can {
    /// `yes` 能恢复 · `switch` 在跑（切过去）· `bg` 分身会话（恢复主会话）。
    pub resume: CanResume,
    /// 恢复时能不能选号。
    pub accounts: bool,
    /// 能不能从某一轮分叉。
    pub fork: bool,
    /// `yes` · `live` 在跑（先结束它）· `unsure` 说不清在不在跑。
    pub delete: CanDelete,
}

/// [`Can::resume`] 的三个词。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "lowercase")]
pub enum CanResume {
    Yes,
    Switch,
    Bg,
}

/// [`Can::delete`] 的三个词。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "lowercase")]
pub enum CanDelete {
    Yes,
    Live,
    Unsure,
}

/// 一条注解（别名与缺省见各字段）。
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct Entry {
    #[serde(default)]
    pub starred: bool,
    #[serde(default, rename = "customTitle", alias = "custom_title")]
    pub custom_title: Option<String>,
    /// 单个那一格：`on` 隐藏 · `off` 不隐藏 · 缺 ＝ 照规则。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub hide: Option<Hide>,
    /// 没人答的提问等几分钟替人答一句（判定住后端 `control::asks`）；缺 ＝ 关。
    #[serde(
        default,
        rename = "autoAnswer",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub auto_answer: Option<crate::needs::AutoAnswer>,
    /// 终端没了时换的模型 / 权限档，下次恢复照它起（只由那两条换的命令写）；缺 ＝ 没记。
    #[serde(
        default,
        rename = "resumeWith",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub resume_with: Option<ResumeWith>,
    #[serde(default, rename = "updatedAt", alias = "updated_at")]
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub updated_at: i64,
}

/// 下次恢复用的那一项（[`Entry::resume_with`]）：原值同 `session-model` · `session-permission` 交的那一格。
/// `end` ＝ 记下那一刻记录的字节长度：记录长过它（恢复过 · 又写过）⇒ 这一份作废，照记录。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct ResumeWith {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub permission_mode: Option<String>,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub end: u64,
}

/// sid → 注解。
pub type Table = BTreeMap<String, Entry>;
