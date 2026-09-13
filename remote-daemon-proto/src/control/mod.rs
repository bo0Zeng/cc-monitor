//! U3（2026-08-01）：**控制面** —— 会改变世界，或产出「要怎么改变世界」的计划。
//!
//! §1.1 第二条解耦线的另一半。三个模块各自改变的东西不同：
//!
//! - [`fork_write`]：**写文件系统**（`O_EXCL` 新建一个 `<new-sid>.jsonl`）。
//!   全 crate **唯一**的写盘白名单模块，红线 I7 的那个洞口。
//! - [`tmux_hook`]：**改 tmux server 状态**（`tmux set-hook -g`）+ **发信号**（`SIGUSR1`）。
//! - [`gate`]（F03）：**§34 Gate 2（identity）在本侧的承载** —— 探一次 tmux 拿回
//!   `@ccm_sid` 与 `#{session_id}` 句柄，判定本身在共享的 `gate-core`（定框 C1）。
//!   它**只读** tmux，但归 control/ —— 因为它是「能不能改这个会话」这个**决策**的一部分
//!   （定框 C13：区别不在进程在哪，在它有没有决策权）。
//! - [`identity_tag`]（`U-NP④`）：**把 `@ccm_sid` 打到 tmux 会话上**（改 tmux server 运行期状态）。
//!   接的是 `shared/ccm` 那条**每会话一条、每秒一轮**的身份 poller 的班（用户 08-14 裁定
//!   「不要轮询」「ccm 做到必须走 daemon」）。触发时机来自 observe 侧的 pidfile inotify
//!   ⇒ 这是 `layering_guard` 里**第二条**登记在案的 `observe → control` 跨层边。
//!   探测复用 [`gate::probe`]，只在真要改值时多起一个 `tmux set-option`。
//! - [`capture_pane`]（`K-R86`，09-13）：**抓一次某个 tmux 会话此刻那一屏**（`capture-pane -p`）。
//!   **只读**，起进程一处（`tmux`，argv 直传不过 shell），已登记进
//!   `readonly_guard::spawn_registry::ALLOWED`；而「这一处只读」这件事那张表的键**分不出**
//!   （它只记「起什么程序」），所以另有 `readonly_guard::capture_is_read_only` 逐元素钉 argv。
//!   ⚠ 它**只抓一次就返回** —— 轮询归 `K-R87`，别在这里顺手做掉（`KR86D3`）。
//!   ⚠ 它归本层的理由**不是** `gate` 那条「有决策权」（定框 C13），逐条写在它自己的头注里。
//! - [`oneshot_session`]（`K-R87`，09-13）：**起一个到点自己会死的 tmux 会话**
//!   （`new-session -d` ＋ 一条 `setsid sh -c 'sleep N; tmux kill-session …'` 的外部看门狗）。
//!   起进程**两处**（`tmux` 一处、看门狗那个 launcher 一处），都已登记进
//!   `readonly_guard::spawn_registry::ALLOWED`。
//!   🔴 **看门狗不许搬进 daemon 代码**：零定时器铁律的人群是「本 crate `src/` 的源码文本」，
//!   而**被起进程的行为不在里面** —— 把那个「等 N 秒」写进本层当场撞铁律（`KR87D1`）。
//!   ⚠ 名字**前缀专属**（`ccm-oneshot-`）：撞名**拒绝**，绝不像 [`launch`] 的
//!   `create-or-attach` 那样静默接回（接回等于替别人的会话定了死期，`KR87D2`）。
//!   ⚠ 它**只起一个有寿命的会话** —— 「隔多久看一眼画面」归 `K-R101`，别在这里顺手做掉。
//! - [`kill`]（F04a）：**杀一个 tmux 会话**。过 §34 三道门（Gate 3 = `windows==1` 只给它），
//!   对 `#{session_id}` 句柄下手而不是名字。⚠ 本模块落地 ≠ monitor 那条路已切过来（那是 F04b，C6 顺序）。
//! - [`launch`]（U8a-2b）：**起 tmux 会话 / 往已有会话键入载荷**（U8a 分解里的「平面 ②」）。
//!   起进程（`tmux`，argv 直传不过 shell），已登记进 `readonly_guard::spawn_registry`。
//!   **不 attach** —— 那是平面 ③，daemon 在远端开不了你面前的窗。
//! - [`cli_control`]（P4d）：控制面的**第二个入口** —— 一次性 CLI。
//!   它**不实现任何命令**，只把 `--<name>` 还原成 `<name>` 去 `inbound::REGISTRY` 查那条登记、
//!   跑它自己的 `run`。⇒ 起进程点一处不增（本模块零 `Command::new`），
//!   `readonly_guard::spawn_registry` 的数不用动。
//! - [`cc_bus`]（P4f）：**cc-bus 的基础命令**（`bus-list` / `bus-send`）。
//!   起进程（转调本机的 cc-bus 命令，argv 直传不过 shell），**一处**，已登记进
//!   `readonly_guard::ALLOWED`。⚠ 它**不读** cc-bus 的任何数据文件 ——
//!   用户 08-13 明说「后面我可能要改ccbus」⇒ 只把它的**命令**当接口。
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

pub(crate) mod capture_pane;
pub(crate) mod cc_bus;
pub(crate) mod ccm;
pub(crate) mod cli_control;
pub(crate) mod fork_write;
pub(crate) mod gate;
pub(crate) mod identity_tag;
pub(crate) mod kill;
pub(crate) mod launch;
pub(crate) mod oneshot_session;
pub(crate) mod resolve_query;
pub(crate) mod tmux_hook;
