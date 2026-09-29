//! F09：**monitor 的 Rust 侧周期唤醒清账** —— 补上 `polling_registry` 明确留下的那半。
//!
//! # 为什么补这一半
//!
//! backend 侧的 `no_timer_guard`（§41）是**零容忍**的。它的头注写着范围：
//! 「**只钉 backend crate。** monitor 侧另有自己的轮询纪律…**要钉那半得单独论证**。」
//! 而 monitor 侧的 `polling_registry` 只覆盖了 **TS 与 `shared/ccm`**，它自己的头注逐字写着：
//! 「**monitor 的 Rust 侧刻意不在范围内**…逐条论证是另一件事。**如实登记为未做，不假装覆盖了。**」
//!
//! ⇒ 那「另一件事」就是本模块。F02 摸底时那条线索指向这里，F09 把它做掉。
//!
//! # 论证：为什么是**登记表**而不是禁令
//!
//! 实测（F09 摸底）monitor Rust 生产段的 43 处 `sleep`/`Duration::from_*` 里：
//!
//! - **`time::interval` 零处** —— 没有一个 tokio 节拍器；
//! - **23 处 `Duration::from_*` 是 timeout / debounce / 退避上限** ——
//!   那是「等待的**上界**」，不是「自己醒过来」。把它们混进禁令就是后端那条护栏
//!   头注预言的**噪音**；
//! - **真正要清的是 `sleep` 那一族** —— 手工 grep 数出 8 处，
//!   而本表首跑（正确剥段后）数出 **13 处**（见下）。⚠ **以机器那个数为准。**
//!
//! ⇒ 禁令会误伤 23 处正当上界；登记表能把那 13 处逐个说清。**分类同 `polling_registry`**
//! （同一套词汇，别造第二套）：
//!
//! | 类别 | 含义 | 要求 |
//! |---|---|---|
//! | `ticker` | **真节拍器** —— 无限循环 + 周期唤醒，没有终止条件 | **必须写明事件源在哪 + 谁退役它** |
//! | `wait-for-condition` | 等一个一次性条件，**有次数/时间上限** | 说清等什么、上限是多少 |
//! | `throttle` | 分块/错开，**有明确的元素上界** | 说清上界从哪来 |
//! | `startup-delay` | 一次性启动延时 | 说清为什么要让路 |
//!
//! # ★ 它一上岗就抓到**两个**真节拍器，而且我摸底时都数漏了
//!
//! 1. `bind.rs::run_heartbeat` = `loop { sleep(10s); cleanup_dead(); }` —— 无限、周期、无上限。
//! 2. ★★ `ssh_source.rs` 的 **daemonless 数据轮询**（`BACKENDLESS_POLL_INTERVAL = 2s`）——
//!    **它与定框 C7（没有 daemonless）和 C8（不许轮询）直接冲突**，而且是本工作区的正题。
//!    〔`K-R59` 09-11：**这一条已经退役** —— 定框 `K35` 把那一档整个取消。本段记的是
//!     「它一上岗抓到了什么」，不是今天的清单；今天的清单以 `REGISTERED` 与
//!     `every_ticker_names_its_event_source_and_owner` 里那个 `tickers` 数为准。〕
//!
//! **两个都此前完全没有被任何账本记过**：`polling_registry` 按设计不管 Rust 侧，
//! `no_timer_guard` 只管 backend crate ⇒ 它们正落在「两个护栏各自划了范围、中间那块没人管」里。
//!
//! ⚠ 而且**我自己摸底时数漏了**：手工 grep 数出 8 处 `sleep`、`ssh_source` 只数到 1 处；
//! 本表首跑用 `guard_core::production_code` 正确剥段后数出 **13 处**、`ssh_source` **4 处**、
//! 还多出整个 `lib.rs` 的 3 处。**人数出来的和机器数出来的不一样** ——
//! 与 `quote_singleton_guard` 那次（我数四份、它数五份）完全同形。
//!
//! # 它查什么、查不了什么
//!
//! 查 monitor Rust **生产段**里的 `thread::sleep` / `tokio::time::sleep` / `time::interval`，
//! 逐处要求登记。
//!
//! ⚠ **查不了「不用 sleep 的忙等」**（`loop { if cond { break } }` 纯自旋）——
//! 那种形态在语法上与正常循环无法区分。**比没有强，别读成证明。**
//! ⚠ **也不查 `Duration::from_*` 本身**：实测 23 处里全是上界，查它只会得到噪音
//! （这是本模块选登记表而不选禁令的同一条理由）。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/rust_timer_registry_tests.rs"]
mod tests;
