//! `platform/` —— 唯一允许出现平台原语与平台 `cfg` 的地方。
//!
//! 真判据是跨 target 编译（门禁 `winchk-backend`；CI 的 backend job 带 zig 跑 `--target x86_64-pc-windows-msvc`）——
//! 「`platform/` 之外出现平台 cfg 就红」那种位置扫描抓不到无条件编译的平台代码。跨 target 编译跑不到的地方由 [`cfgless_guard`]
//! 补一道源码面的判据：量不带 cfg 的平台代码（它不是跨 target 编译的替代品，完备性做不到，写在它的头注里）。
//! 本段不逐字写那个只在 unix 上有的文件模式扩展 trait 的名字：`readonly_guard` 的默认层连注释一起扫，那个名字在它的禁词表上。
//!
//! # 本层装了什么
//!
//! - [`proc`]：`/proc` 与进程身份（`pid_alive` / `proc_starttime` / `session_alive` / 两个 `/proc` 格式解析器 …）
//! - [`liveness`]：判活的纯判定表（`is_same_live_process`）—— 跨平台共用的那一半
//! - [`paths`]：`path_key`（NTFS 大小写折叠）＋ `temp_root` / `current_uid` 等路径语义
//! - [`pidwatch`]：`pidfd_open` + [`pidwatch::watch_pid_until_exit`]
//! - [`signal`]：`send_sigusr1`
//! - [`ssh_agent`]：连本机 ssh-agent（Unix 套接字 / Windows 命名管道）—— 拨号代理没配私钥路径时用
//! - [`stderr_fd`]：把本进程的 fd 2 换到一份文件上 · 问它多长（脱离常驻的后端把 stderr 落盘，`crate::stderr_log`）
//! - [`tcp_rtt`]：一条已连上的 TCP 的往返时间（问内核 `TCP_INFO`，不掐表）—— 压缩判准要它（`dial/connect.rs::compression_for`）
//! - [`lock`]：后端自有状态文件（第四层）的跨进程锁 —— 锁那份文件所在的目录（unix `flock` · Windows 命名互斥量）
//! - [`acct_view`]：账号库读盘的平台原语（不跟链接地看一项 · unix 权限位 · 这台做不做得了多账号）—— 只读
//! - [`fs`]：文件管理写面的两样原语：不覆盖改名（`rename_noreplace`）· 开文件不跟链接的旗（`NO_FOLLOW`）
//! - [`child_env`]：常驻后端自有的那几格环境的名字（[`child`] 起每个子进程都摘掉）
//! - [`child`]：后端起子进程的唯一原语（期限必填 · 超时杀整组 / 整个 Job · 自有环境无条件摘 · 脱离起 · ccm 最终那一跳）
//! - `win_proc`：Windows 上判活 / 起始时刻 / 等进程退出的 Win32 读法（只在 Windows 编译时存在，不写 intra-doc 链接）
//! - [`shell`]：shell 方言与「把一串命令交给这台的 shell」那一跳

pub(crate) mod acct_view;
#[cfg(test)]
#[path = "../../../tests/backend/platform/cfgless_guard.rs"]
mod cfgless_guard;
pub(crate) mod child;
/// 常驻后端自有的那几格环境的名字（监听口 · 钥匙文件 · 诊断文件）：起子进程原语无条件摘它们，帧面监听与诊断文件读它们。
pub(crate) mod child_env;
#[cfg(test)]
#[path = "../../../tests/backend/platform/fallback_guard.rs"]
mod fallback_guard;
pub(crate) mod fs;
pub(crate) mod liveness;
pub(crate) mod local_tz;
pub(crate) mod lock;
pub(crate) mod paths;
pub(crate) mod pidwatch;
pub(crate) mod proc;
pub(crate) mod shell;
pub(crate) mod signal;
pub(crate) mod ssh_agent;
pub(crate) mod stderr_fd;
pub(crate) mod tcp_rtt;
#[cfg(windows)]
pub(crate) mod win_proc;
#[cfg(windows)]
pub(crate) mod win_tables;
#[cfg(windows)]
pub(crate) mod win_tz;

/// ↗ 那一问的系统事实（已建立的 TCP 连接表 ＋ 进程表，一行 JSON）。只有 Windows 有这一问；别的平台 ⇒ `Err`。
pub(crate) fn connection_and_process_tables() -> Result<String, String> {
    #[cfg(windows)]
    {
        win_tables::connection_and_process_tables()
    }
    #[cfg(not(windows))]
    {
        Err("connection / process tables are only read on Windows".to_string())
    }
}
// Windows 判活那一臂在本机（Linux）够得着的那几半：纯换算 · 与 Linux 同契约的映射 · 唯一住址。
#[cfg(test)]
#[path = "../../../tests/backend/platform/win_proc_contract_tests.rs"]
mod win_proc_contract_tests;
