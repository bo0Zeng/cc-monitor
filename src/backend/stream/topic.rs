//! 「X 变了 ⇒ 重读」那一种推送（`changed`）的主题表 —— **只此一处**：名字 · 可不可丢 · `key` / `rev` 说什么 · 帧里带不带小成品、上限多少 · 客户端重问哪条。
//!
//! 线上一种帧 `{"kind":"changed","topic":…, "key"?, "rev"?, "body"?}`（[`super::wire::Frame::Changed`]）。
//! 壳不认主题：按 `topic` 原样扇给订了 `changed/<topic>` 的界面订阅；界面按 [`Topic`]（生成的类型）订、读。
//! 加一个主题 ＝ 这里加一行 ＋ 发端发它 ＋ 界面订它，别处不动。
//!
//! 可丢性跟着发的那条路走：可丢的走 tap 那条（丢了下一次变化或重问就补上）；不可丢的走 watcher 的出方向（丢了按 `overflow.lost` 报身份）。
//! 带了 `body` 也不改可丢性。`body` 只带小成品（序列化后 ≤ 那一格的上限，超了就不带、客户端照旧重问）。

use serde::Serialize;

/// 推送主题（线上 `changed.topic`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    /// 这台的账号清单（账号 manifest 被改写）。
    Accounts,
    /// 这台的配置文件（`~/.cc-monitor/profiles.toml`）。
    Profiles,
    /// 这台的额度账显示得出来的那几格。
    Quota,
    /// 这台某个会话的轮换或「账号」格（`key` ＝ sid）。
    Rotation,
    /// 这台的轮换规则表或默认指向。
    RotationRules,
    /// 这台某个 pb 工作区的计划（`key` ＝ 工作区根，`rev` ＝ 新的输出摘要，与手上那一份相同 ⇒ 不用问；
    /// `body` ＝ `{needs}`：这个工作区此刻要你看的数，没认可的、不含 agent 问人那一种，同 `plan-read` 的 `needCount`）。
    Plan,
    /// 这台某个会话的任务清单（`key` ＝ sid）。
    Tasks,
}

/// 主题表的一行。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopicSpec {
    /// 线上名（＝ serde 名，判据对拍）。
    pub name: &'static str,
    /// 丢了能不能恢复（[`super::wire::Frame::loss_is_recoverable`] 照它答）。
    pub lossy: bool,
    /// 带不带 `key`（说的是什么写在 [`Topic`] 各项的文档与 IPC-PROTOCOL 主题表）。
    pub key: bool,
    /// 带不带 `rev`。
    pub rev: bool,
    /// 带不带 `body`（带的那几样上限见 `body_cap`）。
    pub body: bool,
    /// `body` 序列化后的上限（字节）；超了就不带。不带 `body` 的主题 ＝ 0。
    pub body_cap: usize,
    /// 客户端收到之后重问哪条（`body` 带着的时候可以不问）。
    pub reask: &'static str,
    /// 小成品怎么现算：照 `reask` 那条命令、按这几样参数问它自己的处理器（[`super::topic_body::body_now`]）；[`Ask::None`] ＝ 不现算（不带，或由发端给）。
    pub ask: Ask,
}

/// 现算小成品时给重问那条命令的参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// 不现算。
    None,
    /// 不带参数（`{}`）。
    NoArgs,
    /// `{sid: key}`。
    Sid,
    /// `{sids: [key]}`。
    Sids,
}

impl Topic {
    /// 全部主题（表的行序）。
    pub const ALL: [Topic; 7] = [
        Topic::Accounts,
        Topic::Profiles,
        Topic::Quota,
        Topic::Rotation,
        Topic::RotationRules,
        Topic::Plan,
        Topic::Tasks,
    ];

    /// 这个主题那一行。穷尽 `match`：加主题时编译期就被逼着填完这一行。
    pub const fn spec(self) -> TopicSpec {
        match self {
            Topic::Accounts => TopicSpec {
                name: "accounts",
                lossy: false,
                key: false,
                rev: false,
                body: false,
                body_cap: 0,
                reask: "accounts-list",
                ask: Ask::None,
            },
            Topic::Profiles => TopicSpec {
                name: "profiles",
                lossy: false,
                key: false,
                rev: false,
                body: false,
                body_cap: 0,
                reask: "profiles-read",
                ask: Ask::None,
            },
            Topic::Quota => TopicSpec {
                name: "quota",
                lossy: true,
                key: false,
                rev: false,
                body: true,
                body_cap: 8192,
                reask: "quota-read",
                ask: Ask::NoArgs,
            },
            Topic::Rotation => TopicSpec {
                name: "rotation",
                lossy: true,
                key: true,
                rev: false,
                body: true,
                body_cap: 4096,
                reask: "rotation-session-read",
                ask: Ask::Sids,
            },
            Topic::RotationRules => TopicSpec {
                name: "rotation_rules",
                lossy: true,
                key: false,
                rev: false,
                body: true,
                body_cap: 8192,
                reask: "rotation-rules-read",
                ask: Ask::NoArgs,
            },
            Topic::Plan => TopicSpec {
                name: "plan",
                lossy: true,
                key: true,
                rev: true,
                body: true,
                body_cap: 64,
                reask: "plan-read",
                ask: Ask::None,
            },
            Topic::Tasks => TopicSpec {
                name: "tasks",
                lossy: false,
                key: true,
                rev: false,
                body: true,
                body_cap: 8192,
                reask: "tasks-list",
                ask: Ask::Sid,
            },
        }
    }

    /// 线上名。
    pub const fn name(self) -> &'static str {
        self.spec().name
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/stream/topic_tests.rs"]
mod tests;
