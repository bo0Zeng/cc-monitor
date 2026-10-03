//! **常驻后端起子进程时，不把起它的那一方交给它自己的那几格环境传下去。**
//!
//! 起常驻后端的那一方（本机 monitor · 远端 `--resident-ensure`）交给它的监听口 · 钥匙文件 · 中转口 · 诊断文件，
//! 是**只给它自己用的**。它起的 tmux 在 server 还没起时会**起 server**，而 server 把调用者的环境整份拷成全局环境
//! （tmux(1)「GLOBAL AND SESSION ENVIRONMENT」）⇒ 不清掉的话，之后这个 server 里每个窗格的 shell · ccm · claude
//! 以及 claude 起的每个工具进程都带着它们。⇒ 起子进程的每一处在 `Command::new(…)` 之后紧跟 [`WithoutOwnEnv::without_own_env`]。
//!
//! 故意交下去的（不在 [`OWN_ENVS`] 里）：家（`CCM_DATA_DIR`，隔离跑时整棵进程树都该住同一个家）·
//! 中转口（`CCM_RELAY_PORT`：窗格里的 `ccm` 靠它找同机的中转，不是秘密；清掉它 ccm 会去连默认口）·
//! 设置里填的 Claude 目录（`CLAUDE_CONFIG_DIR`，给 agent 的）· 各起进程处自己显式 `.env(…)` 交的（清在前、交在后）。
//!
//! 例外只有 `control/ccm/mod.rs` 那两处：那是 `ccm` 一趟（终端里敲的、或常驻后端起的那个子进程）自己往下起，
//! 它沿用调用者的环境原样（常驻后端起它那一下已经清过）。

/// 常驻后端自有的那几格（起它的那一方只交给它自己用；中转口除外，见头注）。名字只住各自的模块，这里只列。
pub(crate) const OWN_ENVS: [&str; 3] = [
    crate::stream::listen::ENV_PORT,
    crate::stream::listen::ENV_TOKEN_FILE,
    crate::stderr_log::ENV,
];

/// 起子进程那一处紧跟着调：清掉 [`OWN_ENVS`]（不论是继承来的还是此前 `.env` 设的）。
pub(crate) trait WithoutOwnEnv {
    fn without_own_env(self) -> Self;
}

impl WithoutOwnEnv for std::process::Command {
    fn without_own_env(mut self) -> Self {
        for k in OWN_ENVS {
            self.env_remove(k);
        }
        self
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/common/child_env_tests.rs"]
mod tests;
