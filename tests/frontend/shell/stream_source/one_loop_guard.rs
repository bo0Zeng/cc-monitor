//! **重连循环只有一个诞生点，在 `resident-client`**（mobcore 第 6 步）：桌面与手机同一条循环（`resident_client::feed::Feed`），
//! monitor 只管宿主那几件（放字节 · 开窗 · 通道），不许再长出自己的循环。
//!
//! 认法：一条重连循环手里必有一份退避（`Backoff::new(`）；全仓生产段里它只许出现一次、就在 `feed.rs`。
//! monitor 生产段里连退避那一家的名字都不许出现（用到它 ＝ 又在自己数退避）。

use std::path::Path;

/// 认得出一条循环的那一针。
const BIRTH: &str = "Backoff::new(";

/// monitor 生产段里不许出现的退避那一家的名字。
const NOT_IN_MONITOR: [&str; 4] = [
    "resident_client::backoff",
    "should_reset_backoff",
    "before_wait(",
    "after_wait(",
];

fn births_in(prod: &str) -> usize {
    prod.matches(BIRTH).count()
}

fn monitor_hits(prod: &str) -> Vec<&'static str> {
    NOT_IN_MONITOR
        .iter()
        .copied()
        .filter(|n| prod.contains(n))
        .collect()
}

#[test]
fn the_reconnect_loop_is_born_once_and_only_in_resident_client() {
    // 正控：同一对判定函数喂一段合成的循环，两针都中。
    let synthetic = format!(
        "let mut b = Backoff::{n}();\nlet h = resident_client::{k}::should_reset_backoff(true, d);\nb.before_wait(h);\nb.after_wait(false, h);\n",
        n = "new",
        k = "backoff"
    );
    assert_eq!(births_in(&synthetic), 1, "认循环那一针是瞎的");
    assert_eq!(
        monitor_hits(&synthetic).len(),
        NOT_IN_MONITOR.len(),
        "认退避名字那一针是瞎的"
    );

    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut born = Vec::new();
    let mut n = 0usize;
    for (path, raw) in guard_core::scan_tree!(&repo.join("src"), &["rs"]) {
        n += 1;
        let prod = guard_core::production_code(&raw);
        for _ in 0..births_in(&prod) {
            born.push(
                path.strip_prefix(&repo)
                    .unwrap_or(&path)
                    .display()
                    .to_string()
                    .replace('\\', "/"),
            );
        }
    }
    assert!(n >= 300, "只扫到 {n} 份 .rs —— 遍历坏了");
    assert_eq!(
        born,
        vec!["src/comms/resident-client/feed.rs".to_string()],
        "重连循环（手里一份 `Backoff`）的诞生点不是恰好一处、在 `resident-client/feed.rs`"
    );

    let shell = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut bad = Vec::new();
    for (path, raw) in guard_core::scan_tree!(&shell, &["rs"]) {
        // 只认 monitor 自己那棵（扫描会顺着 `#[path]` / 工作区摸到别的 crate）。
        if !path.starts_with(&shell) {
            continue;
        }
        let prod = guard_core::production_code(&raw);
        for h in monitor_hits(&prod) {
            bad.push(format!("{} · `{h}`", path.display()));
        }
    }
    assert!(
        bad.is_empty(),
        "monitor 生产段里又在自己数退避（那是第二条重连循环）：\n{}",
        bad.join("\n")
    );
}
