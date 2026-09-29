//! 〔DP1 · 第四波〕**对外文案表的 Rust 读口** —— 与前端 `src/copy-table.ts::copyText` 读的是同一份
//! `src/shared/copy/table.json`（`设计/91 §5.1.1`：在表里 = 对外，不在表里 = 不对外，没有第三种）。
//!
//! 要求住址：`设计/01 §6.9`「所有对外文案与报错都从一张表来（结构化的 key → 文本，插值点留在表里）」。
//! 这一拍只有一处用它（`byte_table::Refusal::say`，`96 §7.1.4b` 那几个 `deploy.*` 拒绝句）；
//! 全量抽表在最后一波。
//!
//! # 纪律（与 TS 那一侧同一套，判据住 `tests/copy/copy-table.vitest.ts`）
//!
//! - key 必须是字面量（判据按调用形状从 `.rs` 里抠 `copy_text("…", &[…])`，与表两向相等）；
//! - 参数是 `&[("名", 值)]` 的数组字面量，名的集合 == 表里那一条的 `args`；
//! - 占位符只许具名 `{name}`。

// 〔CP2c · 第四波 4C〕**取文实现搬进了共享 crate `copy-core`**（`src/common/copy-core`）：
// 常驻后端与 `creds-core` 也要出句子，而 `creds-core` 被两个宿主同时链接、够不着这里 ——
// 各写一份就是 `91 §5.1` 的先例 B 形。本模块只剩转发，签名与语义一个字没变（调用点一个不动）。
// 理由全文：`调研/第四波记录/CP2c.md §2.2`。

/// 同一份表（`copy-core` 编译期内嵌的那一份；判据核它与盘上逐字节相等）。
#[cfg(test)]
use copy_core::TABLE_JSON;

/// 取一条文案并填上具名占位符（转发 `copy_core::copy_text`：表里没有 ⇒ `〔key〕`，不 panic）。
pub(crate) fn copy_text(key: &str, args: &[(&str, &str)]) -> String {
    copy_core::copy_text(key, args)
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/copy_table_tests.rs"]
mod tests;
