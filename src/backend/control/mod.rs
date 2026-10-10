//! 控制面：会改变世界，或产出「要怎么改变世界」的计划。
//!
//! - [`fork_write`]：写文件系统（`O_EXCL` 新建一个 `<new-sid>.jsonl`）。写盘白名单的唯一真相是 `readonly_guard::WRITE_WHITELIST_MODULES`。
//! - [`files_write`]：文件管理的写面（`readonly_guard` 第三层：改，但每一处先过围栏、且只从文件管理面来）。
//! - [`exit_policy`]：写后端自己的那一份状态文件（`~/.cc-monitor/backend.json`）；不碰用户数据，`readonly_guard` 为它单开一层。
//! - [`tmux_hook`]：改 tmux server 状态（`tmux set-hook -g`）+ 发信号（`SIGUSR1`）。
//! - [`gate`]：Gate 2（identity）在本侧的承载，判定本身在 [`gate_rules`]。它只读 tmux，但归 control/：它是「能不能改这个会话」这个决策的一部分。
//! - [`identity_tag`]：把 `@ccm_sid` 打到 tmux 窗格上；触发来自 observe 侧的 pidfile inotify（`layering_guard` 里登记在案的 `observe → control` 跨层边）。
//! - [`capture_pane`]：抓一次某个窗格此刻那一屏（只读、只抓一次；`readonly_guard::capture_is_read_only` 逐元素钉 argv）。
//! - [`kill`]：杀一个 tmux 会话，过三道门（Gate 3 = `windows==1` 只给它），对 `#{session_id}` 句柄下手。
//! - [`launch`]：起 tmux 会话 / 往已有会话键入载荷（argv 直传不过 shell）；不 attach —— backend 在远端开不了你面前的窗。
//! - [`cli_args`]：CLI 面读入参的唯一一处（`cli_control` 与 `--resolve` 共用）。
//! - [`cli_control`]：控制面的第二个入口（一次性 CLI），不实现任何命令，只去 `inbound::REGISTRY` 查那条登记、跑它自己的 `run`。
//! - [`cc_bus`]：cc-bus 的基础命令（`bus-list` / `bus-send`），转调本机的 cc-bus 命令；不读 cc-bus 的任何数据文件，只把它的命令当接口。
//! - [`deploy_plan`]：部署计划（帧命令 `deploy-plan`）；只读那台，放字节归 monitor。
//! - [`resolve_query`]：产出 `CommandPlan`（「这个会话该怎么起」）—— 名字里有 `query`，但它是计划面，属于控制的前半。
//!
//! # 这一层不许引用 [`crate::observe`]
//!
//! 由 `crate::layering_guard` 机检。通用的安全读文件住 `common/fs.rs`，不靠给反向边开例外。

pub mod capture_pane;
pub(crate) mod cc_bus;
pub mod ccm;
pub mod cli_args;
pub mod cli_control;
pub mod deploy_plan;
pub mod exit_policy;
pub mod files_commit;
// 解压（`files-extract`）＋ 第三层「建链接」那一个动词的住址（复制链接本身 · 解压包里的链接）。
pub mod files_extract;
// 上传的块形：把送进暂存区的块拼成暂存件（SFTP 起始目录不是后端 home 时走这条）。
pub mod files_upload_chunks;
pub mod files_write;
// 分叉之后起：新会话的工作目录 / 号 / 终端从哪知道（逐格知道 ＋ 来源码 / 不知道 ＋ 原因码，不猜）。
pub(crate) mod fork_launch;
pub mod fork_write;
pub(crate) mod gate;
// §34 Gate 2 与 tmux 会话名两条规则（原共享 crate `gate-core`：monitor 那一侧的门删了，只剩本层用）。
pub(crate) mod gate_rules;
pub(crate) mod identity_tag;
pub(crate) mod kill;
pub(crate) mod launch;
// 起会话用哪个号：判定（`pick`）＋「这条会话上次用哪个号起的」（ccm 留便条、观测侧认便条记账；后端自有状态，第四层）。
pub(crate) mod launch_account;
// 起会话的计划与渲染：本机起会话 · `ccm …` 调用行 · 载荷 ＋ 外层 tmux 三格。
pub mod launch_render;
pub mod resident;
pub mod resolve_query;
pub(crate) mod session_batch;
pub(crate) mod session_new;
pub(crate) mod ship_text; // CLI `--text`：回包里写好的格（顶上 `text` · 每一处 `rows`）拼成字，所有命令通用
                          // 起新会话那一趟的票（期限到了再问一次 ⇒ 认出同一趟，不起第二个）。
pub(crate) mod session_new_ticket;
// 换号重启：查号 → 先压缩（可选）→ 停旧 → 同一终端名用新号起 → 等报出（帧面宿主 `faces/session_restart_face.rs`）。
pub(crate) mod session_restart;
// 终端管理 L1：名单 · 抓一屏 · 送字送键（两个前端共用，形状与宿主无关；这一版宿主是 tmux）。
pub(crate) mod terminals;
// 终端实时预览（L2）：订阅一个终端的画面，有变化就推一整屏（tmux 控制模式报变化 · 一帧在途等回执）。
pub(crate) mod terminal_follow;
pub mod tmux_hook;
// 传输台住本机常驻后端（第三层成员：本机下载落点的写 · 票表 · 进度帧）。
pub mod transfer;
