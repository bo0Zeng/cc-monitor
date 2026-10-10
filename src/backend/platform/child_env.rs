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

/// 后端内部的变量族（前缀）：起子进程原语对**每个**子进程无条件摘掉名字以它们打头的变量 —— 不论是不是本进程交的。
/// 会话环境里会混进来源别处的这几族（monitor 给终端窗口导的后端二进制路径 · 旧版常驻后端留在长寿 tmux server 全局环境里的监听口与钥匙文件），
/// 它们一路继承到 agent 起的每个工具进程（跑测试的那一个也在里面），台架漏进真环境就是从这里来的。往下传的只有头注那几格。
pub(crate) const INTERNAL_PREFIXES: [&str; 2] = ["CCM_BACKEND_", "CCM_LISTEN_"];

/// 上层登记进来、同样一律不往下传的那几个名字：各家 agent「我是哪个会话」的变量（适配层 `agents::self_sid_envs`）——
/// 常驻后端若是在某个会话里起的，它起的会话不许把那个会话错当成父。本层不认识 agent（最下层只朝下引），
/// 由入口在起第一个子进程之前登记一次（`agents::install_child_env_filter`）。
static ALSO_INTERNAL: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();

/// 登记那几个名字（只认第一次）。
pub(crate) fn also_internal(names: Vec<&'static str>) {
    ALSO_INTERNAL.get_or_init(|| names);
}

/// 这个名字是不是不往下传的（[`OWN_ENVS`] ∪ [`INTERNAL_PREFIXES`] 那几族 ∪ 上层登记的会话号变量）。
pub(crate) fn is_internal(name: &str) -> bool {
    OWN_ENVS.contains(&name)
        || INTERNAL_PREFIXES.iter().any(|p| name.starts_with(p))
        || ALSO_INTERNAL.get().is_some_and(|v| v.contains(&name))
}
