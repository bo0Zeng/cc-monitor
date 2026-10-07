//! ① 找它 —— 按调用方给的候选顺序找一个可执行文件，找不到时说清楚查过哪儿。
//!
//! 候选列表是参数：两个真实实现的查找顺序级数不同、中间那一级语义相反（一份先找「装到用户盘上的位置」，另一份先找「同仓相对路径」）；
//! 本层进了 `agent_boundary_guard::CORE_FILES`，插件的候选路径里带着 agent 的名字 ⇒ 写死在这里当场会红。
//! 不能只靠 `PATH`：backend 可能由 app 经 SSH exec 起（非登录 shell），用户级的 `bin` 目录未必在 `PATH` 里 ⇒ 固定候选先走一遍，`PATH` 只当兜底。

use copy_core::copy_text;
use std::path::{Path, PathBuf};

/// 这个路径今天是不是一个能跑的文件。判文件再判执行位，两个都要：只判执行位会被同名的目录劫持。
///
/// 非 unix：Windows 那一臂是 [`windows_launchable_name`]（只认 `.exe`）；其余平台保守 `false`（确证的空实现：一个插件都找不到）——
/// 不返回「什么都算可执行」：那会让 [`on_path`] 把 `PATH` 上第一个同名文件当成插件交出去（[`crate::platform::cfgless_guard`] 钉着这一族）。
/// 找不到的那条路上有 [`not_installed_message`]，它会说出查过哪些地方。
pub(crate) fn is_executable(p: &Path) -> bool {
    let Ok(md) = std::fs::metadata(p) else {
        return false;
    };
    if !md.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        md.permissions().mode() & 0o111 != 0
    }
    #[cfg(windows)]
    {
        windows_launchable_name(p)
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// Windows 上「这个普通文件能不能被当程序起」—— 只认 `.exe`（大小写不敏感）。
///
/// 不读 `PATHEXT`：那一整套（`.bat` / `.cmd` / `.com` …）是「在 `PATH` 上按名字找命令」的规矩，而经这一口起的东西名字里就带着 `.exe`；
/// `.bat` / `.cmd` 要经 `cmd.exe` 解释才起得来，插件口是 argv 直传、不过 shell 的。这不是完整的 Windows 语义。
/// 纯函数（不碰盘、不带平台门），在哪个平台上都能测；只有 Windows 那一臂用它。
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn windows_launchable_name(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
}

/// 在 `PATH` 上找一个可执行文件（不看固定候选）。
pub(crate) fn on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join(name))
            .find(|c| is_executable(c))
    })
}

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
    let path_dirs: Vec<PathBuf> = if search_path {
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    for d in &path_dirs {
        let c = d.join(name);
        if is_executable(&c) {
            return Ok(c);
        }
    }
    Err(not_installed_message(name, fixed, path_dirs.len(), hint))
}

#[cfg(test)]
#[path = "../../../tests/backend/plugin/discover_tests.rs"]
mod tests;
