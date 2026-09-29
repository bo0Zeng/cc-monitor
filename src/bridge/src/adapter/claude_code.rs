//! F-MA:Claude Code 适配器(**第一个实例**)。把现有 CC 专属逻辑原样包进 [`AgentAdapter`],
//! **零行为变化**——第一刀是纯抽象,现有测试全绿即验收。

use super::AgentAdapter;
use std::path::PathBuf;

/// CC 的嵌套会话 env(resume 前清洗,否则 CC 自认嵌套子会话不写 JSONL/不注册 pidfile,spec §5)。
/// ⚠ **顺序不是随手排的（U8c-2a 起）**：它与 TS `AGENT_PROFILE.nestedEnvVars` **逐项同序**。
/// `unset A B` 与 `unset B A` 语义等价，两侧的守卫也都按**集合**比 —— 但自从
/// 载荷改由 Rust 编译（`backend::control::payload`；〔AR1〕当年点名的第一个是用量探针的载荷，
/// 那条探针轴已随 `设计/50` 删了），这个顺序就**直接决定了送到远端的那条命令的字节**。同序 ⇒ 与搬家前逐字节相同，
/// 也让 `src/backend/control/launch_render/fixtures/payload-golden.json`（TS 生成）继续代表生产字节。
static CLAUDE_NESTED_ENV: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
];

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
    fn nested_env_to_scrub(&self) -> &'static [&'static str] {
        CLAUDE_NESTED_ENV
    }
    fn resume_flag(&self) -> &'static str {
        "--resume"
    }
    fn default_launcher(&self) -> &'static str {
        "claude"
    }
    fn launcher_alias(&self) -> Option<&'static str> {
        Some("cc") // 用户的 shell 集成 wrapper（含代理/env）;检测不到回退 claude。
    }
}

#[cfg(test)]
#[path = "../../../../tests/bridge/adapter/claude_code_tests.rs"]
mod tests;
