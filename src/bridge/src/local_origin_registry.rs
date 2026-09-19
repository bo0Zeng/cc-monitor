//! P4d-Y5：**「先查远端配置再干活」的地方，必须先分本机** —— 按形状收口，不按症状。
//!
//! # 这一族错误本轮出现了五次
//!
//! `load_remote_config_by_label(&origin)` 对 `<local>` 拿不到东西，于是报
//! **「未找到远端配置: `<local>`」** —— 一句与真实原因毫无关系的话。真实原因从来不是
//! 「配置没找到」，而是「这条路是远端专属的，本机根本不该走到这里」。
//!
//! 已逐个修过三处（`daemon_kill` / `list_remote_tmux` / `launch_remote_terminal`），
//! 而 `P3b` 的 E 阶段又量到第四处（`capture_remote_pane`）。
//!
//! ⇒ **别再一个一个修。** 一个一个修的问题不是慢，是它对「第六次」毫无办法：
//! 下一个人写下一处时，前面四处的教训对他不可见。
//!
//! # 本护栏钉什么
//!
//! 每一处 `load_remote_config_by_label(` 调用，**要么**它所在的函数在调用之前就分了本机，
//! **要么**它在 [`REMOTE_ONLY`] 里逐条登记「为什么 `<local>` 够不到这里」。
//!
//! ## 为什么是**位置**比较，不是「函数里提过 `LOCAL_ORIGIN`」
//!
//! 后者是**块粒度**，而块粒度在这个仓里栽过：一句该红的话紧挨着一句含关键词的话就能蒙混
//! （`launch.rs` 那条散文守卫按块扫时的真实读数）。这里的失败形状一模一样 ——
//! 函数末尾写一句「本机走另一条」的注释，不影响开头那句照样对 `<local>` 报假话。
//! ⇒ 分本机的动作必须**在**那次调用之前。
//!
//! ## 它挡什么、不挡什么（如实登记）
//!
//! - **挡**：新写一处「先查远端配置」而不先分本机、也不登记 ⇒ 红。
//! - **不挡**：分了本机但**分错了**（本机分支自己的逻辑是坏的）。本护栏只保证
//!   「本机这条路被单独想过一次」，不保证想对了。这条边界写在这里，
//!   免得下一个人以为它保证了更多。
//! - **不挡（08-12 由变异逼出来的第二格）**：**隔了一层包装的调用**。
//!   `cc_bus.rs::cfg_of` 就是这种 —— 它自己直接调，所以本护栏看得见它；
//!   而 `cc_bus_send` / `cc_bus_spawn` 调的是 `cfg_of`，**本护栏对它们是瞎的**。
//!   实测：拿掉 `cc_bus_send` 的本机拒绝，本条**照样绿**（P4a 的变异 M5）。
//!   ⇒ 那两条今天由两样东西兜着：`cfg_of` 自己的兜底分支（结构）+
//!   `cc_bus::tests::the_write_face_refuses_local_before_it_asks_for_a_remote_config`（位置）。
//!   要把包装那一层也纳进来，得先有一份「哪些函数是远端配置的包装」的表 ——
//!   **今天没有，就别假装有。**

#[cfg(test)]
const CALL: &str = "load_remote_config_by_label(";

/// 本机**结构性够不到**的调用点，逐条登记：(文件, 函数, 为什么 `<local>` 到不了这里)。
///
/// ⚠ 登记的是「够不到」，不是「还没做」。**「以后再说」不是理由** —— 那种进 [`TRIAGE_DEBT`]。
#[cfg(test)]
const REMOTE_ONLY: &[(&str, &str, &str)] = &[];

/// ★★ **本轮没有逐条量过的存量**（`P4d-Y5` 08-12 立表 19 条；`P4a` 08-12 还掉 3 条 ⇒ 16；
/// `K-R56` 09-11 还掉 1 条 —— `tmux.rs::tmux_send_keys`，它是 `K-R54` 逐处裁定表第 1 处
/// 点名的那一条「`kill` 有的『本机不许回落』保护，`send-keys` 没有」⇒ **15**）。
///
/// # 为什么它不是 [`REMOTE_ONLY`] 的一部分
///
/// 这 19 处**我没有逐个读过**。给它们各编一句「本机够不到，因为……」很容易，
/// 而那正是 `P3b` 刚刚立判据去抓的东西：**B 类假理由（写下时就没验证过）**。
/// B 类的特点是时间线扫不到它 —— 它从来没真过，所以没有「哪天变假的」那一刻可查。
/// 在这里造 19 条，等于亲手制造一批下次审计要花力气才能识别的假话。
///
/// ⇒ 本表逐字承认：**这是欠账，不是裁定。** 它对本护栏的作用只有一个 ——
/// 挡住**新增**。存量该怎么处置，归 ROADMAP 那件事，不藏在护栏的白名单里。
///
/// ⚠ **只许变短。** 下面那个数是等号不是地板：少一条要回来改它（那是好事，说明有人真去量了），
/// 多一条同样会红（新增的必须走 `REMOTE_ONLY` 或者去加本机分支）。
#[cfg(test)]
const TRIAGE_DEBT: &[(&str, &str)] = &[
    // 🔴 **`K-R104`（09-13）：用量探针那一行还掉了，不是删掉。**
    //    它欠的是「这一处**够不够得到本机**没人量过」。今天量得出来了，而且答案变了：
    //    编排搬上后端帧面之后，`account_usage`（远端）与 `account_usage_local`（本机）  〔散文墓碑〕
    //    **是同一个函数**，只差一个 origin —— `<local>` 也是一个 origin，`client_for` 两侧都答得出。
    //    ⇒ 它不再是「只服务远端」的那一族。**表只许变短，这一次它真的短了。**
    ("accounts.rs", "cfg_for"),
    ("ccm_probe.rs", "probe_ccm_cli"),
    ("hooks_diag.rs", "diagnose_remote_cc_bus_hooks"),
    ("launch.rs", "build_remote_ssh_ps_command"),
    ("mcp.rs", "list_remote_mcp_project_dirs"),
    ("mcp.rs", "read_remote_mcp_servers"),
    ("mcp.rs", "read_remote_project_mcp"),
    ("mcp.rs", "remove_remote_mcp_server"),
    ("mcp.rs", "write_remote_mcp_server"),
    ("port_forward.rs", "start_forward"),
    ("remote_branch.rs", "create_remote_branch_session"),
    ("remote_history.rs", "require_cfg_by_label"),
    ("ssh_source.rs", "connect_via_jump"),
    ("tmux.rs", "list_remote_tmux"),
    // `K-R56`（09-11）：`tmux.rs::tmux_send_keys` 从这里**还掉了** —— 它现在在
    // `load_remote_config_by_label` 之前分本机（`Routed::NoChannel` 那一臂的早退）。
    // 行为那一半由 `tmux::tests::the_local_send_keys_never_falls_back_to_ssh` 钉着。
];

#[cfg(test)]
#[path = "../../../tests/bridge/local_origin_registry_tests.rs"]
mod tests;
