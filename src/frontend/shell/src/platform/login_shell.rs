//! 本机「新开一个终端」跑的是哪一族 shell〔阶段 H：；原是 `ccm_probe.rs` · `cc_bus_deploy.rs` · `profile_installer.rs` 里的 cfg 分身〕。
//!
//! 只答这一件平台事实。拿它去定「本机 `ccm` 怎么问」「装 cc-bus 前的预检怎么说」「用户级 PATH 那一格做不做」的规则都留在调用方。

/// 这台系统上新开一个终端窗口时跑的 shell。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoginShell {
    /// POSIX 登录 shell：`bash -lic` 读得到用户 rc 改过的 `PATH`。
    Posix,
    /// Windows：PowerShell；`PATH` 由注册表里机器级 ＋ 用户级拼成（有「用户级 PATH」这一档），没有 `bash -lic` 那条路。
    PowerShell,
}

impl LoginShell {
    /// 有没有「用户级 PATH」这一档（只有 PowerShell 那一族有：注册表 `HKCU\Environment`）。
    pub fn has_user_level_path(self) -> bool {
        matches!(self, LoginShell::PowerShell)
    }
}

/// 本机那一族。
pub const LOGIN_SHELL: LoginShell = if cfg!(windows) {
    LoginShell::PowerShell
} else {
    LoginShell::Posix
};

/// 界面上「cc-monitor 这一份 ccm 在哪」那一句的写法：POSIX 写 `$HOME/…`（与生成进命令里的那一形同）；
/// Windows 写绝对路径（`$HOME/…` 不是那边说路径的样子）。
pub fn ccm_entry_shown(abs: &std::path::Path, home_form: &str) -> String {
    LOGIN_SHELL.entry_shown(abs, home_form)
}

impl LoginShell {
    /// [`ccm_entry_shown`] 按这一族答（判据两族都喂得到）。
    pub fn entry_shown(self, abs: &std::path::Path, home_form: &str) -> String {
        match self {
            LoginShell::PowerShell => abs.display().to_string(),
            LoginShell::Posix => home_form.to_string(),
        }
    }
}
