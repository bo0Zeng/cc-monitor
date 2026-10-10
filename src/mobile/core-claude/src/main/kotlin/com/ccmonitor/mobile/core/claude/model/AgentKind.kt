package com.ccmonitor.mobile.core.claude.model

/**
 * 会话所属的 agent CLI 种类。会话定位、记录解析、turn-end、判活、用量、resume 各按它选实现。
 */
enum class AgentKind {
    /** Claude Code（`~/.claude/projects/<cwd>/<sid>.jsonl`，扁平 Anthropic Messages）。 */
    ClaudeCode,

    /** Codex CLI（`~/.codex/sessions/日期/rollout-*.jsonl`，`{timestamp,type,payload}` 信封 + OpenAI Responses API）。 */
    Codex,
}
