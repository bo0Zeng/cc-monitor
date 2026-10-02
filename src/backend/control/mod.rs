//! U3（2026-08-01）：**控制面** —— 会改变世界，或产出「要怎么改变世界」的计划。
//!
//! §1.1 第二条解耦线的另一半。三个模块各自改变的东西不同：
//!
//! - [`fork_write`]：**写文件系统**（`O_EXCL` 新建一个 `<new-sid>.jsonl`）。
//!   红线 I7 的第一个洞口。⚠ 本行原先写着「全 crate **唯一**的
//!   写盘白名单模块」—— 那句话今天不成立了，见下面 [`files_write`]。
//!   白名单从来就是**一张表**（`K-W2D` 09-10 起），唯一的真相在
//!   `readonly_guard::WRITE_WHITELIST_MODULES`，**别在散文里再抄一份数**。
//! - [`files_write`]（步 23b，09-19）：**写文件系统**（`O_EXCL` 在用户指定的文件管理目标下
//!   新建一份此前不存在的文件）。红线 I7 的第二个洞口，那个
//!   「带围栏的模块」。已接命令面（`files-create` ＋ 改动既有数据的五件），
//!   并已从白名单层搬到 `readonly_guard` 第三层（「改，但每一处先过围栏、且只从文件管理面来」）。
//! - [`exit_policy`]（B2 · 条 66）：**写后端自己的那一份状态文件**
//!   （`~/.cc-monitor/backend.json`，「退出行为」那个值）。它**不碰用户数据** ——
//!   `readonly_guard` 为它单开一层「后端自有状态文件」（按文件登记、动词闭集、只从 `inbound.rs` 进），
//!   理由与射程住那一层的登记表。
//! - [`tmux_hook`]：**改 tmux server 状态**（`tmux set-hook -g`）+ **发信号**（`SIGUSR1`）。
//! - [`gate`]（F03）：**§34 Gate 2（identity）在本侧的承载** —— 探一次 tmux 拿回
//!   `@ccm_sid` 与 `#{session_id}` 句柄，判定本身在 [`gate_rules`]（从共享 crate 收回本层）。
//!   它**只读** tmux，但归 control/ —— 因为它是「能不能改这个会话」这个**决策**的一部分
//!   （定框 C13：区别不在进程在哪，在它有没有决策权）。
//! - [`identity_tag`]（`U-NP④`）：**把 `@ccm_sid` 打到 tmux 会话上**（改 tmux server 运行期状态）。
//!   接的是 `shared/ccm` 那条**每会话一条、每秒一轮**的身份 poller 的班（用户原话
//!   「不要轮询」「ccm 做到必须走后端」）。触发时机来自 observe 侧的 pidfile inotify
//!   ⇒ 这是 `layering_guard` 里**第二条**登记在案的 `observe → control` 跨层边。
//!   探测复用 [`gate::probe`]，只在真要改值时多起一个 `tmux set-option`。
//! - [`capture_pane`]（`K-R86`，09-13）：**抓一次某个 tmux 会话此刻那一屏**（`capture-pane -p`）。
//!   **只读**，起进程一处（`tmux`，argv 直传不过 shell），已登记进
//!   `readonly_guard::spawn_registry::ALLOWED`；而「这一处只读」这件事那张表的键**分不出**
//!   （它只记「起什么程序」），所以另有 `readonly_guard::capture_is_read_only` 逐元素钉 argv。
//!   ⚠ 它**只抓一次就返回** —— 轮询归调用方，别在这里顺手做掉（`KR86D3`）。
//!   ⚠ 它归本层的理由**不是** `gate` 那条「有决策权」（定框 C13），逐条写在它自己的头注里。
//!   ★ **`K-R104`（09-13）：它有第二个面了** —— 帧面命令 `capture-pane`
//!   （`inbound::REGISTRY`），与 CLI 那条 `--capture-pane` **共用同一个本体**
//!   （`K33` 逐字「所有命令只许有一处，其他都是根据传参来调用」）。
//!   两个面**只差取参数与包信封的方式**：`capture_for_inbound` 收 `{name}`、回 `{name, screen}`；
//!   `run` 收一个位置参数、把那一屏原样写 stdout。起进程点**一处不增**。
//! - [`kill`]（F04a）：**杀一个 tmux 会话**。过 §34 三道门（Gate 3 = `windows==1` 只给它），
//!   对 `#{session_id}` 句柄下手而不是名字。⚠ 本模块落地 ≠ monitor 那条路已切过来（那是 F04b，C6 顺序）。
//! - [`launch`]（U8a-2b）：**起 tmux 会话 / 往已有会话键入载荷**（U8a 分解里的「平面 ②」）。
//!   起进程（`tmux`，argv 直传不过 shell），已登记进 `readonly_guard::spawn_registry`。
//!   **不 attach** —— 那是平面 ③，backend 在远端开不了你面前的窗。
//! - [`cli_control`]（P4d）：控制面的**第二个入口** —— 一次性 CLI。
//!   它**不实现任何命令**，只把 `--<name>` 还原成 `<name>` 去 `inbound::REGISTRY` 查那条登记、
//!   跑它自己的 `run`。⇒ 起进程点一处不增（本模块零 `Command::new`），
//!   `readonly_guard::spawn_registry` 的数不用动。
//! - [`cc_bus`]（P4f）：**cc-bus 的基础命令**（`bus-list` / `bus-send`）。
//!   起进程（转调本机的 cc-bus 命令，argv 直传不过 shell），**一处**，已登记进
//!   `readonly_guard::ALLOWED`。⚠ 它**不读** cc-bus 的任何数据文件 ——
//!   用户 08-13 明说「后面我可能要改ccbus」⇒ 只把它的**命令**当接口。
//! - [`deploy_plan`]（MIG-3b）：产出「那台的后端要不要换、换成哪一格」的部署计划（帧命令 `deploy-plan`）。
//!   **只读**那台（stat · read · 两条只读 exec），放字节归 monitor（经 `files` 链路）。归本层的理由同下面 `resolve_query`。
//! - [`resolve_query`]：产出 `CommandPlan`（「这个会话该怎么起」）。
//!   名字里有 `query` 但它不是观测 —— 账本 S14 明写它是 backend 的**计划面**。
//!   按「读 / 改变世界」这条线分，产计划属于控制的前半。
//!
//! # 这一层**不许**引用 [`crate::observe`]
//!
//! 由 `crate::layering_guard` 机检。U3 摸底时真有过一条反向边
//! （`fork_write` → `accounts_query::read_regular_capped`），**没有给它开例外** ——
//! 那个函数根本不是 observe 的域逻辑，是通用安全读文件，搬进 `common/fs.rs` 之后
//! 反向边自然消失。铁律 6：改结构让问题不存在。

pub mod capture_pane;
pub(crate) mod cc_bus;
pub mod ccm;
pub mod cli_control;
pub mod deploy_plan;
pub mod exit_policy;
pub mod files_commit;
// 解压（`files-extract`）＋ 第三层「建链接」那一个动词的住址（复制链接本身 · 解压包里的链接）。
pub mod files_extract;
// 上传的块形：把送进暂存区的块拼成暂存件（SFTP 起始目录不是后端 home 时走这条）。
pub mod files_upload_chunks;
pub mod files_write;
pub mod fork_write;
pub(crate) mod gate;
// §34 Gate 2 与 tmux 会话名两条规则（原共享 crate `gate-core`：monitor 那一侧的门删了，只剩本层用）。
pub(crate) mod gate_rules;
pub(crate) mod identity_tag;
pub(crate) mod kill;
pub(crate) mod launch;
// 起会话的计划与渲染（从 monitor 搬来）：本机起会话 · `ccm …` 调用行 · 载荷 ＋ 外层 tmux 三格。
pub mod launch_render;
pub mod resident;
pub mod resolve_query;
pub(crate) mod session_batch;
pub mod tmux_hook;
// 传输台住本机常驻后端（第三层成员：本机下载落点的写 · 票表 · 进度帧）。
pub mod transfer;
