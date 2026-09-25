//! U2（2026-08-01）：**`platform/` —— 唯一允许出现平台原语与平台 `cfg` 的地方**（§1.1 第一条解耦线）。
//!
//! # 判据不是「cfg 出现在哪」
//!
//! 计划自审 §0.5-3 已订正过一次：「`platform/` 之外出现平台 cfg 就红」这条机检**是安慰剂** ——
//! 本 crate 在 Windows 上编不过的 12 个错里，头号的 `pidfd_open` **根本没有 cfg**，
//! 它是无条件编译的 Linux-only 代码。
//!
//! **真判据只有一个：跨 target 编译。**
//! `cargo check --all-targets --target x86_64-pc-windows-msvc` 必须绿，且要进 CI
//! （在 ubuntu 上就能跑，`check` 不链接）。那是 **U4** 的 DoD，不是本模块的。
//!
//! # 🔴 〔`K-R52` 09-11 补〕上面那句话**今天仍然对，但它不够** —— 那道真判据够不着这里
//!
//! 「唯一真判据是跨 target 编译」写下时是真的，而 09-10 有一处
//! （已删的那条按需拉取路里「落一个可执行文件」那一跳 —— 引了 `std::os::unix::fs` 里那个给 `mode(…)` 的
//! 扩展 trait，**一个 cfg 都没有**）**照样漏了进来，且当天门禁全绿**。
//! ⚠ 这里刻意不把那个 trait 的名字逐字写出来：`readonly_guard` 的默认层**连注释一起扫**
//!   （那是它 fail-closed 的设计），而那个名字在它的禁词表上、本文件不在写面白名单里
//!   —— 写出来当场红。实测过一次，如实记在这里。
//! 原因是那道真判据在本机与 CI
//! 上各有一个够不着的理由，两条都是现打的读数、不是推断：
//!
//! - **本机**：`tests/scripts/verify-committed-state.sh` 的 `backend-win` 卡在 `ring` 的 C 构建脚本上
//!   （09-11 沙箱实测：EXIT=101 · `failed to find tool "lib.exe"` ·
//!   `Checking cc-monitor-backend` 命中 **0** ⇒ 根本没走到我们的代码）。
//!   而沙箱门禁那道 `winchk` 射程逐字是 `-p monitor`，**不含本 crate**。
//! - **CI**：backend job 那一步带 zig、口径是对的，但它只看 `origin/main`，
//!   而本仓的红线是**不 push** —— 09-11 现打：最后一趟 CI 跑在 `14e0f05`（09-10 17:16Z，
//!   `success`，那一步逐条绿），而**那个提交里根本没有 `acquire.rs` 这份文件**；
//!   缺陷住在本地领先的 25 拍里。⇒ **CI 没有红，是因为它从来没见过那一拍。**
//!
//! ⇒ 补一道**源码面**的判据：[`cfgless_guard`]。它量的正是上面那句「头号错根本没有 cfg」
//! 所描述的那一族 —— **不带 cfg 的平台代码**。
//! 🔴 它**不是**跨 target 编译的替代品（完备性做不到，那一格如实写在它的头注里），
//! 它买到的是「在那道真判据跑不到的地方也能出声」。
//!
//! # 本层现在装了什么
//!
//! - [`proc`]：`/proc` 与进程身份（`pid_alive` / `proc_starttime` / `proc_claude_config_dir` /
//!   `session_alive` / 两个 `/proc` 格式解析器 …）
//! - [`liveness`]：判活的**纯判定表**（`is_same_live_process`）—— U4a 从 `proc` 上提，
//!   因为它是 Windows 侧要复用的那一半（读事实的方式不同，判定规则相同）
//! - [`paths`]：`path_key`（NTFS 大小写折叠 —— **路径**语义，不是 `/proc`）
//!   ＋ `temp_root` / `current_uid`（`K-R55` 09-11 从 `observe/watcher.rs` 下沉）
//! - [`pidwatch`]：`pidfd_open` + [`pidwatch::watch_pid_until_exit`]
//! - [`signal`]：`send_sigusr1`（U3 从 `control/tmux_hook.rs` 下沉）· 〔RM1f〕`kill_group`（插件口可打断的那一形被丢时收掉整组子进程）
//! - [`ssh_agent`]：连本机 ssh-agent（Unix 套接字 / Windows 命名管道）—— 〔C2〕拨号代理没配私钥路径时用
//! - [`stderr_pipe`]：〔NT2 · S1〕把本进程的 fd 2 换成一根管子的写端、交回读端（脱离常驻的后端把 stderr 落盘，`crate::stderr_log`）
//! - [`tcp_rtt`]：〔NT1〕一条已连上的 TCP 的往返时间（问内核 `TCP_INFO`，不掐表）—— 压缩判准要它（`dial/connect.rs::compression_for`）
//! - [`detach`]：〔RM1a〕把要起的子进程放进自己的进程组（远端那台的中转不许随 SSH 断而没了）· 〔RM1f〕插件口可打断的那一形也用它（让「杀一组」有组可杀）
//! - `win_proc`：〔WN1 · U4b〕Windows 上判活 / 起始时刻 / 等进程退出的 Win32 读法（`OpenProcess` 一族），
//!   只在 Windows 编译时存在（刻意不写 intra-doc 链接：本机 Linux 上它不存在，链接会悬空）
//! - [`shell`]：`posix_shell`（`K-R55` 09-11 从 `observe/watcher.rs` 下沉 ——
//!   那两处 `Command::new("sh")` 正是 `K-R52` 立表时挂在 A2「真漏」堆上的头两条）
//!
//! **前三个是从 `watcher.rs` 逐字搬来的**（U2 纯重构，行为逐字不变）。
//! [`signal`] 不是 —— 它是**重写**：原实现内联在 `tmux_hook` 里、失败时 `return 0`；
//! 现在返回 `bool` 由调用方丢弃。语义等价（两条路径旧版都返回 `0`，那个 `return` 是纯提前返回），
//! 但「逐字搬来」这句话覆盖不到它，故单列。

#[cfg(test)]
#[path = "../../../tests/backend/platform/cfgless_guard.rs"]
mod cfgless_guard;
pub(crate) mod detach;
#[cfg(test)]
#[path = "../../../tests/backend/platform/fallback_guard.rs"]
mod fallback_guard;
pub(crate) mod liveness;
pub(crate) mod paths;
pub(crate) mod pidwatch;
pub(crate) mod proc;
pub(crate) mod shell;
pub(crate) mod signal;
pub(crate) mod ssh_agent;
pub(crate) mod stderr_pipe;
pub(crate) mod tcp_rtt;
#[cfg(windows)]
pub(crate) mod win_proc;
// 〔WN1 · U4b〕Windows 判活那一臂在本机（Linux）够得着的那几半：纯换算 · 与 Linux 同契约的映射 · 唯一住址。
#[cfg(test)]
#[path = "../../../tests/backend/platform/win_proc_contract_tests.rs"]
mod win_proc_contract_tests;
