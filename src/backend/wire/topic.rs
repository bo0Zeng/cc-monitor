//! 「X 变了 ⇒ 重读」那一种推送（`changed`）的主题表 —— **只此一处**：名字 · 可不可丢 · `key` / `rev` 说什么 · 帧里带不带小成品、上限多少 · 客户端重问哪条。
//!
//! 线上一种帧 `{"kind":"changed","topic":…, "key"?, "rev"?, "body"?}`（[`cmd_wire::frame::Changed`]：一个主题一臂，体有类型，发端造那一臂）。
//! 壳不认主题：按 `topic` 原样扇给订了 `changed/<topic>` 的界面订阅；界面按 [`Topic`]（生成的类型）订、读。
//! 加一个主题 ＝ 这里加一行 ＋ cmd-wire 那个枚举加一臂 ＋ 发端发它 ＋ 界面订它，别处不动。
//!
//! 可丢性跟着发的那条路走：可丢的走 tap 那条（丢了下一次变化或重问就补上）；不可丢的走 watcher 的出方向（丢了记下主题与 `key`，那台在同一条连接上重报时补发）；
//! 只推给发起它的那条连接的（`login`）走那条连接的应答通道（不丢）。
//! 带了 `body` 也不改可丢性。`body` 只带小成品（序列化后 ≤ 那一格的上限，超了就不带、客户端照旧重问）。

/// 推送主题（线上 `changed.topic`）：名字的闭集住 `cmd-wire`（客户端按同一份解），这张表（可丢性 · 小成品上限 · 重问哪条）住这里。
pub use cmd_wire::frame::Topic;

/// 主题表的一行。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopicSpec {
    /// 线上名（＝ serde 名，判据对拍）。
    pub name: &'static str,
    /// 丢了能不能恢复（[`super::FrameRules::loss_is_recoverable`] 照它答）。
    pub lossy: bool,
    /// 带不带 `key`（说的是什么写在 [`Topic`] 各项的文档与 IPC-PROTOCOL 主题表）。
    pub key: bool,
    /// 带不带 `rev`。
    pub rev: bool,
    /// 带不带 `body`（带的那几样上限见 `body_cap`）。
    pub body: bool,
    /// `body` 序列化后的上限（字节）；超了就不带。不带 `body` 的主题 ＝ 0。
    pub body_cap: usize,
    /// 客户端收到之后重问哪条（`body` 带着的时候可以不问）。现算小成品的那几个主题跑的就是这条命令的处理器（`stream/topic_body.rs`）。
    pub reask: &'static str,
}

/// 主题表（扩展特征：孤儿规则不许在 `cmd-wire` 的类型上加固有方法）。
pub trait TopicExt {
    /// 这个主题那一行。穷尽 `match`：加主题时编译期就被逼着填完这一行。
    fn spec(self) -> TopicSpec;
    /// 线上名。
    fn name(self) -> &'static str;
}

impl TopicExt for Topic {
    fn spec(self) -> TopicSpec {
        match self {
            Topic::Accounts => TopicSpec {
                name: "accounts",
                lossy: false,
                key: false,
                rev: false,
                body: false,
                body_cap: 0,
                reask: "accounts-list",
            },
            Topic::Profiles => TopicSpec {
                name: "profiles",
                lossy: false,
                key: false,
                rev: false,
                body: false,
                body_cap: 0,
                reask: "profiles-read",
            },
            Topic::Quota => TopicSpec {
                name: "quota",
                lossy: true,
                key: false,
                rev: false,
                body: true,
                body_cap: 8192,
                reask: "quota-read",
            },
            Topic::Rotation => TopicSpec {
                name: "rotation",
                lossy: true,
                key: true,
                rev: false,
                body: false,
                body_cap: 0,
                reask: "rotation-view",
            },
            Topic::RotationStrategies => TopicSpec {
                name: "rotation_strategies",
                lossy: true,
                key: false,
                rev: false,
                body: true,
                body_cap: 8192,
                reask: "rotation-strategies",
            },
            Topic::Plan => TopicSpec {
                name: "plan",
                lossy: true,
                key: true,
                rev: true,
                body: true,
                body_cap: 64,
                reask: "plan-read",
            },
            Topic::Tasks => TopicSpec {
                name: "tasks",
                lossy: false,
                key: true,
                rev: false,
                body: true,
                body_cap: 8192,
                reask: "tasks-list",
            },
            // 走 watcher 的出方向（不可丢）：需手动的铃（`history/needs_changed.rs`）响一声推一帧。
            Topic::Needs => TopicSpec {
                name: "needs",
                lossy: false,
                key: true,
                rev: false,
                body: false,
                body_cap: 0,
                reask: "sessions-needs",
            },
            // 走发起它的那条连接的应答通道（不丢）；万一没收到，重问需手动清单就知道那个号登没登上。
            Topic::Login => TopicSpec {
                name: "login",
                lossy: false,
                key: true,
                rev: false,
                body: true,
                body_cap: 8192,
                reask: "sessions-needs",
            },
            // 走 tap 那条（可丢：丢了下一次变化或重问就补上）。
            Topic::Machines => TopicSpec {
                name: "machines",
                lossy: true,
                key: false,
                rev: false,
                body: false,
                body_cap: 0,
                reask: "machines-read",
            },
            // 走 tap 那条（可丢：成品整份，丢了下一次变化或重问就补上）；`body` 由发端给（判定在 `deploy::machine`）。
            Topic::Machine => TopicSpec {
                name: "machine",
                lossy: true,
                key: true,
                rev: false,
                body: true,
                body_cap: 8192,
                reask: "machine-read",
            },
            // 走 tap 那条（可丢：一句整份，丢了下一次变化或重问就补上）；哪一台的成品一变、它变了才推（判定在 `deploy::machine::empty`）。
            Topic::Empty => TopicSpec {
                name: "empty",
                lossy: true,
                key: false,
                rev: false,
                body: true,
                body_cap: 1024,
                reask: "machine-read",
            },
            // 走 watcher 的出方向（不可丢：标签页挪进挪出靠它）；`body` 由发端给（判定在 `history/session_hidden.rs`）。
            Topic::SessionHidden => TopicSpec {
                name: "session_hidden",
                lossy: false,
                key: true,
                rev: false,
                body: true,
                body_cap: 1024,
                reask: "history-list",
            },
            // 走 tap 那条（可丢：丢了下一次动静或重问就补上）。
            Topic::FilesDir => TopicSpec {
                name: "files_dir",
                lossy: true,
                key: true,
                rev: false,
                body: false,
                body_cap: 0,
                reask: "files-ls",
            },
            // 走 tap 那条（可丢：丢了下一次变化或重问就补上）；节点见 `history/list_node.rs`。
            Topic::HistoryList => TopicSpec {
                name: "history_list",
                lossy: true,
                key: false,
                rev: false,
                body: false,
                body_cap: 0,
                reask: "history-list",
            },
            // 走 watcher 的出方向（不可丢）：注解文件重读时组那一半变了才推（`observe/watcher.rs::reload_annotations`）。
            Topic::Groups => TopicSpec {
                name: "groups",
                lossy: false,
                key: false,
                rev: false,
                body: false,
                body_cap: 0,
                reask: "groups-read",
            },
            // 走 watcher 的出方向（不可丢：各端只照这一份画队）；`body` 由发端给（`observe/watcher.rs::queue_changed`，跑 `session-queue-read` 在这台那一半）。
            Topic::SessionQueue => TopicSpec {
                name: "session_queue",
                lossy: false,
                key: true,
                rev: false,
                body: true,
                body_cap: 16384,
                reask: "session-queue-read",
            },
        }
    }

    fn name(self) -> &'static str {
        self.spec().name
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/wire/topic_tests.rs"]
pub(super) mod tests;
