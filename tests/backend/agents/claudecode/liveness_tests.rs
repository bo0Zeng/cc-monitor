//! # 要求住址：`设计/30 §3.5.6`（活性由身份复核判）＋ `INVARIANTS §18`（证据缺席就放行）
//!
//! 核原文：`设计/30 §3.5.6` 活性那一行逐字「身份复核；主证据「进程启动时刻 vs pidfile mtime」」—— cmdline 是主证据缺席时的那一格兜底；
//! `INVARIANTS §18` 逐字「代价是极小概率误判活跃但远好过完全看不见 Tab」—— 「明显不像才判冒名、空串放行」这个方向。
//! ⚠ 候选退役：`tests/backend/observe/watcher_tests.rs` 那三条冒名 / 放行判据经生产调用点直调同一个函数，盖住了本条每一格（见 `JA1.md`）。〔JA1 点址 2026-09-24〕

use super::cmdline_may_be_agent;

/// 三个方向各一格：像 · 空串（证据缺席，放行）· 明显不像。
/// `watcher` 那边还有一组端到端的（`claude_like_cmdlines_pass` 等），`S3` 搬迁时一字未改。
#[test]
fn only_an_obviously_foreign_cmdline_is_rejected() {
    for like in [
        "claude --resume abc",
        "/usr/bin/node /home/u/.local/bin/claude",
        "node index.js",
        "",
        "   ",
    ] {
        assert!(cmdline_may_be_agent(like), "不该被判成冒名：{like:?}");
    }
    for foreign in ["/usr/bin/vim", "bash -l", "sshd: u@pts/0"] {
        assert!(!cmdline_may_be_agent(foreign), "该被判成冒名：{foreign:?}");
    }
}
