use super::*;
use crate::adapter::{self, AgentKind};

/// 契约：锁 Codex 那几格（防将来无声漂移）。〔MOD〕取 sid 的策略那一格随记录解释进了后端（`agents/codex/parse.rs::codex_sid_from_path`）。
#[test]
fn codex_layout_locked() {
    let a = CodexAdapter;
    assert_eq!(a.id(), "codex");
    assert_eq!(a.default_launcher(), "codex");
}

/// `for_kind` 派发到 Codex；`active()` 仍是 Claude（零回归）。
#[test]
fn for_kind_dispatches_codex_and_active_stays_claude() {
    assert_eq!(adapter::for_kind(AgentKind::Codex).id(), "codex");
    assert_eq!(adapter::for_kind(AgentKind::ClaudeCode).id(), "claude-code");
    assert_eq!(adapter::active().id(), "claude-code");
}

// 〔MOD〕按文件名取 sid 那两条随函数删了（读正文按文件名派发那一处进了后端；后端 `agents/codex/parse_tests.rs` 钉着同一件事）。
