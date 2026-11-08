//! 契约错的那一句话：调用方是我们自己的代码（monitor / 前端 / 另一台后端）发来的请求格式不对 —— 缺字段、类型不对、越界、链路 id 重复 ……
//! 用户手上没有东西造得出它们，只有版本错位或我们的 bug 才会出现（线上 `code`：`bad_args` / `bad_request`）。
//!
//! 进文案表的只有一句（`beContract.malformed.say`「请求格式不对」）；`detail` 是给报 bug 的人看的诊断，写英文、不进表、不上句子，
//! 只记一行 warn 日志（这台后端的日志；调错的若是另一台的前端，它看不到这一行 —— 已知，见文案十四交回）。
//! 调用处写 `contract::malformed("missing `to`")`，一眼认得出「这是契约错，不是对用户说的话」。
//! 判哪句算契约错：值能不能来自用户手敲。会话名、目录、`--ccm-tmux=<名>` 这种用户打得出来的，不归这里，各自进表说人话；判不准的也不归这里。
//! `detail` 里出现汉字 ⇒ 普查会把那个字面量数成对外文案 ⇒ `copy-ledgers.vitest.ts` 的 CP2（全仓对外字面量零条）红。

/// 拆契约错那一串的一处（`parts`）与反方向（`joined`）：本体与分隔字符住 `copy_core::said`（全仓一份），这里只转用。
/// 诊断跟着那一串走（隔一个分隔字符），由失败信封那一处（`parts`）拆开：句子照旧只有「请求格式不对」，
/// 诊断进复制详情的「原话」那一行 —— 看的人知道缺的是哪一格、要什么形状，不必去翻那台的日志。
/// 失败信封（`wire::detail::Failed` · `wire::spec::fail`）只经 `parts` 拆；没有诊断 ⇒ 原样。
pub use copy_core::said::{joined, parts};

/// 契约错的那一句（`detail`：英文诊断）：记一行 warn 日志（这台后端的日志），回那一串（`copy_core::said::malformed_said`）。
pub fn malformed(detail: &str) -> String {
    tracing::warn!("contract: malformed request: {detail}");
    copy_core::said::malformed_said(detail)
}

// 判据读「契约错记进日志的那条诊断」的 `tests::diag`：本包与上层各族的测试档共用。
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../tests/backend/common/contract_support.rs"]
pub mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/common/contract_tests.rs"]
mod judges;
