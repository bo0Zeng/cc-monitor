//! **对外文案表的 Rust 取文口** —— 全仓 Rust 一侧唯一的一份实现（〔CP2c · 第四波 4C〕从 monitor 的
//! `copy_table.rs` 搬来：那一份原样转发到这里）。
//!
//! 要求住址：`设计/01 §6.9` 逐字「**所有对外文案与报错都从一张表来**（结构化的 key → 文本，插值点留在表里）」；
//! `设计/91 §5.1` 决定 2 逐字「**一份文件，两侧各读，零转换**」。
//!
//! # 为什么住共享 crate、不住某一个宿主里
//!
//! 读它的有三方：monitor（界面进程）· 常驻后端（`src/backend`，自己出句子 —— 选 A 的理由住
//! `调研/第四波记录/CP2c.md §2.1`）· `creds-core`（被上面两方同时链接，自己也有要对人说的话）。
//! `creds-core` 够不着任何一个宿主的 `crate::copy_table`；各写一份取文实现就是 `91 §5.1` 的先例 B 形。
//!
//! # 纪律（与 TS 那一侧 `src/copy-table.ts::copyText` 同一套，判据住 `tests/copy/copy-table.vitest.ts`）
//!
//! - key 必须是字面量（判据按调用形状从 `.rs` 里抠 `copy_text("…", &[…])`，与表两向相等）；
//! - 参数是 `&[("名", 值)]` 的数组字面量，名的集合 == 表里那一条的 `args`；
//! - 占位符只许具名 `{name}`。
//!
//! 表是**编译期内嵌**的：改一句话要重编才生效；远端那台后端说的是它自己那一版的话（与它的行为同版）。

use std::sync::OnceLock;

/// 同一份表（前端 `copy-table.ts` 经 Vite 读它；这里编译期内嵌）。
pub const TABLE_JSON: &str = include_str!("../../../../shared/copy/table.json");

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
pub fn copy_text(key: &str, args: &[(&str, &str)]) -> String {
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

/// 同一条文案，但要一个 `&'static str`（**没有参数**的那种）。
///
/// 后端有几处的类型刻意是 `&'static str`（例：`accounts::upstream::table::Rejected::why` ——
/// 「进日志是安全的由类型兜着，不是由记得别把文件内容塞进来兜着」）。句子进表之后不许为了它
/// 把类型放宽成 `String`：每个调用点展开成一个自己的 `static LazyLock<String>`，取一次、住一辈子。
///
/// 判据（`tests/copy/copy-table.vitest.ts::rustRefsIn`）把 `copy_static!("…")` 与 `copy_text("…", &[…])`
/// 一样收进「引用」一侧：key 必须是紧跟的字符串字面量，没有参数。
#[macro_export]
macro_rules! copy_static {
    ($key:literal) => {{
        static TEXT: ::std::sync::LazyLock<::std::string::String> =
            ::std::sync::LazyLock::new(|| $crate::copy_text($key, &[]));
        TEXT.as_str()
    }};
}

#[cfg(test)]
#[path = "../../../../../tests/bridge/crates/copy-core/lib_tests.rs"]
mod tests;
