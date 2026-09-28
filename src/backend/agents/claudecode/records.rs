//! Claude 会话记录的**文件形态**：后缀与命名。
//!
//! ⚠ 只装"文件长什么名字"，不装"怎么读它" —— 读取（tail / 偏移 / 截断重读 / 逐行解析）
//! 是**通用机器**，留在 `observe/` 与 `control/`。见 [`super`] 头注。

use std::path::Path;

/// 会话记录的扩展名。⚠ **Codex 也用 `.jsonl`** —— 它区分不了是谁的记录，
/// 只说明"这是某个 agent 的会话文件"。判据因此按**住址**分区，不按针分 agent（`S3` §1b-2）。
const SESSION_EXT: &str = "jsonl";

/// 这个路径是不是一份会话记录。
pub(crate) fn is_session_file(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == SESSION_EXT)
}

/// 会话 id → 记录文件名（`<sid>.jsonl`）。
pub(crate) fn session_file_name(sid: &str) -> String {
    format!("{sid}.{SESSION_EXT}")
}

/// 〔MOD〕会话文件 ⇒ sid（Claude 的会话文件名就是 `<sid>.jsonl`）。
pub(crate) fn session_id_of(p: &Path) -> Option<String> {
    p.file_stem().and_then(|s| s.to_str()).map(str::to_string)
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/records_tests.rs"]
mod tests;
