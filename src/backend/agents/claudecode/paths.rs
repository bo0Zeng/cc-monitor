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
pub fn resolve_home() -> PathBuf {
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

/// 一个路径**在不在 Claude 的那几棵树里** —— `~/.claude*` 那个星号的**唯一住址**。
///
/// 〔步 23b · 2026-09-19〕`设计/60 §6.5.2 A` 给新的写模块定的围栏逐字是
/// 「写点……**不许**落进 `~/.claude*` 那几棵树」。这句话里的**布局知识**
/// （根叫什么、星号包含哪些）归本层 —— `control/` 是通用层，
/// 它不该知道这个目录叫什么（`agent_locality_guard` 的针就钉在这上面）。
///
/// 两条各治一形，任一命中即真：
///
/// 1. **在配置根之下**（含它自己）。根由 [`resolve_home`] 现打解析，所以账号隔离
///    把根切到一个**不带这个名字**的地方时（`cc-acct-iso` 每天在做的事），
///    这一条仍然认得出来。
/// 2. **任何一段以 [`HOME_DIR_NAME`] 开头**。它兜的是第 1 条够不着的那些树：
///    此刻**没有被选中**的那几个账号目录、同名的备份文件、工程里的那一份。
///
/// ⚠ [`Path::starts_with`] 是**按段**比的 ⇒ 同前缀的兄弟目录不会被第 1 条误判；
/// 而它会被第 2 条拦下 —— 这是刻意的，星号逐字包含它。
///
/// ⚠ **它不解 symlink**：入参是什么就判什么。要挡「目录里藏一条指过去的链接」，
/// 得由调用方先把路径解成真路径再来问（`control/files_write.rs` 的围栏② 就是那么做的）。
pub fn is_inside_tree(home: &Path, target: &Path) -> bool {
    if target.starts_with(home) {
        return true;
    }
    target.components().any(|c| match c {
        std::path::Component::Normal(seg) => {
            seg.to_str().is_some_and(|s| s.starts_with(HOME_DIR_NAME))
        }
        _ => false,
    })
}
