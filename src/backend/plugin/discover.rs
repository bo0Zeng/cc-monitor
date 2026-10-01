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
//! backend 可能由 app 经 **SSH exec** 起，那是**非登录 shell**，
//! 用户级的 `bin` 目录未必在 `PATH` 里（08-13 实测）⇒ 只靠 `PATH` 会出现
//! 「明明装了却找不到」。所以固定候选先走一遍，`PATH` 只当兜底。

use copy_core::copy_text;
use std::path::{Path, PathBuf};

/// 这个路径今天是不是一个能跑的文件。
///
/// ⚠ 判**文件**再判执行位，两个都要：只判执行位会被同名的**目录**劫持
///（那是反推样本之一逐字记下来的坑）。
///
/// # 🔴 `K-R52`（09-11）：非 unix 那一臂此前是**裸 `true`**
///
/// 「什么都算可执行」。它的后果不是编不过，是**跑不对**：[`on_path`] 会把 `PATH` 上
/// 第一个同名文件当成插件交出去，不管那是不是一个能跑的东西。
///
/// ⚠ **仓里自己点过名、放了很久没治** —— `platform/fallback_guard.rs` 的头注承认过：
/// 它要挡的那个形状**就在 `plugin/discover.rs` 活着，而人群够不着它**
/// （那道护栏只扫 `platform/` 一个目录）。`K-R52` 把人群补上了，判据住
/// [`crate::platform::cfgless_guard`]，而这一处正是那条新判据**修之前先红**的那个样本。
/// 〔那份头注与它的 `g6_reach::counterexample_b…` 已**同轮摘登记**，改成过去时；
///  那条反例从**活体**退成了**合成**，差别写在它自己的头注里。〕
///
/// **Windows 那一臂今天有了真实现**（[`windows_launchable_name`]：只认 `.exe`，理由在它的头注）；
/// 其余非 unix 平台仍是下面这段说的保守 `false`。〔下面三段是 `K-R52` 那一拍的原话，留作来历。〕
///
/// # 改成 `false` 是「保守方向」，不是 Windows 实现
///
/// `fallback_guard` 头注列的三个诚实取值里，`false` 那一条的括号写着
/// 「保守方向……**调用方本就容忍**」—— 这里正是：找不到的那条路上有
/// [`not_installed_message`]，它会说出**查过哪些地方**，不会报成笼统失败。
///
/// 🔴 **如实登记这一臂今天不是什么**：它**不是** Windows 的语义。Windows 上
/// 「这个文件能不能跑」是按**扩展名**判的（`PATHEXT`：`.exe` / `.bat` / `.cmd` …），
/// 那是一份**真实现**，该住进 `platform/`。本件 `K-R52` 的射程是「让那条线守得住」、
/// **一行代码都不搬**，写一份 Windows 实现是一次产品决策 ⇒ **本件不写，已走上报口交回 PM**。
/// ⇒ 今天非 unix 上这条能力是**确证的空实现**（一个插件都找不到），不是「不知道」。
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

/// Windows 上「这个普通文件能不能被当程序起」—— **只认 `.exe`**（大小写不敏感）。
///
/// # 为什么今天要它（上面 `K-R52` 那段「本件不写，已走上报口交回 PM」的那一次产品决策）
///
/// 用户 09-24 V108：代码全景「之后本机也走这条路、monitor 摘内嵌引擎」⇒ Windows **本机**的全景
/// 从那一刻起就是「本机后端 → 插件口 → `cc-monitor-panorama.exe`」。这一臂仍是恒 `false` 的话，
/// Windows 本机的全景从「能用」（monitor 进程内那一份）变成「这台机器上还没装代码全景组件」。
///
/// # 为什么只认 `.exe`、不读 `PATHEXT`
///
/// `PATHEXT` 那一整套（`.bat` / `.cmd` / `.com` …）是「在 `PATH` 上按名字找命令」的规矩，
/// 而今天经这一口起的东西都是**固定候选**、名字里就带着 `.exe`（全景小程序；cc-bus 那一族在 Windows 上
/// 本来就不在 —— `C12`「windows 不要 tmux」）。`.bat` / `.cmd` 要经 `cmd.exe` 解释才起得来，
/// 插件口是 argv 直传、不过 shell 的 ⇒ 认它们等于认一个起不来的东西。**如实写：这不是完整的 Windows 语义。**
///
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
