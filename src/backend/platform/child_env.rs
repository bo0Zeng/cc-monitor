//! **常驻后端自有的那几格环境**（数据）。
//!
//! 起常驻后端的那一方（本机 monitor · 远端 `--resident-ensure`）交给它的常驻开关 · 诊断文件，
//! 是**只给它自己用的**。它起的 tmux 在 server 还没起时会**起 server**，而 server 把调用者的环境整份拷成全局环境
//! （tmux(1)「GLOBAL AND SESSION ENVIRONMENT」）⇒ 不清掉的话，之后这个 server 里每个窗格的 shell · ccm · claude
//! 以及 claude 起的每个工具进程都带着它们。⇒ 起子进程原语（`platform/child.rs`）对每个子进程无条件摘掉本表这几格。
//!
//! 故意交下去的（不在 [`OWN_ENVS`] 里）：家（`CCM_DATA_DIR`，隔离跑时整棵进程树都该住同一个家）·
//! 中转口（`CCM_RELAY_PORT`：窗格里的 `ccm` 靠它找同机的中转，不是秘密；清掉它 ccm 会去连默认口）·
//! 设置里填的 Claude 目录（`CLAUDE_CONFIG_DIR`，给 agent 的）。

/// 宿主告诉后端「你是常驻的那一个」的 env 名（值 `1`；帧面监听 `stream/listen.rs` 读它）。
/// 听哪个套接字不交：按家算（`relay_route_core::listen_socket_for`），宿主与后端同一个函数。
pub(crate) const RESIDENT: &str = "CCM_RESIDENT";

/// stderr 诊断文件完整路径的 env 名（`stderr_log.rs` 读它）。
pub(crate) const STDERR_LOG: &str = "CCM_BACKEND_STDERR_LOG";

/// 常驻后端自有的那几格（起它的那一方只交给它自己用；中转口除外，见头注）。名字住这里（最下层），读它们的上层引这里。
pub(crate) const OWN_ENVS: [&str; 2] = [RESIDENT, STDERR_LOG];
