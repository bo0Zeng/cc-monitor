//! 找 pb：这台设了 pb 路径（偏好 `pbDir`，插件根目录）⇒ 只认它；没设 ⇒ 在这台装着的插件里
//! （插件根 ＋ 清单里的名字，配置根底下哪几层装插件、清单长什么样是适配层的知识）认出名字是 pb、且带着入口脚本的那一个。
//! 设之前校验一个路径能不能用（[`check`]）。

use copy_core::copy_text;
use serde_json::{json, Value};
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

/// 找不到 pb 的两种。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Missing {
    /// 设了的那个路径底下没有 pb 的入口。
    SetDir(PathBuf),
    /// 没设，装着的插件里也没有。
    NotInstalled,
}

impl Missing {
    /// 给人看的那一句。
    pub(crate) fn said(&self) -> String {
        match self {
            Missing::SetDir(d) => copy_text("bePlan.face.pbDirGone", &[("dir", &d.display().to_string())]),
            Missing::NotInstalled => copy_text("bePlan.face.noPb", &[]),
        }
    }
}

/// 这台的 pb 入口：设了路径只认它，没设认装着的插件。
pub(crate) fn pick(set: Option<&Path>, plugins: &[(PathBuf, String)]) -> Result<PathBuf, Missing> {
    match set {
        Some(d) => entry_in(d).ok_or_else(|| Missing::SetDir(d.to_path_buf())),
        None => entry_among(plugins).ok_or(Missing::NotInstalled),
    }
}

/// 校验一个路径：`{state: ok | missing | no_entry | not_runnable, said}`（以人的身份起一次 `pb help`：入口在、解释器在、
/// pb 自己的几个模块都载得进来才算能用）。不写盘。
pub(crate) fn check(dir: &Path) -> Value {
    let shown = dir.display().to_string();
    let out = |state: &str, key: &str, why: Option<&str>| {
        let mut args = vec![("dir", shown.as_str())];
        args.extend(why.map(|w| ("why", w)));
        json!({"state": state, "said": copy_text(key, &args)})
    };
    if !dir.is_dir() {
        return out("missing", "bePlan.pbDir.missing", None);
    }
    let Some(entry) = entry_in(dir) else {
        return out("no_entry", "bePlan.pbDir.noEntry", None);
    };
    match super::command::run_as(&entry, dir, &["help"], false) {
        Ok(_) => out("ok", "bePlan.pbDir.ok", None),
        Err((_, said, _)) => out("not_runnable", "bePlan.pbDir.notRunnable", Some(&said)),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/locate_tests.rs"]
mod tests;
