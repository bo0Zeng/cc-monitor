//! 找 pb：在交进来的几个候选插件目录里，认出名字是 pb、且带着入口脚本的那一个。
//!
//! 候选目录从哪来不归这里（agent 的配置根底下哪几层装插件是适配层的布局知识，帧面宿主问注册表拿）；
//! 这里只认插件自己的两样：清单里的名字 · 入口脚本在不在。

use std::path::{Path, PathBuf};

/// 插件清单（相对插件根）。
const MANIFEST: &str = ".claude-plugin/plugin.json";
/// 清单里 pb 的名字。
pub(crate) const PLUGIN_NAME: &str = "planned-build";
/// 入口脚本（相对插件根）：`python3 <它> <动词>`。
pub(crate) const ENTRY: &str = "skills/planned-build/tools/entry.py";
/// 读清单的上限（手写的几行 JSON，远不到这个量级）。
const MANIFEST_MAX_BYTES: u64 = 64 * 1024;

/// 第一个是 pb 的候选的入口脚本；一个都不是 ⇒ `None`。次序就是候选的次序。
pub(crate) fn entry_among(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find_map(|d| entry_in(d))
}

/// 这个目录是 pb 的插件根 ⇒ 它的入口脚本。
pub(crate) fn entry_in(dir: &Path) -> Option<PathBuf> {
    let bytes = crate::common::fs::read_regular_capped(&dir.join(MANIFEST), MANIFEST_MAX_BYTES).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    if v.get("name").and_then(serde_json::Value::as_str) != Some(PLUGIN_NAME) {
        return None;
    }
    let entry = dir.join(ENTRY);
    entry.is_file().then_some(entry)
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/locate_tests.rs"]
mod tests;
