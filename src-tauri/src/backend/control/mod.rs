//! `backend/control/` —— **写/控制面**（§1.1 的能力线在 monitor 侧的对侧）。
//!
//! 今天住在这里的是「起一个会话」那条路的 monitor 半边：
//! 前端发结构化请求 → wire 适配 → 渲染出一条要在**用户自己的终端里** exec 的命令串。
//!
//! # 为什么渲染 shell 串是这一侧的事，而不是 daemon 的
//!
//! §1.3 把最终 exec 钉在**用户自己的终端进程**里（pid 必须等于 pidfile 名、tty/Ctrl-C
//! 必须落在 agent 上）；而 U8a-2b 把 daemon 的执行面定成 **argv 直传、不过 shell**
//! （`remote-daemon-proto/src/control/launch.rs` 头注逐字写着「这条路根本不过 shell」）。
//! ⇒ **「渲染一条 shell 命令串」永远属于开终端的那一侧。** 这不是权宜之计，
//! 也不是「将来还要搬去 daemon」—— 是它本来的归属地（P4a 摸底把这条理由换硬了）。

pub mod ccm_invocation;
pub mod daemon_kill;
pub mod daemon_launch;
pub mod launch_wire;
pub mod local_backend;
pub mod payload;

#[cfg(test)]
mod agent_profile_parity;
#[cfg(test)]
mod gate2_parity;
#[cfg(test)]
mod launch_cli_parity;
#[cfg(test)]
mod launch_payload_parity;
