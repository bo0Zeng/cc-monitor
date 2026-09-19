use super::*;
use crate::adapter::{self, AgentKind, SidStrategy};
use std::path::Path;

/// 契约：锁 Codex 定位布局（防将来无声漂移）。
#[test]
fn codex_layout_locked() {
    let a = CodexAdapter;
    assert_eq!(a.id(), "codex");
    assert_eq!(a.default_launcher(), "codex");
    let l = a.layout();
    assert_eq!(l.sessions_subdir, "sessions");
    assert_eq!(l.record_ext, "jsonl");
    assert_eq!(l.sid_strategy, SidStrategy::CodexRollout);
    assert_eq!(l.tasks_subdir, None);
    assert!(l.skip_segments.is_empty());
}

/// `for_kind` 派发到 Codex；`active()` 仍是 Claude（零回归）。
#[test]
fn for_kind_dispatches_codex_and_active_stays_claude() {
    assert_eq!(adapter::for_kind(AgentKind::Codex).id(), "codex");
    assert_eq!(adapter::for_kind(AgentKind::ClaudeCode).id(), "claude-code");
    assert_eq!(adapter::active().id(), "claude-code");
}

/// Codex sid 提取：本机真实 rollout 文件名 → 末 36 字符 UUID（时间戳内也含 `-`，不误切）。
#[test]
fn codex_sid_from_real_rollout_filename() {
    let codex = adapter::for_kind(AgentKind::Codex).layout();
    let p = Path::new(
        "/home/u/.codex/sessions/2026/07/18/rollout-2026-07-18T20-25-05-019f7867-efe6-71d0-a237-c3edc281f89b.jsonl",
    );
    assert_eq!(
        adapter::session_id_from_path_with(codex, p).as_deref(),
        Some("019f7867-efe6-71d0-a237-c3edc281f89b")
    );
    // 非 rollout 前缀 / 短名 / 末段非 UUID → None（不臆造）。
    for bad in [
        "/x/notrollout-abc.jsonl",
        "/x/rollout-short.jsonl",
        "/x/rollout-2026-07-18T20-25-05-not-a-valid-uuid-here-zz.jsonl",
    ] {
        assert_eq!(
            adapter::session_id_from_path_with(codex, Path::new(bad)),
            None,
            "应拒 {bad}"
        );
    }
}

/// Claude sid（Stem 策略）不受泛化影响：`<sid>.jsonl` → stem。
#[test]
fn claude_sid_stem_unchanged() {
    let claude = adapter::for_kind(AgentKind::ClaudeCode).layout();
    let p = Path::new("/h/.claude/projects/enc/abcd-1234.jsonl");
    assert_eq!(
        adapter::session_id_from_path_with(claude, p).as_deref(),
        Some("abcd-1234")
    );
}
