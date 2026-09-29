//! F03：**monitor 这一侧对 §34 Gate 2 判定表的独立对拍**。
//!
//! 判定表 `fixtures/gate2-golden.tsv` 有三个读者，**各自独立读它**：
//!
//! | 轨道 | 谁 | 读法 |
//! |---|---|---|
//! | monitor（Rust） | **本模块** | `include_str!` + `gate_core::gate2` |
//! | backend（Rust） | `src/backend/control/gate.rs` 的测试 | 跨仓相对路径 `include_str!` |
//! | 真二进制（bash） | `tests/e2e/backend-gate2-acceptance.sh` | `cut -f`，跑真 backend + 真 tmux |
//!
//! ⚠ **绝不许一侧在运行时去调另一侧** —— 那样两侧一起错也全绿。
//! 这条纪律与 `launch_cli_parity` / `launch_payload_parity` 同族：
//! 夹具入库，两侧各自对夹具，夹具本身进 git ⇒ 谁改了判定表 diff 里看得见。
//!
//! # 本模块与 `gate-core` 自己的单测有什么不同
//!
//! `gate-core` 的单测是**作者写给自己的**；这张表是**跨轨契约**。
//! 区别在改动成本：改 gate-core 的单测只影响那个 crate，
//! 改这张表会让**三条轨道同时**要重新解释 —— 这正是我们要的摩擦。

#[cfg(test)]
#[path = "../../../../../../tests/frontend/shell/backend/control/gate2_parity_tests.rs"]
mod tests;
