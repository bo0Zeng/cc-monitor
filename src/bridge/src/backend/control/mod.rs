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

// 〔C4e · 第四波 4C〕这里原来还有四份：`backend_kill` / `backend_launch` / `backend_send_keys`（杀会话 · 就地 resume ·
//   送键三个发送端）与 `command_args`（它们共用的参数构造器）。四条 Tauri 命令迁到界面（`src/tmux-control.ts`
//   经通道直接说后端的 `kill` / `launch` / `capture-pane`）之后，它们没了生产调用方，整份删了。
pub mod backend_route;
// 🔴 〔步 8 · 归属 2026-09-19〕从 `lib.rs` 顶层搬进来的三份 —— 它们干的全是控制面的活：
//    `backend_control`（每台机一个开关的命令层）· `cc_bus`（起 / 杀 / 发）·
//    `inbound_client`（流通道的 wire 客户端）。
//    ⚠ **同一批里有三份搬不动，原因写在这里免得下一个人再试一次**（三条都是**试过、红了**）：
//      · `backend_policy.rs` —— 它是 `record_death` 的唯一住址，而
//        `backend_policy_tests.rs::the_supervisor_itself_never_records_a_death` 逐字
//        「`backend/` 的生产段里出现了 `record_death(` ⇒ **判与记该在宿主层**，
//        `backend/` 那半**只搬证据**」。搬进来当场红；
//      · `ssh_source.rs` —— 生产段抓着 GUI 宿主的把手（`tauri::AppHandle` · `Emitter` · `.emit(`，
//        现打 3 处），搬进来会当场踩 `the_backend_layer_stays_host_agnostic`（定框 `C13`）；
//      · `sftp.rs` —— **试过、退回来了**：它的住址被 **6 条互锁的登记表**引着，其中
//        `sftp_move_ledger`〔散文墓碑〕（那张「SFTP 那 14 处拨号今天各自卡在哪」的挡路石底账；〔SR1b〕已退役）
//        把别的判据里的**逐字行**当成自己的「校验位」，而搬文件恰好会改写那几行；
//        它红的时候逐字说：「**别顺手把校验位改成新的原文 —— 那等于把「有人动过」
//        这件事抹掉**」。⇒ 照它说的停下来，交回去重读。
//      · `local_backend_host.rs` —— 生产段有 15 处平台 cfg / 平台原语，会踩
//        `the_backend_half_stays_platform_agnostic`（定框 `C10`：`platform/` 是唯一允许它们的地方，
//        而 monitor 侧今天还没有 `platform/`）。它自己的头注早就写着「这边是**宿主知识**」。
//      ⇒ 那两份是**解耦**的活（`设计/99 §4` 的 E/H），不是改名一刀能搬的。
pub mod backend_control;
// cc-bus 的**起 / 杀 / 发**
//    全是控制面的活（它的命令面逐条登记在 `plugin_class_registry`）。
pub mod cc_bus;
pub mod ccm_invocation;
pub mod tmux;
// 🔴 〔步 8 · 归属 2026-09-19〕从 `lib.rs` 顶层搬进来的。那张「表外但归这一半」的登记表（`EXTRA_BACKEND_FILES`，已随本拍整张删除）
//    当年逐字登记着它：「backend 流通道的 wire 客户端 …… **它不在 backend/ 下是历史位置，
//    不是它不属于这一半**」。⇒ 这一拍把那句话落成事实，那张表随之整张删掉（它空了）。
//    住 `control/` 而不是 `backend/` 根下：`every_file_under_backend_lives_on_a_capability_line`
//    逐字「根下只允许 mod.rs」，而它当年的三个消费者（杀会话 · 就地 resume · 送键的发送端，〔C4e〕已迁到界面）
//    全在这条能力线上；今天用它的是 `frame_query` / `cc_bus` / `backend_control` 与通道宿主。
pub mod inbound_client;
// 〔C1 · 09-24〕只读查询走已有长连接的发送端（history-* / accounts-* 八条帧命令）。
pub(crate) mod frame_query;
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
// `设计/90 §4 E`：外层 tmux 那三格的跨语言逐字节对拍（内层那半是上面 `launch_payload_parity`）。
#[cfg(test)]
mod launch_tmux_outer_parity;
// 〔C4e · 第四波 4C〕「创建路径不许铸出主路杀不掉的名字」与它的发现口径（`creation_detect`）原本挂在杀会话的发送端
//   `backend_kill.rs` 下面；发送端随杀会话迁到界面删了，**判据不跟着走** —— 它守的是「谁在建 tmux 会话 ↔ 后端 kill 的
//   形状门」，与 monitor 里有没有发送端无关。⇒ 文件原地不动，改挂在这一层（测试段）。
#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/backend_kill_creation_detect.rs"]
pub(crate) mod creation_detect;
#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/backend_kill_tests.rs"]
mod kill_name_tests;
