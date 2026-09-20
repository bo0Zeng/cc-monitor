//! `Origin` —— **「这一趟问的是哪台机器」的唯一类型**。
//!
//! # 它为什么存在（`设计/00 §2.5 ①` 逐字：「origin 归一 —— **这是地基**」）
//!
//! 在它之前，同一个概念在盘上有**五种表达**（`真相源/97` 现打）：
//!
//! | 侧 | 现打 |
//! |---|---|
//! | Rust | `origin: &str`（46 处）· `origin: String`（33）· `origin: Option<String>`（8） |
//! | TS | `origin: string` · `origin: string \| null` |
//!
//! 而「本机」那个哨兵值是**两份手抄的字面量** —— Rust 一份、TS 一份，
//! 靠一条跨语言对拍判据钉着（`inbound_client_tests.rs`）。
//! 那条判据是对的，但它买的是「两个字面量相等」，**不是**「只有一个类型」。
//!
//! # 🔴 三个线上值，**两个**变体 —— 为什么不是三个变体
//!
//! `设计/00 §2.5 ①` 逐字写着 `null | "<local>" | host`。线上是**三个值**，
//! 而 Rust 这一侧只有**两个变体** —— 因为后两个值的**线上形状相同**（都是字符串）：
//!
//! · `Unspecified(())`（线上 `null`）—— **调用方没说**。它不等于「本机」：
//!   `INVARIANTS §40` 逐字「本地 ＝ 不走 ssh 的远端」⇒ 「没说」要么该被拒、
//!   要么该由**调用点**补一个默认，**不许在类型这一层悄悄当成本机**。
//!   （那正是今天 `Option<String>` 那 8 处的歧义所在：`None` 到底是「本机」还是「没说」，
//!    要读每一处的上下文才知道。）
//! · `Named(String)` —— `"<local>"`（本机那台）**或**某台远端的机器名
//!   （`RemoteConfig` 里那个值）。
//!   ⚠ **`serde` 分不开这两者** —— 分它们的是读进来之后的 [`Origin::is_local`]，
//!   不是类型本身。想让类型分开就得改线上形状，而那是协议变更 ⇒ 本步刻意不做。
//!
//! # ⚠ 射程（写死，别读宽）
//!
//! · 它买的是「**这个概念只有一个类型**」。**买不到**「所有调用点都用上了它」——
//!   替换 93 处签名是分批的活，进度由
//!   `tests/bridge/origin_tests.rs::ORIGIN_MIGRATION_CEILING`
//!   那张**递减棘轮**现打（数还在用裸 `&str`/`String` 的处数，只许变少）。
//! · 它**不判**某个 origin 今天连不连得上。那是 `inbound_client` 的事。
//!
//! # 线上形状：**刻意与今天逐字节相同**
//!
//! `serde(untagged)` ＋ 三个变体 ⇒ `null` / `"<local>"` / `"devbox"` 三种线上值，
//! 与今天前端送的、后端读的**一个字节不差**。
//! ⇒ 本文件落地那一拍**零行为变化**，有金标准钉着（`the_wire_shape_is_byte_identical`）。

use serde::{Deserialize, Serialize};

/// 本机那台机器的哨兵值。
///
/// 🔴 **这是全仓第三处写这个字面量，而且是刻意的** —— 另两处是
/// `backend::control::inbound_client::LOCAL_ORIGIN`（Rust）与 `src/backend-policy.ts`（TS）。
/// 三处由判据两向钉住（`the_sentinel_agrees_with_the_two_existing_homes`）：
/// 它们必须逐字节相同，而**不是**由本文件去替换那两处 —— 替换要动 46+33 处签名，
/// 那是分批的活（见头注射程）。本常量在这里的作用是让 `Origin` 自己
/// **不必再手抄一次**，并让「三处都相等」成为一条会红的事。
pub const LOCAL: &str = "<local>";

/// 「这一趟问的是哪台机器」。线上形状与今天逐字节相同（见模块头注）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(untagged)]
pub enum Origin {
    /// 调用方**没说**。线上是 `null`。
    ///
    /// 🔴 **它不等于 `Local`。** 谁拿到 `Unspecified` 要么拒、要么在**自己那一层**
    /// 补一个默认并说清为什么 —— 不许在类型这一层悄悄当成本机。
    Unspecified(()),
    /// 本机那台（`"<local>"`）或某台远端（机器名）。
    ///
    /// ⚠ 两者**同一个线上形状**（都是字符串）⇒ `serde` 分不开它们，
    /// 只能在**读进来之后**用 [`Origin::is_local`] 分。这是刻意的：
    /// 线上形状必须与今天逐字节相同，不许为了类型好看而改协议。
    Named(String),
}

impl Origin {
    /// 本机那一个。
    pub fn local() -> Self {
        Origin::Named(LOCAL.to_string())
    }

    /// 这一趟问的是不是本机。`Unspecified` ⇒ **`false`**（「没说」不是「本机」）。
    pub fn is_local(&self) -> bool {
        matches!(self, Origin::Named(s) if s == LOCAL)
    }

    /// 这一趟问的是不是某台远端。`Unspecified` ⇒ `false`。
    pub fn is_remote(&self) -> bool {
        matches!(self, Origin::Named(s) if s != LOCAL)
    }

    /// 远端那台的机器名。本机与「没说」都回 `None`。
    ///
    /// ⚠ 名字是 `host_name` 不是 `name`：`Local` 也有名字（`"<local>"`），
    /// 而这个方法**只答远端**。叫 `name` 会让调用方以为本机也能取到。
    pub fn host_name(&self) -> Option<&str> {
        match self {
            Origin::Named(s) if s != LOCAL => Some(s),
            _ => None,
        }
    }

    /// 线上那个字符串（`Unspecified` 没有）。给还在用 `&str` 的那 46 处调用点过渡用。
    pub fn as_wire_str(&self) -> Option<&str> {
        match self {
            Origin::Named(s) => Some(s),
            Origin::Unspecified(()) => None,
        }
    }

    /// **合并后的命令唯一的入口分派**（步 12·C）。
    ///
    /// # 为什么它是一个方法，而不是每条命令自己写一个 `if origin.is_local()`
    ///
    /// 合并 N 对命令就会有 N 处「先分本机」。那 N 处里只要有一处把 `Unspecified`
    /// 顺手归进本机，`INVARIANTS §40`（「本地 ＝ 不走 ssh 的远端」）在那一条命令上
    /// 就悄悄破了 —— 而**那种破法不报错**：调用方送 `null`，命令去动了本机的文件。
    /// ⇒ 三态在这里一次穷尽（`match` 没有 `_` 臂），调用方拿到的是一个
    /// **只有两个可能**的值，「没说」那一支在类型上就到不了它手里。
    ///
    /// ⚠ 它**不判**那台远端今天连不连得上（那是 `inbound_client` 的事），
    /// 也**不判**本机那条路该不该做这件事（那是各命令自己的守卫）。
    pub fn route(&self, command: &str) -> Result<Route<'_>, String> {
        match self {
            Origin::Named(s) if s == LOCAL => Ok(Route::Local),
            Origin::Named(s) => Ok(Route::Remote(s)),
            Origin::Unspecified(()) => Err(format!(
                "`{command}` 没收到 origin（线上 `null`）。\
                 「没说」不是「本机」—— `INVARIANTS §40` 逐字「本地 ＝ 不走 ssh 的远端」，\
                 本机是一个**具名**的 origin，要逐字送 `\"{LOCAL}\"`。"
            )),
        }
    }
}

/// [`Origin::route`] 的产出：**分过本机之后，只剩两种可能**。
///
/// 🔴 刻意没有第三个变体。「调用方没说」在 [`Origin::route`] 那一步就变成了 `Err`，
/// 所以拿到 `Route` 的代码**没有办法**把「没说」误当本机或误当某台远端。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route<'a> {
    /// 本机那一条路（不走 ssh）。
    Local,
    /// 某台远端，带着它的机器名（`RemoteConfig` 那个 label）。
    Remote(&'a str),
}

#[cfg(test)]
#[path = "../../../tests/bridge/origin_tests.rs"]
mod tests;
