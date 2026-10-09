//! 找 pb：在这台装着的插件里（插件根 ＋ 清单里的名字，配置根底下哪几层装插件、清单长什么样是适配层的知识），
//! 认出名字是 pb、且带着入口脚本的那一个。

use std::path::{Path, PathBuf};

/// 清单里 pb 的名字。
pub(crate) const PLUGIN_NAME: &str = "planned-build";
/// 入口脚本（相对插件根）：`python3 <它> <动词>`。
pub(crate) const ENTRY: &str = "skills/planned-build/tools/entry.py";

/// 第一个是 pb 的插件的入口脚本；一个都不是 ⇒ `None`。次序就是交进来的次序。
pub(crate) fn entry_among(plugins: &[(PathBuf, String)]) -> Option<PathBuf> {
    plugins
        .iter()
        .find_map(|(d, name)| (name == PLUGIN_NAME).then(|| entry_in(d)).flatten())
}

/// 这个插件根底下的入口脚本（在才给）。
pub(crate) fn entry_in(dir: &Path) -> Option<PathBuf> {
    let entry = dir.join(ENTRY);
    entry.is_file().then_some(entry)
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/locate_tests.rs"]
mod tests;
