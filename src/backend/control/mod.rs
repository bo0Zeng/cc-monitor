//! 控制面：会改变世界，或产出「要怎么改变世界」的计划。
//!
//! - [`fork_write`]：写文件系统（`O_EXCL` 新建一个 `<new-sid>.jsonl`），经只会新建的 `NewFile`。
//! - [`exit_policy`]：写后端自己的那一份状态文件（`~/.cc-monitor/backend.json`）；不碰用户数据，`readonly_guard` 为它单开一层。
//! - [`tmux_hook`]：改 tmux server 状态（`tmux set-hook -g`）+ 发信号（`SIGUSR1`）。
//! - [`gate`]：Gate 2（identity）在本侧的承载，判定本身在 [`gate_rules`]。它只读 tmux，但归 control/：它是「能不能改这个会话」这个决策的一部分。
//! - [`identity_tag`]：把 `@ccm_sid` 打到 tmux 窗格上；触发来自 observe 侧的 pidfile inotify（watcher 只报一声，`main.rs` 把 `identity_tag::tag_seen` 接成 `observe::watcher::Reactions::tag`）。
//! - [`capture_pane`]：抓一次某个窗格此刻那一屏（只读、只抓一次；`readonly_guard::capture_is_read_only` 逐元素钉 argv）。
//! - [`kill`]：杀一个 tmux 会话，过三道门（Gate 3 = `windows==1` 只给它），对 `#{session_id}` 句柄下手。
//! - [`launch`]：起 tmux 会话 / 往已有会话键入载荷（argv 直传不过 shell）；不 attach —— backend 在远端开不了你面前的窗。
//! - [`cli_args`]：CLI 面读入参的唯一一处（`stream::cli` 经它读）。
//! - [`cc_bus`]：cc-bus 的读命令（`bus-list` / `bus-inbox`），转调本机的 cc-bus 命令；不读 cc-bus 的任何数据文件，只把它的命令当接口。
//! - [`cc_bus_envelope`]：cc-bus 信封的唯一一处校验（`bus-envelope`；`cc-bus send` 发之前问它，不合格当场拒）。
//! - [`cc_bus_scope`]：cc-bus 的范围（组）：同组才送得到，名单读不到就拒。
//! - [`deploy_plan`]：给那台装 / 升级后端（帧命令 `deploy-install`；编排住 `deploy` crate）。
//!
//! # 这一层不许引用 [`crate::observe`]
//!
//! 由 `crate::layering_guard` 机检。通用的安全读文件住 `platform/fs/ro.rs`，不靠给反向边开例外。

// 待答登记表（会话里弹的批准 / 提问 / 批计划经答题 hook 挂进来，任一出口答）＋ hook 那一头的客户端（`--answer-hook`）。
pub mod asks;
pub(crate) mod bookmarks; // 这台的书签（`~/.cc-monitor/bookmarks.json`）：读 · 锁里读—改—写，文件窗口与手机按机器名问
pub mod capture_pane;
pub(crate) mod cc_bus;
pub(crate) mod cc_bus_envelope;
pub(crate) mod cc_bus_scope; // cc-bus 的范围：组（`bus-envelope` 核完各格之后判）
pub(crate) mod cc_bus_wire; // cc-bus 那三条帧命令的形状（入参 · 回包 · 码表 · 怎么跑）
pub mod ccm;
pub mod cli_args;
pub mod deploy_plan;
pub mod exit_policy;
pub mod feeds; // 各台的常驻流（机器表里要连的那几台各一条；那一态直接进机器状态 · 装后端在 connect 那一支）
pub(crate) mod machine_prefs;
pub use machine_prefs::apply_agent_homes;
pub(crate) mod machine_menu; // 命令面板打机器名：`machine-find` 认那台 · `machine-menu` 那台能做的几样
pub(crate) mod machine_state;
pub(crate) mod machines;
pub(crate) mod monitor_board; // 监控板的成品：每台写行 · 各台合成（分组 · 组内先后 · 组头 · 汇总；宿主 `faces/monitor_board_face.rs`）
                              // 分叉之后起：新会话的工作目录 / 号 / 终端从哪知道（逐格知道 ＋ 来源码 / 不知道 ＋ 原因码，不猜）。
pub(crate) mod fork_launch;
pub mod fork_write;
pub(crate) mod gate;
// §34 Gate 2 与 tmux 会话名两条规则（原共享 crate `gate-core`：monitor 那一侧的门删了，只剩本层用）。
pub(crate) mod gate_rules;
pub mod identity_tag;
pub(crate) mod kill;
pub(crate) mod launch;
// 起会话用哪个号：判定（`pick`）＋「这条会话上次用哪个号起的」（ccm 留便条、观测侧认便条记账；后端自有状态，经 `Own`）。
pub mod launch_account;
// 起会话的计划与渲染：本机起会话 · `ccm …` 调用行 · 载荷 ＋ 外层 tmux 三格。
pub mod launch_render;
pub mod place_self;
pub mod resident;
// 编辑已发的一句：调出那一家的回退菜单 · 每按一键前读屏核对 · 送改好的那句 · 读回主线（按法认法住适配层 `RewindFace`）。
pub(crate) mod rewind;
pub(crate) mod session_arrive; // 帧命令 `session-arrive`：等起好的会话报到（认法住观测层，抓屏住 terminals）
pub(crate) mod session_batch;
pub(crate) mod session_open;
pub(crate) mod session_set; // 帧命令 `set-plan`：补足一组会话的判定（每行已有几个 · 再起几个 · 进哪个组）
pub(crate) mod ship_text; // CLI `--text`：回包里写好的格（顶上 `text` · 每一处 `rows`）拼成字，所有命令通用
                          // 起新会话那一趟的票（期限到了再问一次 ⇒ 认出同一趟，不起第二个）。
pub(crate) mod session_new_ticket;
// 换号重启：查号 → 先压缩（可选）→ 停旧 → 同一终端名用新号起 → 等报出（帧面宿主 `faces/session_restart_face.rs`）。
pub(crate) mod session_restart;
// 终端管理 L1：名单 · 抓一屏 · 送字送键（两个前端共用，形状与宿主无关；这一版宿主是 tmux）。
pub(crate) mod terminals;
pub(crate) mod terminals_board; // 「终端」视图那张名单的成品：每台写行 · 各台合成（宿主 `faces/terminals_board_face.rs`）
                                // 终端管理 L1 三条的线上形（入参 · 回包 · 码）与 typed 登记。
pub(crate) mod terminals_wire;
// 终端实时预览（L2）：订阅一个终端的画面，有变化就推一整屏（tmux 控制模式报变化 · 一帧在途等回执）。
pub(crate) mod terminal_follow;
pub mod tmux_hook;
// 传输台住本机常驻后端（文件管理写面的成员：本机下载落点的写 · 票表 · 进度帧）。
pub mod transfer;
pub(crate) mod transfer_copy; // 两台之间复制一组（经本机中转，目录整棵、逐项结局）

// 那几条命令被拒时给人看的那一句（各条命令自己的「码 → 句」表）。
#[cfg(test)]
#[path = "../../../tests/backend/control/said_tests.rs"]
mod said_tests;

// 监听口拒绝理由闭集那一条（要问本层小中继的 `relay_refusal`；帧面零件那个包够不到这里）。
#[cfg(test)]
#[path = "../../../tests/backend/wire/listen_on_control_tests.rs"]
mod listen_on_control_tests;

// 宿主口判据里要问控制面的那几条（名单那一形 · 持有者那一句 · 回滚上限；宿主口那个包够不到这里）。
#[cfg(test)]
#[path = "../../../tests/backend/host/on_control_tests.rs"]
mod host_on_control_tests;

// 账号域里在 app 里登录一个号那几条（连接号跟生产同一个发号器 `asks::Desk` 取；账号域那个包够不到这里）。
#[cfg(test)]
#[path = "../../../tests/backend/accounts/sign_in_tests.rs"]
mod accounts_sign_in_tests;
