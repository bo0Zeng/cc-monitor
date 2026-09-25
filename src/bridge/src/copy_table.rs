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

use std::sync::OnceLock;

/// 同一份表（前端 `copy-table.ts` 经 Vite 读它；这里编译期内嵌）。
const TABLE_JSON: &str = include_str!("../../shared/copy/table.json");

fn entries() -> &'static serde_json::Map<String, serde_json::Value> {
    static TABLE: OnceLock<serde_json::Map<String, serde_json::Value>> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str::<serde_json::Value>(TABLE_JSON)
            .ok()
            .and_then(|v| v.get("entries").and_then(|e| e.as_object()).cloned())
            .unwrap_or_default()
    })
}

/// 取一条文案并填上具名占位符。
///
/// 表里没有这个 key ⇒ 回 `〔key〕`（走不到：`copy-table.vitest.ts` 把 `.rs` 的每个调用点与表两向对拍；
/// 但不许 panic —— 一句话缺了不该拖垮它所在的那条路）。
pub(crate) fn copy_text(key: &str, args: &[(&str, &str)]) -> String {
    let Some(zh) = entries()
        .get(key)
        .and_then(|e| e.get("zh"))
        .and_then(|z| z.as_str())
    else {
        return format!("〔{key}〕");
    };
    let mut out = zh.to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

#[cfg(test)]
#[path = "../../../tests/bridge/copy_table_tests.rs"]
mod tests;
