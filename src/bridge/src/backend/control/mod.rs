//! `backend/control/` —— **写/控制面**（§1.1 的能力线在 monitor 侧的对侧）。
//!
//! 今天住在这里的是「起一个会话」那条路的 monitor 半边：
//! 前端发结构化请求 → wire 适配 → 渲染出一条要在**用户自己的终端里** exec 的命令串。
//!
//! # 为什么渲染 shell 串是这一侧的事，而不是后端的
//!
//! §1.3 把最终 exec 钉在**用户自己的终端进程**里（pid 必须等于 pidfile 名、tty/Ctrl-C
//! 必须落在 agent 上）；而 U8a-2b 把后端的执行面定成 **argv 直传、不过 shell**
//! （`src/backend/control/launch.rs` 头注逐字写着「这条路根本不过 shell」）。
//! ⇒ **「渲染一条 shell 命令串」永远属于开终端的那一侧。** 这不是权宜之计，
//! 也不是「将来还要搬去后端」—— 是它本来的归属地（P4a 摸底把这条理由换硬了）。

pub mod backend_kill;
pub mod backend_launch;
pub mod backend_route;
pub mod backend_send_keys;
pub mod ccm_invocation;
// 🔴 〔步 8 · 归属 2026-09-19〕从 `lib.rs` 顶层搬进来的。那张「表外但归这一半」的登记表（`EXTRA_BACKEND_FILES`，已随本拍整张删除）
//    当年逐字登记着它：「backend 流通道的 wire 客户端 …… **它不在 backend/ 下是历史位置，
//    不是它不属于这一半**」。⇒ 这一拍把那句话落成事实，那张表随之整张删掉（它空了）。
//    住 `control/` 而不是 `backend/` 根下：`every_file_under_backend_lives_on_a_capability_line`
//    逐字「根下只允许 mod.rs」，而它的三个消费者（`backend_kill` / `backend_launch` /
//    `backend_send_keys`）全在这条能力线上。
pub mod inbound_client;
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
