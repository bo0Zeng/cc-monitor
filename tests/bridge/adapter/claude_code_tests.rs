use super::*;

/// 契约测试:锁死 CC 会话源布局,防将来改 adapter 时子目录名/约定无声漂移(F-MA 第一刀)。
#[test]
fn claude_layout_locked() {
    let a = ClaudeCodeAdapter;
    assert_eq!(a.id(), "claude-code");
    let l = a.layout();
    assert_eq!(l.sessions_subdir, "projects");
    assert_eq!(l.liveness_subdir, "sessions");
    assert_eq!(l.tasks_subdir, Some("tasks"));
    assert_eq!(l.record_ext, "jsonl");
    assert_eq!(l.sid_strategy, SidStrategy::Stem);
    assert_eq!(l.skip_segments, ["subagents"]);
}
