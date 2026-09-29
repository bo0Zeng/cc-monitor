//! `Origin` —— **「这一趟问的是哪台机器」的唯一类型**。
//!
//! # 🔴 通信层成员 `COMM-LAYER-MEMBER`〔`设计/05 §8` 步 3，2026-09-20〕
//!
//! 这一枚标记是**盘上那一侧**的凭据（登记那一侧在
//! `tests/frontend/shell/comm_boundary_registry_tests.rs::REGISTERED`，两向集合相等）。
//! 盖上它 = **上锁**，不是放行：本文件从此被 `C1`–`C5` ＋ `X1`–`X6` 十一条一起管着。
//!
//! **凭什么它属于通信层**：`设计/05 §2` 逐字列了这一层认识的四样东西 ——
//! 「**地址**（`origin` / 路由键）、**操作名**（不透明字符串）、**载荷**（不透明字节）、
//! **流的订阅与分发**」。`§4` 那张一层两面图里，面 A 的寻址键逐字就是 `origin`。
//! ⇒ 本文件**就是那个「地址」**，而且**只是**那个地址：它不知道会话、账号、skill、agent，
//! 不读盘、不起进程、不碰期限。步 2 刚把它收得更紧（`null` 退役 ⇒ 地址只有一种形状）。
//!
//! ⚠ **它不是「传输面」** —— `§8` 步 3 的题面逐字是「把传输面（SSH / SFTP / 池 / 重连）
//! 圈出来」，而 `ssh_source.rs` / `sftp.rs` / `sftp_pool.rs` 今天**一份都圈不进来**
//! （C1 逐份咬在哪，逐份读数在 `真相源/`）。本文件是那一步能诚实圈进来的东西之一，
//! **别把它读成「传输面已经进来了」**。
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
//! # 🔴 〔`设计/05 §8` 步 2，2026-09-20〕**线上只剩两个值** —— `null` 退役了
//!
//! `设计/00 §2.5 ①` 原先逐字写着三个线上值 `null | "<local>" | host`。
//! 步 2 逐字「**`origin` 去 `null` 化 —— 本机也带 origin**」⇒ 线上从此**只有字符串**：
//!
//! | 线上值 | 意思 |
//! |---|---|
//! | `"<local>"` | 本机那台。**它是一个具名的 origin**，不是「缺省」 |
//! | `"aya"`（任意机器名）| 那台远端（`RemoteConfig` 里那个 label） |
//! | ~~`null`~~ | **拒收** —— 见下一节 |
//!
//! ⚠ `INVARIANTS §40` 逐字「本地 ＝ 不走 ssh 的远端」⇒ 本机本来就该有名字。
//! 「省掉 origin」与「本机」长得一样，正是这一层要治的东西。
//!
//! # 🔴 `Unspecified(())` 那个变体：**退役**（处置与理由，逐条写明）
//!
//! 上一拍这个类型有两个变体，`Unspecified(())` 承载线上的 `null`（「调用方没说」）。
//! 本拍**把它整个删掉**，理由三条：
//!
//! 1. **`设计/05 §8` 逐字要求的就是结构上的「不容忍」** —— 那一节的警告逐字：
//!    「⚠ **2 在 5 之前**（否则 `call` 的第一个参数还得容忍 `null`）」。
//!    留着变体就是**留着容忍**：`call(origin: Origin, …)` 在类型上仍然装得下「没说」，
//!    于是「不许把它当本机」这条纪律要在**每一个**新消费者身上重新打一遍
//!    （今天唯一被机器守着的漏斗是 [`Origin::route`]，而它只盖得住
//!    `#[tauri::command]` 那一族 —— `call` 不会是其中之一）。
//! 2. **「留着当拒收的类型位」在这一层是个空位。** 变体只能从线上来；
//!    把它排除出线上形状（`serde(skip)`）之后它永远构造不出来 ⇒ 纯死重。
//!    而不排除出线上形状，`Origin.ts` 就还是 `null | string`，
//!    `§8` 承诺的「**`tsc` 就能验**」当场落空。两条路只能选一条。
//! 3. 本仓纪律逐字「**不为旧配置留兼容** —— 旧形直接退役」。`null` 是旧形。
//!
//! ## ⚠ 而它守的那条性质**一个字都没丢** —— 岗位挪到了两处
//!
//! 它守的是「**「没说」不许被悄悄当成本机**」。退役之后这条性质由两道闸接着守：
//!
//! | 闸 | 拦的是 | 在哪 |
//! |---|---|---|
//! | ① **反序列化边界** | 线上 `null`（以及数字 / 数组 / 对象 / 布尔）| 本文件手写的 `Deserialize`：它**构造不出**一个「没说」的 `Origin` ⇒ 命令的代码**结构上**见不到那一档 |
//! | ② [`Origin::route`] | **空白名**（`""` / 全空白）—— `null` 消失之后，线上唯一还能表达「没说」的值 | 拒的那句话点名是哪条命令、并**逐字**给出本机该送什么 |
//!
//! 🔴 ② 不是凑数的：`subagent.rs` 的 `Backend::for_origin` 上一拍**逐字**把
//! 「`origin` 缺省 / **空串** = 本机」写在一起 —— 也就是说空串在盘上**真的**被当过本机。
//! 删掉 `null` 而不管空串，等于把同一个洞从一个值搬到另一个值。
//!
//! ⚠ ① 那道闸的**话说得清不清楚**是靠 `expecting()` 买的，不是白送的：
//! serde 的 stock 报错是「invalid type: null, expected a string」，它不会告诉调用方
//! 「本机该送 `"<local>"`」。那句话是本文件自己写的，由 `origin_tests.rs` 的 `A` 组钉着。
//!
//! # ⚠ 射程（写死，别读宽）
//!
//! · 它买的是「**这个概念只有一个类型**」。**买不到**「所有调用点都用上了它」——
//!   替换 93 处签名是分批的活，进度由
//!   `tests/frontend/shell/origin_tests.rs::ORIGIN_MIGRATION_CEILING`
//!   那张**递减棘轮**现打（数还在用裸 `&str`/`String` 的处数，只许变少）。
//! · 它**不判**某个 origin 今天连不连得上。那是 `inbound_client` 的事。
//! · 🔴 它**只管入方向**（前端 → 命令）。**出方向那一半今天还有 `null`** ——
//!   `bridge::JsonlLinePayload.origin` 一族仍是 `Option<String>`（`None` = 本机），
//!   而那几份不在本步的写区里。**逐份住址与为什么没动，写在交回件里，别读成「已经没有 `null` 了」。**
//!
//! # 线上形状：**本机与远端那两个值与今天逐字节相同**
//!
//! `serde(transparent)` ⇒ `"<local>"` / `"aya"` 两种线上值与今天**一个字节不差**；
//! 变的只有 `null` 那一档（从「解出一个变体」变成「**报错**」）。
//! 两侧都有金标准钉着（`the_two_surviving_wire_values_are_byte_identical`
//! ＋ `an_unknown_shape_is_refused_not_guessed`）。

use crate::copy_table::copy_text;
use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

/// 本机那台机器的哨兵值。
///
/// 🔴 **这是全仓第三处写这个字面量，而且是刻意的** —— 另两处是
/// `backend::control::inbound_client::LOCAL_ORIGIN`（Rust）与 `src/backend-policy.ts`（TS）。
/// 三处由判据两向钉住（`the_sentinel_agrees_with_the_two_existing_homes`）：
/// 它们必须逐字节相同，而**不是**由本文件去替换那两处 —— 替换要动 46+33 处签名，
/// 那是分批的活（见头注射程）。本常量在这里的作用是让 `Origin` 自己
/// **不必再手抄一次**，并让「三处都相等」成为一条会红的事。
pub const LOCAL: &str = "<local>";

/// 「这一趟问的是哪台机器」。线上就是**一个字符串**（本机逐字 `"<local>"`）。
///
/// ⚠ 本机与远端**同一个线上形状**（都是字符串）⇒ `serde` 分不开它们，
/// 只能在**读进来之后**用 [`Origin::is_local`] 分。这是刻意的：
/// 那两个值的线上形状必须与今天逐字节相同，不许为了类型好看而改协议。
///
/// ⚠ 字段是 `pub` 的：`Origin` 是**一个机器名**，不是一个带不变式的容器。
/// 唯一的不变式（「名字不许是空白」）由 [`Origin::route`] 那个漏斗执行 ——
/// 装在构造器上反而会把它拆成「构造时拒」与「路由时拒」两处，而那两处会漂。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../generated/"))]
#[serde(transparent)]
pub struct Origin(pub String);

/// 手写而不是 `derive` —— 为的就是 `expecting()` 那句话。
///
/// 🔴 线上 `null` 在这里**变成一次反序列化失败**，而失败那句话必须自带出路：
/// 「本机逐字送 `"<local>"`」。`derive` 出来的版本只会说
/// 「invalid type: null, expected a string」——那句话是对的，但它把调用方留在原地。
///
/// ⚠ 它**不判**名字合不合法（空白名走 [`Origin::route`] 那道闸）：
/// 两道闸各只有一个住址，合在一处的话「构造时拒」与「路由时拒」会各说各的。
impl<'de> Deserialize<'de> for Origin {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct OriginVisitor;
        impl Visitor<'_> for OriginVisitor {
            type Value = Origin;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(
                    f,
                    "{}",
                    copy_text("rsOrigin.expecting.name", &[("local", &LOCAL.to_string())])
                )
            }

            fn visit_str<E: DeError>(self, s: &str) -> Result<Origin, E> {
                Ok(Origin(s.to_string()))
            }
        }
        d.deserialize_str(OriginVisitor)
    }
}

impl Origin {
    /// 本机那一个。
    pub fn local() -> Self {
        Origin(LOCAL.to_string())
    }

    /// 这一趟问的是不是本机。
    pub fn is_local(&self) -> bool {
        self.0 == LOCAL
    }

    /// 这一趟问的是不是某台远端。**空白名 ⇒ `false`**（「没给名字」不是远端，也不是本机）。
    pub fn is_remote(&self) -> bool {
        !self.0.trim().is_empty() && self.0 != LOCAL
    }

    /// 远端那台的机器名。本机与空白名都回 `None`。
    ///
    /// ⚠ 名字是 `host_name` 不是 `name`：本机也有名字（`"<local>"`），
    /// 而这个方法**只答远端**。叫 `name` 会让调用方以为本机也能取到。
    pub fn host_name(&self) -> Option<&str> {
        self.is_remote().then_some(self.0.as_str())
    }

    /// 线上那个字符串。给还在用 `&str` 的那 46 处调用点过渡用。
    ///
    /// ⚠ 它**不再回 `Option`** —— 上一拍那个 `None` 是 `Unspecified` 的出口，
    /// 而那个变体已经退役（头注逐条写了为什么）。线上没有「没有字符串」这一档了。
    pub fn as_wire_str(&self) -> &str {
        &self.0
    }

    /// **合并后的命令唯一的入口分派**（步 12·C）。
    ///
    /// # 为什么它是一个方法，而不是每条命令自己写一个 `if origin.is_local()`
    ///
    /// 合并 N 对命令就会有 N 处「先分本机」。那 N 处里只要有一处把「没给名字」
    /// 顺手归进本机，`INVARIANTS §40`（「本地 ＝ 不走 ssh 的远端」）在那一条命令上
    /// 就悄悄破了 —— 而**那种破法不报错**：调用方送一个空串，命令去动了本机的文件。
    /// ⇒ 两态在这里一次分完，调用方拿到的是一个**只有两个可能**的值，
    /// 「没给名字」那一支在类型上就到不了它手里。
    ///
    /// # ⚠ 〔步 2〕这道闸拦的**换了**：从 `null` 换成**空白名**
    ///
    /// `null` 现在被反序列化那一层挡掉了（构造不出来），拦不到这里。
    /// 而空串是 `null` 退役之后线上**唯一**还能表达「没说」的值 ——
    /// 而且它在盘上**真的**被当过本机（`subagent·rs` 那个 `Backend::for_origin`〔散文墓碑〕上一拍
    /// 逐字写着「`origin` 缺省 / 空串 = 本机」）。⇒ 岗位没撤，只是换了被拦的那个值。
    ///
    /// ⚠ 它**不判**那台远端今天连不连得上（那是 `inbound_client` 的事），
    /// 也**不判**本机那条路该不该做这件事（那是各命令自己的守卫）。
    /// ⚠ 它**不 trim**：非空白的名字**原样**交出去（`" aya "` 仍然是 `" aya "`）——
    /// trim 会让今天找不到配置的 label 突然找得到，那是行为变更，不归本步。
    pub fn route(&self, command: &str) -> Result<Route<'_>, String> {
        if self.0.trim().is_empty() {
            return Err(copy_text(
                "rsOrigin.route.blank",
                &[
                    ("command", &command.to_string()),
                    ("local", &LOCAL.to_string()),
                ],
            ));
        }
        if self.0 == LOCAL {
            Ok(Route::Local)
        } else {
            Ok(Route::Remote(&self.0))
        }
    }
}

/// [`Origin::route`] 的产出：**分过本机之后，只剩两种可能**。
///
/// 🔴 刻意没有第三个变体。「没给名字」在 [`Origin::route`] 那一步就变成了 `Err`，
/// 线上的 `null` 更是连一个 `Origin` 都构造不出来，
/// 所以拿到 `Route` 的代码**没有办法**把「没说」误当本机或误当某台远端。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route<'a> {
    /// 本机那一条路（不走 ssh）。
    Local,
    /// 某台远端，带着它的机器名（`RemoteConfig` 那个 label）。
    Remote(&'a str),
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/origin_tests.rs"]
mod tests;
