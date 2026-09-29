//! # 要求住址：`设计/00 §1.6.6`（会话目录布局归 `agents/`）
//!
//! 核原文：`设计/00 §1.6.6` 逐字「会话文件长什么样 · 住哪个目录、文件怎么命名」归 `agents/` —— 本族判 Claude Code 那份布局
//! 逐字段不许无声漂移（子目录名 · 扩展名 · sid 取法 · 跳过的段）。〔JA1 点址 2026-09-24〕

use super::*;

/// 契约测试:锁死 CC 会话源布局,防将来改 adapter 时子目录名/约定无声漂移(F-MA 第一刀)。
#[test]
fn claude_layout_locked() {
    let a = ClaudeCodeAdapter;
    assert_eq!(a.id(), "claude-code");
    // 〔LOC1b · 4D〕子目录名 / 活性目录 / 扩展名三格随 monitor 读本机盘那几份实现删了（目录布局归后端 `agents/claudecode/`）；
    //   〔MOD〕取 sid 的策略那一格随记录解释也进了后端。
}
