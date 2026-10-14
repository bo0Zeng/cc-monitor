//! ① 找它 —— 按调用方给的候选顺序找一个可执行文件，找不到时说清楚查过哪儿。
//!
//! 候选列表是参数：两个真实实现的查找顺序级数不同、中间那一级语义相反（一份先找「装到用户盘上的位置」，另一份先找「同仓相对路径」）；
//! 本层进了 `agent_boundary_guard::CORE_FILES`，插件的候选路径里带着 agent 的名字 ⇒ 写死在这里当场会红。
//! 不能只靠 `PATH`：backend 可能由 app 经 SSH exec 起（非登录 shell），用户级的 `bin` 目录未必在 `PATH` 里 ⇒ 固定候选先走一遍，`PATH` 只当兜底。

#[cfg(test)]
use crate::platform::fs::windows_launchable_name;
use crate::platform::fs::{is_executable, Ro};
use copy_core::copy_text;
use std::path::{Path, PathBuf};

/// 找不到时说**查过哪些地方** —— 纯函数。
///
/// 「没装 / 找不到」必须是一个**能自证的**回答，不能报成笼统的失败：
/// 用户级 `bin` 目录不在非登录 shell 的 `PATH` 里是**常态**，
/// 说不出查过哪儿的话，人只能靠猜。
pub(crate) fn not_installed_message(
    name: &str,
    fixed: &[PathBuf],
    path_dirs: usize,
    hint: &str,
) -> String {
    let places: Vec<String> = fixed.iter().map(|p| p.display().to_string()).collect();
    copy_text(
        "beDiscover.notInstalledMessage.notFound",
        &[
            ("name", &name.to_string()),
            (
                "places",
                &(if places.is_empty() {
                    copy_text("beDiscover.notInstalledMessage.noPlaces", &[])
                } else {
                    places.join(&copy_text("beDiscover.notInstalledMessage.placesSep", &[]))
                })
                .to_string(),
            ),
            ("pathDirs", &path_dirs.to_string()),
            ("hint", &hint.to_string()),
        ],
    )
}

/// 按 `fixed` 的顺序找，找不到再看要不要走 `PATH`。
///
/// 四个入参**没有一项是常量**：名字、候选顺序、要不要兜 `PATH`、找不到时那句话的尾巴，
/// 全由调用方给（理由见模块头注第一段）。
///
/// `Err` 是**整句话**（已经说明查过哪儿），调用方只需给它套上自己的语义码。
pub(crate) fn find(
    name: &str,
    fixed: &[PathBuf],
    search_path: bool,
    hint: &str,
) -> Result<PathBuf, String> {
    for c in fixed {
        if is_executable(c) {
            return Ok(c.clone());
        }
    }
    if !search_path {
        return Err(not_installed_message(name, fixed, 0, hint));
    }
    // `PATH` 那一档：找法只住 `platform::shell::find_session_command`（Windows 上 [`is_executable`] 只认 `.exe` ⇒ 找到的就是 `<名>.exe`）。
    let path = std::env::var("PATH").ok();
    if let crate::platform::shell::SessionCommand::Found { file, .. } =
        crate::platform::shell::find_session_command(
            name,
            path.as_deref(),
            cfg!(windows),
            &is_executable,
        )
    {
        return Ok(file);
    }
    let searched = path
        .as_deref()
        .map_or(0, |p| crate::platform::shell::path_dirs(p).len());
    Err(not_installed_message(name, fixed, searched, hint))
}

#[cfg(test)]
#[path = "../../../tests/backend/plugin/discover_tests.rs"]
mod tests;
