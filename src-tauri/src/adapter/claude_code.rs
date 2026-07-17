//! F-MA:Claude Code 适配器(**第一个实例**)。把现有 CC 专属逻辑原样包进 [`AgentAdapter`],
//! **零行为变化**——第一刀是纯抽象,现有测试全绿即验收。

use super::{AgentAdapter, SessionLayout};
use std::path::PathBuf;

/// CC 会话源布局:`~/.claude/{projects,sessions,tasks}`;记录 `<projects>/<enc(cwd)>/<sid>.jsonl`
/// (读侧只 WalkDir 扫 + `file_stem`=sid,不算 enc);子会话在 `/subagents/` 段跳过。
static CLAUDE_LAYOUT: SessionLayout = SessionLayout {
    sessions_subdir: "projects",
    liveness_subdir: "sessions",
    tasks_subdir: Some("tasks"),
    record_ext: "jsonl",
    sid_from_stem: true,
    skip_segments: &["subagents"],
};

/// Claude Code 适配器(ZST)。
pub struct ClaudeCodeAdapter;

impl AgentAdapter for ClaudeCodeAdapter {
    fn id(&self) -> &'static str {
        "claude-code"
    }
    fn data_root(&self) -> Option<PathBuf> {
        // 三级回退:用户配置 claudeDir → CLAUDE_CONFIG_DIR → ~/.claude。
        crate::paths::resolve_claude_dir()
    }
    fn layout(&self) -> &SessionLayout {
        &CLAUDE_LAYOUT
    }
}

#[cfg(test)]
mod tests {
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
        assert!(l.sid_from_stem);
        assert_eq!(l.skip_segments, ["subagents"]);
    }
}
