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
