//! 本机「新开一个终端」跑的是哪一族 shell。只答这一件平台事实；拿它去定「用户级 PATH 那一格做不做」的规则留在调用方。

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
