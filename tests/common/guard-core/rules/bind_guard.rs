//! `DoD-4㈠` 零命中：**中转的生产段里不许出现非回环 bind 的字面量。**
//!
//! 人群：中转 crate（`src/comms/outward`）＋ 后端里它的宿主（`src/backend/relay`，绑口在 `listen.rs`）。
//!
//! # 它哪天会变瞎
//!
//! 它认的是下面那 4 个拼写，别的一律看不见，拼不拼都一样：
//! - **字面量通配地址**（`Ipv4Addr::new(0,0,0,0)`，一个字都没拼）：审计 `CT2` 实测只红行为那条，本规则绿。
//! - 「拼出来的通配地址」与「具体非回环地址」（`Ipv4Addr::new(192, 168, 1, 5)`）都没人打过，是从拼写表推的。
//!
//! ⇒ 本规则单独存在时是安慰剂，**必须**与 `server.rs` 里那条行为断言
//! （`the_relay_port_is_not_reachable_from_a_non_loopback_address`）配着用。

use super::{none, rule};

/// 非回环 bind 的那 4 个拼写。针运行时拼：本文件不在人群里（入口摘掉规则目录），拼是为了不让别的扫仓判据在这里命中。
fn needles() -> [String; 4] {
    [
        format!("0.0.{}", "0.0"),
        format!("[{}]", "::"),
        format!("Ipv4Addr::{}", "UNSPECIFIED"),
        format!("Ipv6Addr::{}", "UNSPECIFIED"),
    ]
}

/// 一段生产段里每一处命中：`(行号, 那一行)`。
fn hits(prod: &str) -> Vec<(usize, String)> {
    let needles = needles();
    prod.lines()
        .enumerate()
        .filter(|(_, l)| needles.iter().any(|n| l.contains(n.as_str())))
        .map(|(i, l)| (i + 1, l.trim().to_string()))
        .collect()
}

#[test]
fn no_non_loopback_bind_literal_in_relay_production_code() {
    rule(|repo| {
        let mut files = repo.crate_tree("src/comms/outward", &["rs"]);
        files.extend(repo.under(&["src/backend/relay"], &["rs"]));
        let found: Vec<String> = files
            .iter()
            .flat_map(|f| {
                hits(f.prod())
                    .into_iter()
                    .map(move |(no, line)| format!("{}:{no}: {line}", f.rel))
            })
            .collect();
        let control = hits(&format!(
            "TcpListener::bind((Ipv4Addr::{}, 0))",
            "UNSPECIFIED"
        ))
        .len();
        none("中转生产段出现非回环 bind 字面量", found, control);
    });
}
