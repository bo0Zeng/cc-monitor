//! Claude 的目录布局：配置根怎么解析、根下面有哪两个子目录。

use std::path::{Path, PathBuf};

/// 覆盖配置根的环境变量名。**账号隔离（cc-acct-iso）就是靠切它**，
/// 所以它不只是"一个环境变量"，是账号这个概念在 Claude 侧的载体。
pub(crate) const CONFIG_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";

/// 默认配置根在 `$HOME` 下的名字。
const HOME_DIR_NAME: &str = ".claude";

/// 解析配置根：`$CLAUDE_CONFIG_DIR` 优先，否则 `$HOME/.claude`。
///
/// Windows（只为编译/冒烟，真实目标是 Linux）：`$HOME` 缺失时退 `%USERPROFILE%\.claude`，
/// 再退 cwd 下的 `.claude`，让二进制至少起得来。
///
/// `S3` 从 `main.rs::resolve_claude_dir` 原样搬来（逻辑一字未改）。
pub(crate) fn resolve_home() -> PathBuf {
    if let Some(dir) = std::env::var_os(CONFIG_DIR_ENV) {
        return PathBuf::from(dir);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(HOME_DIR_NAME);
    }
    #[cfg(windows)]
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        return PathBuf::from(profile).join(HOME_DIR_NAME);
    }
    PathBuf::from(HOME_DIR_NAME)
}

/// `<home>/projects` —— 会话记录树的根。
///
/// `S3` 从 `common/paths.rs` 搬来。⚠ 它当初被建出来是为了**去重**（U2 实测五处副本，
/// 第五处是内联的、grep 函数名找不到），那个性质本件**原样保留** ——
/// 变的只是它住哪一层：目录名 `projects` 是 **Claude 的布局知识**，
/// 而 `common/` 的三条门槛第③条逐字写着「无域知识」。它当初就不该在那儿。
pub(crate) fn projects_root(home: &Path) -> PathBuf {
    home.join("projects")
}

/// `<home>/sessions` —— **pidfile 目录**（`<PID>.json`，判活用）。
///
/// ⚠ 与 Codex 的 `sessions/`（会话记录根）**同名不同物**。`S2` 就是因为这个
/// 把 `sessions/` 从 codex 的针里剔了出去。
pub(crate) fn sessions_root(home: &Path) -> PathBuf {
    home.join("sessions")
}
