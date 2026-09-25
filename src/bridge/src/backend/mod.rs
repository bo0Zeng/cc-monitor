//! `backend/` —— **monitor 侧的后端边界**（P4a，§1.4b，用户 2026-08-03 选 A）。
//!
//! # 病：这个边界的缺席，造出了第三个地方
//!
//! backend 内部有 §1.1 的三分（`platform/` · `observe/` · `control/`），而 monitor 侧
//! **一个边界都没有** —— `src/bridge/src/` 是平铺的五十多个 `.rs`，唯一子目录是 `adapter/`。
//! 于是 U8c 一族要给「起会话的渲染」找个家时，只剩共享 crate 一条路 ⇒
//! **那个共享 crate（当时叫 `launch-core`）的存在是「因为没有第二个地方，才造了第三个地方」**
//! （架构审计 2026-08-03 实测：backend 对整个 crate 的用量只有一行 `posix_quote`）。
//!
//! # 顶层架构里它是谁
//!
//! 「后端 = 一份代码、两种宿主（本机进程 / 远端进程）」。**这个目录是本机那种宿主**；
//! `src/backend/` 是远端那种。两边**同一套分解**：读（observe）与控制（control）。
//!
//! # `observe/` 那个决定：🔴 **它已经被叫醒了，只是没人听见**〔订正 2026-09-10〕
//!
//! 〔墓碑 —— 原话逐字（两段，先后写于 P4a 与 F18）：
//!
//!  「⚠ **`observe/` 今天刻意还没建**：monitor 侧的读面（`local_accounts.rs` 30KB +
//!  `history_query.rs` 一族）正是 **U7 要退役的那批** —— 现在把它们搬进来、再由 U7 删掉
//!  是纯搬运；只建一个空目录则是装饰。⇒ 先建 `control/`，`observe/` 等那批读面退役。」
//!
//!  「⚠⚠ **谁来叫醒这个决定**〔F18 补〕：上面只说了「等 U7」——那是个**没有触发器的等待**，
//!  而 U7 的正题被「安装包里真有本机后端」挡着（F05b，要真 Windows 机）。
//!  真正会叫醒它的是 `local_read_surface_registry` 里那条**前提触发器**：
//!  `tauri.conf.json` 一出现 `externalBin`，它就红 —— 那一刻本机才真有个后端可切，
//!  「把读面搬进 `observe/` 再退役」才不是纯搬运。**别再把这句读成「等某个人想起来」。**」〕
//!
//! ## 为什么说它已经被叫醒了（三条，逐条有出处）
//!
//! ① **前提翻了**：F05b 已落地并随 v3.7.0 发出去。09-10 干净 win11 上现打（PM 的读数）——
//!    装出来那份 `cc-monitor-backend.exe` **2 个进程在跑**、裸 `monitor.exe` **0 个**。
//! ② **不只是「有后端可切」，是已经切过两次**：`local_read_surface_registry` 的棘轮史逐字
//!    记着 `11 → 10`（F10b 第一批，`usage.rs` 改走本机后端的 `--usage`）→ `9`
//!    → `8`（F10b 第二批·下半，`local_accounts.rs` 改走本机后端的 `--session-accounts`）
//!    → `7` → `8`（`P8a` 新增）。接线点今天就在生产段上：`local_accounts.rs::check_local_acct_iso`
//!    （一个 `#[tauri::command]`）直接调 `backend::observe::local_query::run_query(…, ["--acct-iso-status"])`。
//!    〔C4a · 第四波：这里原先点的是 E79 那条本机会话账号查询 —— 它调的其实是 `--session-accounts`，
//!     且已随「本机与远端同一条路」改走通道退役；换成真调 `--list-accounts` 的那一条，论点不变。〕
//!    〔C4c · 第四波 4B：`--list-accounts` 那一条（本机账号清单）也改走通道退役了 ⇒ 再换成 `acct-iso` 那一问，论点不变。〕
//!    〔`设计/50`：这段原先举的例子是用量那一轴（那个命令与它调的 `--usage` 今天都不存在了）——
//!     换成同形的账号那一轴，**论点一个字没变**。〕
//! ③ 🔴 **`backend/` 里已经住着读面代码，而它当时挂在 `control` 线上** ——
//!    那个住户头注第一句逐字是「backend 的**读面**是 14 条一次性查询子命令」。
//!    ⇒ 「只建一个空目录是装饰」那条反对理由**不成立**：`observe/` 一建出来就有真住户。
//!    〔`K-R71` 09-12 **办掉了**：它今天住 `observe/local_query.rs`。
//!    本条原话逐字是「**只是挂在 `control` 线上**」「而那个住户**此刻**住在**错的能力线**上」
//!    —— 那两句描述的是 09-12 之前的盘面，留着是因为它是下面那一节的**理由**，
//!    不是因为它还成立。〕
//!
//! ⚠ 墓碑第一段里另有三处已经不成立的事实，一并记下（同族，S11「描述当下的字段最易腐」）：
//! `local_accounts.rs` 的读面**已经退役了**，它不再是「U7 要退役的那批」；
//! `history_query.rs` **不在 monitor 侧** —— 它住 `src/backend/observe/history_query.rs`，
//! 是后端的文件，monitor 从来没有过这个文件（`git log --all` 对该路径零提交）；
//! 「30KB」今天是 62KB。
//!
//! ## 下一步是什么 —— 🔴 **09-12 做掉了**（`K-R71`，4a）
//!
//! 〔原话逐字：「**建 `observe/`，把 `control/local_query.rs` 挪成 `observe/local_query.rs`**，
//! 连 `BACKEND_FILES` 那条登记的能力线一起改成 `observe`」「⚠ **本拍没做这一步**（写区不含
//! `control/mod.rs` 与那批 `use`）」「这条**不是遗漏**，`observe/` 一建出来就自动纳入」。〕
//!
//! 落地形态就是那一句：`observe/local_query.rs` ＋ `BACKEND_FILES` 里那一行的能力线是
//! `observe` ＋ `control/mod.rs` 不再 `pub mod` 它 ＋ 两个真调用方（`usage.rs`、
//! `local_accounts.rs`）改走 `backend::observe::local_query`。
//!
//! ## 🔴 「两条判据已经在那儿等着」这句话，`K-R71` **真去验了**（此前没有人验过）
//!
//! 那两条是下面的 `every_file_under_backend_is_registered_with_a_reason`（自陈「目录与登记表
//! **两个方向都查**」）与 `every_file_under_backend_lives_on_a_capability_line`
//! （它**认** `observe/`，也只认 `control` / `observe` 两条线）。
//! 09-12 的实测读数（每一刀最小面、逐条住 `tests/evidence/K-R71-observe归位.md`）：
//!
//! - 往 `observe/` 下多放一份不登记的文件 ⇒ 前者**红**（方向：盘上多、表上无）；
//! - `BACKEND_FILES` 里留一条指向不存在文件的条目 ⇒ 前者**也红**（方向：表上有、盘上无）
//!   —— 「两个方向都查」这句**自陈**至此有了实测，不再只是代码自己说的话；
//! - 把那一行的能力线改回 `"control"` ⇒ 后者**红**（路径前缀与能力线对不上那一断言）。
//!
//! ⚠ 三刀都**只红该红的那一条**（另加编译期就该炸的不算读数）。⇒ 「一建出来就自动纳入」
//! 也**不再是**一句没人验过的话。
//!
//! ## 🔴 但原句的后半（「把**那批读面**搬进来再退役」）没有被叫醒 —— 今天没有触发器
//!
//! 挡着它的是两样有名有姓的东西，**F05b 一条都没动**：
//!
//! - **backend 侧的查询集缺口**：剩下 8 条 `reader` 每一条的退役条件写的都是后端侧一个
//!   具体缺口（`--list-projects` 缺会话 sid 清单 · backend 侧没有 codex 的项目枚举 ·
//!   `--search` 没索引 · 缺 `--list-marketplaces` · 删会话后端侧无对侧）。
//!   `local_read_surface_registry` 头注逐字：「至此本机读面**在现有后端查询集下已无可退**」。
//! - **宿主耦合**：`backend/` 有一道宿主无关守卫（下面 `the_backend_layer_stays_host_agnostic`，
//!   禁 `AppHandle` / `tauri::Window` / `WebviewWindow` / `State<` / `.emit(` / `Emitter` / `Manager`）。
//!   8 条 `reader` 里**有 2 条今天就带着这些把手**（09-10 现打：`tasks.rs` 有
//!   `use tauri::{AppHandle, Emitter}` 与一处 `app.emit(`；`search.rs` 有三处 `tauri::State<`）
//!   ⇒ 它们**原样搬不进来**，得先把宿主耦合剥掉。
//!
//! ⇒ **今天没有触发器**：仓里没有任何一条判据会在「那 8 条能退役了」的那一刻红。
//! 🔴 **不许再在这里编一个** —— 上面墓碑第二段就是那么来的：一个不会响的闹钟比一句过期的话更贵，
//! 因为它让人以为有人在看着。
//! ⚠ 尤其**别**把 `local_read_surface_registry` 那条本机后端判据当成它的闹钟：那条 2026-08-04
//! 就换过靶（它自陈「已经触发过一次，这是它的后继形态」），今天盯的是**配置文件的形状**
//! （`externalBin` 不在 `tauri.conf.json` · 在 `tauri.sidecar.conf.json` · stem 与 `LOCAL_BACKEND_STEM`
//! 一致），**一格都不读「本机后端起没起来」**；而且「`tauri.conf.json` 一出现 `externalBin`
//! 就红」这句今天**语义是反的** —— 它红是「有人把它搬回主配置了」（回归），不是「那一刻到了」。

pub mod control;
pub mod observe;

/// 本目录下每个文件的**归属登记**：`(相对 `backend/` 的路径, 能力线, 一句为什么在这里)`。
///
/// 加文件不写理由 ⇒ 下面那条判据红。这就是当初 `src/` 摊成五十多个平铺文件的那道缺口 ——
/// 一个目录只要没有「什么该进来」的说法，它就会变成下一个平铺堆。
#[cfg(test)]
const BACKEND_FILES: &[(&str, &str, &str)] = &[
    (
        "mod.rs",
        "-",
        "边界本身的说明 + 本文件里那两族机检：`tests`（谁住在哪条线上 · 宿主无关 · 平台无关）\
         与 `layering`（`K-R73`：线与线之间谁能引用谁）。\
         ⚠ **这一格刻意不再写「几条」**：它先写「两条」、实测已有四条，F18 改成「六条」，\
         而 09-12 现打是 14 条 —— **这个数在这里腐了两轮**，而没有任何判据读它。\
         S11 那一族（「描述当下」的字段最易腐）在本仓的又一处：\
         治法不是把数改对，是让这一格不再存那个数",
    ),
    ("control/mod.rs", "control", "写/控制面的说明"),
    (
        "control/tmux.rs",
        "control",
        "🔴 〔步 8 · 归属 2026-09-19〕**从 `lib.rs` 顶层搬进来的。**\
         tmux 的**三条原语**（列会话 / 抓屏 / send-keys）＋ 杀会话 —— 全是控制面。\
         `daemon` 密度第五高的那份（`设计/90 §1.1` 那条顺序事实），\
         而它一行 GUI 把手、一行平台 cfg 都没有（这正是它搬得动、\
         而 `ssh_source.rs` / `local_backend_host.rs` 搬不动的分水岭）。",
    ),
    (
        "control/backend_control.rs",
        "control",
        "🔴 〔步 8 · 归属 2026-09-19〕**从 `lib.rs` 顶层搬进来的。**\
         `C8` 那三件（起 / 停 / 状态）的**命令层** —— 只认 origin，不认 ssh、也不碰进程监护。\
         `daemon` 密度最高的那批文件之一（`设计/90 §1.1` 那条顺序事实），\
         而它做的事一格都不是前端的。",
    ),
    (
        "control/cc_bus.rs",
        "control",
        "🔴 〔步 8 · 归属 2026-09-19〕**从 `lib.rs` 顶层搬进来的。**\
         cc-bus 的驾驶舱：读状态（`read_cc_bus_state` / `read_cc_bus_inbox`）＋\
         **起 / 杀 / 发**（`cc_bus_spawn` / `cc_bus_kill` / `cc_bus_send` / `cc_bus_broadcast`）。\
         后四条是**真控制面**（起进程、杀进程、投递），归 `control`。\
         ⚠ 本拍**只挪住址**，一个字都没动「它属于哪一类」那件事。",
    ),
    (
        "control/inbound_client.rs",
        "control",
        "🔴 〔步 8 · 归属 2026-09-19〕**从 `lib.rs` 顶层搬进来的。**\
         backend 流通道的 wire 客户端 —— `C1`「一份代码两种承载」里「本机进程」那一半的传输层，\
         被 `backend_kill` / `backend_launch` / `backend_send_keys` 共同依赖。\
         它此前住 `src/` 顶层，靠那张「表外但归这一半」的登记表（`EXTRA_BACKEND_FILES`，已随本拍整张删除）\
         「表外但归这一半」的登记**纳入管辖**；那张表的头注逐字写着理由是\
         「挪文件半径远大于收益 …… **先把它纳入管辖，挪不挪是另一件事**\
         （若将来挪进 `backend/`，把这一行删掉即可）」。步 8 是全仓冻结窗口，\
         那个「半径太大」的前提当场消失 ⇒ 真挪了，那张表随之整张删掉（它空了）。",
    ),
    (
        "control/ccm_invocation.rs",
        "control",
        "ctx → `ccm …` 调用行（维度注册表 + 诚实降级）。P4b 从共享 crate 搬回归属地",
    ),
    (
        "control/payload.rs",
        "control",
        "`env 前缀 → cd → argv → wrap` 载荷编译器。P4b 从共享 crate 搬回归属地",
    ),
    (
        "control/backend_launch.rs",
        "control",
        "U8a-2c-1：backend `launch` 的发送端（`send-into` 那半边；attach 留在用户终端）",
    ),
    (
        "control/frame_query.rs",
        "control",
        "〔`C1` · 2026-09-24〕**只读查询走已有长连接的发送端**（`history-*` / `accounts-*` 八条帧命令）。\
         归 `control/` 而不是 `observe/`：它不产观测帧，是一问一答的发送端 —— 与 \
         `cc_bus.rs` 查名单同一形（〔C4e〕`tmux.rs` 抓屏那一处迁到界面了），走 `backend_route` 的分流器、登记在 `SENDERS`；\
         住 `observe/` 会凭空造出 `observe → control` 的边（它要 `inbound_client` 与 `backend_route`）",
    ),
    (
        "control/command_args.rs",
        "control",
        "🔴 〔`设计/05 §8.1` 步 3.5 · 2026-09-21〕**从 `control/inbound_client.rs` 剥出来的业务契约** —— \
         `launch` 的**参数构造器**（`launch_args` / `LaunchExtras`；〔C4e〕`capture-pane` 那个随抓屏迁到界面删了）。\
         剥的理由不是整理：`设计/05 §2` 的 `C1`（零业务语义）在 `inbound_client.rs` 上咬到 \
         `sid` 与 `agent` 两个词，**两处都在这三样身上**（`ccm_sid` 参数 · `extras.agent` 字段）。\
         「一条命令要带哪几个业务字段」是**载荷的内容**，而 `inbound_client.rs` 只该管载荷的搬运 —— \
         它是 `§8` 步 3 点名要圈进通信层的传输面之一。\
         ⇒ 剥完现打：那一份从 `C1 · X4`（13/2）变成 `X4` 独咬（14/1）。\
         ⚠ 它自己**不是**通信层成员，也不该是：这里就是那些业务词的家。",
    ),
    (
        "control/backend_route.rs",
        "control",
        "F04c：「这条命令能不能回落」的**唯一**判定（`kill` 与 `send-keys` 共用）。\
         分界线是「能不能**证明**这条命令根本没发出去」，不是「成功/失败」。\
         两份实现必漂，而漂开的后果是把一次**被门拒绝**洗成另一条路的成功",
    ),
    (
        "control/backend_send_keys.rs",
        "control",
        "F04c：backend `send-keys` 的发送端。★ `enter` 落在**两个 mode 名**上而不是一个字段 —— \
         `parse_request` 不 deny unknown fields ⇒ 旧后端会静默忽略字段照样附 `Enter`，\
         把「打断当前回合」变成「提交用户输入框里排队的文本」",
    ),
    (
        "control/backend_kill.rs",
        "control",
        "F04b：backend `kill` 的发送端（C6 那条顺序的最后一步）。★ 结局是**三态**而不是两态 —— \
         分界线是「能不能证明这条命令根本没发出去」：能证明才许回落到过渡期的 SSH 路（C7），\
         否则一律不回落。把 `wrong_owner`/`too_many_windows` 当成「backend 不可用」而回落，\
         等于把一次**被门拒绝**洗成另一条路的成功",
    ),
    (
        "control/local_backend.rs",
        "control",
        "F05a：本机后端进程的「起与看住」。决策那半（local_backend 路径解析 + 崩溃频率上限）是纯函数；\
         监护器用 `std::process::Command`，等子进程死靠**读它 stdout 到 EOF**（零定时器，C12）。\
         ⚠ 今天只认打包进安装包的本机后端、不扫 dev 产物 —— 理由是后端一起来就无条件\
         往 tmux server 装全局 hook 且没有开关（F05 摸底 §2.5）",
    ),
    (
        "control/launch_wire.rs",
        "control",
        "前端结构化请求 → wire 适配 → ccm 调用行 / 裸载荷（两个 tauri 命令）",
    ),
    ("observe/mod.rs", "observe", "读面的说明 + 什么该进来的判准"),
    (
        "observe/local_query.rs",
        "observe",
        "F10a：**本机一次性查询的传输** —— 「本地 = 不走 ssh 的远端」那一跳的本地版。\
         backend 的读面是 14 条一次性子命令，**不在常驻通道上**（hello 的 `commands` 里\
         一条读命令都没有）⇒ 切读面 = exec 一次本机后端拿 stdout。协议一个字不改。\
         ★ **它是本条能力线上今天唯一的住户** —— 〔`K-R71` 09-12 归位；本格原话逐字：\
         「★ **它是本目录里唯一的读面文件** —— 挂在 `control` 线上只因为 `observe/` 还没建，\
         见本文件头注「下一步是什么」那一节」，`observe/` 建起来之后那句话失效〕。\
         〔订正 2026-09-10 —— 本格原话逐字：「⚠ 今天**零生产调用方**（F10b 接线），\
         由它自己那条前提触发器盯着 —— 一有调用方就红，逼 `local_read_surface_registry` \
         的棘轮跟着往下拧」。**两句今天都不成立**：那条触发器 2026-08-04 就已经响过一次并\
         换成了后继形态（`every_caller_of_this_transport_is_already_off_the_read_surface_ledger`，\
         改成「每个调用方都必须已经从棘轮账上下来」）；而生产调用方 09-10 现打**不是零** —— \
         `local_accounts.rs::check_local_acct_iso` 等在调它（〔C4a〕原举的 E79 那条已退役；〔C4c〕本机账号清单那条也退役）\
         （原话还并列了用量那一轴，`设计/50` 把它整轴删了）。\
         ⇒ 这里不再写「有几个调用方」这种会腐的数，那个数的家在那条判据里〕",
    ),
    (
        "control/agent_profile_parity.rs",
        "control",
        "F06：agent 适配表（`fixtures/agent-profile-golden.tsv`）的跨语言对拍 —— \
         **C4「ccm 变零决策」的前置**：搬之前先证明三份副本逐字一致。\
         另钉两条前提：ccm 独有的两个决策仍无 Rust 对侧 · ccm 仍拒绝 codex 的 subcommand 形 resume",
    ),
    (
        "control/gate2_parity.rs",
        "control",
        "F03：§34 Gate 2 判定表（`fixtures/gate2-golden.tsv`）在 monitor 这一侧的独立对拍。\
         同一张表另有两个读者：backend 的 `control/gate.rs` 与 `tests/e2e/backend-gate2-acceptance.sh`",
    ),
    (
        "control/launch_cli_parity.rs",
        "control",
        "上面那条 ccm 调用行与 TS 黄金串的跨语言逐字节对拍",
    ),
    (
        "control/launch_payload_parity.rs",
        "control",
        "上面那条裸载荷与 TS 黄金串的跨语言逐字节对拍",
    ),
    (
        "control/launch_tmux_outer_parity.rs",
        "control",
        "`设计/90 §4 E`：**外层 tmux 那三格**（`new-session` / `send-keys` / `attach`）\
         与 TS 黄金串的跨语言逐字节对拍。与上面那条的分工：那条管**内层载荷**，这条管**外层容器**",
    ),
];

#[cfg(test)]
#[path = "../../../../tests/bridge/backend_tests.rs"]
mod tests;

// ═════════════════════════════════════════════════════════════════════════════
// ★★★ `K-R73`（09-12，`DECISIONS.md#R29` 裁定三）：monitor 侧的**层间方向判据**
//
// # 它补的是什么
//
// 上面那两条只管「文件**住在哪条线上**」（`every_file_under_backend_lives_on_a_capability_line`
// 逐条比登记的能力线与路径前缀）。**「线与线之间谁能引用谁」此前一条判据都没有。**
// backend 侧同一条纪律由 `src/backend/layering_guard.rs` 管，它有**两样**：
// ① `ALLOWED_OBSERVE_TO_CONTROL` —— 正向逐条列举、条数被钉住；
// ② `control_layer_must_not_reference_observe` —— 反向零容忍。
// `K-R71` 把 `observe/` 建出来、当场造出 monitor 侧第一条跨能力线的边之后，
// 这两样在 monitor 侧**一样都没有**。本模块是它们在这一侧的对应物。
//
// ⚠ **形照抄后端，不自己发明**；backend 那份是只读样板，本件一个字节都没改它。
//
// # 一处**与后端不同**的地方，别读成照抄漏了
//
// backend 的层直接住 `src/<层>/`，锚点因此是 `crate::<层>` 与 `super::super::<层>`。
// monitor 的层住 `src/backend/<层>/` ⇒ 锚点是 `crate::backend::<层>`、
// `super::super::<层>`（层内某个文件看兄弟层）与 `super::<层>`（层自己的 `mod.rs` 看兄弟层）。
// 🔴 **最后那一种后端那份认不出来**（它的层 `mod.rs` 写 `super::<兄弟层>` 同样能编过）——
// 如实登记为**样板自己的一处洞**，交回 PM，本件不去改它。
// ═════════════════════════════════════════════════════════════════════════════
#[cfg(test)]
#[path = "../../../../tests/bridge/backend_layering.rs"]
mod layering;
