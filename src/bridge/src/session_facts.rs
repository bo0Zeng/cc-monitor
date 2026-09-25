//! 〔U4b · 第四波〕**monitor 侧「会话事实」的一个口** —— 两条后端流（本机 / 远端）交来的、要送到前端的那几件事。
//!
//! # 为什么要这一个口
//!
//! 本机那条流的读循环（`backend::control::local_backend::absorb_local_frame`）**手里没有 `AppHandle`**：
//! 它跑在本机后端宿主的裸线程上，今天只往几本账里记（`tmux ls` 原文、应答路由、链路字节）。
//! 而 `U4b` 要从这条流上拿两件得**发给前端**的事：
//!
//! 1. **活会话的容器**（`session_added.container`，`第四波记录/U4.md §0.1` G3）。远端流手里有 `AppHandle`，
//!    但同一个事实走两条出口就是两份实现 —— `INVARIANTS §40`：本机 ＝ 不走 ssh 的远端。
//!    ⇒ 两条流都交给 [`note_container`]，出口只有一个（`session-container` 事件）。
//! 2. **本机「可重连 → 已结束」**（G2 的收割那一半）：本机 tmux 会话关了，那条 idle 的 tab 该落到已结束。
//!    写 idle 账本（`ssh_source::clear_idle`）只许在 `lib.rs` 里调（`remote_idle_single_writer_guard`）
//!    ⇒ 收割算出来的结论交给 [`retire_local_idle`]，由 `lib.rs` 装上的出口去落地。
//!
//! 出口（[`install_sink`]）由 `lib.rs` 的 setup 装一次：它拿着 `AppHandle`，按 [`Fact`] 发事件。
//! 装上之前交来的容器事实只记账不发 —— 反正前端那时还没在听（事件不进 replay buffer）；
//! `frontend-ready` 对账那一拍会把账本整个重发一遍（[`containers_snapshot`]）。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// 一条活会话住在什么容器里。**判不了的不在这里**（缺席 = 不知道）。
///
/// 线上两个字面量与后端 `wire::SessionContainer` 逐字一致（`CONTAINER_TMUX` / `CONTAINER_NONE`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Container {
    Tmux,
    None,
}

/// 后端 `session_added.container` 的两个线上字面量。**双写点**：与后端 `src/backend/wire.rs::SessionContainer`
/// 的 serde 名逐字一致（后端那侧由 `wire_tests::session_added_container_is_additive_with_two_literals` 钉精确字节，
/// 这一侧由 `ssh_source_parse_frame_tests` 用同一串帧喂 `parse_frame` 钉）。
pub const CONTAINER_TMUX: &str = "tmux";
pub const CONTAINER_NONE: &str = "none";

impl Container {
    /// 线上字面量 → 容器；**不认识的取值当不知道**（`None`）—— 宁可说「没报」，不许凭一个不认识的词说「不在 tmux 里」。
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            CONTAINER_TMUX => Some(Container::Tmux),
            CONTAINER_NONE => Some(Container::None),
            _ => None,
        }
    }

    /// 发给前端的那个字（与线上同一个字面量）。
    pub fn as_wire(self) -> &'static str {
        match self {
            Container::Tmux => CONTAINER_TMUX,
            Container::None => CONTAINER_NONE,
        }
    }
}

/// 交给出口的一件事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fact {
    /// 这条活会话的容器（发 `session-container`）。
    Container { sid: String, container: Container },
    /// 本机一条可重连的会话，它的 tmux 会话没了 ⇒ 落到已结束（出口：`clear_idle` ＋ `session-ended`）。
    LocalIdleGone { sid: String },
}

type Sink = Box<dyn Fn(Fact) + Send + Sync>;

fn sink() -> &'static OnceLock<Sink> {
    static SINK: OnceLock<Sink> = OnceLock::new();
    &SINK
}

fn ledger() -> &'static Mutex<HashMap<String, Container>> {
    static LEDGER: OnceLock<Mutex<HashMap<String, Container>>> = OnceLock::new();
    LEDGER.get_or_init(Default::default)
}

/// 装出口。**只装一次**（第二次装被忽略并记一句 —— setup 只跑一次，多装一定是接线错了）。
pub fn install_sink(f: impl Fn(Fact) + Send + Sync + 'static) {
    if sink().set(Box::new(f)).is_err() {
        tracing::warn!("session_facts 的出口装了第二次 —— 忽略（setup 只该装一次）");
    }
}

fn emit(fact: Fact) {
    if let Some(f) = sink().get() {
        f(fact);
    }
}

/// 一条会话宣告时带来的容器事实。`None`（判不了 / 旧后端）⇒ 账本里忘掉它、不发事件
/// （前端那一格保持「没报」；复活时前端本来就把它清回「没报」）。
pub fn note_container(sid: &str, container: Option<Container>) {
    let mut led = ledger().lock().unwrap_or_else(|e| e.into_inner());
    match container {
        Some(c) => {
            led.insert(sid.to_string(), c);
            drop(led);
            emit(Fact::Container {
                sid: sid.to_string(),
                container: c,
            });
        }
        None => {
            led.remove(sid);
        }
    }
}

/// 会话离开活跃集（本机 / 远端的 removed 臂）⇒ 忘掉它的容器（它是**那个进程**的事实，不跨进程沿用）。
pub fn forget(sid: &str) {
    ledger()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(sid);
}

/// 账本整份（`frontend-ready` 对账那一拍重发用）。按 sid 排序 —— 重发顺序与哈希无关。
pub fn containers_snapshot() -> Vec<(String, Container)> {
    let mut v: Vec<(String, Container)> = ledger()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|(k, c)| (k.clone(), *c))
        .collect();
    v.sort();
    v
}

/// 本机收割的结论交出口（见模块头注第 2 条）。
pub fn retire_local_idle(sids: Vec<String>) {
    for sid in sids {
        emit(Fact::LocalIdleGone { sid });
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/session_facts_tests.rs"]
mod tests;
