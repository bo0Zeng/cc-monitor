//! Phase 2（Codex 泛化）：OpenAI Codex CLI 适配器（AgentKind 的「第二个样本」）。
//!
//! **F1 slice：只落定位/发现相关字段**（data_root / layout / sid 策略）——`active()` 仍默认 Claude，
//! 本适配器经 `for_kind(AgentKind::Codex)` 取用 + 单测覆盖，尚未接进 discovery 派发（后续 slice）。
//! F4（判活）/F6（resume）相关方法先给**占位实现 + 标注**，到那两个 feature 再落实。
//!
//! Codex 事实源：`code-picture/codex-vs-claude-事实对照_2026-07-18.md`（本机实测 codex-cli 0.144.6 +
//! openai/codex 源码/web 交叉核）。要点：会话 `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`
//! （按日期分区、sid=文件名末 UUID）；无 pidfile（判活走 F4 的 logs_2/proc 策略、不用 liveness 目录）。

use super::{AgentAdapter, SessionLayout, SidStrategy};
use std::path::PathBuf;

/// Codex 会话源布局：`~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`。
/// 与 Claude 的差异（见事实对照）：**按日期分区**（无 `enc(cwd)` 项目目录）、sid 从文件名末 UUID 取
/// （非 stem）。`liveness_subdir` 对 Codex **是占位**——Codex 无 pidfile 目录，判活（F4）走
/// `logs_2.sqlite` process_uuid + `/proc/<pid>`，不 join 此字段。`tasks_subdir` 无对应。
static CODEX_LAYOUT: SessionLayout = SessionLayout {
    sessions_subdir: "sessions",
    // F4 占位：Codex 无 pidfile 判活目录；per-kind liveness 落地时对 Codex 作废此字段。
    liveness_subdir: "sessions",
    tasks_subdir: None,
    // F1：live/warm 会话为 `.jsonl`；冷会话 `.jsonl.zst`（压缩）发现留 F2/history。
    record_ext: "jsonl",
    sid_strategy: SidStrategy::CodexRollout,
    // Codex 子 agent 会话是独立 rollout（谱系在 state_5.thread_spawn_edges）；无 Claude 那样的
    // `/subagents/` 路径段可跳。子 agent 过滤（按 agent_role）留后续 feature。
    skip_segments: &[],
};

/// Codex 数据根：`$CODEX_HOME`（若设）→ `~/.codex`。（Claude 侧还有「设置面板手选」一级；Codex 的
/// 用户手选路径留到 settings feature 再加，F1 先 env + 默认。）
fn resolve_codex_dir() -> Option<PathBuf> {
    if let Ok(env_path) = std::env::var("CODEX_HOME") {
        let p = PathBuf::from(env_path);
        if p.exists() {
            return Some(p);
        }
    }
    Some(dirs::home_dir()?.join(".codex"))
}

/// OpenAI Codex CLI 适配器（ZST）。
pub struct CodexAdapter;

impl AgentAdapter for CodexAdapter {
    fn id(&self) -> &'static str {
        "codex"
    }
    fn data_root(&self) -> Option<PathBuf> {
        resolve_codex_dir()
    }
    fn layout(&self) -> &SessionLayout {
        &CODEX_LAYOUT
    }
    fn nested_env_to_scrub(&self) -> &'static [&'static str] {
        // F6 占位：Codex 的嵌套会话 env（若有）待 resume feature 实测/查证再填；F1 不用（空安全）。
        &[]
    }
    fn resume_flag(&self) -> &'static str {
        // F6 占位：Codex resume 是**子命令** `codex resume <uuid>`（非 `--flag`）——F6 让命令构建支持
        // subcommand 形；此处先给子命令名，resolve/launch 接线时按 kind 区分 flag vs subcommand。
        "resume"
    }
    fn default_launcher(&self) -> &'static str {
        "codex"
    }
    fn launcher_alias(&self) -> Option<&'static str> {
        None
    }
}

#[cfg(test)]
mod tests {
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
}
