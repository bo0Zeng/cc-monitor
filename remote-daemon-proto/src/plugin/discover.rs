//! ① **找它** —— 按调用方给的候选顺序找一个可执行文件，找不到时说清楚查过哪儿。
//!
//! # 为什么候选列表是参数
//!
//! 反推样本里的两个真实实现，查找顺序**级数不同、且中间那一级语义相反**：
//! 一份先找「装到用户盘上的位置」，另一份先找「同仓相对路径」。
//! 把任何一份写死在这里，都是把某一个插件的处境刻进通用层。
//!
//! 还有一条更硬的：本层进了 `agent_boundary_guard::CORE_FILES`，
//! 而那六根针里有 agent 的名字 —— 第一刀那个插件的候选路径里逐字带着它
//! ⇒ 写死在这里**当场会红**。两条独立的理由，同一个结论。
//!
//! # 为什么不能只靠 `PATH`
//!
//! daemon 可能由 app 经 **SSH exec** 起，那是**非登录 shell**，
//! 用户级的 `bin` 目录未必在 `PATH` 里（08-13 实测）⇒ 只靠 `PATH` 会出现
//! 「明明装了却找不到」。所以固定候选先走一遍，`PATH` 只当兜底。

use std::path::{Path, PathBuf};

/// 这个路径今天是不是一个能跑的文件。
///
/// ⚠ 判**文件**再判执行位，两个都要：只判执行位会被同名的**目录**劫持
///（那是反推样本之一逐字记下来的坑）。
pub(crate) fn is_executable(p: &Path) -> bool {
    let Ok(md) = std::fs::metadata(p) else {
        return false;
    };
    if !md.is_file() {
        return false;
    }
    // Windows 上没有执行位这个概念；daemon 的目标平台是 Linux，但它**必须在 Windows 上编得过**
    //（`C16`：动 daemon 就跑 `npm run verify:committed`）。
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        md.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
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
    format!(
        "找不到 `{name}`：查过 {}，以及 PATH 上的 {path_dirs} 个目录。{hint}",
        if places.is_empty() {
            "<没有可查的固定位置：HOME 也没有>".to_string()
        } else {
            places.join(" · ")
        }
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
mod tests {
    use super::*;

    /// 找不到时那句话要**逐条列出查过的位置**，并把调用方的那句尾巴带上。
    #[test]
    fn the_not_installed_message_names_every_place_it_looked() {
        let fixed = vec![PathBuf::from("/opt/a/tool"), PathBuf::from("/home/u/bin/tool")];
        let msg = not_installed_message("tool", &fixed, 9, "装了吗？（用 TOOL_DIR 指过来）");
        assert!(msg.contains("/opt/a/tool"), "{msg}");
        assert!(msg.contains("/home/u/bin/tool"), "{msg}");
        assert!(msg.contains("9 个目录"), "PATH 那半没说：{msg}");
        assert!(msg.contains("TOOL_DIR"), "调用方的尾巴丢了：{msg}");
    }

    /// 一个固定位置都没有（比如 HOME 也解不出来）时，不许打出一句空白。
    #[test]
    fn an_empty_candidate_list_still_says_something_useful() {
        let msg = not_installed_message("tool", &[], 0, "");
        assert!(msg.contains("HOME"), "空候选时那句话什么都没说：{msg}");
    }

    /// ★ 顺序**就是**优先级：排在前面的先中。
    #[test]
    fn the_first_candidate_that_exists_wins() {
        let dir = std::env::temp_dir().join(format!("pd-find-{}", std::process::id()));
        let a = dir.join("a");
        let b = dir.join("b");
        std::fs::create_dir_all(&a).expect("mkdir a");
        std::fs::create_dir_all(&b).expect("mkdir b");
        for d in [&a, &b] {
            let f = d.join("tool");
            std::fs::write(&f, b"#!/bin/sh\n").expect("write");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755))
                    .expect("chmod");
            }
        }
        let fixed = vec![b.join("tool"), a.join("tool")];
        let got = find("tool", &fixed, false, "").expect("应当找得到");
        assert_eq!(got, b.join("tool"), "排在前面的那个没有优先");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// ★ 同名的**目录**不许被当成可执行文件（反推样本逐字记着的那个坑）。
    #[test]
    fn a_directory_with_the_same_name_is_not_executable() {
        let dir = std::env::temp_dir().join(format!("pd-dir-{}", std::process::id()));
        let trap = dir.join("tool");
        std::fs::create_dir_all(&trap).expect("mkdir trap");
        assert!(
            !is_executable(&trap),
            "同名目录被当成了可执行文件 —— 那正是被劫持的形状"
        );
        let fixed = vec![trap.clone()];
        let err = find("tool", &fixed, false, "尾巴").expect_err("目录不该被当成找到了");
        assert!(err.contains("尾巴"), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `search_path: false` 时**真的不看** `PATH` —— 那句话里的目录数必须是 0。
    #[test]
    fn opting_out_of_path_really_skips_it() {
        let err = find("no-such-tool-anywhere", &[], false, "").expect_err("不该找得到");
        assert!(
            err.contains("PATH 上的 0 个目录"),
            "说没走 PATH，但那句话里的目录数不是 0：{err}"
        );
    }
}
