//! `DoD-4㈠` 零命中守卫：**中转的生产段里不许出现非回环 bind 的字面量。**
//!
//! # 为什么它单住一个文件，而不是待在 `server.rs` 里
//!
//! 它扫的正是 `server.rs`（`LOOPBACK` 那个常量住在那儿）。而 monitor 侧的
//! `scanning_guard_registry` 立过一条**递减棘轮**：扫描型判据不许裸遍历目录，
//! 要走 `guard_core::scan_tree!` —— 那个宏**按构造摘掉调用者自己那份**，
//! 治的是「判据在自己的登记表/注释/常量里找到自己 ⇒ 恒绿」那一族
//!（那边逐字记着：实测五次，五次都不是被判据变红发现的）。
//!
//! ⇒ 判据与被扫的代码**必须不在同一个文件**，否则「摘掉自己」正好把靶子摘了。
//! 第一版就是写在 `server.rs` 里的，`cargo test --manifest-path src-tauri/Cargo.toml`
//! 当场把它点名（「有扫描型判据在测试段里裸遍历目录，且不在存量清单里」）。
//!
//! # 它哪天会变瞎
//!
//! ⚠ 订正〔回修轮 08-25，承接 D1 `重要-3`：先前这里只写了「**地址一旦是拼出来的**
//! （`format!` / 读配置 / 读 env），源码扫描就看不见了」—— **实测证伪，漏说了一大半**〕：
//! 它认的是下面 needle 表里那 **4 个拼写**，**别的一律看不见，拼不拼都一样**。
//! 审计实测：把 `LOOPBACK` 换成**字面量** `IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))`
//! （**一个字都没拼**）⇒ 本守卫**照样绿**，只有行为那条红（审计 `CT2`）。
//! 同理 `Ipv4Addr::new(192, 168, 1, 5)` 这类**具体**非回环地址它也看不见
//!（**这一形没人打过，是从 needle 表推的**）。
//! ⇒ 本条单独存在时是安慰剂，**必须**与 `server.rs` 里那条行为断言
//! （`the_relay_port_is_not_reachable_from_a_non_loopback_address`）配着用。
//! 两条各自单断过：拼出来的通配地址只红行为那条，没被用到的字面量只红本条。

#[cfg(test)]
mod tests {
    #[test]
    fn no_non_loopback_bind_literal_in_relay_production_code() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/relay");
        // 针**运行时拼**：直接写字面量的话本文件自己就是命中源
        //（而 `scan_tree!` 已经摘掉了本文件 —— 两道保险，别只靠一道）。
        let needles = [
            format!("0.0.{}", "0.0"),
            format!("[{}]", "::"),
            format!("Ipv4Addr::{}", "UNSPECIFIED"),
            format!("Ipv6Addr::{}", "UNSPECIFIED"),
        ];
        let files = guard_core::scan_tree!(&dir, &["rs"]);
        assert!(
            files.len() >= 5,
            "只扫到 {} 个文件 —— 取法坏了，本断言在空转",
            files.len()
        );
        let mut hits: Vec<String> = Vec::new();
        for (path, raw) in &files {
            let prod = crate::guard_support::production_code(raw);
            for (no, line) in prod.lines().enumerate() {
                for n in &needles {
                    if line.contains(n.as_str()) {
                        hits.push(format!("{}:{}: {}", path.display(), no + 1, line.trim()));
                    }
                }
            }
        }
        assert!(
            hits.is_empty(),
            "中转生产段出现非回环 bind 字面量：{hits:?}"
        );
    }
}
