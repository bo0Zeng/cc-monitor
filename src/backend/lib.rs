//! `cc-monitor-backend` 的**库面**。
//!
//! 🔴 **它为什么存在**（`设计/00 §1.5.4` 前置 1、2）：目标形态是
//! 「`backend-core` 一份，两个宿主 link 同一份」—— 本机 GUI **进程内**直接跑它，
//! 远端仍走那个薄 `main` 壳。今天这个 crate 只有 `main.rs`，模块全是 bin 私有的
//! ⇒ **没有任何东西能 link 它**，in-process 那条路在编译期就不存在。
//!
//! ⚠ **本拍是纯机械搬家，不改行为**：模块声明、`PROTO_VERSION`、身份那一块
//! （`BUILD_ID` ＋ 戳）、`SUBCOMMANDS` 搬进来；**分派留在 `main.rs`**（规格逐字）。
//!
//! 🔴 **`BUILD_ID` 必须住这里，不能留在 `main.rs`** —— in-process 那条路**没有
//! 那个 `main.rs`**，身份会跟着消失。它的三处盘上消费者已同拍改到本文件：
//!   · `src/bridge/build.rs`（`backend_lib_rs()` ⇒ 两条内嵌路的编译期期望值）
//!   · `.github/workflows/release.yml`（清单里那个 build_id）
//!   · `tests/backend/build_id_guard.rs`（真身份住址的逐条核对）
//! **别在第四处写它的住址。**

pub mod accounts; // 账号层（apikey 端点改写）：`resolve` 那张决策表 ＋ 表 ＋ 凭据 ＋ 热重载。**不是中转**，不住 relay/
#[cfg(test)]
#[path = "../../tests/backend/agent_boundary_guard.rs"]
mod agent_boundary_guard; // S1：通用层不许知道任何 agent 的名字与文件格式（整体 #[cfg(test)]）
#[cfg(test)]
#[path = "../../tests/backend/agent_locality_guard.rs"]
mod agent_locality_guard; // S2：codex 的格式知识只许住 agents/codex/ + kind 派发点逐条登记（整体 #[cfg(test)]）
pub mod agents; // S2/S3：agent 适配层——每个 agent 一份，装它专属的知识（codex + claudecode）
#[cfg(test)]
mod alloc_probe; // U-2：线程级内存量具（F22：`VmHWM` 是进程级的，会把邻居测试算进来）
pub mod asset_catalog; // 〔AS2 · 第四波 4B · V113〕资产目录：帧面 `assets-catalog` / `assets-catalog-merge`（后端自有状态 `~/.cc-monitor/assets-catalog.json`，第四层；一个用户文件都不写）
pub mod asset_sync; // 〔AS2 · 第四波 4B · V113〕资产目录的自动同步：帧面 `assets-sync`（本机常驻后端沿池里那条 SSH 拉 / 并 / 推；写口由 inbound 递进来）
#[cfg(test)]
#[path = "../../tests/backend/build_id_guard.rs"]
mod build_id_guard; // E77：加了子命令必须 bump BUILD_ID（内部整体 #[cfg(test)]，生产构建为空）
#[cfg(test)]
#[path = "../../tests/backend/cc_bus_boundary_guard.rs"]
mod cc_bus_boundary_guard; // P4f-Y2：backend 不许碰 cc-bus 的数据布局（整体 #[cfg(test)]）
pub mod common; // U2：两边都要、又不含平台原语的纯工具（§0.5-6 打掉了「三分够用」那个判断）
pub mod control; // U3：控制面 —— 会改变世界（写盘 / 改 tmux server / 发信号），或产出改变世界的计划
pub mod dial; // K-P6b / C2 / 〔SR1a〕：SSH 的一切 —— 握手 · 连接池 · 链路（**只此一处**，判据在它自己的测块）
pub mod feature_face; // 〔RM1b · 第四波〕功能侧只读查询的帧面宿主（tasks-list …）—— 薄壳，本体在 observe/，与 read_face 分家的理由在它头注
pub mod files; // 步 24f：`files-read` 这一族（**只读**）—— 常驻文件名索引 ＋ 四条只读能力（`设计/96 §2.9`）
pub mod footprint; // 〔RM1a · 第四波〕「足迹」的这台机器那一半：帧面 `footprint-probe`（只读路径事实，判定住 monitor）
#[cfg(test)]
mod guard_support; // U-1：各条源码扫描型守卫共用的「只留生产段」剥法（仅测试构建）
pub mod history_annotations; // 〔C4d · 第四波 4B〕历史注解（星标 / 改名 / 隐藏 / 上次账号）：帧面 `history-annotate` / `history-forget` / `history-last-accounts`（第四层；文件就是 monitor 从前那一份，路径由它交）
pub mod history_join; // 〔C4d · 第四波 4B〕历史跨机 join 的唯一的家：帧面 `history-projects` / `history-sessions` 出成品（这台 ＋ 可达表里的远端，并注解 ＋ 判活）
pub mod inbound; // U6b-1：流连接上的入方向（信封 / 分派 / 取消）
#[cfg(test)]
#[path = "../../tests/backend/layering_guard.rs"]
mod layering_guard; // U3：§1.1 第二条解耦线的机器判据（observe↔control 方向与条数）
pub mod listen; // K-P1：常驻监听口 —— 脱离宿主之后还能被找到 / 被问到 / 被接上（纯判定住这里，接受循环住 main.rs）
pub mod mcp_sync; // 〔AS1 · 第四波 4B〕MCP 资产同步的判定：帧面 `mcp-sync-plan`（差异 · 可疑项 · 写哪几条；只读，写经文件管理那一面）
#[cfg(test)]
#[path = "../../tests/backend/no_timer_guard.rs"]
mod no_timer_guard; // P6：零定时器护栏（内部整体 #[cfg(test)]，生产构建为空）
pub mod observe; // U3：观测面 —— 读，不改变世界
#[cfg(test)]
#[path = "../../tests/backend/panorama_locus_guard.rs"]
mod panorama_locus_guard; // K-W2D KW2D3：全景的解析发生在哪个进程的地址空间（整体 #[cfg(test)]）
pub mod platform; // U2：唯一允许平台原语与平台 cfg 的层（§1.1 第一条解耦线）
pub mod plugin; // K-W1A：插件通用调用口 —— 找它 / 传 argv 起它 / 问它会什么（方向由 layering_guard 钉）
#[cfg(test)]
#[path = "../../tests/backend/plugin_walk_fixture.rs"]
mod plugin_walk_fixture; // K-W2E：最小假插件走通全流程（夹具 + 判据，整个文件级 cfg(test)，生产构建为空）
#[cfg(test)]
#[path = "../../tests/backend/protocol_doc_guard.rs"]
mod protocol_doc_guard; // U6a：IPC-PROTOCOL.md 与真实协议面的对拍
#[cfg(test)]
#[path = "../../tests/backend/ratchet_guard.rs"]
mod ratchet_guard; // K-P1 KPY7：本件动过的那几张登记表，**断言那几行**逐字没动（整体 #[cfg(test)]）
pub mod read_face; // 〔C1 · 09-24〕只读查询的帧面宿主（8 条：history-* / accounts-*）—— 薄壳，本体在 observe/，住顶层的理由同 files/
#[cfg(test)]
#[path = "../../tests/backend/readonly_guard.rs"]
mod readonly_guard; // F08a：backend 只读机器护栏（内部整体 #[cfg(test)]，生产构建为空）
pub mod relay; // K-H1：HTTP 中转（搬字节那半）——只听回环、按路径前缀分流、逐块透传 + tee
pub mod remote_ask; // 〔C4d · 第四波 4B〕本机后端问远端后端的那一跳（池里那条 SSH 上 capture 一次性子命令）＋ 可达表 —— 全后端只此一处；帧面 `remote-reach`
#[cfg(test)]
#[path = "../../tests/backend/single_stream_guard.rs"]
mod single_stream_guard; // K-P1 KPY8：「多客户端的流」明确不做 —— 三处「恰好一个客户端」的触发器（整体 #[cfg(test)]）
pub mod skill_install; // 〔AS2 · 第四波 4B · V113〕skill「装到这台」：帧面 `skill-read`（来源那台）/ `skill-install-plan`（要被写的那一台；复用 AS1 的差异与闸）。只读
pub mod wire;

/// Streaming wire-protocol major version, reported as `v` in the `Hello` frame.
/// Bump ONLY on a breaking wire change; additive forward-compatible frame kinds
/// (e.g. `Overflow`, #32) do NOT bump it — old parsers skip unknown kinds.
/// The monitor negotiates against its own `EXPECTED_PROTO_V` (#33).
pub const PROTO_VERSION: u32 = 1;

/// Backend build id reported in the `Hello` frame (#33 version negotiation).
/// Human-readable, monotonic build/feature tag; the monitor compares it against
/// `EXPECTED_BACKEND_BUILD_ID` and warns the user when a manually-deployed backend
/// is stale (staleness 提示 + 部署确认)。
///
/// **与 F66 `capabilities` 两轴正交（§26）**：`build_id` = backend 的**身份/构建版本**
/// （改了后端二进制就该 bump，用于 staleness + 部署确认）；`capabilities` = 该版本
/// **声明支持什么能力**（用于运行时门控发哪些 flag）。两者不混——bump build_id 是
/// 「我是新构建」，声明 capability 是「我这个构建支持 X」。
///
/// **★ F66 待 bump（发版前一套动作，Phase G 记账）**：F66 给后端加了 `capabilities`
/// 声明（wire.rs Hello + 下面的 `CAPABILITIES`），是新构建 → **发版前应 bump 到 p1i-xxx**。
/// 但 bump **必须与 re-zigbuild 内嵌二进制 + 更新 `embedded-backends/*.build_id` 清单一套做**
/// （只 bump 源码不 re-embed = 源码 build_id 与内嵌清单不一致的半 bump，更糟）。当前 p1h
/// 不 bump **良性**：monitor 乐观路径照发 flag、旧内嵌二进制自 F24/F25 起就剥
/// `--with-bg`/`--tail-only`，不死循环、不降级（Phase G 双 agent 核实）。
/// - p1a-history  = 一次性历史查询模式（#16，--list-projects 等）
/// - p1b-overflow = + 探活精确化（#34）+ overflow 信号（#32）
/// - p1c-f20-addtime = + add-time 冒名判定（Batch5-F20：pidfile procStart 身份
///   比对为主证据，mtime 时间证据 + cmdline 白名单为 fallback，修 tmux 僵尸 tab）
/// - p1d-lifecycle = + kind 交互性门（Batch6-F21：kind:"bg" 后台任务不成 tab）
///   + sid 原地变更 removed + 同 sid 多 PID 引用计数（Batch6-F22）
/// - p1e-bg-tree = + --with-bg 放行 bg 会话、session_added 帧附 session_kind/cwd/name
///   （Batch7-F24，additive 向后兼容）
/// - p1f-tail-snapshot = + --tail-only（连接不重放历史，seq=行号语义）、
///   session_added 帧附 path（Batch8-F25，历史改走 monitor 旁路 --read-session 快照）
/// - p1g-status-tail = + session_status 帧/宣告带初始 status（Batch9-F27 远端红绿灯）、
///   --read-session-tail 尾部优先查询（Batch9-F30）
/// - p1h-bg-badge = --list-sessions 输出附 isBg（记录级 sessionKind:"bg" 探测，
///   Batch11-F32 历史 ⚙ 徽标；additive，查询字段无版本门控问题）
/// - p1i-line-offset = Line 帧附 `byte_offset`（backend-01/gap#2，累计原始字节、逐字节对齐 aterm
///   `LineFramer`：计 CRLF `\r`、含 `\n`、残行不计；给 offset 续拉/截断检测。additive、不 bump
///   PROTO_VERSION——旧 client 忽略、旧后端缺字段 client 得 0）
/// - p1j-offset-resume = + `--read-session-from-offset <path> <offset>` 一次性查询（backend-02/
///   Phase 1）：从字节 offset 透传 [offset,EOF] = aterm `tail -c +(offset+1)`；配 p1i 的
///   `byte_offset` 做重连/断线 offset 续拉。additive 子命令（旧后端报 unknown arg、client 降级）
/// - p1k-resolve-rpc = + `--resolve` advisor RPC（backend-04/Phase 1）：读 stdin ResumeSpec JSON →
///   出 stdout CommandPlan JSON（camelCase，caps 复用 aterm `SessionCapabilities` 4 名），错误
///   exit2+stderr `{code,message}`。契约与 aterm cc-bus 对齐定死。additive 子命令、advisory 零 handle
/// - p1l-audit-fixes = backend Phase 1 三视角代码审查修复（backend-05）：一次性查询模式不再向 stderr
///   打 info（`--resolve` 错误信封 stderr 纯 `{code,message}`）；resolve base(launchCandidate) 补
///   shell-safe 校验（B2 对称化，新错误码 `unsafe_launch_candidate`）；stdin `.take(1MiB)` 兜 DoS。
///   纯查询/流协议 wire 不变——非破坏、无 PROTO_VERSION bump。
/// - p1m-hello-emits = phase② 联调（backend-08）：Hello 加 `emits:[帧 kind]`（additive，与 capabilities
///   正交、不受 §26）——aterm 门控消费。现声明 line/session_added/session_status/session_removed/
///   overflow；turn_end 待其帧接线后加。additive、无 PROTO_VERSION bump。
/// - p1n-turn-end = phase② 联调（backend-09）：`process_jsonl` 每见 turn-end 记录发 `Frame::TurnEnd
///   {sid,uuid}`（raw-per-record、方案 C 不 dedup；判词 `turn_detect` 对拍 aterm TurnDetector）；
///   `turn_end` 加进 EMITS。dedup 视界在 aterm rolling+debounce baselineByPath。additive、无 bump。
/// - p1o-codex-dg = Phase 2D Codex 泛化（DG3 wire additive agent_kind/liveness_confidence/codex_dir/kinds、
///   DG4 turn-end 检测器、DG5 `--usage` per-kind、DG6 resume）。全 additive、**不 bump PROTO_VERSION**；
///   bump BUILD_ID 给含 DG3-6 的后端独立身份（Phase G 审计 I2：防"同 id 不同内容"静默陈旧）。
///   Codex live 监视/判活（DG1/DG2）暂停、未接线。
/// - p1p-tmux-frame = B2：watch_loop 周期本机 `tmux ls` 发 `TmuxSessions` 帧（+EMITS "tmux_sessions"），
///   替 monitor 每 8s 新建 SSH 跑 tmux ls 的对账刷屏。additive、**不 bump PROTO_VERSION**。
/// - p1q-accounts = A2：多账号只读三命令 `--list-accounts` / `--session-accounts` /
///   `--account-trust`（cc-acct-iso manifest 的消费侧；账号=一个 CLAUDE_CONFIG_DIR）。
///   纯一次性查询、零写入、不 shell out；**不动** PROTO_VERSION / CAPABILITIES / EMITS。
///   bump BUILD_ID 只为给"含账号命令"的后端独立身份，旧版遇到新命令会
///   `unknown argument` exit 2，monitor 侧按"功能不可用"优雅降级。
/// - p1r-event-liveness = zero-poll-liveness P0-P6：判活信号全部换成内核事件
///   （pidfile inotify + pidfd 看进程死 · socket 目录 inotify 看 server 生死复活 ·
///   tmux hook → `--tmux-notify` → SIGUSR1 看会话开关），两条轮询（判活 2s tick /
///   tmux 8s tick）都已删除，生产段零定时器（`no_timer_guard.rs` 钉住）。
///   wire 两处 additive、**不 bump PROTO_VERSION**：`TmuxSessions` 加
///   `observation`（有会话时省略 ⇒ 载荷逐字节不变）+ 新帧 `TmuxSessionClosed`（进 EMITS）。
///   **bump BUILD_ID 是必须的**：旧后端报同一个 id 就不会被判 stale、不自动重装，
///   整轮改动会在已部署的远端**休眠**（本条正是 P1 记档里点名、P5 漏做、P7 补上的那次 bump）。
/// - p1t-removal-cause = **修 v3.4.0 发出去的一个真 bug**：`--account-trust-zero`
///   在 `accounts_query.rs` 里实现完整，但本文件的 match 漏列它 ⇒ 落进 `_` 臂走历史查询
///   ⇒ 回 `unknown argument` + exit 2，而 monitor 的账号 0 信任预检**真的在发这条命令**。
///   **必须 bump**：不 bump 的话已部署的 v3.4.0 backend 不被判 stale、不会自动换掉，
///   修了也到不了用户手上（P5 漏做、P7 补上的那一课）。
/// - p1u-fork-session = **G2/G6（branch-anywhere）新增 `--fork-session`**：backend 第一次
///   有写盘能力（`fork_write.rs`，`readonly_guard` 两层白名单只放行它一个模块）。
///   **必须 bump**：monitor 侧的远端分叉命令要靠这个 id 判 stale 才会自动重装；不 bump
///   的话已部署的后端报同一个 id ⇒ 不判 stale ⇒ 不重装 ⇒ 用户点远端 `⑂` 永远只拿到
///   「版本过旧，请重新部署」。**这条是 Phase G 审计当场抓出来的** —— 上面 p1r/p1t 两段
///   逐字写着这课，本轮仍然漏了，说明「加子命令」这一步该有机检而不是靠记性（登记 E77）。
/// - p1v-attachable = **E73**：`SessionAdded` 帧 additive 加 `attachable`（来自 pidfile 的同名布尔）。
/// - p1w-inbound-in-fingerprint = **audit-0805 F02**：**结清一笔从 08-02 起就欠着的 bump**。
///   `inbound::COMMANDS` 从零条长到 5 条（`cancel`/`ping`/`resolve` 08-02、`kill` 08-04），
///   而 `build_id_guard` 的指纹只看 `main.rs` 的 `Some("--`（一次性子命令那一面）
///   ⇒ **加了整整一个命令面，一次 bump 都没被逼出来**。
///   ⚠ 后果不是纸面的：`sftp.rs::deploy_decision` 判**版本那一维**的唯一判据是 build_id 字符串
///   （〔K-W4 09-04〕backend 部署路另看「落点文件在不在」；〔DP1 09-25〕今天读那份字节自报的身份戳；stale 但文件在时仍只凭 build_id），
///   报同一个 id ⇒ 判 `Skip` ⇒ 已部署的旧 backend **整个控制面静默不可用**。
///   本轮把通道面纳入指纹并 bump；**本条 bump 本身就是那笔欠账的偿付** ——
///   报 `p1v` 的远端从此会被判 stale 并重装。CLI 那一面**一字未改**。
/// - p1x-overflow-identity = **audit-0805 F03**：`Overflow` additive 加 `lost` / `lost_truncated`。
///   出方向那条通道此前对 11 种帧一视同仁地 `try_send` 丢弃，而「丢一帧可恢复」**只对内容帧成立** ——
///   `session_added`/`session_removed`/`tmux_session_closed` 是一次差分的结果、别处不存在，
///   客户端拿着「丢了 N 条」没法重同步。现在不可恢复的那些会带 `kind`+`subject` 出来（有界 64）。
///   ⚠ wire additive、**不 bump `PROTO_VERSION`**；但二进制行为变了 ⇒ 照 p1v 的先例 bump build_id。
///   `session_kind` 此前把两件事压在一个轴上 —— ①「该不该在 UI 出现」②「attach 进去对人有没有
///   意义」。SDK / 脚本驱动的会话正好「①要②不要」：它**有** tmux、`@ccm_sid` 也对，但
///   `stdin=DEVNULL`，用户敲的字会被脚本吃掉。省略 = true（存量零迁移）。
///   **必须 bump**：monitor 要靠新后端才拿得到这个字段；不 bump 就不判 stale、不重装。
///   （wire 是 additive、旧 monitor 忽略未知字段 ⇒ **不 bump PROTO_VERSION**。）
///
/// - p2a-rewatch-sessions〔`P0b-Y2` 08-13〕：**盯着的 `sessions/` 被换掉/还没出现时会重挂**。
///   wire 一个字节没变（不 bump `PROTO_VERSION`），但**二进制行为变了** ⇒ 照上面的先例 bump。
///   ★ **必须 bump**：旧后端在这条路上是**静默失效**的（活着、不吭声、不发 `session_added`），
///   报同一个 id 就不会被判 stale、不会自动重装 —— 用户会带着一个永远不宣告会话的后端过日子。
///
/// - p2e-dial〔`K-P6b` 09-06〕：新增 `--dial` —— 把 **backend 那条长连接流**的 SSH 握手
///   搬进一个由界面起的子进程（候选 E 的字节代理）。
///   ⚠ **必须 bump**：`--dial` 是**新的进程形态**（常驻、只有一条管子进一条管子出），
///   已部署的旧后端根本没有这条臂；而 monitor 判 stale 只看 build_id
///   ⇒ 不 bump 就不重装（p1r / p1t / G2 / p2d 那四次的同一个形状）。
///   🔴 **别把这条读成「拨号搬出去了」**：`connect_session` 的 7 处生产调用点里
///   本件只覆盖 1 处，SFTP / 端口转发 / 跳板 / 其余 exec 路径**界面仍然自己拨**。
///   ★ 与 `p2d-relay` 同一条如实登记：这一半是**源码半**，re-embed（CI 交叉编译）归发版那一拍。
///
/// - p2f-build-stamp〔`K-R70` 09-12〕：**这一份二进制第一次说得出自己是谁。**
///   两件事同拍落：① 下面那个 [`CC_MONITOR_BUILD_STAMP`] —— 一段**保证连续**的
///   `<<ccm-build-id:…:ccm-build-id>>`，谁拿到字节都扫得出来；
///   ② `--ccm-probe` 多吐一行 `build=<BUILD_ID>`（**能跑它的人直接问**）。
///   ⚠ **必须 bump**：在此之前，「这份二进制是谁」只能去读它**旁边**那个 `.build_id`
///   文本文件，而那个文件与二进制是两回事（`K-R68` 现打：三个载体的 `.build_id`
///   全部从同一处源码常量抠出来 ⇒ 恒等 ⇒ 一格证据都不提供）。
///   已部署的旧 backend **既没有戳、也答不出 `build=`** ⇒ 它必须被判 stale 换掉，
///   否则「问得出它是谁」这条性质在已部署的机器上永远为假。
///
/// - p2g-capture-pane〔`K-R86` 09-13〕：新增 `--capture-pane` —— **一条只读的一次性原语**，
///   把某个 tmux 会话此刻那一屏抓回来（`tmux -u capture-pane -p -t '=名:'`）。
///   在此之前后端会列会话、会探 `@ccm_sid`、会杀、会键入，**唯独没有「把那一屏取回来」**；
///   monitor 侧账本 `parity_ledger` 的 `tmux.manage` 那一格为此挂了一个月的欠账。
///   ⚠ **必须 bump**：这是**新增的一条子命令**，已部署的旧后端上它 `exit 2`
///   （落进 `unknown argument`），而调用方判「这台机有没有这条能力」看的是 build_id
///   ⇒ 不 bump 就不判 stale、不重装，整条能力在已部署的远端休眠
///   （p1r / p1t / G2 / p2d / p2e 那五次的同一个形状）。
///   ★ 同 `p2d` / `p2e` 那条如实登记：这一半是**源码半**，re-embed（CI 交叉编译）归发版那一拍，
///   本轮**没做**（本工作树也没铺 `src/bridge/embedded-backends/`）。
///   🔴 **别把它读成「远端画面预览通了」**：本件只出后端这一侧的原语，
///   monitor 那条 `capture_remote_pane` 一个字节没动 —— 欠账换了个名字，没有被结掉。
///
/// - p2h-oneshot-session〔`K-R87` 09-13〕：新增 `--oneshot-session` —— **一次性会话**，
///   起一个到点**自己会死**的 tmux 会话（`new-session -d -P -F '#{session_id}'` ＋
///   一条 `setsid sh -c 'sleep N; tmux kill-session -t $N'` 的**外部**看门狗）。
///   `K-R86` 出的是「看得见」那一半（抓一屏），这一条是「有寿命」那一半 ——
///   在它之前后端建得出会话、杀得掉会话，**唯独没有「建出来的这个到点自己没」**。
///   ⚠ **必须 bump**：又一条**新增的子命令**，已部署的旧后端上它落进
///   `unknown argument` + exit 2，而调用方判「这台机有没有这条能力」看的是 build_id
///   ⇒ 不 bump 就不判 stale、不重装，整条能力在已部署的远端休眠
///   （p1r / p1t / G2 / p2d / p2e / p2g 那六次的同一个形状）。
///   ★ 同 p2d / p2e / p2g 那条如实登记：这一半是**源码半**，re-embed（CI 交叉编译）
///   归发版那一拍，本轮**没做**（本工作树也没铺 `src/bridge/embedded-backends/`）。
///   🔴 **别把它读成「用量探针搬进后端了」**：本件只出后端这一侧的原语，
///   monitor 的 `account_usage` 那条 shell 串编排**一个字节没动**。
///
/// - p2i-frame-tmux-primitives〔`K-R104` 09-13〕：**`capture-pane` 与 `oneshot-session`
///   上了帧面** —— `inbound::REGISTRY` 8 → 10。CLI 那一面（`SUBCOMMANDS`）**一个字没动**，
///   这是本谱系里第一次**只有通道面**变。
///   为什么非搬不可：那两条此前只有 CLI 面，而 CLI 面**每调一次一次 SSH 握手** ——
///   用量探针两段轮询上限 12+20 轮 ⇒ 单次探测最多 **36** 次握手，
///   撑破 monitor 侧的 `EXEC_TIMEOUT_SECS = 25` ⇒ **结构上超时**，不是慢。
///   帧面是一条长连接上多次往返，握手恒 1 次。
///   ⚠ **必须 bump**，而这一次的形状与前七次不同：旧后端不是「`exit 2`」，
///   是它的 `hello.commands` 里**根本没有这两条** ⇒ monitor 的 `InboundClient::accepts`
///   当场判 `CallError::Unsupported`、一个字节都不发（`bus-send` 是现成先例）。
///   ⇒ 探针在已部署的旧远端上整条不可用，而判 stale 只看 build_id。
///   ★ 同 p2d / p2e / p2g / p2h：这一半是**源码半**，re-embed 归发版那一拍，本轮**没做**。
///
/// - p2j-bus-state〔`K-R113` 09-13〕：新增 `bus-state` —— cc-bus 的**具名读命令**，
///   总线名单 ＋ spawn 台账**一次回全**。两个命令面**同拍都动**（`SUBCOMMANDS` 26 → 27、
///   `inbound::REGISTRY` 与 `COMMANDS` 10 → 11），这是本谱系里第一次两面一起变。
///   它补的是 `K-R111` 摸底点名的那个缺口：monitor 侧 `read_cc_bus_state` 想改走后端，
///   而**后端没有对侧** —— 那条读面的头注逐字写着解锁条件是「格式契约稳下来」，
///   届时「正确形状多半不是把 shell 串搬过去，而是后端出一条**具名的读命令**」。
///   ⚠ **必须 bump**，而这一次两个失效形状**同时**成立：CLI 面那半是 p1r/p1t/G2/p2d/p2e/p2g/p2h
///   那七次的 `unknown argument` + exit 2；帧面那半是 p2i 那次的 `hello.commands` 里没有它
///   ⇒ monitor 的 `InboundClient::accepts` 判 `Unsupported`、一个字节都不发。
///   ★ 同 p2d / p2e / p2g / p2h / p2i 如实登记：这一半是**源码半**，re-embed（CI 交叉编译）
///   归发版那一拍，本轮**没做**（本工作树也没铺 `src/bridge/embedded-backends/` ⇒ 不涉及 re-embed）。
///   🔴 **别把它读成「驾驶舱那条读面接上后端了」**：本件只出后端这一侧的命令，
///   monitor 的 `read_cc_bus_state` **一个字节没动**（那是下一件）。
///
/// - p2k-usage-retired〔`设计/50` 删用量〕：**减法那一侧的第一条** —— 两条子命令与一条帧面命令
///   同拍**退役**（`SUBCOMMANDS` 27 → 25：`--usage` ＋ `--oneshot-session`；
///   `inbound::REGISTRY` 与 `COMMANDS` **11 → 10**：`oneshot-session`）。
///   起因是产品裁定：用量的**聚合轴**（后端服务端聚合 `--usage`）与**探针轴**
///   （一次性会话跑 `/usage` 抓屏）两轴整轴不做了；`oneshot-session` 这条原语当初
///   （`K-R87`）就是为探针建的，探针没了它零生产调用方 ⇒ 随之退役。
///   ⚠ **`capture-pane` 不在这一刀里**：拉屏预览真在用它（`tmux.rs::capture_via_backend`）。
///   ⚠ **必须 bump，而这一次的理由与前九次相反**：前九次是「新能力在旧后端上休眠」，
///   这一次是**旧后端上那三条还在**，而新 monitor 不再调它们 ——
///   真正会出事的是**反向**：一台装着新后端的远端，旧 monitor 仍会去调
///   `--usage` / `ch:oneshot-session`，得到 `unknown argument` / `Unsupported`。
///   判「这台机上的后端是不是我们这一版」看的就是 build_id ⇒ 减法同样要 bump，
///   否则「它变了」这件事在协议面上无人可知。
///   ★ 同 p2d / p2e / p2g / p2h / p2i / p2j 如实登记：这一半是**源码半**，
///   re-embed（CI 交叉编译）归发版那一拍，本轮**没做**。
/// - p2l-rename-daemon-to-backend〔`设计/99 §4` 步 8 · 全仓改名一刀〕：**第一次「只换口，不换能力」** ——
///   `--daemon-probe` → `--backend-probe`。`SUBCOMMANDS` 仍是 25 条，帧面那半一个字没动。
///   ⚠ **必须 bump**：同一条能力换了名字，**两个方向都会断** —— 旧后端不认 `--backend-probe`，
///   新后端不认 `--daemon-probe`，两边都落 `unknown argument` + exit 2。而「这台机上的后端是哪一版」
///   只有 build_id 答得了。
///   🔴 **`build_id_guard::SUBCOMMAND_HISTORY` 里那 13 行历史快照一个字节没改** ——
///   机械替换第一版曾把它们全改成新名字，那等于宣布「历史上每一版都有 `--backend-probe`」，
///   而护栏照样绿（当前指纹与快照一起说了谎）。历史证据不是待同步的副本。
///   ★ 同 p2d / p2e / p2g / p2h / p2i / p2j / p2k 如实登记：这一半是**源码半**，
///   re-embed（CI 交叉编译）归发版那一拍，本轮**没做**。
///
/// - p2m-files-read-online〔步 `24f` **第二刀** · `设计/96 §2.9` · `设计/60 §3.5`〕：
///   **`files-read` 这一族上线** —— 四条**纯读**能力同拍进两个命令面
///   （[`SUBCOMMANDS`] 25 → 29：`--files-ls` / `--files-stat` / `--files-find` /
///   `--files-index-status`；`inbound::REGISTRY` 与 `inbound::COMMANDS` 10 → 14）。
///   这是本谱系里第二次两面一起变（上一次是 p2j）。
///   ⚠ **必须 bump**，而这一次 p2j 那两个失效形状**同时**成立：
///   ① CLI 面 —— 旧后端上这四个 flag 落进 `unknown argument` + exit 2；
///   ② 帧面 —— 旧后端的 `hello.commands` 里没有它们 ⇒ monitor 的
///      `InboundClient::accepts` 当场判 `CallError::Unsupported`、一个字节都不发。
///   两条都止于「调用方判 stale 只看 build_id」⇒ 不 bump 就不重装，能力在远端休眠。
///   🔴 **别把它读成「文件管理器的搜索能用了」**，两条如实登记（全文在 `files/mod.rs` 头注）：
///   ① **索引今天没有任何线上办法叫它建** —— `设计/60 §3.5.2a` 把节拍留在调用方，
///      而那条「重走」命令**不在** `设计/96 §2.9` 那张四条的表里
///      ⇒ `files::index::rebuild_once` 与 `files::browse_watch::set_browsing` 至今
///      零生产调用方 ⇒ 真机上 `files-find` 恒回 `index_missing: true`。
///      〔⚠ 2026-09-21 收窄：**「零生产调用方」这半已假** —— 波 β 的 `P2` 接上了
///       `src/bridge/src/filewin/find.rs`。而「恒回 `index_missing`」这半**只在没人
///       开那个窗口去搜的时候**还成立。这一段是当时的账，留着当历史。〕
///      这是设计面的缺口（要补得先在那张表上裁第五条），**不在本刀里自己长出来**。
///   ② **消费侧还没有** —— `src/bridge` 那一头一个字节没动。
///   ★ 同 p2d / p2e / p2g / p2h / p2i / p2j / p2k / p2l 如实登记：这一半是**源码半**，
///   re-embed（CI 交叉编译）归发版那一拍，本轮**没做** ——
///   本工作树没铺 `src/bridge/embedded-backends/`，现打
///   `bash tests/scripts/re-embed.sh --check` 答的是「这棵树上没有一份对不上的字节」，
///   **不是**「字节是对的」（那条边界是它自己头注里逐字写的）。
///
/// - p2n-files-rebuild-and-browse〔步 `24f` **第三刀** · `设计/96 §2.9`（PM 2026-09-21 裁）〕：
///   **同族第五、第六条上线** —— `files-index-rebuild`（`index::rebuild_once` 的线上面）
///   与 `files-browse`（`browse_watch::set_browsing` 的线上面），两条**仍然纯读**、
///   两个命令面同拍（[`SUBCOMMANDS`] 29 → 31；`inbound::REGISTRY` 与
///   `inbound::COMMANDS` 14 → 16）。这是本谱系里第三次两面一起变（前两次是 p2j / p2m）。
///   ⚠ **必须 bump**，失效形状与 p2m 逐字相同（① 旧后端上这两个 flag 落进
///   `unknown argument` + exit 2；② 旧后端的 `hello.commands` 里没有它们 ⇒ monitor 的
///   `InboundClient::accepts` 判 `Unsupported`、一个字节都不发），两条都止于
///   「调用方判 stale 只看 build_id」。
///   🔴 **别把它读成「`设计/60 §3.5.2a` 那个缺口填上了」** —— 补的只是**机制的线上面**：
///   节拍仍归调用方（`no_timer_guard` 那条铁律一个字没动），而「调用方到底发不发那条
///   命令」后端这棵树的判据钉不住 ⇒ 没人发的时候 `files-find` 照旧恒回 `index_missing`。
///   〔✅ 2026-09-21：**那条判据有了，住在发命令那一侧** ——
///    `filewin::find::tests` 四条两侧都钉（没索引 ⇒ 恰好 1 条重走、顺序也钉 ·
///    **阴性对照**：不过期 ⇒ 一条都不发 · 只差 `stale` 一个布尔的对照 · 连打五趟只发一趟）。
///    ⇒ 「后端这棵树钉不住」仍然成立，而**那一格换成由 bridge 那棵树钉着**。〕
///   🔴 另一条如实登记：`files::browse_watch::BrowseWatcher`（真把 `inotify` 挂上去那一跳）
///   **仍然零生产调用方** ⇒ `files-browse` 买到的是「发命令那一刻那几个目录是新的」，
///   不是「此后一有动静就跟着新」。
///   ★ 同 p2d…p2m 如实登记：这一半是**源码半**，re-embed（CI 交叉编译）归发版那一拍，
///   本轮**没做** —— 本工作树没铺 `src/bridge/embedded-backends/`，现打
///   `bash tests/scripts/re-embed.sh --check` 答的仍是「这棵树上没有一份对不上的字节」。
///
/// ★★ **欠着一笔 bump：`设计/80 §8.7` 步 2（`rbind-token`）** —— 本轮**刻意不 bump**。
/// 照 F66 那条「待 bump」的先例，把账挂在看得见的地方。
///
/// **这一刀真的动了线上面，三处**：① `hello.capabilities` 多一条 `rbind-token`
/// （那一帧的线上字节真的变了）；② 多认一条流 flag `--with-rbind-token`；
/// ③ `session_added` 多一个 additive 字段 `rbind_token`（**没索要时字节不变**）。
/// 按本谱系里 p1v-attachable 的先例（那次只加了一个 additive wire 字段就 bump 了），
/// **该 bump**。
///
/// **那为什么这一拍不 bump —— 三条，缺一条我就该顺手 bump 了**：
/// ① 🔴 **这一刀单独上线什么都不改变。** `§8.7` 明写「1 与 2 之间有顺序依赖
///    （没有 token 进环境，后端读不到）」：步 1（往启动命令里注这个变量）与
///    步 3（本地半认这个 marker）都还没做，monitor 侧也还不发那条 flag
///    ⇒ 现在 bump 只会让**每一台**已部署的远端被判 stale、白重装一轮，
///    换回来一个今天一定是空的字段。这正是 p1w 那一段逐字记过的判法
///    （「没有『已部署的远端缺这些能力』这笔欠账，而无谓的 bump 会让所有远端被判 stale、
///    白重装一轮」）。
/// ② **这一波是多棵树并行**（步 1 在另一棵工作树上）。两棵树各 bump 一次 =
///    一次合并冲突 ＋ 一个谁也说不清的版本号。**bump 是发版那一拍的动作，不是步的动作。**
/// ③ 本工作树没铺 `src/bridge/embedded-backends/`，`release-gate` 的 ⑬「bump 的同拍要
///    re-embed」在这里只答得出「这棵树上没有一份对不上的字节」⇒ 真 bump 也买不到那一半。
///
/// ⇒ **解锁条件（发版那一拍，同轮做完）**：步 1／3 落地、monitor 侧开始发
/// `--with-rbind-token` 之后，bump 到 `p2o-rbind-token`（或那一波的合并版本号）
/// ＋ **同拍 re-embed**。**在那之前这条能力在已部署的远端上是休眠的 —— 这是刻意的。**
///
/// ★★★ **p2o-files-write-and-rbind-token**（2026-09-24，第一波合并那一拍）：
/// 上面那笔「欠着的 bump」**在这一拍还掉**，与 F1 写面六条命令合成一次 ——
/// 解锁条件逐条核过：步 1（`CCM_RBIND_TOKEN` 进环境）与步 3（铸币口 ＋ 本地半）已落，
/// monitor 已在协商到 `rbind-token` 时发 `--with-rbind-token`。
/// 线上面这一拍动了：① `files-create/-mkdir/-rename/-delete/-chmod/-write-text`
/// 两个命令面各 ＋6（写面只从 `inbound.rs` 那一扇门进，见 `files::module_boundary_guard`）；
/// ② `hello.capabilities` 的 `rbind-token` 从此有人认、有人发。
/// ★ re-embed 归发版那一拍（同 p2d…p2n 的登记）；本机 `--native` 那份由合并那一拍重打。
///
/// ★★★ **p2p-readface-outline-acctiso-spawn-504**（2026-09-24，第二波合并那一拍）：
/// 子命令 ＋21 —— C1 只读查询面八条上帧面（及其自动派生的 CLI 面）· SE1 `--list-user-inputs` ·
/// A3 `--acct-iso-status` / `--acct-iso-shellinit` · BS1b `bus-spawn`（两个命令面）；
/// ＋ 一处**行为**变更（R2：中转层 1 传输失败回 504 并说清卡在哪一步）。
/// ★ re-embed 归发版那一拍（同 p2d…p2o）。
///
/// ★★★ **p2q-no-ccm-self**（2026-09-24，第二波 MC1+AL1 合并那一拍）：**子命令集一个没变，是行为变了**
/// —— 后端不再读 `CCM_SELF`，重起自己只认 `ccm::self_invocation`（CC1 那一刀之上）；远端 shim 回到一行裸 `exec`。
/// ⚠ 旧后端配新 shim ⇒ 容器路内层缺 `ccm` 那个词（入口②的老病复发）⇒ **必须**让已部署的远端被判 stale。
/// 照 p1v 的先例（只改行为/wire、不改子命令 ⇒ bump 但**不**往 `SUBCOMMAND_HISTORY` 加行）。
/// ★ re-embed 归发版那一拍。
///
/// ★★★ **p2r-apikey-naming**（2026-09-24，第三波 R3 合并那一拍）：**子命令集一个没变，是行为变了** ——
/// 账号层读的凭据文件改叫 `apikey-credentials.json`、环境变量改叫 `CCM_APIKEY_CREDENTIALS` /
/// `CCM_AGENT_UPSTREAM_CLAUDE_CODE`（旧名逐条登记在 `tests/naming/account-vs-relay-naming.vitest.ts` 那张表里，
/// 这里**刻意不复写**，否则那条判据当场红）。用户裁「不要把账号和中转混为一谈」，不留兼容读旧名。
/// ⚠ 新 monitor 递新变量名、旧后端不认 ⇒ 回头读旧文件名 ⇒ 界面配好了、请求静默 404 ⇒ **必须**让已部署的后端被判 stale。
/// 线上字节不变（`wire_golden` 未动）。照 p1v 先例不往 `SUBCOMMAND_HISTORY` 加行。
///
/// ★★★ **p2s-files-copy-exit-policy**（2026-09-24，第三波 F7a ＋ B2 合并那一拍）：子命令 ＋10 ——
/// F7a `files-copy` / `files-read-text` / `files-home`，B2 `exit-policy-read` / `exit-policy-set`（两个命令面）；
/// ＋ B2 行为：常驻后端最后一条流断开时现读 `backend.json` 决定退不退（读到「结束」就自己退）。
///
/// ★★★ **p2t-commit-upload-dial-v2**（2026-09-24，第三波收尾）：子命令 ＋2（F7c `files-commit-upload`，两个命令面）
/// ＋ C2 行为：`--dial` 成为界面进程拨 SSH 的唯一代理（stream / capture / forward 三种用法、ack v2、ssh-agent 鉴权）。
///
/// ★★★ **p2u-stage-find-tasks**（2026-09-24，第四波 4A 第一批合并那一拍）：子命令 ＋5 ——
/// F9c `files-stage-chunk` / `files-commit-text`（大文件分块进暂存区、后端读回拼接提交）· SE2 `find-in-session` ·
/// RM1b `tasks-list` / `plugins-marketplaces`（两个命令面都动）。
/// ＋ 行为：FW5 递归删非空目录（逐条目过围栏）与批量改权限 · F9c 超长请求行的应答从前 4 KiB 抠回 `id` ·
/// S4 冷启动首建索引的读数进 `files.index.status`。
///
/// ★★★ **p2v-resident-link**（2026-09-24，第四波 SR1a 合并那一拍）：子命令集大改 ——
/// `--dial` 删（界面进程不再起拨号代理）；入方向 ＋ `link-open` / `link-data` / `link-credit` / `link-close`（只在帧面），
/// ＋ `history-index` / `history-user-inputs` / `history-find`（骨架索引 · 大纲清单 · 会话内查找上帧面，CLI 面同名派生）。
/// 线上多三种出方向帧：`link_data` · `link_end` · `accounts_changed`。
/// ⚠ 旧本机后端不认 `link-open` ⇒ 界面判「本机后端太旧」、不回落（D11）⇒ **必须**让它被判 stale。
///
/// ★★★ **p2w-apikey-relay-footprint**（2026-09-24，第四波 C4a ＋ RM1a 合并那一拍）：子命令 ＋5 ——
/// RM1a `apikey-key-set` / `apikey-read`（账号层那份凭据文件：远端由那台后端读写，第四层）· `relay-status` / `relay-ensure`
/// （远端中转）· `footprint-probe`（足迹的这台机器那一半），两个命令面都动。
/// ＋ 行为：后端开 `creds-core` 的 `harden`（远端要写那份文件；「后端写不了」从编译期收窄成两条判据）。C4a 不动后端。
///
/// ★★★ **p2x-user-files-put**（2026-09-24，第四波 RW1 合并那一拍）：子命令 ＋3 ——
/// `files-peek` / `files-put`（读改写，CAS）· `files-delete-session`（只收 sid 的会话文件围栏例外），两个命令面都动。
/// ＋ 行为：后端开始写**用户**文件（别名 / `$PROFILE` / `.mcp.json` / skill `INBOX.txt` / cc-bus skill 部署），
/// 本机分叉与删历史会话改走后端 —— 旧后端不认这三条 ⇒ 这些按钮在旧后端上会明确报错，所以必须判 stale。
///
/// ★★★ **p2y-win-proc**（2026-09-24，第四波 WN1 合并那一拍）：子命令集不变，**行为**变更 ——
/// Windows 上的判活（`pid_alive` / `proc_starttime`）与进程看守从 `unimplemented!()` / 空壳换成真实现
/// （`platform/win_proc.rs` · `pidwatch/win32.rs`）。旧后端在 Windows 本机见到第一个会话就 panic ⇒ 必须判 stale。
/// 照 p1v 先例不往 `SUBCOMMAND_HISTORY` 加行。
///
/// ★★★ **p2z-relay-in-resident**（2026-09-24，第四波 RL1 合并那一拍）：子命令集不变，**行为**变更 ——
/// 流模式后端被交了 `CCM_RELAY_PORT` 就在本进程里起中转（V107：中转住本机常驻后端，monitor 不再单独起它）。
/// 旧后端不开中转 ⇒ 本机 apikey 号起会话会被「中转没在跑」拒掉 ⇒ 必须判 stale。照 p1v 先例不加历史行。
///
/// ★★★ **p3a-panorama-engine**（2026-09-24，第四波 RM1c 合并那一拍）：子命令 ＋1 —— `panorama`（两个命令面）：
/// 后端经插件口起独立全景小程序 `cc-monitor-panorama`，只说查询语义（V108）；后端本体仍零 code-picture。
///
/// ★★★ **p3b-session-facts**（2026-09-24，第四波 U4b 合并那一拍）：子命令 ＋1 —— `history-record`（`{sid}` → 记录在不在，两个命令面）。
/// ＋ 线上：`session_added` 多一个可选字段 `container`（tmux / none）· 新出方向帧 `sessions_replayed`。
///
/// ★★★ **p3c-panorama-plan**（2026-09-24，第四波 RM1d 合并那一拍）：子命令集不变，**行为**变更 ——
/// 后端 `panorama` 的 op 表 ＋7（六个 `plan_*` 只回算好的新内容、不写盘 ＋ `refresh_doc_links`）；
/// 旧后端不认 ⇒ 写批注会回 `unsupported` ⇒ 必须判 stale。照 p1v 先例不加历史行。
///
/// ★★★ **p3d-sftp-resident**（2026-09-24，第四波 SR1b 合并那一拍）：子命令 ＋4 —— `ch:transfer-upload` / `-download` / `-start` / `-stop`
/// （传输台搬进后端，只在帧面）。＋ 线上：链路多一种用途 `use:"files"`（sftp 子系统上的一问一答）· 新出方向帧 `transfer`。
/// 界面进程从此零 SSH（V89）：旧后端不认 `files` 用途 ⇒ 部署 / 传输全断 ⇒ 必须判 stale。
///
/// ★★★ **p3e-shapes-cas-withbg**（2026-09-25，第四波 RM1e ＋ C4b ＋ CF1 合并那一拍）：子命令集不变，**行为**变更 ——
/// RM1e `files-delete` 多收可选 `expect`、多回 `stale`（旧后端会忽略 expect 照删 ⇒ CAS 是空的）；
/// C4b `history-index` / `history-user-inputs` / `history-find` / `plugins-marketplaces` 四条应答形状变成后端出成品；
/// CF1 本机常驻后端起参统一加 `--with-bg`（adopt 只比 build_id ⇒ 不 bump 会接上按旧起参起的后端，bg 会话内容缺）。
/// 照 p1v 先例不加历史行。
///
/// ★★★ **p3f-mcp-sync-exitwire**（2026-09-25，第四波 AL1d ＋ S5 ＋ AS1 合并那一拍）：子命令 ＋1 —— AS1 `mcp-sync-plan`（两个命令面）。
/// ＋ 行为：S5 `exit-policy-*` 线上删 `shell` 字段 · `ccm` 直路给了 `--ccm-sid` 而无令牌时 stderr 说一句 · `--list-accounts` 认带 BOM 的 manifest。
///
/// ★★★ **p3g-conn-family**（2026-09-25，第四波 NT1 合并那一拍）：子命令集不变、线上字节不变，**行为**变更 ——
/// 长流在时传输走同一身份的第二条 SSH 连接 · 被远端回拒的连接不再摘 · 在黑洞上等回话被打断时摘掉那条 ·
/// 传输用完的 sftp 会话停着复用（远端多一个空闲 sftp-server）。照 p1v 先例不加历史行。
///
/// ★★★ **p3h-accounts-product**（2026-09-25，第四波 C4c 合并那一拍）：子命令 ＋1 —— `accounts-trust`（两个命令面）。
/// ＋ 行为：`accounts-list` 应答改成后端出成品 `{meta, accounts, notice}`、并上这台自己的 apikey 表（agent 随请求带）。
///
/// ★★★ **p3i-assets-cancel**（2026-09-25，第四波 AS2 ＋ RM1f 合并那一拍）：子命令 ＋5 —— AS2 `assets-catalog` / `assets-catalog-merge` /
/// `assets-sync` / `skill-read` / `skill-install-plan`（两个命令面）。
/// ＋ 行为：RM1f `panorama` 改异步档、`cancel` 真撤（杀子进程组）· Windows 上找全景小程序认 `.exe` 后缀。
///
/// ★★★ **p3j-history-lines**（2026-09-25，第四波 CF2 合并那一拍）：子命令 ＋1 —— `history-lines`（按可计行号取原文，两个命令面）。
///
/// ★★★ **p3k-deploy-by-bytes**（2026-09-25，第四波 DP1 合并那一拍）：子命令集不变，**行为**变更 ——
/// 下载失败留 `.part`（只清零字节的空 `.part`）· 上传失败先等完已发出的写再走 · 远端身份改由 monitor 读字节里的戳判（`.build_id` 退役）。
/// 照 p1v 先例不加历史行。
pub const BUILD_ID: &str = "p3k-deploy-by-bytes";

/// 身份戳的两个界标。**闭集只有这一处住址**（`brief` 13b）——
/// `src/bridge/build.rs` 从本文件的源码里抠这两个串（同 `extract_build_id` 那条既有机制），
/// 再经 `BACKEND_STAMP_OPEN` / `BACKEND_STAMP_CLOSE` 交给 monitor 生产段。
/// **别在第二处写这两个字面量。**
pub const BUILD_STAMP_OPEN: &str = "<<ccm-build-id:";
/// 见 [`BUILD_STAMP_OPEN`]。
pub const BUILD_STAMP_CLOSE: &str = ":ccm-build-id>>";

pub const BUILD_STAMP_LEN: usize =
    BUILD_STAMP_OPEN.len() + BUILD_ID.len() + BUILD_STAMP_CLOSE.len();

/// 编译期把 `<开>` ＋ `BUILD_ID` ＋ `<关>` 拼成一段**定长字节**。
///
/// 🔴 **为什么必须是 `static [u8; N]` 而不是一个 `&str` 常量** —— 这一条是本件的支点，
/// 它治的是 `build.rs` 里逐字记着的那次失败（`embed_backends` 头注）：
/// 「编译器可把 BUILD_ID 优化成立即数指令（字符串在字节里**不连续**），
///  运行时 `bytes_contain` 启发式会误拒正品二进制」⇒ 当时的出路是**旁挂一份清单**，
/// 也就是「把标签抄到旁边」。
/// 一个带地址、被 `#[used]` 钉住的 `static` 数组**不可能**被拆成立即数：它有地址、要进
/// `.rodata`、字节按定义连续。⇒ 「扫字节问身份」从一条启发式变成一条**结构性成立**的事。
/// 〔实测：release + `lto` + `strip` 与 debug 测试壳两侧都扫得出，且**恰好一处**。〕
pub const fn build_stamp() -> [u8; BUILD_STAMP_LEN] {
    let mut out = [0u8; BUILD_STAMP_LEN];
    let mut i = 0usize;
    let open = BUILD_STAMP_OPEN.as_bytes();
    let mut j = 0usize;
    while j < open.len() {
        out[i] = open[j];
        i += 1;
        j += 1;
    }
    let id = BUILD_ID.as_bytes();
    let mut j = 0usize;
    while j < id.len() {
        out[i] = id[j];
        i += 1;
        j += 1;
    }
    let close = BUILD_STAMP_CLOSE.as_bytes();
    let mut j = 0usize;
    while j < close.len() {
        out[i] = close[j];
        i += 1;
        j += 1;
    }
    out
}

/// 🔴 `K-R70`：**这一份后端二进制自己带着的身份**。
///
/// 拿到一份字节（内嵌的 / 装出来的 / 推到远端的那份都算），**不看它旁边任何文件**，
/// 搜 [`BUILD_STAMP_OPEN`] 就问得出它是谁。消费者：
/// `src/bridge/build.rs`（内嵌两条路的构建期校验）· `src/bridge/src/sftp.rs`
/// （推远端之前的运行期见证）· 本 crate `build_id_guard` 的自扫判据。
///
/// `#[used]` ＋ `#[no_mangle]`：前者挡「没人读它就优化掉」，后者让它在符号表里也留个名
/// （`strip` 之后符号没了，**数据还在** —— 判据扫的是数据不是符号）。
#[used]
#[no_mangle]
pub static CC_MONITOR_BUILD_STAMP: [u8; BUILD_STAMP_LEN] = build_stamp();

/// ② 一次性查询子命令：**只有 `args[0]` 是其中之一才进查询模式**。
pub const SUBCOMMANDS: &[&str] = &[
    "--account-trust",
    "--account-trust-zero",
    // 〔`A3` 第二波〕本机 `cc-acct-iso` 的两问（`accounts/iso.rs`）。登记理由同下面那几条：
    // `is_query_mode` 那道闸门读本表，不在表里 ⇒ 当未知 flag 静默进流模式。
    // ⚠ 加这两行会逼出一次 `BUILD_ID` bump（`build_id_guard`）—— 本路**不 bump**，合并那一拍统一做。
    "--acct-iso-shellinit",
    "--acct-iso-status",
    // 〔`C1` · 2026-09-24〕只读查询面那八条帧命令**自动派生**出来的 CLI 面
    //（`cli_control::cli_exposed`）。登记在这里的理由与下面那几族逐字相同 ——
    // `is_query_mode` 那道闸门读的就是本表；不在表里 ⇒ 当未知 flag ⇒ 静默进流模式。
    // ⚠ 它们与 `--list-accounts` / `--session-accounts` 是**同一个函数的两个宿主**，
    //   不是第二份实现（理由整段在 `inbound::REGISTRY` 那一段）。
    "--accounts-list",
    "--accounts-sessions",
    // 〔C4c · 第四波 4B〕帧命令 `accounts-trust` 自动派生出来的 CLI 面（与 `--account-trust` / `--account-trust-zero`
    //   是同一个函数的两个宿主）。⚠ 逼出一次 `BUILD_ID` bump —— 本路不 bump，合并那一拍统一做。
    "--accounts-trust",
    // ── P4d：控制面的 CLI 面。**它们不在这里各写一条实现** ——
    // 分派臂按 `cli_control::spec_for` 派生（见下面那条臂），实现落在 `inbound::REGISTRY`。
    // 登记在这张表里是因为 `is_query_mode` 与 `argv_table_guard` 都读它，
    // 而且 `build_id_guard` 的指纹也取自它 ⇒ 加在这里会**逼出一次 BUILD_ID bump**，那正是要的。
    // P4f：cc-bus 的两条基础命令。⚠ **不加这两行的后果是静默的** ——
    // 分派臂是派生的（认得出来），但 `is_query_mode` 这道**闸门**读的是本表：
    // 不在表里 ⇒ 当成未知 flag ⇒ 打一行 warn 之后**照常进流模式**，
    // CLI 面看上去"存在"却永远调不到（08-13 实测到了这个形状）。
    // ⇒ 现由 `cli_control::tests::every_cli_exposed_command_is_in_the_query_mode_gate` 钉住。
    "--bus-kill",
    "--bus-list",
    "--bus-send",
    // 〔BS1b 09-24〕派生协作 agent（`inbound::REGISTRY` 的 `bus-spawn`）。登记理由同上面那三条。
    "--bus-spawn",
    // `K-R113`：cc-bus 的**具名读命令**（名单 ＋ spawn 台账一次回全）。
    // 登记在这里的理由与上面那三条逐字相同 —— `is_query_mode` 那道闸门读的就是本表。
    "--bus-state",
    // `K-R86`：只读的一次性抓屏原语。登记在这里的理由与上面那几条逐字相同 ——
    // `is_query_mode` 那道**闸门**读的就是本表，不在表里 ⇒ 被当未知 flag ⇒
    // 打一行 warn 之后**照常进流模式**，调用方拿到一堆 jsonl 行而不是那一屏。
    "--capture-pane",
    // 〔B2 · 条 66〕「退出行为」那个值的两条命令（`inbound::REGISTRY` 的 `exit-policy-*`）自动派生的 CLI 面。
    // 登记理由与上面那几族逐字相同 —— `is_query_mode` 那道闸门读本表，不在表里 ⇒ 当未知 flag 静默进流模式。
    // ⚠ 加这两行会逼出一次 `BUILD_ID` bump（`build_id_guard`）—— 本路**不 bump**，合并那一拍统一做。
    "--exit-policy-read",
    "--exit-policy-set",
    // 〔RM1a · 第四波〕账号层那份凭据文件在这台机器上的两条命令（`inbound::REGISTRY` 的 `apikey-*`）
    // 自动派生的 CLI 面。⚠ `--apikey-key-set` 的入参（含 key）**从 stdin 读**（`takes_input: true`），
    // 不收 argv —— argv 在同机任何用户的 `ps` 里都看得见。加这两行会逼出一次 `BUILD_ID` bump，本路不 bump。
    "--apikey-key-set",
    "--apikey-read",
    // 〔RM1a · 第四波〕中转（层 1）那两条（`inbound::REGISTRY` 的 `relay-*`）自动派生的 CLI 面。
    // 入参只有端口，从 stdin 读。同上：加这两行会逼出一次 `BUILD_ID` bump，本路不 bump。
    "--relay-ensure",
    "--relay-status",
    // 〔RM1a · 第四波〕「足迹」的这台机器那一半（`inbound::REGISTRY` 的 `footprint-probe`）派生的 CLI 面。只读。
    "--footprint-probe",
    // 〔AS1 · 第四波 4B〕MCP 资产同步的判定（`inbound::REGISTRY` 的 `mcp-sync-plan`）派生的 CLI 面。只读，入参从 stdin 读。
    // 加这一行会逼出一次 `BUILD_ID` bump（`build_id_guard`）—— 本路**不 bump**，合并那一拍统一做。
    "--mcp-sync-plan",
    // 〔AS2 · 第四波 4B〕资产目录那两条（`inbound::REGISTRY` 的 `assets-catalog` / `assets-catalog-merge`）派生的 CLI 面。
    // 加这两行会逼出一次 `BUILD_ID` bump（`build_id_guard`）—— 本路**不 bump**，合并那一拍统一做。
    "--assets-catalog",
    "--assets-catalog-merge",
    // 〔AS2〕`assets-sync` 是真异步命令 ⇒ 按派生规则上 CLI 面（`cli_control::cli_exposed`：非内建即上）。
    // ⚠ 一次性进程没有常驻那一个的连接池与可达表：它自己新拨一条、对那一台做一趟，扇出恒为零台。
    "--assets-sync",
    // 〔AS2〕skill「装到这台」那两条（`skill-read` / `skill-install-plan`）派生的 CLI 面。只读，入参从 stdin 读。
    "--skill-read",
    "--skill-install-plan",
    // 〔C4d · 第四波 4B〕可达表登记（`inbound::REGISTRY` 的 `remote-reach`）派生的 CLI 面，入参从 stdin 读。
    // ⚠ 一次性进程的可达表随进程退出就空 —— 真正的用法是常驻后端的帧面。加这一行会逼出一次 `BUILD_ID` bump，本路**不 bump**。
    "--remote-reach",
    // 〔C4d · 第四波 4B〕历史注解那三条（`inbound::REGISTRY` 的 `history-annotate` / `-forget` / `-last-accounts`）派生的 CLI 面。
    // ⚠ 一次性进程多半没被交 `CCM_HISTORY_METADATA` ⇒ 明拒（不猜路径）。加这三行会逼出一次 `BUILD_ID` bump，本路**不 bump**。
    "--history-annotate",
    "--history-forget",
    "--history-last-accounts",
    "--backend-probe",
    // 〔SR1a · 09-24〕`--dial`（拨号代理，`K-P6b` / C2）**从本表摘掉了**：拨号挪进本机那一个常驻后端、
    // 经流上的链路（`link-*` 四条，`dial/link.rs`）做，不再每条链路起一个进程。
    // 〔墓碑 —— 那一行原来的理由要点：「**常驻**（起来就一直搬字节，不返回），配置走**环境变量**
    //  `CCM_DIAL_REQUEST`，argv 与 stdin 都不走」。〕⚠ 摘这一行会让 `build_id_guard` 红 —— 本路**不 bump**，合并那一拍统一做。
    // 〔步 `24f` 第二刀 09-20〕`files-read` 这一族的 CLI 面。登记在这里的理由与上面
    // 那几条逐字相同 —— `is_query_mode` 那道**闸门**读的就是本表；不在表里 ⇒ 当未知 flag
    // ⇒ 打一行 warn 之后照常进流模式，调用方拿到的是一堆 jsonl 行而不是它要的应答。
    // ⚠ 线上名用 `-` 不用 `.`（能力名仍是 `files.ls` 那一套）：理由整段在
    // `files::answer_wire` 的头注 —— 一句话是 `"--files.ls"` 会被本仓两个 token
    // 取词器**静默丢弃**，那等于把这四条从三条判据底下同时抽走而三条都照常报绿。
    // 〔步 `24f` 第三刀 09-21〕同族的第五、第六条（`设计/96 §2.9` 裁）。登记在这里的理由
    // 与上面那四条逐字相同 —— `is_query_mode` 那道闸门读的就是本表。
    // 🔴 〔波 5 ㈠ · 2026-09-23 · `设计/60 §8.6` 第 2 步〕**本族第一条会往盘上写的命令。**
    //
    // 它落在这张表里**不是选择，是派生的必然**：`cli_control::cli_exposed` 逐字是
    // `!matches!(spec.run, Run::Builtin)` —— 只要一条命令进了 `inbound::REGISTRY`
    // 且不是那个硬臂，CLI 面就**自动**认得它；而本表是 `is_query_mode` 那道闸门。
    // ⇒ 不加这一行的后果与上面那几条逐字相同（当未知 flag、照常进流模式）。
    // ⚠ **如实登记一条代价**：因此「写这一面只从帧面进来」这句话**做不到**，
    //   两个宿主共用同一个 `run`（`设计/60 §8.1` 的「一份代码两种宿主」正是这一形）。
    //   写面的「窄」因此**不靠命令面**，而靠 `readonly_guard` 第三层那条
    //   「只从声明过的那一面来」—— 它判的是**谁引用得到那个模块**，不是谁发得出命令。
    "--files-create",
    // 〔波 5 ㈡ 09-23〕同一面的另外五条（`设计/60 §8.6` 第 3 步）。登记理由与上一行逐字相同。
    "--files-chmod",
    "--files-delete",
    "--files-mkdir",
    "--files-rename",
    "--files-write-text",
    // 〔F7a · 第三波 09-24〕同一面第七条（同根内复制）。登记理由与上面逐字相同。
    "--files-copy",
    // 〔F7c · 第三波 09-24〕上传的提交（`设计/60 §13`）。登记理由同上面写面那几条（CLI 面从 `REGISTRY` 派生）。
    "--files-commit-upload",
    // 〔F9c · 第四波〕存盘装不进一行时的两步（逐块进暂存区 ＋ 读回拼起来原地覆盖）。登记理由同上。
    "--files-stage-chunk",
    "--files-commit-text",
    // 〔RW1 · 第四波 09-24〕用户文件的读改写 ＋ 删历史会话。登记理由同上面写面那几条（CLI 面从 `REGISTRY` 派生）。
    "--files-delete-session",
    "--files-peek",
    "--files-put",
    "--files-browse",
    "--files-find",
    "--files-index-rebuild",
    "--files-index-status",
    "--files-ls",
    "--files-stat",
    // 〔F7a · 第三波 09-24〕同族第七、第八条（`设计/60 §13`）。登记理由与上面那几条逐字相同。
    "--files-home",
    "--files-read-text",
    // 〔SE2 · `设计/10 §6 步 6`〕会话内查找（Ctrl+F）。**是新子命令** ⇒ `build_id_guard` 红是预期的，
    // BUILD_ID 由合并那一拍统一 bump（本路不 bump）。
    "--find-in-session",
    "--fork-session",
    // 〔`C1`〕同上一段：`history-*` 六条帧命令的 CLI 面。
    // 〔SR1a〕+2：`history-index` / `history-user-inputs`（骨架索引与大纲清单上帧面）的 CLI 面，
    //   登记理由同上 —— `is_query_mode` 那道闸门读本表。⚠ 逼出 `BUILD_ID` bump，本路不 bump。
    "--history-find",
    "--history-index",
    // 〔CF2 · 第四波 4B〕`history-lines`（按行号取回）的 CLI 面。**是新子命令** ⇒ `build_id_guard` 红是预期的，
    //   BUILD_ID 由合并那一拍统一 bump（本路不 bump）。
    "--history-lines",
    "--history-projects",
    "--history-read",
    // 〔U4b · 第四波〕`history-record` 的 CLI 面（CLI 面从 `REGISTRY` 派生，`is_query_mode` 那道闸门读本表）。
    // **是新子命令** ⇒ `build_id_guard` 红是预期的，BUILD_ID 由合并那一拍统一 bump（本路不 bump）。
    "--history-record",
    "--history-search",
    "--history-sessions",
    "--history-subagents",
    "--history-tail",
    "--history-user-inputs",
    "--kill",
    "--launch",
    "--list-accounts",
    "--list-subagents",
    // 〔`设计/10 §2.2b ⑥` · SE1〕大纲的数据源：「你说过的话」清单。**是新子命令** ⇒
    // `build_id_guard` 红是预期的，BUILD_ID 由合并那一拍统一 bump（本路不 bump）。
    "--list-user-inputs",
    "--list-projects",
    "--list-sessions",
    // 〔RM1c · 第四波〕`panorama` 帧命令**自动派生**出来的 CLI 面（`cli_control::cli_exposed`），
    // 登记理由同 `--tasks-list` 那一段：不在表里 ⇒ `is_query_mode` 当未知 flag ⇒ 静默进流模式。
    // ⚠ 新子命令 ⇒ `build_id_guard` 红是预期的，BUILD_ID 由合并那一拍统一 bump（本路不 bump）。
    "--panorama",
    "--ping",
    // 〔RM1b · 第四波〕`plugins-marketplaces` 帧命令的 CLI 面（同 `--tasks-list` 那一段的理由）。
    "--plugins-marketplaces",
    "--read-session",
    "--read-session-from-offset",
    "--read-session-tail",
    // K-H1：起 HTTP 中转（常驻，不是一次性查询 —— 它住在这张表里是因为
    // `is_query_mode` 那道闸门读的是本表；不登记就会被当成未知 flag 静默进流模式）。
    "--relay",
    "--resolve",
    "--search",
    "--session-accounts",
    // 〔RM1b · 第四波〕`tasks-list` 帧命令**自动派生**出来的 CLI 面（`cli_control::cli_exposed`），
    // 登记理由同 `C1` 那一段：不在表里 ⇒ `is_query_mode` 当未知 flag ⇒ 静默进流模式。
    // ⚠ 新子命令 ⇒ `build_id_guard` 红是预期的，BUILD_ID 由合并那一拍统一 bump（本路不 bump）。
    "--tasks-list",
    "--tmux-notify",
];

// ── 〔步 9 · 09-19〕argv 那一族 ＋ 四个贴着 `main.rs` 的测试模块 ─────────────
//
// 🔴 **它们为什么也在这里，而不是留在 bin**：那四个测试模块用 `super::CAPABILITIES` /
//   `super::is_query_mode` / `super::split_stream_flags` 引这几项。模块搬进 lib 之后
//   `crate::` 指的是 lib ⇒ 要么这几项跟着来，要么在 bin 里**再声明一份**
//   `guard_support` / `protocol_doc_guard`。
//   ⚠ 后者的代价是**那两份的测试会跑两遍**，`backend` 那一格的读数凭空变大 ——
//     「一件事两份账」正是本仓反复治的那个毛病。⇒ 选前者：**零重复编译、测试条数不变**。
//
// ⚠ 这几项本来也不属于「分派」：`is_query_mode` / `split_stream_flags` 是对 argv 的纯函数，
//   `CAPABILITIES` 是一张声明表。in-process 那条路一样要问「这次调用是不是查询模式」。
//   真正的分派（那个 `match`）仍然留在 `main.rs`，规格 `00 §1.5.4` 逐字。

/// F66（#58③）：本构建**声明支持的能力 token**（hello 帧 `capabilities` 字段）。
/// monitor 按此决定发 `--with-bg`/`--tail-only`，不再靠 build_id 精确匹配去猜
/// （闭合 2026-07-09「漏拷身份清单 → 确认不了 → 全降级」事故：能力由后端自己
/// 声明，即使清单丢失也照开）。
///
/// **加法式，两轴正交**：加新能力就往这里加 token（旧 monitor 忽略未知 token）；
/// **绝不为此 bump `PROTO_VERSION`**（那是破坏性变更专用，会把每台旧后端误判
/// Incompatible）。build_id 继续管 staleness / 重部署提示，与能力正交。
///
/// **§26 死循环护栏（硬约束）**：只声明本 backend **会在一次性查询判定前剥离对应
/// flag** 的能力——即每个 token 必须有 `split_stream_flags`（`:76`）里对应的剥离分支。
/// `bg`→`--with-bg`、`tail-only`→`--tail-only`，二者 `split_stream_flags` 都剥。
/// 加新能力 token 时，必须同时给它的 flag 加剥离分支，否则声明它 = 埋死循环
/// （monitor 发对应 flag → 本后端不剥 → 当查询退出 → 无 hello → 重连死循环）。
/// **此硬约束由 `every_capability_token_is_strippable` 测试代码强制**（不再只是约定）。
///
/// # 🔴 `rbind-token`〔`设计/80 §8.7` 步 2，2026-09-22〕—— 它为什么住**这一张**表
///
/// `设计/80 §8` 的方案 E 要把「会话身份」从 tmux 上解绑：起会话的那一方注一个
/// `CCM_RBIND_TOKEN`，后端从 `/proc/<pid>/environ` 读出来、随 `session_added` 报回去。
/// `§8.6 ④` 逐字要求「**能力协商** ＋ 老后端诚实降级」。
///
/// **为什么「字段在不在」自己不够**：`session_added` 上**没有** `rbind_token`
/// 会同时表达两件相反的事 ——「**这台后端不报令牌**」（老后端）与
/// 「**这条会话真的没有令牌**」（不是 monitor 起的，`§8.5 ②` 那个布尔）。
/// 一个缺席的字段分不开它们，而它们要的动作不同（降级回标题路 / 说一句准确的话）。
/// ⇒ 协商必须在**握手**那一层。
///
/// **为什么落在 `capabilities` 而不是 `emits`**：`rbind_token` 是**既有帧上的一个字段**，
/// 不是一个新帧 kind —— 而 `emits` 的取值空间是帧 kind（`wire.rs` 那个字段的头注逐字），
/// 把一个字段名塞进去是在那张表上说假话。
///
/// **它的 flag 是真的，不是为了喂饱护栏编出来的**（`wire.rs` 里 hello 那个
/// 「我做得到什么」面的头注逐字警告过「被迫编一个假 flag（更坏）」）：`--with-rbind-token` 让报令牌这件事
/// **默认关、由客户端显式索要**。依据是 `§8.6 ③` —— 令牌是**敏感数据**，
/// 默认不往 wire 上放，只有真要做 ↗ 关联的那个客户端才请它。
/// 它与 `--with-bg` 是同一族语义（「这条流多报一样东西」），`split_stream_flags` 照样剥它
/// ⇒ `every_capability_token_is_strippable` 拿到的是一条**真** flag。
// ⚠ **排序照字典序**（不是按加入时间）：`capability_ledger_guard::the_stream_flag_list_keeps_its_own_narrow_semantics`
// 拿汇总那侧（排过序）与本表**逐项相等**。
pub const CAPABILITIES: &[&str] = &["bg", "rbind-token", "tail-only"];

// ══════════════════ 步 `8a`：能力清单的**汇总** —— `设计/96 §2` 第 2 层 ══════════════════
//
// 🔴 **这一段填的是 `files/mod.rs` 头注自己登记的那个缺口**，逐字：
//   「`CAPABILITIES` 的汇总没接。`设计/96 §2` 第 2 层要求『能力清单从实现派生，
//    `CAPABILITIES` 由它们汇总而来』。本族把自己那一份声明成了**数据**，但**没有**
//    把它汇进 `lib.rs::CAPABILITIES` —— 那一处的语义今天是『会在一次性查询判定前
//    剥离对应 flag 的**流**能力』，本族六条都不是那种东西，硬塞进去会当场红，
//    **而且会是红对了**。⇒ 汇总要先有第 2 层那个派生机制，那是另一件活。」
//
// 🔴 **「硬塞进去会当场红」这句话本轮现打过，成立**：把 `"files.ls"` 加进上面那个
//   [`CAPABILITIES`] 之后 `backend` 套 `769 passed / 1 failed`，**只红一条**，
//   而且是 `main_stream_flag_tests::every_capability_token_is_strippable` 逐字点名
//   「无 flag 映射 …… 否则埋 §26 死循环」。⇒ 那一处的语义**保持不动**，
//   它在本汇总里是**一个面**（`stream-flags`），不是汇总本身。
//
// # 这一层买到什么 · 买不到什么（`设计/96 §2` 那张三层表逐字）
//
// | 层 | 做什么 | 本段 |
// |---|---|---|
// | 1 | 跨 target 编译门禁 | ❌ 不是本段（门禁的 `muslbuild` / `winchk-backend` 那两格） |
// | 2 | **能力清单从实现派生，`CAPABILITIES` 由它们汇总而来，不许手写** | ✅ **本段** |
// | 3 | 一条对等判据：所有 target 的能力集**完全相等**，不相等要逐条登记豁免 | ❌ 不是本段（见下面「没买到」） |
//
// 🔴 **第 2 层那一栏「买到什么」逐字是「声明与实现不可能不一致（类型层保证）」** ——
//   注意它说的是**不可能**，不是「不一致会被逮到」。⇒ [`capability_ledger`] 刻意写成
//   一个对 [`CAPABILITY_FACES`] 的**纯函数**：汇总侧没有第二份可以漂开的名单，
//   一个能力名**只有一个住址**（它自己那一族的声明表）。
//   而「某一族整个没被登记进来」那一形函数拦不住 —— 那一格由
//   `tests/backend/capability_ledger_guard.rs` 的**源码树点名**接着（两侧异源）。

/// 一个能力**属于哪一类**。`设计/96 §2` 那条射程要求的兑现处。
///
/// # 🔴 为什么这个枚举必须有两个成员（不是分类癖）
///
/// `设计/96 §2` 逐字：
///
/// > **清单的射程要能装下「协议级能力」** —— 不只是「能管哪几类资产」。
/// > ⇒ **清单的条目 ＝「我能管哪几类资产」＋「我认不认这条协议帧」两类**，
/// > 第 2 层那条派生要把后者也派生进来；否则 `05 §8` 步 8 接上来的时候，
/// > 它要协商的东西在清单里找不到住址。
///
/// ⇒ 两个成员就是那两类。**而且两类今天都有真成员**（不是「留着以后用」）：
/// `files-read` 那六条是 [`CapabilityKind::Asset`]，`stream-flags` 那两条是
/// [`CapabilityKind::Protocol`] —— 后者正是 `05 §3.3.3`「对面认不认这条」那一形。
/// 「两个成员各有人用」由 `capability_ledger_guard` 钉住：只剩一类时这条射程就是空话。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CapabilityKind {
    /// **我能管哪几类资产** —— 文件 / 会话 / 插件那一轴。
    Asset,
    /// **我认不认这条协议上的东西** —— 流 flag / 帧 / 命令那一轴（`05 §3.3.3`）。
    Protocol,
}

/// 编译 target —— `设计/96 §2` 那条跨 target 对拍的人群。
///
/// # 🔴 它为什么住在这一层（`设计/96 §2` 第 2 层逐字「**每个能力面**声明自己在哪些
/// target 上有实现」）
///
/// 这个轴原先住 `files/mod.rs`（那时只有一个能力面）。汇总一接上，声明 target 的面
/// 就不止一个 ⇒ 轴留在任何**一个**面里，其余的面都得从一个兄弟那里引它。
/// ⇒ 轴住汇总这一层，`files` 再导出（`files::Target` 原样可用，那一族一个字没改）。
///
/// # 🔴 **本轴判「做得到」，不判「编得过」**〔用户 2026-09-21 拍板，逐字「**做得到**」〕
///
/// 上一版本轴判的是「编不编得过」，并逐字论证过「ccm 声明了 `tmux` 却在 Windows 上
/// 做不到」**不算假声明**，因为它编得过。**用户把这条判法推翻了。**
///
/// 他那一拍是三句，合起来是一条完整裁决：
/// ① 判「**做得到**」；② 「**除非暂时不做**」；③ 「**Windows 用 Windows 自己的后台服务，后面再做**」。
///
/// ⇒ 于是本轴的形状变了三处：
/// 1. `targets` 那一栏的意思从「编得过」变成「**这一面在这个 target 上真的做得到**」；
/// 2. 做不到的那几条要**逐条登记**成 [`TARGET_GAPS`]，每条写明**为什么做不到**
///    与**将来怎么办**（「暂时不做」也是一种答复，但必须写出来）；
/// 3. 「编不编得过」**仍然是一条真性质**，但它不再由本轴承担 —— 它由跨 target
///    编译门禁（`设计/96 §2` 第 1 层）承担，而那一层**本仓今天只有两格**（两条 `-gnu`），
///    macOS 那一格**两道都没有**。
///
/// ⚠ **`wire::Unavailable` 那个轴仍然是另一个轴，别合并** —— 它是**运行期逐机器**的
/// 「这条命令我接得下，但在这台机器上做不到，以及为什么」。
/// 两个轴的区别现在不是「编译期 vs 运行期」了（本轴也谈做得到），而是**粒度**：
/// 本轴谈**一个 target 上普遍做不做得到**（进得了源码树的常量），
/// 那个轴谈**这一台机器上此刻做不做得到**（要连上去才知道，比如 tmux 装没装）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    /// `x86_64-unknown-linux-gnu`（本机原生构建，也是 in-process 那条路的宿主）。
    LinuxGnu,
    /// `*-unknown-linux-musl`（部署到被观测机器的那份静态字节，两个 arch）。
    LinuxMusl,
    /// `x86_64-pc-windows-*`。
    Windows,
    /// `*-apple-darwin`。
    MacOs,
}

/// 全体 target。
pub const TARGETS: &[Target] = &[
    Target::LinuxGnu,
    Target::LinuxMusl,
    Target::Windows,
    Target::MacOs,
];

/// 一个**能力面**的登记 —— `设计/96 §2` 第 2 层那条派生的**人群**。
///
/// ⚠ [`CapabilityFace::declares`] 是一个**函数指针**，不是又抄一份名单。
/// 那是本结构最要紧的一栏：它让「这一面有哪些能力」**只有一个住址**
///（那一族自己的声明表），汇总侧连一个可以漂开的副本都没有。
pub struct CapabilityFace {
    /// 这一面的族名。`设计/96 §2.9` 那种标题。
    pub family: &'static str,
    /// 它属于射程里的哪一类。
    pub kind: CapabilityKind,
    /// 🔴 **这一面的能力名从哪里来** —— 指向那一族**自己**的声明表，不是副本。
    pub declares: fn() -> Vec<&'static str>,
    /// 🔴 **声明表住哪一份文件**（本 crate 源码树内的相对路径）。
    /// `capability_ledger_guard` 拿它与源码树**现打**出来的那一组对拍 ——
    /// 那是「新长出一族却没汇进来」唯一拦得住的地方。
    pub declared_in: &'static str,
    /// 它在哪些 target 上**有实现**（第 2 层逐字要求每个面自己说）。
    pub targets: &'static [Target],
    /// 🔴 上一栏的**依据**，一句话。**不许留空**（判据有长度地板）。
    /// 空着等于「抄了个 `TARGETS` 上去」，而 `设计/96 §2.9` 逐字管那叫**假声明**。
    pub target_basis: &'static str,
}

/// 🔴 **全部能力面 —— 汇总的人群，唯一住址。**
///
/// 加一族能力面 = 往这张表加一行。**漏加**那一形由
/// `capability_ledger_guard::every_capability_table_in_the_tree_is_a_registered_face`
/// 拦住（它从源码树上现打，不问这张表）。
pub const CAPABILITY_FACES: &[CapabilityFace] = &[
    CapabilityFace {
        family: "files-read",
        kind: CapabilityKind::Asset,
        declares: files::capability_names,
        declared_in: "files/mod.rs",
        targets: TARGETS,
        target_basis: "本族目录下**零**平台 `cfg`（本轮现打：`src/backend/files/` 递归 \
                       `grep cfg(target_os|windows|unix|target_family` 零命中）\
                       ⇒ 四个 target 同一份源码。另有本族自己那条更强的：每条能力的 \
                       `Capability::targets` 与 `TARGETS` 做**集合相等**（边界②）。",
    },
    CapabilityFace {
        family: "stream-flags",
        kind: CapabilityKind::Protocol,
        declares: stream_flag_capability_names,
        declared_in: "lib.rs",
        targets: TARGETS,
        target_basis: "`split_stream_flags` 是对 argv 的**纯函数**（`retain` + 两次 \
                       `iter().any`），整份实现零平台 `cfg` ⇒ 四个 target 上逐字同一份。",
    },
    CapabilityFace {
        family: "ccm-launcher",
        kind: CapabilityKind::Asset,
        declares: ccm_capability_names,
        declared_in: "control/ccm/mod.rs",
        targets: TARGETS,
        target_basis: "四个 target 上**这一面都做得到**（起会话 · 认账号 · 选模型 · 传 cwd \
                       那几条在直路上是**原生动作**：`set_var` / `set_current_dir` / \
                       `Command::new` ＋ `#[cfg(not(unix))]` 那条「起它 · 等它 · 透传退出码」\
                       的回退，一步都不经 shell）。⚠ **但不是每一条都做得到** —— \
                       tmux 那一族 6 条在 Windows 上做不到，逐条登记在 `TARGET_GAPS`。\
                       〔2026-09-21 改判：上一版这一栏写的是「四个 target 都编得过」，\
                        而用户把本轴从「编得过」改成了「做得到」⇒ 那句依据不再回答本轴的问题。\
                        「编得过」仍然是真的（两处平台分叉各带 `#[cfg(not(unix))]` 回退分支），\
                        但它归第 1 层那个跨 target 编译门禁。〕\
                       🔴 **〔`P19` 09-22 订正一句账〕** \
                       上一版括号里那句「**不依赖任何 Unix 专有设施**」，\
                        在写下的那一刻对 `codex` 那一支就是假的。\
                        〔⚠ 这句被订正的原话**刻意在源码里连着写**：本轮差点栽在自己的 grep 上 —— \
                         初稿把它断在了 Unix 与「专有设施」之间（Rust 的续行反斜杠），\
                         于是拿「Unix 空格 专有」当针去 grep 全仓**零命中**，\
                         而零命中看起来和「这句话仓里没有」一模一样。\
                         下一个人要找这笔账，靠的就是它在源码里连着。〕\
                        `control/ccm/mod.rs::exec_direct` 从前只要 `needs_bus_id(agent)` 为真\
                        就把**整条**改走 `sh -c`，而它对 codex 恒真 ⇒ 真机现打 \
                        `--agent codex --launcher hostname` → `EXIT=4 program not found`\
                        （`真相源/106 §3.3`；同一个 launcher 在 `--agent claude` 那趟 `EXIT=0`）。\
                        `P19` 把那道闸收窄成「那段配方**真有事可做**时」—— 配方整段裹在 \
                        `if [ -n \"${TMUX:-}\" ]` 里，`$TMUX` 空则它一个字不做 ⇒ \
                        Windows 上（`$TMUX` 恒空）直路不再请 shell 进来，**那句话才开始成立**。\
                        ⚠ **今天仍然非得要 POSIX shell 的两条，它们不在上面那四条里，逐条写明**：\
                        ① `CCM_ENV` 非空（那是一段任意 shell，只有 shell 解释得了）；\
                        ② `resume` 且后端答出一整条命令串（要 shell 拆词：`set -f; exec $cmd`）。\
                        两条都**不假装做得到** —— 只是从今天起说得出口（`no_shell:` ＋ 逐条归因，\
                        `D7`）。\
                        🚫 **这一栏买不到的那一维（别读成已验证）**：\
                        `cargo check --all-targets --target x86_64-pc-windows-gnu` ＋ \
                        `needs_shell` 那张判定表买到的是「**源码里没有那条 Unix 依赖了**」；\
                        「在真 Win11 上 `--agent codex` 真的 `EXIT=0`」**没买到** —— \
                        那要那台独占的 Win11 虚拟机，`P19` 一个字节都没去动它。\
                        能接上的只有一条推理（写出来，别当读数）：claude 那一支的 `EXIT=0` 是 \
                        09-21 真机现打的，而 codex 今天走的是**同一段原生代码、动作还更少**\
                        （`nested_env(\"codex\")` 是空的）。",
    },
];

/// 一条 [`TargetGap`] 属于哪一档 —— 〔PR1 · 2026-09-24〕「差异」要分两档登记，**不许合成一档**。
///
/// # 为什么非分不可
///
/// 两档要的**下一步动作相反**：结构上没有的那一格，正确的结局是**永远留在表里**；
/// 欠着的那一格，正确的结局是**有一天被删掉**（`P19` 删 `agent` 那一行就是这一形）。
/// 混成一档 ⇒ 读表的人分不清「这一行该不该有人去还」，而「欠着」会被读成「本来就这样」——
/// 那正是 `真相源/30` 与 `no_timer_guard` 各栽过一次的静默缩水。
///
/// ⚠ 与 `parity_ledger`（本机 ↔ 远端那条轴）的 `NaturallyAsymmetric` / `ParityDebt` 是**同一对**，
/// 只是轴换成了编译 target。两张表不合并：轴不同，人群不同。
///
/// # 判准（写死在这里，判据按它查 `why` 的措辞）
///
/// - [`GapKind::Structural`]：这条能力的**名字与定义**就绑在一个那个 target 上不存在、也不打算补的
///   机制上（例：tmux 的 `base-index`）。那边将来若长出等价物，它是**另一条能力、另立一行**，
///   不是把这一条补上。⇒ `why` 要说出「不跨过去 / 不照搬」，**不许**再说「暂时不做」（那是对欠账的措辞）。
/// - [`GapKind::Owed`]：这条能力本身与机制无关，那个 target **该有**而今天没有。
///   ⇒ `why` 要说出谁来还、什么时候还（「暂时不做」也算答复，但要写出来）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GapKind {
    /// 这个 target **结构上**没有这条能力。
    Structural,
    /// **欠着**：该有、今天没有、写明将来怎么还。
    Owed,
}

/// 一条**逐能力**的豁免：这一面在这个 target 上**做不到**这一条能力。
///
/// # 🔴 它为什么必须存在（而上一版没有它）
///
/// 本轴一旦从「编得过」改成「**做得到**」（用户 2026-09-21 拍板），
/// `ccm-launcher` 那一面就**不可能**再对四个 target 整面成立 —— Windows 上没有 tmux。
/// ⇒ 要么把整面从 Windows 上摘掉（**那会连 12 条真做得到的一起摘掉，是假的**），
/// 要么逐条登记豁免。**只有后者说的是真话。**
///
/// ⚠ **「暂时不做」是一种合法答复，但必须写出来** —— 用户那一拍逐字
/// 「**做得到. 除非暂时不做. windows用windows自己的后台服务. 后面在做**」。
/// ⇒ [`TargetGap::why`] 要同时答两件：**今天为什么做不到** ＋ **将来怎么办**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetGap {
    /// 哪一面。必须是 [`CAPABILITY_FACES`] 或 [`COMMAND_FACES`] 里真有的族名。
    pub family: &'static str,
    /// 哪一条能力。必须是那一面**真的声明过**的名字（判据拦幽灵）。
    pub capability: &'static str,
    /// 哪个 target 上做不到。
    pub target: Target,
    /// 🔴 **哪一档**：结构上没有 · 欠着。两档不许合成一档（[`GapKind`] 头注）。
    pub kind: GapKind,
    /// 🔴 **为什么做不到 ＋ 将来怎么办。** 不许留空（判据有长度地板）。
    pub why: &'static str,
}

/// 🔴 **全部逐能力豁免 —— 唯一住址。**
///
/// 〔FW5 · 09-24〕**14 → 16**：`files-chmod` × Windows（帧面 ＋ CLI 面各一行，档 = 结构），
/// 由 `no_unix_mode` 码 × [`unix_mode_bits_on`] 现推出来（表尾那一段）。
///
/// 〔PR1 · 09-24〕**8 → 14**：命令面并进第 3 层之后，帧面 3 条 ＋ CLI 面 3 条（`capture-pane` /
/// `kill` / `launch` × Windows）被那条横向两向相等**现推出来**、逐条登记在表尾。下面那段账说的是
/// `ccm-launcher` 那 8 条：
///
/// 那 8 条都在 `ccm-launcher` × Windows 这一格上：
/// **6 条**是读源码推出来的（tmux 那一族），**另 2 条是真机现打补的**
/// （`bus-register` / `ccm-sid`）—— 上一版的账把它们记成「做得到」，
/// 🔴 **那两格是错的，不是缺的**。逐条读数住 `真相源/106`。
///
/// 🔴 〔`P19` 09-22〕**9 → 8：`agent` 那一条删了，而它是这张表里第一条被「做掉」的。**
/// 前两次改这张表都是**往里加**（读源码推的 6 条 → 真机现打补 3 条），本轮第一次**往外减**
/// ⇒ 减法的判准与加法不同，写在那一行的墓碑里（`capability` 那一栏搜 `P19`）：
/// 加一条要的是「现打出它做不到」，**减一条要的是「那条『做不到』的根因在源码里没了」＋
/// 一句如实登记「真机那一维这次买不到」**。别把它读成「真机上复验过了」。
pub const TARGET_GAPS: &[TargetGap] = &[
    // ── `ccm-launcher` × Windows：tmux 那一族 6 条 ─────────────────
    //
    // 🔴 **将来的答复是用户给的，不是我们挑的**：他 2026-09-21 逐字
    //    「**windows用windows自己的后台服务. 后面在做**」
    //    ⇒ 方向已定（**不是**去 Windows 上装个 tmux，也**不是**自己写一个终端复用器），
    //      落地时间**明确推后**。这六条的 `why` 都指着同一句裁决。
    TargetGap {
        family: "ccm-launcher",
        capability: "tmux",
        target: Target::Windows,
        kind: GapKind::Owed,
        why: "Windows 上没有 tmux（也不打算装）。将来由 **Windows 自己的后台服务**               承担「会话活在前端之外」这件事〔用户 2026-09-21 拍板〕，**暂时不做**。",
    },
    TargetGap {
        family: "ccm-launcher",
        capability: "attach",
        target: Target::Windows,
        kind: GapKind::Owed,
        why: "「接回一个还活着的会话」今天的实现是 `tmux attach`。Windows 上那条路不存在               ⇒ 等那个后台服务落地时一并给出等价物〔同上裁决〕，**暂时不做**。",
    },
    TargetGap {
        family: "ccm-launcher",
        capability: "detach",
        target: Target::Windows,
        kind: GapKind::Owed,
        why: "「把会话留在后台」今天是 `tmux detach`。同 `attach`，等那个后台服务，**暂时不做**。",
    },
    TargetGap {
        family: "ccm-launcher",
        capability: "tmux-size",
        target: Target::Windows,
        kind: GapKind::Structural,
        why: "给 tmux 那个窗格定尺寸。没有 tmux 就没有这一格；将来那个后台服务里              「会话的终端多大」是另一种形状，**不照搬这一条** ⇒ 这一条本身不跨过去；\
              那个后台服务里的尺寸若要有，是**另一条**能力、另立一行〔PR1 分档：结构〕。",
    },
    TargetGap {
        family: "ccm-launcher",
        capability: "tmux-base",
        target: Target::Windows,
        kind: GapKind::Structural,
        why: "tmux 的窗格编号基数（`base-index`）。它是 tmux 自己的配置面，              Windows 上连对应概念都没有 ⇒ **不是推后，是这一条本身不该跨过去**。",
    },
    // ── `ccm-launcher` × Windows：**真机现打补上的三条**〔2026-09-21〕 ───────
    //
    // 🔴 上面那六条是**读源码**推出来的（「Windows 上没有 tmux」）。
    //    这三条不是 —— 它们是在本机那台 Win11 虚拟机上**真跑 `ccm.exe`** 量出来的，
    //    而上一版的账把它们记成「做得到」。⇒ **那三格是错的，不是缺的。**
    //
    // ⚠ 如实写清本表买不到什么：`why` 里那几句是**读数的转述**，
    //    而「这条能力在 Windows 上做不到」这件事**本 crate 的判据买不到**
    //    （要真机）。判据能守的只有「名字真存在」「答了将来怎么办」「不是整面豁免」。
    //    真机那一维的住址是 `真相源/106`（跑法、命令、读数逐条在那儿）。
    TargetGap {
        family: "ccm-launcher",
        capability: "bus-register",
        target: Target::Windows,
        kind: GapKind::Owed,
        why: "**硬链在 tmux 上，不是自己做不到**：`argv.rs` 两道闸串着 —— \
              `--bus-register` 要 `--detach`，而 `--detach` 要 `--tmux`。\
              真机现打（Win11）把两个 bus 脚本都种齐、排掉「脚本缺失」这个变量之后，\
              仍然 `EXIT=4 no_tmux` ⇒ **唯一闸门就是 tmux**。\
              ⇒ 等那个 Windows 后台服务落地时，「把会话登记到总线上」要重新回答一次\
              〔用户 2026-09-21 拍板〕，**暂时不做**。",
    },
    TargetGap {
        family: "ccm-launcher",
        capability: "ccm-sid",
        target: Target::Windows,
        kind: GapKind::Owed,
        why: "〔S5 · 第四波 09-24〕**直路语义已定，Windows 上仍欠在读侧。** \
              从前：`--ccm-sid` 只在容器（tmux）那条路上被消费（`Container.ccm_sid` → \
              `tmux set-option @ccm_sid_expect`），直路上被接受、零效果、不出声。\
              主会话裁：**不报错**（报错 ＝ 让它依赖 tmux，撞 V63），直路语义走启动期令牌 \
              （`control/ccm/plan.rs::DirectIdentity`：有合格的 `CCM_RBIND_TOKEN` ⇒ 由它承载；\
              没有 ⇒ 说一句、照常起）。\
              🔴 **而令牌那条路的读侧在 Windows 上不通**：后端从 agent 进程的环境里读令牌 \
              （`identity_tag::rbind_token_of` → `platform::proc::proc_env_var`），非 Linux 恒 `Unreadable` \
              （`WN1.md §1` 件 E：要读对方 PEB，未做）⇒ 令牌注进去了也读不回来 ⇒ 这一格在 Windows 上仍是欠账。\
              **将来**：件 E（Windows 上读别的进程的环境）落地那一拍，这一行跟着删；在那之前暂时不做。",
    },
    // ── 🔴 〔散文墓碑〕〔`P19` 09-22〕**`agent` 那一条豁免删了，原话留在这里** ──────
    //
    // 它从前逐字写着：
    //   「**只有一半做得到**：`claude` 那支真机 `EXIT=0`，`codex` 那支 `EXIT=4 program not
    //     found`。根因不在 agent 本身 —— `needs_bus_id("codex")` 恒真 ⇒ 那一趟整条改走
    //     `exec_shell` ⇒ `sh -c`，而 Windows 上没有 `sh`。……**将来**：要给 codex 的 cc-bus
    //     身份配方一条**不经 shell** 的路，与那个 Windows 后台服务同一拍做，**暂时不做**。」
    //
    // ⇒ **那条「将来」今天做了，而且它不用等那个 Windows 后台服务**（当初把两件事绑在一拍
    //   是估错了）：`control/ccm/mod.rs::needs_shell` 把闸从「codex 就要 shell」收窄成
    //   「codex **且** `$TMUX` 非空」。收窄靠的是一条**等价**不是近似 ——
    //   `BUS_ID_RECIPE` 整段裹在 `if [ -n "${TMUX:-}" ]` 里，守卫为假时它在 `exec` 之前
    //   一个字都不做（那条前提由
    //   `control::ccm::tests::the_bus_id_recipe_is_wholly_guarded_by_tmux_so_skipping_the_shell_is_exact`
    //   钉着）。Windows 上 `$TMUX` 恒空 ⇒ 那一跳没了。
    //
    // 🚫 **删它买到的与买不到的，分开记（别读成「真机复验过」）**：
    //   · 买到：`cargo check --all-targets --target x86_64-pc-windows-gnu` 过 ＋
    //     `needs_shell` 那张判定表（codex × 在/不在 tmux 四格逐个相等断言）＋
    //     `--print` 逐字节没动（`§33a` 铁律 2）。
    //   · **没买到**：在真 Win11 上 `--agent codex --launcher hostname` 真的 `EXIT=0`。
    //     那台虚拟机是独占资源，`P19` 一个字节都没去动它。
    //   ⇒ 于是这一条的处置只有两种，两种都有代价：**留着**（断言一件已经被源码否掉的
    //     「做不到」，账当天开始撒谎）或**删掉**（把「做得到」建在源码 ＋ 判据 ＋ 一条
    //     推理上，而不是建在真机读数上）。选后者，理由写在 `CAPABILITY_FACES` 那一栏的
    //     `🚫 买不到的那一维` 里 —— **那句话就是这一行的对价，不许连它一起删。**
    //
    // ⚠ **一件真的残留，它不归本条**：Windows 上没有 tmux ⇒ codex 永远拿不到
    //   `CC_BUS_ID`。那不是 `agent` 这条能力做不到，是**载体没了**（与
    //   `base-url-across-tmux` 同一形），已由 tmux 那一族 6 条豁免覆盖；
    //   判准住 `control/ccm/mod.rs::needs_bus_id` 的头注。
    TargetGap {
        family: "ccm-launcher",
        capability: "base-url-across-tmux",
        target: Target::Windows,
        kind: GapKind::Owed,
        why: "把中转地址跨 tmux 会话传下去。载体没了这一条就没了；              将来那个后台服务要自己回答「地址怎么传给它起的会话」，**暂时不做**。",
    },    // ── 〔PR1 · 2026-09-24〕命令面 × Windows：**不是新裁的，是第一次被看见** ────────────
    //
    // 这六行在 PR1 之前就是真的（`K-P4` 那一拍 `unavailable_from` 在 Windows 上就会列出这三条），
    // 只是没有任何东西把命令面放进 target 这根轴里对 —— PR1 把帧面 / CLI 面并进第 3 层的人群，
    // 那条两向相等当场点出它们，于是逐条登记。**差异是现推出来的**（命令自己声明的 `no_tmux` 码 ×
    // Windows 平台档），这里只补理由与档。
    // 档：六条都是**欠着** —— 「起会话 / 杀会话 / 看一眼画面」与 tmux 无关，是 Windows 该有的；
    // 谁来还是用户 09-21 那一句（「Windows 用 Windows 自己的后台服务，后面再做」）。
    TargetGap {
        family: "wire-commands",
        capability: "capture-pane",
        target: Target::Windows,
        kind: GapKind::Owed,
        why: "抓一屏今天就是起一次 `tmux capture-pane`（命令自己声明了 `no_tmux` 码）。\
              Windows 上没有 tmux ⇒ 平台默认做不到。将来由那个 Windows 后台服务给出\
              「看一眼会话画面」的等价物〔用户 2026-09-21 拍板〕，**暂时不做**。",
    },
    TargetGap {
        family: "wire-commands",
        capability: "kill",
        target: Target::Windows,
        kind: GapKind::Owed,
        why: "结束会话今天是起一次 `tmux kill-session`（命令自己声明了 `no_tmux` 码）。\
              Windows 上没有 tmux ⇒ 平台默认做不到。将来由那个 Windows 后台服务\
              管会话的生死〔用户 2026-09-21 拍板〕，**暂时不做**。",
    },
    TargetGap {
        family: "wire-commands",
        capability: "launch",
        target: Target::Windows,
        kind: GapKind::Owed,
        why: "起会话今天是在 tmux 里开一个新会话（命令自己声明了 `no_tmux` 码）。\
              Windows 上没有 tmux ⇒ 平台默认做不到。将来由那个 Windows 后台服务\
              承担「会话活在前端之外」〔用户 2026-09-21 拍板〕，**暂时不做**。",
    },
    TargetGap {
        family: "cli-subcommands",
        capability: "--capture-pane",
        target: Target::Windows,
        kind: GapKind::Owed,
        why: "与帧面 `capture-pane` 那一行是**同一条实现**（CLI 面经 `cli_control::spec_for` \
              派生到同一条登记）⇒ 同一个理由，将来与帧面那一行同拍还，**暂时不做**。",
    },
    TargetGap {
        family: "cli-subcommands",
        capability: "--kill",
        target: Target::Windows,
        kind: GapKind::Owed,
        why: "与帧面 `kill` 那一行是**同一条实现**（CLI 面经 `cli_control::spec_for` \
              派生到同一条登记）⇒ 同一个理由，将来与帧面那一行同拍还，**暂时不做**。",
    },
    TargetGap {
        family: "cli-subcommands",
        capability: "--launch",
        target: Target::Windows,
        kind: GapKind::Owed,
        why: "与帧面 `launch` 那一行是**同一条实现**（CLI 面经 `cli_control::spec_for` \
              派生到同一条登记）⇒ 同一个理由，将来与帧面那一行同拍还，**暂时不做**。",
    },
    // ── 〔FW5 · 第四波 · 2026-09-24〕`files-chmod` × Windows：`设计/96 §8.5` 待拍 3 ────────────
    //
    // PR1 报告「买不到 1」逐字：`change_mode` 在非 unix 上恒回失败，而 `files-chmod` 的 `codes`
    // 说不出「这个平台没有」⇒ 本条判它处处都在。FW5 给它声明了 `no_unix_mode`，现推段按
    // `unix_mode_bits_on` × 这个码把它从 Windows 上摘掉 ⇒ 这两行是**现推出来**、再补理由与档。
    // 档：**结构** —— 这条能力的名字与定义就是「改 unix 权限位」（低 12 位的 rwx/suid/sgid/sticky），
    // Windows 没有这个机制（那边是 ACL 与只读属性）；那边若要「改访问权限」是另一条能力、另立一行。
    TargetGap {
        family: "wire-commands",
        capability: "files-chmod",
        target: Target::Windows,
        kind: GapKind::Structural,
        why: "改的是 **unix 权限位**（低 12 位），命令自己声明了 `no_unix_mode` 码。Windows 上没有这套位\
              （那边是 ACL ＋ 只读属性），`change_mode` 在那里回 `no_unix_mode`、一个字节不动。\
              ⇒ 这一条本身**不该跨过去**；Windows 那边若要「改访问权限」，是另一条能力、另立一行〔FW5 分档：结构〕。",
    },
    TargetGap {
        family: "cli-subcommands",
        capability: "--files-chmod",
        target: Target::Windows,
        kind: GapKind::Structural,
        why: "与帧面 `files-chmod` 那一行是**同一条实现**（CLI 面经 `cli_control::spec_for` \
              派生到同一条登记）⇒ 同一个理由：unix 权限位这一条**不该跨过去**。",
    },
];

/// [`CAPABILITIES`] 的名单，包成 [`CapabilityFace::declares`] 要的形状。
///
/// ⚠ 它**不是**第二份名单，是同一个 const 的一次借用：这一族的能力名住址仍然只有
/// [`CAPABILITIES`] 那一行。
fn stream_flag_capability_names() -> Vec<&'static str> {
    CAPABILITIES.to_vec()
}

/// `control::ccm::CAPABILITIES` 的名单，同 [`stream_flag_capability_names`] 的理由。
fn ccm_capability_names() -> Vec<&'static str> {
    control::ccm::CAPABILITIES.to_vec()
}

/// 🔴🔴 **汇总本体** —— `设计/96 §2` 第 2 层那份「由它们汇总而来」的清单。
///
/// 交出去的是 `(族名, 能力名)` 的**有序**表。
///
/// # 为什么是一个函数，而不是一张 `const`
///
/// 第 2 层那一栏「买到什么」逐字是「**声明与实现不可能不一致（类型层保证）**」。
/// 一张 `const` 汇总表会是**第二个住址** ⇒ 它与各族的声明之间又要一条判据，
/// 而那条判据只能在**漂开之后**出声。写成纯函数之后那一类漂开**不可表示**。
///
/// ⇒ 代价如实写：**本函数与各族的声明是同源的**，「汇总 == 各族的并集」拿本函数
/// 去对各族的声明会是恒真（本仓逐字「恒等两侧同源会恒真」）。所以那条相等断言的
/// 另一侧**不在本 crate 的源码里** —— 它是 `capability_ledger_guard` 里那张住
/// `tests/backend/` 的点名表，两侧**不会被同一次编辑改到**。
pub fn capability_ledger() -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = CAPABILITY_FACES
        .iter()
        .flat_map(|f| (f.declares)().into_iter().map(|n| (f.family, n)))
        .collect();
    out.sort_unstable();
    out
}

// ══════════════ PR1：`设计/96 §2` 第 3 层的**人群** —— 每个 target 上做得到的那一份 ══════════════
//
// 🔴 **这一段只许从声明现推，一个能力名都不许手抄。** 本波另有几路在往命令表 / 能力表里加行
//   （`inbound::REGISTRY` · `SUBCOMMANDS` · 各族 `CAPABILITIES`）；这里要是抄一份清单，
//   合并那天必然分叉。⇒ 下面每一个名字都来自它那一族**自己**的声明表。
//
// 🔴🔴 **这一段刻意一个字都不读 [`TARGET_GAPS`]。** 对等断言的两侧是：
//   ① 本段**现推**出来的「每个 target 上各有哪些」（读实现侧的声明：面的 `targets` ·
//      `ccm-launcher` 那几条的载体 · 命令的 `codes` 里有没有 `no_tmux` × 平台那一维的编译期结论）；
//   ② [`TARGET_GAPS`] 那张**差异登记表**（理由 ＋ 档）。
//   本段要是从 ② 减出 ①，「差异 == 登记表」就是 `x == x`（本仓逐字「恒等两侧同源会恒真」）。
//   ⇒ 由 `target_parity_guard::the_derivation_never_reads_the_gap_table` 从源码上钉零命中。
//
// ⚠ **本段是声明层，不是产物层。** 「每个 `[[bin]]` 都有一条入包路线」那一层是
//   `tests/evidence/K-R124-ruler.py` ⑭ 的事（两向相等，已在门禁上）；本段不复制它。
//   而 `设计/96 §7` 那句「没有字节的 target，第 3 层是在对空集断言」对本段**如实成立**：
//   macOS 今天两道编译门禁都没有、也没有产线（`§7.1.1b` 第 5、6 行）⇒ 本段对 [`Target::MacOs`]
//   那一列判的只是「**声明上**与别的 target 一样」，不是「那边真跑得起来」。

/// 编译 target → 那一格在「tmux 在不在」这件事上**平台这一维**给的档（[`TmuxPlatform`]）。
///
/// # 为什么它是本段的一根柱子
///
/// 本仓今天所有「这个 target 上做不到」的声明，归根到底只有一种机制：**载体是 tmux，
/// 而那个 target 没有 tmux**。[`TmuxPlatform`] 已经把「Windows 上确证没有」这句话
/// 做成了编译期的值（`K-P4` 那一拍，头注逐字「这一句在**编译期**就成立，不需要探针去证」）。
/// ⇒ 本函数只把 [`Target`] 这根轴接到那根轴上，**不另立一份平台知识**。
///
/// ⚠ 两根轴对不对得上，由 `target_parity_guard::the_target_axis_agrees_with_the_host_tmux_platform`
/// （本机那一格，运行期）与下面那条 `#[cfg(windows)]` 编译期断言（Windows 那一格）各钉一半。
pub const fn tmux_platform_of(t: Target) -> TmuxPlatform {
    match t {
        Target::Windows => TmuxPlatform::AbsentUnlessExeOnPath,
        Target::LinuxGnu | Target::LinuxMusl | Target::MacOs => TmuxPlatform::AskThePath,
    }
}

/// 〔FW5 · 第四波〕编译 target → 那个平台**有没有 unix 权限位**（`files-chmod` 那一格的载体）。
///
/// 与 [`tmux_platform_of`] 并列的第二根平台轴，同一条纪律：**只把 [`Target`] 接到一个编译期就成立的事实上**，
/// 不另立平台知识 —— 事实是「`change_mode` 那一支是 `#[cfg(unix)]`」，本函数与它对不对得上由
/// `target_parity_guard::the_unix_mode_axis_agrees_with_what_this_binary_was_compiled_with`（本机一格）
/// 与下面那条 `#[cfg(windows)]` 编译期断言（Windows 一格）各钉一半。
///
/// ⚠ macOS 是 unix（有权限位）⇒ `true`；本仓那一列今天纯声明（同本段头注）。
pub const fn unix_mode_bits_on(t: Target) -> bool {
    match t {
        Target::Windows => false,
        Target::LinuxGnu | Target::LinuxMusl | Target::MacOs => true,
    }
}

/// ★ 只在 Windows 编译时存在：[`unix_mode_bits_on`] 说 Windows 没有 unix 权限位，
/// 而这份二进制确实不是 unix（`change_mode` 编进去的是回 [`NO_UNIX_MODE`] 那一支）。
#[cfg(windows)]
const _: () = assert!(
    !unix_mode_bits_on(Target::Windows) && !cfg!(unix),
    "Target 轴说 Windows 有 unix 权限位，或者这份 Windows 二进制竟然是 unix —— 两根轴分叉了"
);

/// ★ 只在 Windows 编译时存在：[`tmux_platform_of`] 给 Windows 的档 == 那份二进制真编进去的 [`TMUX_PLATFORM`]。
///
/// 同 `TMUX_PLATFORM` 旁边那条：本机（Linux）门禁上它**不存在**，开口的时刻是
/// `winchk-backend` 那一格的 Windows 编译。
#[cfg(windows)]
const _: () = assert!(
    tmux_platform_of(Target::Windows) as u8 == TMUX_PLATFORM as u8,
    "Target 轴说 Windows 是一档，TMUX_PLATFORM 说是另一档 —— 两根平台轴分叉了"
);

/// 在 target `t` 上，**不看任何一台机器**、只凭平台就成立的 tmux 结论。
///
/// 入参 `PATH` 刻意给 `None`：unix 那几档因此答「不知道」（`None`）——
/// 按 [`unavailable_from`] 头注那条三态处置，「不知道」**不许**压成「做不到」；
/// Windows 那一档答「确证没有」（`Some(false)`）—— 那是它的平台默认，不靠探针。
fn tmux_by_platform(t: Target) -> Option<bool> {
    tmux_present(tmux_platform_of(t), None)
}

/// 帧面命令里，在 target `t` 上**平台默认做不到**的那几条（`codes` 里声明了 `no_tmux` 的 ·
/// 〔FW5〕声明了 [`NO_UNIX_MODE`] 且那个 target 没有 unix 权限位的）。
///
/// tmux 那一维读的就是生产里填 `hello.unavailable` 的那一个函数（[`unavailable_from`]），不另写判准；
/// unix 权限位那一维同形：**谁声明会回那个码，谁就依赖那个机制**，从 `codes` 现推，不抄名单。
fn wire_commands_unavailable_on(t: Target) -> Vec<String> {
    let mut out: Vec<String> = unavailable_from(tmux_by_platform(t))
        .into_iter()
        .map(|u| u.command)
        .collect();
    if !unix_mode_bits_on(t) {
        out.extend(
            inbound::REGISTRY
                .iter()
                .filter(|s| s.codes.contains(&NO_UNIX_MODE))
                .map(|s| s.name.to_string()),
        );
    }
    out
}

/// `ccm-launcher`：载体是 tmux 的那几条，在平台确证没有 tmux 的 target 上摘掉。
///
/// 「哪几条载体是 tmux」住 `control::ccm::CCM_TMUX_CARRIED`（紧挨着那一面的 `CAPABILITIES`，
/// 一条能力一个住址）；这里只做「× 平台档」那一步。
fn ccm_launcher_on(t: Target) -> Vec<&'static str> {
    let no_tmux = tmux_by_platform(t) == Some(false);
    control::ccm::CAPABILITIES
        .iter()
        .copied()
        .filter(|c| !(no_tmux && control::ccm::CCM_TMUX_CARRIED.contains(c)))
        .collect()
}

/// 帧面命令：`inbound::REGISTRY` 减去这个 target 上平台默认做不到的那几条。
fn wire_commands_on(t: Target) -> Vec<&'static str> {
    let gone = wire_commands_unavailable_on(t);
    inbound::REGISTRY
        .iter()
        .map(|s| s.name)
        .filter(|n| !gone.iter().any(|g| g == n))
        .collect()
}

/// CLI 面：`SUBCOMMANDS` 里**经 `REGISTRY` 派生**的那几条，跟着它的帧面那一条走；
/// CLI 独有的那几条没有任何逐 target 声明 ⇒ 处处都在（本断言对它们**看不见**差异，如实登记在
/// `target_parity_guard` 头注的「买不到」里）。
fn cli_subcommands_on(t: Target) -> Vec<&'static str> {
    let gone = wire_commands_unavailable_on(t);
    SUBCOMMANDS
        .iter()
        .copied()
        .filter(|flag| {
            control::cli_control::spec_for(flag).is_none_or(|s| !gone.iter().any(|g| g == s.name))
        })
        .collect()
}

fn wire_command_names() -> Vec<&'static str> {
    inbound::REGISTRY.iter().map(|s| s.name).collect()
}

fn cli_subcommand_names() -> Vec<&'static str> {
    SUBCOMMANDS.to_vec()
}

/// 🔴 **命令面** —— 第 3 层人群里、**不进**第 2 层汇总（[`capability_ledger`]）的那两面。
///
/// # 为什么不直接加进 [`CAPABILITY_FACES`]
///
/// 加进去它们就进了 `capability_ledger_guard::ROSTER` 那张**逐条点名表**的射程 ——
/// 那张表是手写的，而本波正有好几路在往这两张命令表里加行 ⇒ 每一路合并都得回来补点名，
/// 漏补就红。那正是「手抄一份清单，合并那天必分叉」。
/// 而这两张表的名字**另有裁决处**：帧面每一条要在 `IPC-PROTOCOL.md §10` 有小节
/// （`protocol_doc_guard`）、CLI 面每一条逼一次 `BUILD_ID` bump（`build_id_guard`）。
/// ⇒ 它们只进第 3 层那条横向对等，不进第 2 层那张点名表。**并不并进汇总是一个待拍的设计题**，已报备。
pub const COMMAND_FACES: &[CapabilityFace] = &[
    CapabilityFace {
        family: "wire-commands",
        kind: CapabilityKind::Protocol,
        declares: wire_command_names,
        declared_in: "inbound.rs",
        targets: TARGETS,
        target_basis: "帧面每一条命令四个 target 上**都编得进去**（`inbound::REGISTRY` 零平台 `cfg`）；\
                       做不做得到按它**自己声明的码**分：`codes` 里有 `no_tmux` 的，在平台确证没有 tmux 的 \
                       target 上做不到 —— 与生产里填 `hello.unavailable` 的是同一个函数（`unavailable_from`）。",
    },
    CapabilityFace {
        family: "cli-subcommands",
        kind: CapabilityKind::Protocol,
        declares: cli_subcommand_names,
        declared_in: "lib.rs",
        targets: TARGETS,
        target_basis: "`SUBCOMMANDS` 四个 target 上逐字同一张表；经 `cli_control::spec_for` 派生到 \
                       `REGISTRY` 的那几条，做不做得到跟着帧面那一条走。CLI 独有的那几条**没有任何逐 target \
                       声明**（例：`--tmux-notify` 在 Windows 上没人会调它，但声明里说不出来）⇒ 判成处处都在。",
    },
];

/// 一面在某个 target 上的**收窄** —— 这一面自己的声明里有逐 target 的信息时，从哪里读。
///
/// 没登记收窄的面 ⇒ 在它 `targets` 里的每个 target 上，整面都在。
pub struct TargetNarrowing {
    /// 哪一面（[`CAPABILITY_FACES`] 或 [`COMMAND_FACES`] 里的族名）。
    pub family: &'static str,
    /// 这一面在 `t` 上做得到的名字。**只许是 `declares()` 的子集**（判据钉）。
    pub on: fn(Target) -> Vec<&'static str>,
}

/// 🔴 **全部收窄 —— 唯一住址。**
///
/// ⚠ **`files-read` 刻意不在这里**，理由是两条，都如实写：
/// ① 那一族逐条能力的 `Capability::targets` 被它**自己的**边界②钉成了 `TARGETS` 全体
///   （`files/capability_guard.rs::the_capability_set_is_equal_across_every_target`，集合相等）⇒ 今天读它与不读它交出来的是同一份；
/// ② 读它要从这一层伸手进 `files::CAPABILITIES` —— 那是 `files/module_boundary_guard.rs`
///   登记的门之外的**第三扇门**（PR1 落地时现打：那条两向相等当场红，逐字点名 `files::CAPABILITIES`）。
///   开门要改那份门表与 `files` 那一族，不在 PR1 写区里。
/// ⇒ 代价：边界②哪天放宽（某条 files 能力不在某个 target 上），本段**看不见**那一格，
///   要同拍在 `files` 那边开一扇 `capability_names_on(t)` 的门、再在这里登记收窄。
pub const TARGET_NARROWINGS: &[TargetNarrowing] = &[
    TargetNarrowing {
        family: "ccm-launcher",
        on: ccm_launcher_on,
    },
    TargetNarrowing {
        family: "wire-commands",
        on: wire_commands_on,
    },
    TargetNarrowing {
        family: "cli-subcommands",
        on: cli_subcommands_on,
    },
];

/// 第 3 层的人群：两张面表连起来。
pub fn parity_faces() -> impl Iterator<Item = &'static CapabilityFace> {
    CAPABILITY_FACES.iter().chain(COMMAND_FACES.iter())
}

/// 🔴🔴 **在 target `t` 上做得到的 `(面, 名)`，有序** —— 全部从声明现推。
pub fn capabilities_on(t: Target) -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = parity_faces()
        .filter(|f| f.targets.contains(&t))
        .flat_map(|f| {
            let names = match TARGET_NARROWINGS.iter().find(|n| n.family == f.family) {
                Some(n) => (n.on)(t),
                None => (f.declares)(),
            };
            names.into_iter().map(move |n| (f.family, n))
        })
        .collect();
    out.sort_unstable();
    out
}

#[allow(clippy::items_after_test_module)]
#[cfg(test)]
#[path = "../../tests/backend/capability_ledger_guard.rs"]
mod capability_ledger_guard;

/// phase②（backend-08）：本 backend **会发射的帧 kind 集**（snake_case），填进 `Hello.emits`——
/// aterm 门控消费（emits 含 kind → 依赖该帧；不含 → 回退 β/watchdog）。**与 `CAPABILITIES` 正交**：
/// emits 是纯发射声明、无对应流 flag、不受 §26 护栏（见 `wire.rs` Hello.emits）。`turn_end` 待其帧
/// 发射接线（backend-08+）后加入——**在此登记 = 承诺后端真发该帧**，勿提前声明未接线的帧。
pub const EMITS: &[&str] = &[
    "line",
    "session_added",
    "session_status",
    "session_removed",
    "overflow",
    "turn_end",      // backend-09：process_jsonl 已发 TurnEnd（登记=承诺真发，已接线）
    "tmux_sessions", // B2：watch_loop 周期本地 tmux ls 发 TmuxSessions（登记=承诺真发，已接线）
    // P5：与上一份快照差分算出的**正向死亡帧**。登记 = 承诺真发（已接线，见 watcher.rs
    // 的 `diff_closed`）。monitor 收到即 retire、绕过 miss 计数；旧 monitor 忽略未知 kind。
    "tmux_session_closed",
    // 〔SR1a · `设计/05 §13.6 ③`〕账号清单变了（watcher 盯 manifest 所在目录，登记 = 承诺真发，已接线）。
    "accounts_changed",
    // 〔U4b · 第四波〕活会话清单报完了（watch_loop Phase 1 走完那一刻发一次，登记 = 承诺真发，已接线）。
    // 固定复活的 tab 靠它分「说不清」与「已结束」（`设计/30 §3.5.7a`）。
    "sessions_replayed",
    // 〔SR1a〕链路的下行字节与收尾（`dial/link.rs` 的两台泵真发，登记 = 承诺真发）。
    // 只在 monitor 开了链路之后才出现；旧 monitor / 仓外 aterm 不认这两个 kind ⇒ 忽略（additive）。
    "link_data",
    "link_end",
    // 〔SR1b〕一趟传输的进度与终局（`control/transfer.rs` 的转发任务真发，登记 = 承诺真发）。
    // 只在客户端 `transfer-start` 之后才出现；旧客户端不认 ⇒ 忽略（additive）。
    "transfer",
];

/// ① 流模式 flag：出现即剥离并置位，**不影响模式判定**。
///
/// 〔`设计/80 §8.7` 步 2，09-22〕`--with-rbind-token`：客户端**显式索要**
/// `session_added` 上的 `rbind_token`（`CCM_RBIND_TOKEN`）。默认关的理由是
/// 「令牌是敏感数据」（`§8.6 ③`），整段论证住 [`CAPABILITIES`] 的头注。
pub const STREAM_FLAGS: &[&str] = &["--with-bg", "--tail-only", "--with-rbind-token"];

/// ③ 子命令自己的选项：只在某条 [`SUBCOMMANDS`] 之后才有意义，backend 顶层不解释它们。
pub const SUBCOMMAND_OPTIONS: &[&str] = &[
    "--accts-dir",
    "--after-ms",
    // 〔SE1〕`--list-user-inputs` 的增量起点（字节偏移，传上次尾行的 `end`）。
    "--from",
    "--include-tools",
    // 〔`设计/10` 骨架 · 子步 1〕`--read-session-from-offset` 的两个选项（出骨架索引 / 右端收口）。
    // 刻意是**选项**不是新子命令：新子命令会逼出 `BUILD_ID` bump，本轮不许 —— 理由与老后端上的
    // 降级形状住 `observe::history_query::FromOffsetOpts` 的头注。
    "--index",
    "--limit",
    // 〔SE2〕`--find-in-session` 的查询串（选项值，不是位置参数：查询本身可能以 `--` 起头）。
    "--query",
    "--scope",
    "--until",
];

/// 从 argv 剥离流模式 flag，返回（剩余参数, with_bg, tail_only, with_rbind_token）。
///
/// **必须在一次性查询模式判定之前调用**（INVARIANT §26）。
///
/// ⚠ 返回值已经是**三个并列的裸布尔**〔`设计/80 §8.7` 步 2 加的第三个〕。
/// 再加第四个之前先把它们收成一个结构体 —— 位置型布尔到四个就开始靠记性调用了。
/// **本轮刻意没有顺手收**：那是一次会波及 `main.rs` 与 11 处夹具的形状变更，
/// 与「把身份从 tmux 上解绑」不是同一件活，硬塞进来只会让这一刀读不清。
pub fn split_stream_flags(mut args: Vec<String>) -> (Vec<String>, bool, bool, bool) {
    let with_bg = args.iter().any(|a| a == "--with-bg");
    let tail_only = args.iter().any(|a| a == "--tail-only");
    let with_rbind_token = args.iter().any(|a| a == "--with-rbind-token");
    args.retain(|a| !STREAM_FLAGS.contains(&a.as_str()));
    (args, with_bg, tail_only, with_rbind_token)
}

/// 剥完流 flag 之后：这些参数该进查询模式，还是该进流模式？
///
/// 返回 `true` = 一次性查询。判据是 **`args[0]` 是不是一条已登记的子命令**，
/// 不再是「非空即查询」。
///
/// # 未知 `--flag` 忽略，未知**裸参数**仍报错
///
/// 两者要分开：
/// - 未知 `--flag`：可能是**新版 monitor 发给旧版 backend** 的。忽略它 + 一行 warn，
///   照常进流模式发 hello ⇒ monitor 拿得到握手、能看出对面旧、可以降级。
///   这条不能追溯修好**已经部署**的旧后端，但它让**下一个** flag 的新增是安全的。
/// - 未知裸参数（不以 `--` 开头）：那是明确的调用错误。任何未来协议都不会把裸参数放 `args[0]`，
///   静默吞掉只会让人查半天。**仍旧落进查询分支报 `unknown argument` + exit 2。**
pub fn is_query_mode(args: &[String]) -> bool {
    match args.first() {
        None => false,
        Some(first) => {
            if SUBCOMMANDS.contains(&first.as_str()) {
                return true;
            }
            // 不是子命令：这两种都算**明确的调用错误**，交给查询分支报 unknown argument + exit 2。
            //
            // - 还有任何一个**裸参数**（不以 `--` 开头）：任何未来协议都不会这么发。
            // - `args[0]` 是个**子命令选项**（如 `--scope`）：选项脱离了它的子命令，
            //   静默当成起流会让调用方拿到一堆 jsonl 行还以为查询成功了。
            if args.iter().any(|a| !a.starts_with("--"))
                || SUBCOMMAND_OPTIONS.contains(&first.as_str())
            {
                return true;
            }
            for a in args {
                tracing::warn!(
                    "未知 flag {a}：本后端不认识它，已忽略并照常进流模式。\
                     （若这是新版 monitor 的新能力，请升级后端。）"
                );
            }
            false
        }
    }
}

#[allow(clippy::items_after_test_module)]
#[cfg(test)]
#[path = "../../tests/backend/main_fourth_face_tests.rs"]
mod fourth_face_tests;

#[allow(clippy::items_after_test_module)]
#[cfg(test)]
#[path = "../../tests/backend/main_window_raise_guard.rs"]
mod window_raise_guard;

#[allow(clippy::items_after_test_module)]
#[cfg(test)]
#[path = "../../tests/backend/main_stream_flag_tests.rs"]
mod stream_flag_tests;

/// U6b-2：**argv 三分表的完备性与互斥**。
///
/// 这是 [`SUBCOMMANDS`] 那套判据能成立的**唯一**理由：判据改成「`args[0]` ∈ SUBCOMMANDS
/// ⇒ 查询模式」之后，漏登记一条子命令的后果**不再是 exit 2**（吵，但看得见），
/// 而是那条子命令**静默变成「起了个流」** —— 调用方拿到一堆 jsonl 行而不是查询结果。
/// v3.4.0 `--account-trust-zero` 漏登记那次事故的加强版。
#[cfg(test)]
#[path = "../../tests/backend/main_argv_table_guard.rs"]
mod argv_table_guard;

use std::path::PathBuf;

// ── 〔步 9 · 09-19〕第四面：本机到底缺什么（tmux 可用性）＋ `EMITS` ───────────
// 同上一段的理由：它们不是「分派」，是 hello 帧的**内容**。in-process 那条路一样要回答
// 「这台机器有没有 tmux」「本构建会发哪些帧」。`main_fourth_face_tests` 用 `super::` 引它们。
/// `K-P4`（09-04）：命令级 code —— **「这台机器上没有 tmux」**。
///
/// ⚠ 这是这个字面量在仓里的第 N 份，但它**不是第 N 个真相源**：真相是
/// `inbound::REGISTRY` 里各条命令自己登记的 `codes`，本常量只拿它去**查那张表**。
/// 查不到就红（`the_declared_code_is_one_the_registry_already_declares`）⇒
/// 谁把那边的拼写改了，这边不会静默跟丢。
pub const NO_TMUX: &str = "no_tmux";

/// 〔FW5 · 第四波 · 2026-09-24〕命令级 code —— **「这个平台没有 unix 权限位」**。
///
/// 只有 `files-chmod` 声明它（`control/files_write.rs::change_mode` 在非 unix 上回它）。
/// 与 [`NO_TMUX`] 同形：真相是 `inbound::REGISTRY` 里命令自己登记的 `codes`，本常量只拿去查那张表；
/// target 轴（[`unix_mode_bits_on`] × 这个码）由此现推「Windows 上没有 `files-chmod`」
/// ——`设计/96 §8.5` 待拍 3 那一格（「得让它的声明带一个『这个平台没有』的码」）。
///
/// ⚠ 运行期那条轴（`hello.unavailable`，[`unavailable_here`] 今天不接线）**本刀没动**：
/// 它的判准住 [`unavailable_from`]，只看 tmux。要接那天同拍把这一维加进去。
pub const NO_UNIX_MODE: &str = "no_unix_mode";

/// `K-P4`：这台机器上**做不到**的命令 —— **纯判定那一半**（不碰世界 ⇒ 可拿合成读数驱动）。
///
/// # 这里为什么没有一张手写的「命令 → 它依赖什么」表
///
/// 那件事 `inbound::REGISTRY` 已经说过了：**谁会回 `no_tmux`，谁就依赖 tmux**。
/// 再手写一张 = 第二份真相，而两份真相里迟早有一份是旧的（本工作区最贵的那一类病）。
/// ⇒ 直接从 `codes` 派生。副作用正是要的：以后**新加**一条会回 `no_tmux` 的命令，
/// 它自动进这张表，没有人需要记得来改这里。
///
/// # `tmux` 的三态各自落在哪（🔴 这一格是本函数唯一容易假绿的地方）
///
/// · `Some(false)`（**确证没有**）⇒ 列进表；
/// · `Some(true)` ⇒ 不列；
/// · `None`（**判不出来**）⇒ **不列**。
///
/// 「不知道」有两条压法，两条都坏、但坏得不一样：压成**做不到** ⇒ 客户端灰掉按钮 ⇒
/// **能用的功能从界面上消失，而这种消失没有任何回音**（用户只会以为它不支持）；
/// 压成**做得到** ⇒ 客户端照今天的样子办（照发、点了看 `no_tmux`）⇒
/// **一个字节都没退化**，只是这一格没买到。⇒ 后者是唯一安全的那一侧。
/// 一句话：**这张表只在有把握时才开口，没把握时它退回今天的行为。**
#[allow(dead_code)] // 同 `unavailable_here`：唯一的生产调用点是它，而它今天不接线。
pub fn unavailable_from(tmux: Option<bool>) -> Vec<wire::Unavailable> {
    let mut out = Vec::new();
    if tmux == Some(false) {
        for spec in inbound::REGISTRY {
            if spec.codes.contains(&NO_TMUX) {
                out.push(wire::Unavailable {
                    command: spec.name.to_string(),
                    code: NO_TMUX.to_string(),
                });
            }
        }
    }
    out
}

/// `K-P4`：与世界打交道的那一半 —— 给定 `PATH` 的值，`tmux` 在不在它上面。
///
/// # 判准为什么正好是「`PATH` 上有没有一个可执行的 `tmux`」
///
/// 因为**真调用那一刻就是这么找的**：`control/kill.rs` `control/gate.rs` `control/launch.rs`
/// 三处都走 `Command::new("tmux")`，unix 上它是 `execvp` ⇒ 逐字就是 `PATH` 查找。
/// 事前那句话与事后那句话用**同一个判准**，两者才不会各说各话。
///
/// # `None`（判不出来）今天只剩**一个**来源
///
/// ① `PATH` 没设、或切不出任何一个非空目录 ⇒ **无处可查**，「没找到」这句话说不出口。
///
/// # 🔴 首行那个 `cfg!(unix)` 今天只说一件事（`K-P4` 下一拍拆开的就是它）
///
/// 上一版它**同时**说了两句话：「**我这个探针在非 unix 上不工作**」（**真的** ——
/// Windows 的 `CreateProcess` 还会看进程自身目录、当前目录、`System32`，并按 `PATHEXT`
/// 补后缀，真装了的那个叫 `tmux.exe`，而本扫描找的是无后缀的 `tmux`）
/// 与「**所以答案未知**」（**假的**）。后一句让这张表**在 Windows 上恒空**，
/// 而 Windows 正是这一件的动机平台 —— 原始问题在那儿一格都没被治。
///
/// 现在**平台那一维搬去了 [`TmuxPlatform`]**：非 unix 上压根走不到这个函数
/// （`tmux_present` 在 windows 那一档调的是 [`tmux_exe_in`]）。
/// ⇒ 这一行留着，只表达**它自己那一句**：「扫 `PATH` 找无后缀 `tmux`」这个判准
/// 只在 unix 上与 `execvp` 等价，别处不等价，所以它在别处**不开口**。
#[allow(dead_code)] // 同上。
pub fn tmux_in(path: Option<&std::ffi::OsStr>) -> Option<bool> {
    if !cfg!(unix) {
        return None;
    }
    let dirs: Vec<PathBuf> = std::env::split_paths(path?)
        .filter(|d| !d.as_os_str().is_empty())
        .collect();
    if dirs.is_empty() {
        return None;
    }
    Some(
        dirs.iter()
            .any(|d| plugin::discover::is_executable(&d.join("tmux"))),
    )
}

/// `K-P4`（下一拍）：**平台**这一维 —— 与「探针」那一维分开的第二个值。
///
/// # 它治的是「一个值装了两件事」
///
/// 上一拍 `tmux_in` 的首行把「**探针在这个平台上不工作**」（真）与「**答案未知**」（假）
/// 压成了同一个 `None`。于是 `unavailable_from(None)` 不列任何命令 ⇒
/// **这张表在 Windows 上恒空** ⇒「没有 tmux 却宣称我认 `kill`/`launch`」这个原始问题，
/// 在**动机平台**上一格都没治。⇒ 本枚举把那两件事拆成两个值。
///
/// # 三档，🔴 不许再合并
///
/// ⚠ 拆开的是**平台**这一维，**不是**三态处置那条规则 —— 那条（「不知道」不许压成
/// 「做不到」）在 [`unavailable_from`] 的头注里，本拍一个字没动，也不该动：
/// 它论证过「不知道」压成「做不到」会让**能用的功能从界面上无声消失**。
/// 上一拍的病不在那条规则，在**把 Windows 归进了「不知道」这一档**。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)] // 同 `unavailable_here`：生产今天不接线。
pub enum TmuxPlatform {
    /// **unix** —— 平台这一维**没有结论**，整件事交给探针（[`tmux_in`] 扫 `PATH`）。
    /// 探针的三档原样保留：找到 = `Some(true)` · 没找到 = `Some(false)` ·
    /// `PATH` 无处可查 = `None`。
    AskThePath,
    /// **windows** —— 那儿**没有原生 tmux**（它要 `fork` / pty / unix domain socket）
    /// ⇒ 默认答案是**确证的「没有」**，这一句在**编译期**就成立，不需要探针去证。
    ///
    /// 探针在这一档里只有**一个方向**的作用：把答案**抬成「有」**（有人把 MSYS2 /
    /// Cygwin 的 `tmux.exe` 放上了 `PATH` —— 那台机器上 `Command::new("tmux")` 真的能起来）；
    /// 它**永远不会**把答案压回「不知道」。
    ///
    /// ⇒ 漏看的方向也是安全的：`tmux_exe_in` 看不见的地方（进程自身目录 / 当前目录 /
    /// `System32`），我们说「没有」，而同一台机器真调用时 `CreateProcess` **也搜 `PATH`**、
    /// 一样失败、一样回 `no_tmux` ⇒ **事前那句话与事后那句话仍然是同一句**。
    AbsentUnlessExeOnPath,
    /// **既不是 unix 也不是 windows** —— 探针不适用，平台这一维**也没有结论**
    /// ⇒ 老实说「不知道」，按三态处置那条不列进表。
    /// 🔴 **这一档不许再拿来装 Windows**：那正是上一拍的病。
    NoOpinion,
}

/// `K-P4`：Windows 形状的探针 —— `PATH` 上有没有一个叫 `tmux.exe` 的普通文件。
///
/// # 为什么不复用 [`tmux_in`]
///
/// 那个找的是**无后缀**的 `tmux`，还要执行位；而 Windows 上真装了的叫 `tmux.exe`，
/// 且那儿没有执行位这个概念（`plugin::discover::is_executable` 在非 unix 上恒真）。
/// 两条都不是那边的形状 ⇒ 各写各的判准，别让一个函数装两个平台的语义。
///
/// # 它证不到什么（如实写）
///
/// `CreateProcess` 的查找面比 `PATH` 大（进程自身目录 · 当前目录 · `System32` · `Windows`），
/// 还会按 `PATHEXT` 补后缀 ⇒ **它可能漏看**。漏看之后答案是「没有」，
/// 而那正是 [`TmuxPlatform::AbsentUnlessExeOnPath`] 头注里论证过的安全方向。
#[allow(dead_code)] // 同上。
pub fn tmux_exe_in(path: Option<&std::ffi::OsStr>) -> bool {
    let Some(path) = path else {
        return false;
    };
    std::env::split_paths(path)
        .filter(|d| !d.as_os_str().is_empty())
        .any(|d| d.join("tmux.exe").is_file())
}

/// `K-P4`：两维合起来 —— **平台**（编译期定死）× **探针**（去世界上看）。
///
/// 三个入参组合的答案由 [`TmuxPlatform`] 各档头注给；这个函数本身**不碰 `cfg!`**，
/// 平台是**入参**。⇒ 本机是 Linux 也能把「Windows 那台机器」当成一个入参跑出来，
/// 而那正是 `the_windows_answer_is_confirmed_absent_not_unknown` 拿到读数的方式。
#[allow(dead_code)] // 同上。
pub fn tmux_present(platform: TmuxPlatform, path: Option<&std::ffi::OsStr>) -> Option<bool> {
    match platform {
        TmuxPlatform::AskThePath => tmux_in(path),
        TmuxPlatform::AbsentUnlessExeOnPath => Some(tmux_exe_in(path)),
        TmuxPlatform::NoOpinion => None,
    }
}

/// 这份二进制编译到哪个平台 —— **全文件唯一**一处把平台翻成值的地方。
///
/// 「唯一一处」与「windows 那一支给的是哪一档」由
/// `the_windows_arm_is_wired_into_the_source` 钉住（它读的是**磁盘上的源码文本**）。
#[allow(dead_code)] // 同上。
pub const TMUX_PLATFORM: TmuxPlatform = if cfg!(windows) {
    TmuxPlatform::AbsentUnlessExeOnPath
} else if cfg!(unix) {
    TmuxPlatform::AskThePath
} else {
    TmuxPlatform::NoOpinion
};

/// ★ 一条**只在 Windows 编译时存在**的编译期断言。
///
/// 🔴 **它在本仓门禁上给不出任何读数** —— 本机是 Linux，这个 item 在这儿
/// **编译期就不存在**。它开口的时刻是任何一次 **Windows 编译**
/// （backend「必须在 Windows 上编得过」这条纪律见 `plugin/discover.rs::is_executable` 头注）。
/// 写它的理由：Linux 那侧只有源码文本判据（读的是「那一支写在那儿」），
/// 而这一条读的是「**那一支真的被编进去了**」—— 两者证的不是同一件事。
#[cfg(windows)]
const _: () = assert!(
    matches!(TMUX_PLATFORM, TmuxPlatform::AbsentUnlessExeOnPath),
    "Windows 上平台这一维必须是「确证没有」而不是「不知道」——\
     改回 NoOpinion 会让握手帧第四条面在 Windows 上恒空，而 Windows 正是这一件的动机平台。"
);

/// `K-P4`：生产入口 —— 这一帧**真填的话**该填什么。
///
/// ⚠ 今天生产**不调它**（`build_hello` 硬写 `Vec::new()`）——与 `agents::visible_homes()`
/// 同一个口径：**能填不真填**。摘掉下面这个 `allow` 的那天，就是把 `build_hello` 那一行
/// 换成本函数的那天；要**同轮**做的三件事写在 `wire.rs` 那个字段的头注里。
///
/// 🔴 **真填那天连着要想清楚的一件事：这一趟探测的结果会被用很久。**
/// `build_hello` 在分档**之前**只调一次，那一帧随后交给两条载体；常驻那条（`listen.rs`）
/// 服务**不限次**的「只读 hello 就走」⇒ **同一帧被这个进程后续的所有连接共用**。
/// ⇒ 探测本身必须**便宜且挂不住**（所以 `tmux_in` 是纯 `stat` 扫 `PATH`，不是真 exec 一次），
/// 而消费侧必须把它当**提示**（`wire.rs` 那个字段头注的口径③）。
#[allow(dead_code)] // `K-P4`：能填不真填 —— 接线是一次纯发布决策，不是忘了。
pub fn unavailable_here() -> Vec<wire::Unavailable> {
    unavailable_from(tmux_present(
        TMUX_PLATFORM,
        std::env::var_os("PATH").as_deref(),
    ))
}
