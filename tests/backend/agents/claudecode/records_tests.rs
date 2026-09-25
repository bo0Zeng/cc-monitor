//! # 要求住址：`设计/00 §1.6.6`（会话文件怎么命名归 `agents/`）
//!
//! 核原文：`设计/00 §1.6.6` 归 `agents/` 那一列逐字「会话文件长什么样 · 住哪个目录、文件怎么命名」——
//! 本族判会话文件名原样接 `.jsonl`、不做 stem 剥离，`.meta.json` 不算会话文件。（原文只说这件事归谁管，没逐字规定具体形状。）〔JA1 点址 2026-09-24〕

use super::{is_session_file, session_file_name};
use std::path::Path;

/// ⚠ 这一格是**我自己的一次语义漂移**逼出来的：`S3` 搬迁时我一度把
/// `--list-subagents` 里的 `format!("{stem}.jsonl")` 换成一个「拿旁边那个文件的
/// `file_stem()` 再接后缀」的 helper。对 `foo.meta.json` 来说 `file_stem()` 是
/// **`foo.meta`**（它只剥最后一段），于是会去找 `foo.meta.jsonl` —— 找不到，
/// 子 agent 列表静默变空。**327 格全绿，一条都没拦住它**（那条路径没有测试）。
/// ⇒ 命名的口径钉在这里，且调用方保持"自己算 stem"。
#[test]
fn session_file_name_appends_the_suffix_verbatim() {
    assert_eq!(session_file_name("abc-123"), "abc-123.jsonl");
    // 带点的 sid 也原样接后缀，不做任何"聪明"的剥离。
    assert_eq!(session_file_name("a.b"), "a.b.jsonl");
    assert!(is_session_file(Path::new("/p/abc-123.jsonl")));
    assert!(!is_session_file(Path::new("/p/abc-123.meta.json")));
}
