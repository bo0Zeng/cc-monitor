//! **扫描型判据的自匹配元判据**〔audit-0805 F23，F+#3 开〕。
//!
//! # 它治的不是一个 bug，是一个**族**
//!
//! 症状永远一样：**判据在自己的登记表 / 注释 / 常量里找到了自己要找的东西 ⇒ 恒绿**。
//! audit-0805 实测五次：
//!
//! | 何处 | 判据在自己的什么东西里找到了自己 |
//! |---|---|
//! | F12 | 跨语言对拍匹配到自己的**注释** |
//! | F13 | 原子替换登记表匹配到自己的 `RULE` **常量**（写着两个符号的调用示例） |
//! | F18 | 文档副本判据匹配到自己的**登记表**（`HAS_A_GUARD` 里写着那些判据名） |
//! | F05 | 函数体抽取的**锚点**命中了自己 `PHASES` 表里的字符串 |
//! | F05 | 棘轮 `matches()` **数到自己**：6 vs 真实 4 |
//!
//! ★ **五次没有一次是被「判据变红」发现的** —— 四次靠变异、一次靠 clippy。
//! 这一类缺陷的**默认结局是恒绿**，所以「以后小心点」不是修法。
//!
//! # 修法：让它写不出来，而不是再检测一遍
//!
//! `guard_core::scan_tree!` 按构造摘除调用者自己那一份（用 `file!()`，调用方改不错）。
//! 本模块要求：**测试段里不许再出现裸的目录遍历** —— 要么走 `scan_tree!`，
//! 要么在下面这张存量清单里，而清单**只许变短**。
//!
//! ⚠ 清单是**存量盘点，不是逐条论证**：31 个文件里哪些真的自匹配、哪些只是碰巧安全，
//! 逐条判要单独一轮。本条的契约只有两句：**新增的不许出现；存量只许降。**

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// 裸遍历的形态。
    const RAW_WALKS: &[&str] = &["read_dir(", "WalkDir", "collect_rs(", "collect_ts("];

    /// **存量**：测试段里仍在裸遍历的文件（08-06 实测 31 个）。
    ///
    /// ⚠ **只许变短。** 迁一个就从这里删一行并把 `PENDING_CEILING` 调下来。
    /// 不许往里加 —— 新写的扫描型判据必须走 `guard_core::scan_tree!`。
    const PENDING: &[&str] = &[
        "src-tauri/src/account_usage.rs",
        "src-tauri/src/atomic_replace_registry.rs",
        "src-tauri/src/backend/control/daemon_kill.rs",
        "src-tauri/src/backend/control/daemon_route.rs",
        "src-tauri/src/backend/control/launch_wire.rs",
        "src-tauri/src/backend/control/local_query.rs",
        "src-tauri/src/backend/mod.rs",
        "src-tauri/src/cross_half_edge_registry.rs",
        "src-tauri/src/doc_claim_registry.rs",
        "src-tauri/src/doc_copy_registry.rs",
        "src-tauri/src/frame_cadence_guard.rs",
        "src-tauri/src/gate_singleton_guard.rs",
        "src-tauri/src/local_read_surface_registry.rs",
        "src-tauri/src/panorama.rs",
        "src-tauri/src/parity_ledger.rs",
        "src-tauri/src/parser.rs",
        "src-tauri/src/polling_registry.rs",
        "src-tauri/src/profile_installer.rs",
        "src-tauri/src/quote_singleton_guard.rs",
        "src-tauri/src/rust_timer_registry.rs",
        "src-tauri/src/session_name_registry.rs",
        "src-tauri/src/shared_crate_registry.rs",
        "src-tauri/src/ssh_source.rs",
        "src-tauri/src/tmux_daemon_gate_guard.rs",
        "src-tauri/src/utils.rs",
        "remote-daemon-proto/src/layering_guard.rs",
        "remote-daemon-proto/src/no_timer_guard.rs",
        "remote-daemon-proto/src/observe/watcher.rs",
        "remote-daemon-proto/src/platform/fallback_guard.rs",
        "remote-daemon-proto/src/protocol_doc_guard.rs",
        "remote-daemon-proto/src/readonly_guard.rs",
    ];

    /// 存量上限（**递减棘轮**）。
    const PENDING_CEILING: usize = 31;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .to_path_buf()
    }

    /// 抠出所有 `#[cfg(test)]` 段（到下一个顶层 `}` 为止）。
    fn test_regions(src: &str) -> String {
        let mut out = String::new();
        let mut i = 0usize;
        while let Some(j) = src[i..].find("#[cfg(test)]") {
            let at = i + j;
            let end = src[at..].find("\n}\n").map_or(src.len(), |e| at + e);
            out.push_str(&src[at..end]);
            out.push('\n');
            i = at + "#[cfg(test)]".len();
        }
        out
    }

    /// 今天仍在裸遍历的文件（相对仓根）。
    fn raw_walkers() -> Vec<String> {
        let root = repo_root();
        let mut out = Vec::new();
        for sub in ["src-tauri/src", "remote-daemon-proto/src"] {
            // ★ 本模块自己也走 `scan_tree!` —— 它就是那条规矩的第一个遵守者。
            //
            // ⚠ **摘除在这里今天不是承重的**（变异实测）：把 `scan_tree!` 换成一个匹配不上的
            // 摘除名，本条**照样绿** —— 因为真正让本文件不被标记的是下面那个
            // `!regs.contains("scan_tree!")`：本模块的测试段里就写着 `scan_tree!`。
            // 留着摘除是**纵深防御**：哪天本模块多写一个不走 `scan_tree!` 的扫描助手，
            // 没有摘除就会拿 `RAW_WALKS` 里那四个字面量把自己算进去。
            // ★ 「哪一行在真正干活」这种断言**必须变异验过再写** —— 本区第三次
            //（F14 第六刀 `[ -r ]` 不能省 · F12 `uiStrings` 两道都不能省 · 本条）。
            for (f, src) in guard_core::scan_tree!(&root.join(sub), &["rs"]) {
                let regs = test_regions(&src);
                if RAW_WALKS.iter().any(|w| regs.contains(w)) && !regs.contains("scan_tree!") {
                    out.push(
                        f.strip_prefix(&root)
                            .unwrap_or(&f)
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        out.sort();
        out
    }

    /// ★ 正题：**测试段里不许新增裸遍历**。
    #[test]
    fn no_new_guard_walks_the_tree_without_excluding_itself() {
        let found = raw_walkers();
        // 抽取器自检：扫不到时下面的对拍会两边都空、静默变绿。
        assert!(
            found.len() >= 20,
            "只扫到 {} 个裸遍历文件（08-06 实测 31）—— 抽取器坏了",
            found.len()
        );
        let newcomers: Vec<&String> = found
            .iter()
            .filter(|f| !PENDING.contains(&f.as_str()))
            .collect();
        assert!(
            newcomers.is_empty(),
            "有扫描型判据在测试段里**裸遍历目录**，且不在存量清单里：\n{}\n\n\
             ⇒ 改走 `guard_core::scan_tree!(&root, &[\"rs\"])` —— 它按构造摘除调用者自己那份。\n\
             ★ 为什么非要这条：判据在自己的登记表/注释/常量里找到自己 ⇒ **恒绿**，\n\
             audit-0805 实测五次，**五次都不是被判据变红发现的**（四次靠变异、一次靠 clippy）。\n\
             「以后小心点」对这一族无效，所以修法是**让它写不出来**。",
            newcomers
                .iter()
                .map(|s| format!("  {s}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// ★ **递减棘轮**：存量只许降。
    #[test]
    fn the_pending_inventory_only_shrinks() {
        let n = PENDING.iter().filter(|s| !s.is_empty()).count();
        assert!(
            n <= PENDING_CEILING,
            "存量清单涨到 {n}（上限 {PENDING_CEILING}）—— **只许降**。\
             迁一个就删一行并把上限调下来；**不许把上限调上去让今天好过**。"
        );
        // 清单不许长草：登记的文件必须真的还在裸遍历。
        let found = raw_walkers();
        let stale: Vec<&&str> = PENDING
            .iter()
            .filter(|p| !found.iter().any(|f| f == *p))
            .collect();
        assert!(
            stale.is_empty(),
            "存量清单里这些已经不裸遍历了（迁完了或文件没了）：{stale:?}\n\
             ⇒ 删掉它们并把 `PENDING_CEILING` 一起调下来 —— 留着就是把棘轮的余量白送出去。"
        );
    }
}
