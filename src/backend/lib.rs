//! `cc-monitor-backend` 的**库面**。
//!
//! 🔴 **它为什么存在**（前置 1、2）：目标形态是
//! 「`backend-core` 一份，两个宿主 link 同一份」—— 本机 GUI **进程内**直接跑它，
//! 远端仍走那个薄 `main` 壳。今天 link 它的只有那个壳（`main.rs`，`Cargo.toml` 的 `[lib]`）；
//! 本机 GUI 进程内 link 它那一步还没做（monitor 经子进程 / 监听口说话）。
//!
//! 模块声明、`PROTO_VERSION`、身份那一块（`BUILD_ID` ＋ 戳）、`SUBCOMMANDS` 住这里；**分派留在 `main.rs`**。
//!
//! 🔴 **`BUILD_ID` 必须住这里，不能留在 `main.rs`** —— in-process 那条路**没有
//! 那个 `main.rs`**，身份会跟着消失。读它源码的只有发版核对那一侧（字节自报 == 这里的 `BUILD_ID`）：
//!   · `.github/workflows/release.yml`（内嵌校验 · 版本对账）
//!   · `tests/scripts/re-embed.sh`（`--check`）
//!   · `tests/backend/build_id_guard.rs`（真身份住址的逐条核对）
//! monitor 不读它：monitor 的「我这一版」是手上那份内嵌字节自报的 id。**别在第四处写它的住址。**

pub mod accounts; // 账号域：上游选择（`resolve` 那张决策表 ＋ 表 ＋ 凭据 ＋ 热重载）＋ `iso`。**不是中转**，不住 relay/
#[cfg(test)]
#[path = "../../tests/backend/agent_boundary_guard.rs"]
mod agent_boundary_guard; // S1：通用层不许知道任何 agent 的名字与文件格式（整体 #[cfg(test)]）
#[cfg(test)]
#[path = "../../tests/backend/agent_locality_guard.rs"]
mod agent_locality_guard; // S2：agent 的格式知识只许住 agents/<名>/ + 通用层零个 agent 名字面量（整体 #[cfg(test)]）
pub mod agents; // S2/S3：agent 适配层——每个 agent 一份，装它专属的知识（codex + claudecode）
#[cfg(test)]
mod alloc_probe; // U-2：线程级内存量具（F22：`VmHWM` 是进程级的，会把邻居测试算进来）
pub mod assets; // 后端代管的用户资产（别名 · MCP · skill）：D 组的计算与判定，写经本进程的文件管理面
#[cfg(test)]
#[path = "../../tests/backend/build_id_guard.rs"]
mod build_id_guard; // E77：加了子命令必须 bump BUILD_ID（内部整体 #[cfg(test)]，生产构建为空）
#[cfg(test)]
#[path = "../../tests/backend/cc_bus_boundary_guard.rs"]
mod cc_bus_boundary_guard; // P4f-Y2：backend 不许碰 cc-bus 的数据布局（整体 #[cfg(test)]）
pub mod common; // U2：两边都要、又不含平台原语的纯工具（§0.5-6 打掉了「三分够用」那个判断）
pub mod control; // U3：控制面 —— 会改变世界（写盘 / 改 tmux server / 发信号），或产出改变世界的计划
pub mod dial; // K-P6b / C2 /：SSH 的一切 —— 握手 · 连接池 · 链路（**只此一处**，判据在它自己的测块）
pub mod faces; // 帧面宿主：read_face · feature_face · fork_face · resync_face（薄壳，本体在 observe/ · control/）
pub mod files; // 步 24f：`files-read` 这一族（**只读**）—— 常驻文件名索引 ＋ 四条只读能力
pub mod footprint; // 〔RM1a → MIG-3b 续〕「足迹」：帧面 `footprint-report`（申报表 ＋ 判定 ＋ 这台的 stat，出整份成品；只读）
#[cfg(test)]
mod guard_support; // U-1：各条源码扫描型守卫共用的「只留生产段」剥法（仅测试构建）
pub mod history; // 历史跨机 join 与注解（history_join · history_annotations）
#[cfg(test)]
#[path = "../../tests/backend/layering_guard.rs"]
mod layering_guard; // U3：§1.1 第二条解耦线的机器判据（observe↔control 方向与条数）
#[cfg(test)]
#[path = "../../tests/backend/no_timer_guard.rs"]
mod no_timer_guard; // P6：零定时器护栏（内部整体 #[cfg(test)]，生产构建为空）
pub mod observe; // U3：观测面 —— 读，不改变世界
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
#[cfg(test)]
#[path = "../../tests/backend/readonly_guard.rs"]
mod readonly_guard; // F08a：backend 只读机器护栏（内部整体 #[cfg(test)]，生产构建为空）
pub mod relay; // 中转的宿主：绑口 · 钥匙 · 起中转（中转本身是通信层 crate `comms_outward`）
#[cfg(test)]
#[path = "../../tests/backend/runs_guard.rs"]
mod runs_guard; // 子运行：通用层只认「运行」（扫描 ＋ 假适配层与 Claude Code 两套形状跑同一批判据）
#[cfg(test)]
#[path = "../../tests/backend/single_stream_guard.rs"]
mod single_stream_guard; // K-P1 KPY8：多客户按连接各一份 —— 「源码里恰好一份、按连接实例化」那几处的触发器（整体 #[cfg(test)]）
pub mod stderr_log; // 脱离常驻那条载体的 stderr 落进一份有上限、滚动的文件（宿主交 `CCM_BACKEND_STDERR_LOG` 才接；第四层自有状态，写口只从 main.rs 进）
pub mod stream; // 进后端的口 ① 帧面 ＋ 跨机问答原语：wire · inbound · listen · remote_ask · tap

/// Streaming wire-protocol major version, reported as `v` in the `Hello` frame.
/// Bump ONLY on a breaking wire change; additive forward-compatible frame kinds
/// (e.g. `Overflow`, #32) do NOT bump it — old parsers skip unknown kinds.
/// The monitor negotiates against its own `EXPECTED_PROTO_V` (#33).
pub const PROTO_VERSION: u32 = 1;

/// Backend build id reported in the `Hello` frame (#33 version negotiation).
/// Human-readable, monotonic build/feature tag; the monitor compares it against the id its own
/// embedded backend bytes report and warns the user when a
/// manually-deployed backend is stale (staleness 提示 + 部署确认)。
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
///   {sid,uuid}`（raw-per-record、方案 C 不 dedup；判词 `agents/claudecode/turn.rs` 对拍 aterm TurnDetector）；
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
///   ⚠ 后果不是纸面的：部署判定（当年住 monitor 的 `sftp.rs`，今天住 `control/deploy_plan.rs` 的 `identity_decision`，原共享 crate `deploy-core` 的判定那一半）判**版本那一维**的唯一判据是 build_id 字符串
///   （backend 部署路另看「落点文件在不在」；今天读那份字节自报的身份戳；stale 但文件在时仍只凭 build_id），
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
/// - p2a-rewatch-sessions：**盯着的 `sessions/` 被换掉/还没出现时会重挂**。
///   wire 一个字节没变（不 bump `PROTO_VERSION`），但**二进制行为变了** ⇒ 照上面的先例 bump。
///   ★ **必须 bump**：旧后端在这条路上是**静默失效**的（活着、不吭声、不发 `session_added`），
///   报同一个 id 就不会被判 stale、不会自动重装 —— 用户会带着一个永远不宣告会话的后端过日子。
///
/// - p2e-dial：新增 `--dial` —— 把 **backend 那条长连接流**的 SSH 握手
///   搬进一个由界面起的子进程（候选 E 的字节代理）。
///   ⚠ **必须 bump**：`--dial` 是**新的进程形态**（常驻、只有一条管子进一条管子出），
///   已部署的旧后端根本没有这条臂；而 monitor 判 stale 只看 build_id
///   ⇒ 不 bump 就不重装（p1r / p1t / G2 / p2d 那四次的同一个形状）。
///   🔴 **别把这条读成「拨号搬出去了」**：`connect_session` 的 7 处生产调用点里
///   本件只覆盖 1 处，SFTP / 端口转发 / 跳板 / 其余 exec 路径**界面仍然自己拨**。
///   ★ 与 `p2d-relay` 同一条如实登记：这一半是**源码半**，re-embed（CI 交叉编译）归发版那一拍。
///
/// - p2f-build-stamp：**这一份二进制第一次说得出自己是谁。**
///   两件事同拍落：① 下面那个 [`CC_MONITOR_BUILD_STAMP`] —— 一段**保证连续**的
///   `<<ccm-build-id:…:ccm-build-id>>`，谁拿到字节都扫得出来；
///   ② `--ccm-probe` 多吐一行 `build=<BUILD_ID>`（**能跑它的人直接问**）。
///   ⚠ **必须 bump**：在此之前，「这份二进制是谁」只能去读它**旁边**那个 `.build_id`
///   文本文件，而那个文件与二进制是两回事（`K-R68` 现打：三个载体的 `.build_id`
///   全部从同一处源码常量抠出来 ⇒ 恒等 ⇒ 一格证据都不提供）。
///   已部署的旧 backend **既没有戳、也答不出 `build=`** ⇒ 它必须被判 stale 换掉，
///   否则「问得出它是谁」这条性质在已部署的机器上永远为假。
///
/// - p2g-capture-pane：新增 `--capture-pane` —— **一条只读的一次性原语**，
///   把某个 tmux 会话此刻那一屏抓回来（`tmux -u capture-pane -p -t '=名:'`）。
///   在此之前后端会列会话、会探 `@ccm_sid`、会杀、会键入，**唯独没有「把那一屏取回来」**；
///   monitor 侧账本 `parity_ledger` 的 `tmux.manage` 那一格为此挂了一个月的欠账。
///   ⚠ **必须 bump**：这是**新增的一条子命令**，已部署的旧后端上它 `exit 2`
///   （落进 `unknown argument`），而调用方判「这台机有没有这条能力」看的是 build_id
///   ⇒ 不 bump 就不判 stale、不重装，整条能力在已部署的远端休眠
///   （p1r / p1t / G2 / p2d / p2e 那五次的同一个形状）。
///   ★ 同 `p2d` / `p2e` 那条如实登记：这一半是**源码半**，re-embed（CI 交叉编译）归发版那一拍，
///   本轮**没做**（本工作树也没铺 `src/frontend/shell/embedded-backends/`）。
///   🔴 **别把它读成「远端画面预览通了」**：本件只出后端这一侧的原语，
///   monitor 那条远端抓屏命令一个字节没动 —— 欠账换了个名字，没有被结掉。
///
/// - p2h-oneshot-session：新增 `--oneshot-session` —— **一次性会话**，
///   起一个到点**自己会死**的 tmux 会话（`new-session -d -P -F '#{session_id}'` ＋
///   一条 `setsid sh -c 'sleep N; tmux kill-session -t $N'` 的**外部**看门狗）。
///   `K-R86` 出的是「看得见」那一半（抓一屏），这一条是「有寿命」那一半 ——
///   在它之前后端建得出会话、杀得掉会话，**唯独没有「建出来的这个到点自己没」**。
///   ⚠ **必须 bump**：又一条**新增的子命令**，已部署的旧后端上它落进
///   `unknown argument` + exit 2，而调用方判「这台机有没有这条能力」看的是 build_id
///   ⇒ 不 bump 就不判 stale、不重装，整条能力在已部署的远端休眠
///   （p1r / p1t / G2 / p2d / p2e / p2g 那六次的同一个形状）。
///   ★ 同 p2d / p2e / p2g 那条如实登记：这一半是**源码半**，re-embed（CI 交叉编译）
///   归发版那一拍，本轮**没做**（本工作树也没铺 `src/frontend/shell/embedded-backends/`）。
///   🔴 **别把它读成「用量探针搬进后端了」**：本件只出后端这一侧的原语，
///   monitor 的 `account_usage` 那条 shell 串编排**一个字节没动**。
///
/// - p2i-frame-tmux-primitives：**`capture-pane` 与 `oneshot-session`
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
/// - p2j-bus-state：新增 `bus-state` —— cc-bus 的**具名读命令**，
///   总线名单 ＋ spawn 台账**一次回全**。两个命令面**同拍都动**（`SUBCOMMANDS` 26 → 27、
///   `inbound::REGISTRY` 与 `COMMANDS` 10 → 11），这是本谱系里第一次两面一起变。
///   它补的是 `K-R111` 摸底点名的那个缺口：monitor 侧 `read_cc_bus_state` 想改走后端，
///   而**后端没有对侧** —— 那条读面的头注逐字写着解锁条件是「格式契约稳下来」，
///   届时「正确形状多半不是把 shell 串搬过去，而是后端出一条**具名的读命令**」。
///   ⚠ **必须 bump**，而这一次两个失效形状**同时**成立：CLI 面那半是 p1r/p1t/G2/p2d/p2e/p2g/p2h
///   那七次的 `unknown argument` + exit 2；帧面那半是 p2i 那次的 `hello.commands` 里没有它
///   ⇒ monitor 的 `InboundClient::accepts` 判 `Unsupported`、一个字节都不发。
///   ★ 同 p2d / p2e / p2g / p2h / p2i 如实登记：这一半是**源码半**，re-embed（CI 交叉编译）
///   归发版那一拍，本轮**没做**（本工作树也没铺 `src/frontend/shell/embedded-backends/` ⇒ 不涉及 re-embed）。
///   🔴 **别把它读成「驾驶舱那条读面接上后端了」**：本件只出后端这一侧的命令，
///   monitor 的 `read_cc_bus_state` **一个字节没动**（那是下一件）。
///
/// - p2k-usage-retired〔删用量〕：**减法那一侧的第一条** —— 两条子命令与一条帧面命令
///   同拍**退役**（`SUBCOMMANDS` 27 → 25：`--usage` ＋ `--oneshot-session`；
///   `inbound::REGISTRY` 与 `COMMANDS` **11 → 10**：`oneshot-session`）。
///   起因是产品裁定：用量的**聚合轴**（后端服务端聚合 `--usage`）与**探针轴**
///   （一次性会话跑 `/usage` 抓屏）两轴整轴不做了；`oneshot-session` 这条原语当初
///   （`K-R87`）就是为探针建的，探针没了它零生产调用方 ⇒ 随之退役。
///   ⚠ **`capture-pane` 不在这一刀里**：拉屏预览真在用它（今天由界面 `src/frontend/ui/tmux-control.ts::capturePane` 经通道直接问）。
///   ⚠ **必须 bump，而这一次的理由与前九次相反**：前九次是「新能力在旧后端上休眠」，
///   这一次是**旧后端上那三条还在**，而新 monitor 不再调它们 ——
///   真正会出事的是**反向**：一台装着新后端的远端，旧 monitor 仍会去调
///   `--usage` / `ch:oneshot-session`，得到 `unknown argument` / `Unsupported`。
///   判「这台机上的后端是不是我们这一版」看的就是 build_id ⇒ 减法同样要 bump，
///   否则「它变了」这件事在协议面上无人可知。
///   ★ 同 p2d / p2e / p2g / p2h / p2i / p2j 如实登记：这一半是**源码半**，
///   re-embed（CI 交叉编译）归发版那一拍，本轮**没做**。
/// - p2l-rename-daemon-to-backend〔全仓改名一刀〕：**第一次「只换口，不换能力」** ——
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
/// - p2m-files-read-online〔步 `24f` **第二刀**〕：
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
///   ① **索引今天没有任何线上办法叫它建** —— 把节拍留在调用方，
///      而那条「重走」命令**不在** 那张四条的表里
///      ⇒ `files::index::rebuild_once` 与 `files::browse_watch::set_browsing` 至今
///      零生产调用方 ⇒ 真机上 `files-find` 恒回 `index_missing: true`。
///      〔⚠ 2026-09-21 收窄：**「零生产调用方」这半已假** —— 波 β 的 `P2` 接上了
///       `src/frontend/filewin/src/find.rs`。而「恒回 `index_missing`」这半**只在没人
///       开那个窗口去搜的时候**还成立。这一段是当时的账，留着当历史。〕
///      这是设计面的缺口（要补得先在那张表上裁第五条），**不在本刀里自己长出来**。
///   ② **消费侧还没有** —— `src/frontend/shell` 那一头一个字节没动。
///   ★ 同 p2d / p2e / p2g / p2h / p2i / p2j / p2k / p2l 如实登记：这一半是**源码半**，
///   re-embed（CI 交叉编译）归发版那一拍，本轮**没做** ——
///   本工作树没铺 `src/frontend/shell/embedded-backends/`，现打
///   `bash tests/scripts/re-embed.sh --check` 答的是「这棵树上没有一份对不上的字节」，
///   **不是**「字节是对的」（那条边界是它自己头注里逐字写的）。
///
/// - p2n-files-rebuild-and-browse：
///   **同族第五、第六条上线** —— `files-index-rebuild`（`index::rebuild_once` 的线上面）
///   与 `files-browse`（`browse_watch::set_browsing` 的线上面），两条**仍然纯读**、
///   两个命令面同拍（[`SUBCOMMANDS`] 29 → 31；`inbound::REGISTRY` 与
///   `inbound::COMMANDS` 14 → 16）。这是本谱系里第三次两面一起变（前两次是 p2j / p2m）。
///   ⚠ **必须 bump**，失效形状与 p2m 逐字相同（① 旧后端上这两个 flag 落进
///   `unknown argument` + exit 2；② 旧后端的 `hello.commands` 里没有它们 ⇒ monitor 的
///   `InboundClient::accepts` 判 `Unsupported`、一个字节都不发），两条都止于
///   「调用方判 stale 只看 build_id」。
///   🔴 **别把它读成「那个缺口填上了」** —— 补的只是**机制的线上面**：
///   节拍仍归调用方（`no_timer_guard` 那条铁律一个字没动），而「调用方到底发不发那条
///   命令」后端这棵树的判据钉不住 ⇒ 没人发的时候 `files-find` 照旧恒回 `index_missing`。
///   〔✅ 2026-09-21：**那条判据有了，住在发命令那一侧** ——
///    `filewin::find::tests` 四条两侧都钉（没索引 ⇒ 恰好 1 条重走、顺序也钉 ·
///    **阴性对照**：不过期 ⇒ 一条都不发 · 只差 `stale` 一个布尔的对照 · 连打五趟只发一趟）。
///    ⇒ 「后端这棵树钉不住」仍然成立，而**那一格换成由壳（`src/frontend/shell`）那棵树钉着**。〕
/// 上一版这里登记「`BrowseWatcher` 零生产调用方」—— 今天 `files-browse`
///   会让进程里那一个监听器跟上名单（`files::browse_watch::keep_watching`），浏览的目录此后一有动静 overlay 就重列。
///   ★ 同 p2d…p2m 如实登记：这一半是**源码半**，re-embed（CI 交叉编译）归发版那一拍，
///   本轮**没做** —— 本工作树没铺 `src/frontend/shell/embedded-backends/`，现打
///   `bash tests/scripts/re-embed.sh --check` 答的仍是「这棵树上没有一份对不上的字节」。
///
/// ★★ **欠着一笔 bump：（`rbind-token`）** —— 本轮**刻意不 bump**。
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
/// ③ 本工作树没铺 `src/frontend/shell/embedded-backends/`，`release-gate` 的 ⑬「bump 的同拍要
///    re-embed」在这里只答得出「这棵树上没有一份对不上的字节」⇒ 真 bump 也买不到那一半。
///
/// ⇒ **解锁条件（发版那一拍，同轮做完）**：步 1／3 落地、monitor 侧开始发
/// `--with-rbind-token` 之后，bump 到 `p2o-rbind-token`（或那一波的合并版本号）
/// ＋ **同拍 re-embed**。**在那之前这条能力在已部署的远端上是休眠的 —— 这是刻意的。**
///
/// ★★★ **p2o-files-write-and-rbind-token**（2026-09-24）：
/// 上面那笔「欠着的 bump」**在这一拍还掉**，与 F1 写面六条命令合成一次 ——
/// 解锁条件逐条核过：步 1（`CCM_RBIND_TOKEN` 进环境）与步 3（铸币口 ＋ 本地半）已落，
/// monitor 已在协商到 `rbind-token` 时发 `--with-rbind-token`。
/// 线上面这一拍动了：① `files-create/-mkdir/-rename/-delete/-chmod/-write-text`
/// 两个命令面各 ＋6（写面只从 `stream/inbound/` 那一扇门进，见 `files::module_boundary_guard`）；
/// ② `hello.capabilities` 的 `rbind-token` 从此有人认、有人发。
/// ★ re-embed 归发版那一拍（同 p2d…p2n 的登记）；本机 `--native` 那份由合并那一拍重打。
///
/// ★★★ **p2p-readface-outline-acctiso-spawn-504**（2026-09-24）：
/// 子命令 ＋21 —— C1 只读查询面八条上帧面（及其自动派生的 CLI 面）· SE1 `--list-user-inputs` ·
/// A3 `--acct-iso-status` / `--acct-iso-shellinit` · BS1b `bus-spawn`（两个命令面）；
/// ＋ 一处**行为**变更（R2：中转传输失败回 504 并说清卡在哪一步）。
/// ★ re-embed 归发版那一拍（同 p2d…p2o）。
///
/// ★★★ **p2q-no-ccm-self**（2026-09-24）：**子命令集一个没变，是行为变了**
/// —— 后端不再读 `CCM_SELF`，重起自己只认 `ccm::self_invocation`（CC1 那一刀之上）；远端 shim 回到一行裸 `exec`。
/// ⚠ 旧后端配新 shim ⇒ 容器路内层缺 `ccm` 那个词（入口②的老病复发）⇒ **必须**让已部署的远端被判 stale。
/// 照 p1v 的先例（只改行为/wire、不改子命令 ⇒ bump 但**不**往 `SUBCOMMAND_HISTORY` 加行）。
/// ★ re-embed 归发版那一拍。
///
/// ★★★ **p2r-apikey-naming**（2026-09-24）：**子命令集一个没变，是行为变了** ——
/// 上游选择读的凭据文件改叫 `apikey-credentials.json`、环境变量改叫 `CCM_APIKEY_CREDENTIALS` /
/// `CCM_AGENT_UPSTREAM_CLAUDE_CODE`（旧名逐条登记在 `tests/naming/account-vs-relay-naming.vitest.ts` 那张表里，
/// 这里**刻意不复写**，否则那条判据当场红）。账号和中转不混为一谈，不留兼容读旧名。
/// ⚠ 新 monitor 递新变量名、旧后端不认 ⇒ 回头读旧文件名 ⇒ 界面配好了、请求静默 404 ⇒ **必须**让已部署的后端被判 stale。
/// 线上字节不变（`wire_golden` 未动）。照 p1v 先例不往 `SUBCOMMAND_HISTORY` 加行。
///
/// ★★★ **p2s-files-copy-exit-policy**（2026-09-24）：子命令 ＋10 ——
/// F7a `files-copy` / `files-read-text` / `files-home`，B2 `exit-policy-read` / `exit-policy-set`（两个命令面）；
/// ＋ B2 行为：常驻后端最后一条流断开时现读 `backend.json` 决定退不退（读到「结束」就自己退）。
///
/// ★★★ **p2t-commit-upload-dial-v2**（2026-09-24）：子命令 ＋2（F7c `files-commit-upload`，两个命令面）
/// ＋ C2 行为：`--dial` 成为界面进程拨 SSH 的唯一代理（stream / capture / forward 三种用法、ack v2、ssh-agent 鉴权）。
///
/// ★★★ **p2u-stage-find-tasks**（2026-09-24）：子命令 ＋5 ——
/// F9c `files-stage-chunk` / `files-commit-text`（大文件分块进暂存区、后端读回拼接提交）· SE2 `find-in-session` ·
/// RM1b `tasks-list` / `plugins-marketplaces`（两个命令面都动）。
/// ＋ 行为：FW5 递归删非空目录（逐条目过围栏）与批量改权限 · F9c 超长请求行的应答从前 4 KiB 抠回 `id` ·
/// S4 冷启动首建索引的读数进 `files.index.status`。
///
/// ★★★ **p2v-resident-link**（2026-09-24）：子命令集大改 ——
/// `--dial` 删（界面进程不再起拨号代理）；入方向 ＋ `link-open` / `link-data` / `link-credit` / `link-close`（只在帧面），
/// ＋ `history-index` / `history-user-inputs` / `history-find`（骨架索引 · 大纲清单 · 会话内查找上帧面，CLI 面同名派生）。
/// 线上多三种出方向帧：`link_data` · `link_end` · `accounts_changed`。
/// ⚠ 旧本机后端不认 `link-open` ⇒ 界面判「本机后端太旧」、不回落（D11）⇒ **必须**让它被判 stale。
///
/// ★★★ **p2w-apikey-relay-footprint**（2026-09-24）：子命令 ＋5 ——
/// RM1a `apikey-key-set` / `apikey-read`（上游选择那份凭据文件：远端由那台后端读写，第四层）· `relay-status` / `relay-ensure`
/// （远端中转）· `footprint-probe`（足迹的这台机器那一半），两个命令面都动。
/// ＋ 行为：后端开 `creds-core` 的 `harden`（远端要写那份文件；「后端写不了」从编译期收窄成两条判据）。C4a 不动后端。
///
/// ★★★ **p2x-user-files-put**（2026-09-24）：子命令 ＋3 ——
/// `files-peek` / `files-put`（读改写，CAS）· `files-delete-session`（只收 sid 的会话文件围栏例外 —— 当时的说法；
/// 今天写面已无会话文件围栏，它只剩自己「只许删会话形状」那道限制），两个命令面都动。
/// ＋ 行为：后端开始写**用户**文件（别名 / `$PROFILE` / `.mcp.json` / skill `INBOX.txt` / cc-bus skill 部署），
/// 本机分叉与删历史会话改走后端 —— 旧后端不认这三条 ⇒ 这些按钮在旧后端上会明确报错，所以必须判 stale。
///
/// ★★★ **p2y-win-proc**（2026-09-24）：子命令集不变，**行为**变更 ——
/// Windows 上的判活（`pid_alive` / `proc_starttime`）与进程看守从 `unimplemented!()` / 空壳换成真实现
/// （`platform/win_proc.rs` · `pidwatch/win32.rs`）。旧后端在 Windows 本机见到第一个会话就 panic ⇒ 必须判 stale。
/// 照 p1v 先例不往 `SUBCOMMAND_HISTORY` 加行。
///
/// ★★★ **p2z-relay-in-resident**（2026-09-24）：子命令集不变，**行为**变更 ——
/// 流模式后端被交了 `CCM_RELAY_PORT` 就在本进程里起中转（中转住本机常驻后端，monitor 不再单独起它）。
/// 旧后端不开中转 ⇒ 本机 apikey 号起会话会被「中转没在跑」拒掉 ⇒ 必须判 stale。照 p1v 先例不加历史行。
///
/// ★★★ **p3a-panorama-engine**（2026-09-24）：子命令 ＋1 —— `panorama`（两个命令面）：
/// 后端经插件口起独立全景小程序 `cc-monitor-panorama`，只说查询语义；后端本体仍零 code-picture。
///
/// ★★★ **p3b-session-facts**（2026-09-24）：子命令 ＋1 —— `history-record`（`{sid}` → 记录在不在，两个命令面）。
/// ＋ 线上：`session_added` 多一个可选字段 `container`（tmux / none）· 新出方向帧 `sessions_replayed`。
///
/// ★★★ **p3c-panorama-plan**（2026-09-24）：子命令集不变，**行为**变更 ——
/// 后端 `panorama` 的 op 表 ＋7（六个 `plan_*` 只回算好的新内容、不写盘 ＋ `refresh_doc_links`〔散文墓碑〕）；
/// 旧后端不认 ⇒ 写批注会回 `unsupported` ⇒ 必须判 stale。照 p1v 先例不加历史行。
///
/// ★★★ **p3d-sftp-resident**（2026-09-24）：子命令 ＋4 —— `ch:transfer-upload` / `-download` / `-start` / `-stop`
/// （传输台搬进后端，只在帧面）。＋ 线上：链路多一种用途 `use:"files"`（sftp 子系统上的一问一答）· 新出方向帧 `transfer`。
/// 界面进程从此零 SSH：旧后端不认 `files` 用途 ⇒ 部署 / 传输全断 ⇒ 必须判 stale。
///
/// ★★★ **p3e-shapes-cas-withbg**（2026-09-25）：子命令集不变，**行为**变更 ——
/// RM1e `files-delete` 多收可选 `expect`、多回 `stale`（旧后端会忽略 expect 照删 ⇒ CAS 是空的）；
/// C4b `history-index` / `history-user-inputs` / `history-find` / `plugins-marketplaces` 四条应答形状变成后端出成品；
/// CF1 本机常驻后端起参统一加 `--with-bg`（adopt 只比 build_id ⇒ 不 bump 会接上按旧起参起的后端，bg 会话内容缺）。
/// 照 p1v 先例不加历史行。
///
/// ★★★ **p3f-mcp-sync-exitwire**（2026-09-25）：子命令 ＋1 —— AS1 `mcp-sync-plan`（两个命令面）。
/// ＋ 行为：S5 `exit-policy-*` 线上删 `shell` 字段 · `ccm` 直路给了 `--ccm-sid` 而无令牌时 stderr 说一句 · `--list-accounts` 认带 BOM 的 manifest。
///
/// ★★★ **p3g-conn-family**（2026-09-25）：子命令集不变、线上字节不变，**行为**变更 ——
/// 长流在时传输走同一身份的第二条 SSH 连接 · 被远端回拒的连接不再摘 · 在黑洞上等回话被打断时摘掉那条 ·
/// 传输用完的 sftp 会话停着复用（远端多一个空闲 sftp-server）。照 p1v 先例不加历史行。
///
/// ★★★ **p3h-accounts-product**（2026-09-25）：子命令 ＋1 —— `accounts-trust`（两个命令面）。
/// ＋ 行为：`accounts-list` 应答改成后端出成品 `{meta, accounts, notice}`、并上这台自己的 apikey 表（agent 随请求带）。
///
/// ★★★ **p3i-assets-cancel**（2026-09-25）：子命令 ＋5 —— AS2 `assets-catalog` / `assets-catalog-merge` /
/// `assets-sync` / `skill-read` / `skill-install-plan`（两个命令面）。
/// ＋ 行为：RM1f `panorama` 改异步档、`cancel` 真撤（杀子进程组）· Windows 上找全景小程序认 `.exe` 后缀。
///
/// ★★★ **p3j-history-lines**（2026-09-25）：子命令 ＋1 —— `history-lines`（按可计行号取原文，两个命令面）。
///
/// ★★★ **p3k-deploy-by-bytes**（2026-09-25）：子命令集不变，**行为**变更 ——
/// 下载失败留 `.part`（只清零字节的空 `.part`）· 上传失败先等完已发出的写再走 · 远端身份改由 monitor 读字节里的戳判（`.build_id` 退役）。
/// 照 p1v 先例不加历史行。
///
/// ★★★ **p3l-remote-ask-history**（2026-09-25）：子命令 ＋4 —— `remote-reach` · `history-annotate` ·
/// `history-forget` · `history-last-accounts`（两个命令面）。＋ 行为：`history-projects` / `history-sessions` 应答改成
/// 本机后端出的跨机 join 成品 `{rows, notice}`；注解文件的读写者换成本机后端。
///
/// ★★★ **p3m-ssh-zlib**（2026-09-25）：行为 —— russh 换成仓内打补丁的副本（`src/vendor/russh`，
/// 修 zlib 解压一包只交出约 2 倍包长的缺陷），闸 `RUSSH_ZLIB_SOUND` 开 ⇒ 判准下「远」的链路从此真走 zlib@openssh.com。
/// 子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p3n-channel-bus-skill-key**（2026-09-25）：
/// 子命令 ＋4 —— SU1 `--skill-install-record` · `--skill-installs` · `--skill-uninstall-plan`（skill 卸载，两个命令面）·
/// C4e `--bus-broadcast`（cc-bus 广播的挑人与逐个投递挪进后端，两个命令面）。
/// ＋ 行为：RK1 中转口要钥匙（`~/.cc-monitor/relay-key` 0600、跨重起不变；进门三问 403 / 421；`relay-*` 多 `not_ours`）·
/// FN1 文件管理面不再拦会话文件（无围栏，只剩删会话认会话形状）·
/// GP1 `files.stat` 回 `mode` · `history-record` 收 `configDir` · 本机凭据文件的写者换成本机常驻后端 ·
/// NT2 capture 放弃时关通道、`remote_ask` 内层随外层收 · 被交 `CCM_BACKEND_STDERR_LOG` 时 stderr 落有上限、滚动的文件 ·
/// SU1 `skill-install-plan` 带 `take` 时多答 `ledger` · CP2c 后端对外的句子经文案表（`copy-core`）出、契约错改英文诊断。
///
/// ★★★ **p3o-upstream-endpoint**（2026-09-25）：子命令 ＋2 —— `launch-endpoint` · `apikey-routing`
/// （上游选择出成品：这一发走哪、注入什么由后端答，monitor 只转交执行）。＋ 行为：`apikey-read` 应答去掉 `rows`；
/// 路由语法 / 端口 / 钥匙路径改住共享 crate `relay-route-core`；无账号的本机会话在全量注入下走 `/t/…/_/…`（开关仍默认关）。
///
/// ★★★ **p3p-drain-atomic-stop**（2026-09-25）：行为 —— 流模式三个退出口先关闸、等在跑的阻塞命令做完再退
/// （新来的阻塞命令回 `shutting_down`）· 覆盖写改「同目录临时件 → 沿用权限位 → 改名上位」原子化 · tracing 只在 stderr 是终端时上色。
/// 子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p3q-windows-probe-home**（2026-09-25）：行为 —— Windows 上 `ccm --ccm-probe` 不再自报 tmux 那一族能力
/// （`ccm_launcher_with(TMUX_PLATFORM)`，与能力账同一个内核）· `ccm` 找账号库的家目录 `HOME` 为空退 `USERPROFILE`、路径逐段 join ·
/// 远端 `uname` 回话不是 UTF-8 时说清而不照抄乱码。子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p3r-cas-digest-filegone**（2026-09-25）：行为 / 协议 —— `files-read-text` 回 `sha256`，
/// `files-write-text` / `files-commit-text` / `files-commit-upload` 必带 `expect:{sha256}`（不等 ⇒ `stale`、零写；上传暂存件不等即删）·
/// `files-delete` 多一形 `expect:{"empty_dir":true}` · 传输 done 帧带 `sha256` · 新帧 `session_file_gone` / `session_file_reread`
/// （活会话 jsonl 被删 / 截短 / 原地改写变长）。子命令没变，照 p1v 先例不加历史行；旧远端后端会被判旧、自动重装。
///
/// ★★★ **p3s-drain-deadline-viarelay**（2026-09-25）：行为 —— 退出排空加 30 s 期限（`inbound::DRAIN_DEADLINE`，
/// 后端零定时器唯一登记让位的一处；到点记哪几条没做完再退）· 覆盖写遇硬链接 / 别人属主退回就地写并说明 · 自家目录一律 `own_dir::ensure_private_dir`
/// 建成 0700（远端部署新建目录 SETSTAT 0700）· `accounts-sessions` 每行多一格 `viaRelay`。子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p3t-local-longconn-fork**（2026-09-25）：子命令 ＋1 `--session-fork`、帧命令 ＋3（`session-fork` · `acct-iso-status` ·
/// `acct-iso-shellinit`）—— 本机四处一次性 exec 改走 `<local>` 长连接、远端 acct-iso 改问那台后端。＋ 行为：`tasks-list` 应答 `{lines}` → 成品 `{tasks}`。
///
/// ★★★ **p3u-local-same-path**（2026-09-25）：行为 —— `session_added` 多一个 additive 字段 `pid`
/// （与 `rbind_token` 同闸，只有 `--with-rbind-token` 才带）· 按路径读会话的围栏也认 Codex 的记录根（根由适配层给）·
/// 本机冷读 / 搜索 / 判活从此都问本机后端（monitor 侧删内存索引、`session_map` 的 notify / `/proc` / 2 s 心跳）。子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p3v-monotonic-deploy-hostenv**（2026-09-25）：行为 / 协议 —— 部署只升不降（`sftp.rs` 的 `build_order`〔MIG-3b 起住共享 crate `deploy-core`〕，不比这一版旧就 `Keep`）·
/// `put_atomic` 临时件 / 备份件唯一名 · tmux hook 按实例占段 `[50,100)` 一格、起时摘死槽 · 后端自有状态写口跨进程锁（`platform/lock.rs::hold`）·
/// hello 多 additive `host_env`（回显宿主交来的端口 / 凭据路径 / 注解路径，token 永不回显）· `apikey-key-set` 入参 `account` → `configDir`（**不兼容**，旧远端连上即判旧重装）。
/// 子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p3w-relay-tap-stream**（2026-09-25）：行为 —— 进程内中转的 tee 落点由丢弃改交 `TapPort`（新模块 `tap.rs`，
/// 每条流一条 256 件有界通道、只 `try_send`），新帧 `wire::Frame::Tap{stream,resp,n,data|end}`（`EMITS` 加 `tap`）经会话流 `subscribe` 到前端活卡。
/// `--relay` 那一形 stdout 照旧。子命令没变，照 p1v 先例不加历史行；wire 新增帧、`PROTO_VERSION` 不动。
///
/// ★★★ **p3x-ccm-print-preview**（2026-09-26）：子命令 ＋1 `--ccm-print`、帧命令 ＋1 `ccm-print`（别名预览，与 `ccm --print`
/// 共用 `plan_of`；预览环境 = 家目录里新开终端、以 `ccm` 调起、不在 tmux、不继承账号目录）。＋ 行为：远端 `ccm` 入口改走 `read_marker` ＋ `upload_verified`（读回坏了删、下次部署补）。
///
/// ★★★ **p3y-files-inplace**（2026-09-26）：子命令 ＋1 `--files-size`、帧命令 ＋1 `files-size`。＋ 行为：复制保权限位 ·
/// `files-copy` 收 `recursive`（计划趟逐条目解析、整趟拒 / 执行趟逐条目再解析、中途失败撤回本趟所建）· 索引不跨文件系统 · `files-browse` 真挂 watcher
/// （应答多 `watching` / `watch_failed` / `watch_error`）· 暂存件跨盘提交退回「复制后删」（`EXDEV`）· 非 UTF-8 名按字节寻址。
///
/// ★★★ **p3z-fence-home-backendpath**（2026-09-26）：行为 —— 读路径围栏收成 `observe/fence.rs::Fence` 一个家（history / search 两个读者）·
/// `remote-reach` 进门判 `backend` 形状、不合形回 `bad_args`。子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p4a-free-text-gate**（2026-09-26）：行为 —— `ccm` 拒相对 / 带 `..` / 带换行的 `--cwd`，
/// 也拒带 NUL / CR / LF 的启动器 · 透传参数 · 登记备注（`plan.rs::free_text_gate` / `inherited_gate`）· `remote_ask` 拒带换行的 argv（拒绝集只收控制字符 ＋ 形式判定 ＋ 唯一 quote）。
/// 代号 3 → 4（`p3z` 之后按 `(代号, 字母)` 序是 `p4a`，只升不降那条路据此判新旧）。子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p4b-single-home-judgments**（2026-09-26）：行为 —— `ccm` 拒不合规的 sid（`resume` / `--resume` / `--ccm-sid`，
/// `shell-quote-core::session_id_ok`）· `--model`（`model_name_ok`）· `--account`（`account_name_ok`，与 cc-acct-iso 同）；配置目录走 `acct-core` 全表
/// （`config_dir_posix_ok` / `config_dir_ok`）。`resolve_query::is_valid_session_id` 冻结给仓外 aterm、没收。子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p4c-history-facts**（2026-09-26）：子命令 ＋1 `--history-facts`、帧命令 ＋1 `history-facts`（`90` 阶段 C：
/// 后端出会话事实成品 `{end, forkedFrom, touchedFiles, agents, usage}`，续传令牌 = 上一份成品原样交回、后端零状态；分叉判定抽成 `history_query::fork_origin`
/// 与历史树同一个函数）。前端 `onLine` 旁路四个记账员删到只剩真事件。
///
/// ★★★ **p4d-acct-iso-cmd**（2026-09-26）：子命令 ＋1 `--acct-iso-cmd`、帧命令 ＋1 `acct-iso-cmd`（建号命令由后端出、
/// 界面预览 / 弹终端经 `call` 问，TS 零拼 shell 串）。＋ 行为：`bus-send` / `bus-kill` / `bus-spawn` 入口先判 id（`shell_quote_core::bus_id_ok`，拒码 `bad_id`）·
/// `ccm` 新建会话名走 `gate-core::new_tmux_name_issue`（多拒欺骗字符与超过 128 的名字）· agent 工具名收进共享 crate `agent-tools-core`。
///
/// ★★★ **p4e-failure-visible**（2026-09-26）：行为 —— 备份沿用不上原文件权限位 ⇒ 删备份、整趟拒（`files_write`）·
/// 读不动的会话逐份 warn、扫完出总数（`search_query`）· 认不出的帧 / 非 UTF-8 行计数出声（`frame_tally`）· 次要动作失败留一行日志。子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p4f-stdin-line**（2026-09-26）：行为 / 协议 —— CLI 修饰 `--stdin-line` ＋ `capture.stdin`：资产目录推送的载荷改经 stdin 一行交，
/// 命令行不再带载荷（旧远端后端收到 `--stdin-line` 会一直读到 EOF、卡到调用方期限 ⇒ 必须 bump 让它被换掉）。`build_id_guard` 的指纹不数 `SUBCOMMAND_OPTIONS` ⇒ 没红，手动 bump。
///
/// ★★★ **p4g-single-home-3**（2026-09-26）：行为 —— `bus-broadcast` / `bus-send` 的 from 与收件人过 `bus_id_ok`（拒码 `bad_id`）·
/// 启动器一张白名单 `launcher_refused_char`（本机 / 远端载荷 / ccm 同一条）· base URL 写口与装表同一个谓词（新 crate `upstream-url-core`）· tmux Gate 1 并进 gate-core。
/// 子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p4h-search-index-hostkey**（2026-09-26，SX1 ＋ VIS2 合并那一拍）：行为 —— `history-search` 背后常驻内存增量索引（应答逐字节不变）·
/// watcher `agent_home` 不在先挂父目录、出现后挂它本身（`rewatch_agent_home`）· `DialAck` 多一格 `fingerprints`（additive，`ACK_V` 不动）。子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p4i-bus-mcp-tmux-reads**（2026-09-26）：子命令 ＋3 `--bus-inbox` · `--mcp-read` · `--tmux-list`（帧命令同名 ＋3）。
/// ＋ 行为：`bus-state` 读 `cc-list --tsv`（应答形状变）· `kill` 成功后按 pane pid `cc-kill` 注销 · `BUS_ID_RECIPE` 读会话名走 UTF-8 客户端（`-u`）。
///
/// ★★★ **p4j-relay-all-sid-from-claude**（2026-09-26，RELAY 合并那一拍）：行为 / 协议 —— 全量中转默认开（`CCM_RELAY_ALL_SESSIONS=0` 才关）·
/// 注入地址不带会话段，路由 `/s|t/<agent>/<账号>` 两段（`relay_route_core`）· `launch-endpoint` 入参去掉 `key` · 中转从 claude 自己的请求头取流标签
/// （头名登记在适配层 `DefaultUpstream::session_header`）。升级那一刻带旧三段地址的在飞会话会断（不留兼容）。子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p4k-remote-resident**（2026-09-26，HOST 合并那一拍）：子命令 ＋2 `--resident-ensure [--replace]` / `--resident-stop`（远端后端常驻，与本机同形）。
/// ＋ 协议：链路新用法 `tunnel`（池里那条 SSH 上开 direct-tcpip 到远端回环口）· 常驻后端多客户（每条连接各一份 watcher / inbound / writer，tap 扇出）·
/// attach 行可带 `flags` · 远端中转在远端常驻后端进程内起、远端 Tap 回 monitor。
///
/// ★★★ **p4l-ccm-shell**（2026-09-26，AL3 合并那一拍）：行为 —— ccm 是 claude 的壳，壳层选项不加前缀、其余原样交 claude；
/// `--tmux` / `--agent` 让给 claude、ccm 自己的改 `--ccm-tmux` / `--ccm-agent`；`ccm new` 保留、位置词 `attach` 交 claude、接 tmux 用 `--attach`；
/// 启动器按空格拆词 ＋ 打头 `~/` 展开；相对 `--cwd` 补绝对再过 §47；诊断口 `--ccm-print` / `--ccm-help` / `--ccm-version`；`CCM_VERSION` 6。子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p4m-tail**（2026-09-26，TAIL 合并那一拍）：`build_id_guard` 指纹补数 `SUBCOMMAND_OPTIONS`（`#options` 段，W5-AUX `--stdin-line` 那次没红的漏洞）⇒ 追加历史行。
/// ＋ 行为：常驻后端按 `creds_core::store::monitor_data_dir` 自推数据目录两格（谁起都一样，那台自己的 monitor 可收养）· 多客户后 `Adopt::Busy` 删 ·
/// 后端 Gate 1 并进 gate-core、不再拒 `=` · `http://[::1]` 不带端口按默认口读 · CP2c 续抽空 14 份后端文案。
///
/// ★★★ **p4n-backend-log**（2026-09-26，GAP1 合并那一拍）：子命令 ＋1 `--backend-log`、帧命令 ＋1 `backend-log`（远端常驻后端的 stderr 落
/// `~/.cc-monitor/logs/backend/stderr.log`，机器页「日志」经它读尾巴）。＋ 行为：`history-find` 走 SX1 常驻索引 · 账号目录不在 / 重建不再失聪 ·
/// 远端历史判活由那台后端 `--session-accounts` 答。
///
/// ★★★ **p4o-resident-stop**（2026-09-27，STOP 合并那一拍）：行为 —— `--resident-stop` 成为同机监督者（SIGTERM → 宽限期内按 pidfd 等 →
/// 到点 SIGKILL → 回 `graceful` / `killed` / `not_running`），选项 ＋1 `--grace`；本机远端同一条停法，monitor 只发一次、拿回结局；停完收掉陈 pid 记录。
///
/// ★★★ **p4p-copy-extract**（2026-09-27，COPY 合并那一拍）：行为 —— CP2c 余下 8 份的对外文字进文案表（后端 `files/mod` 等按码出话）·
/// 拨号失败按类型分阶段（kex 失败归 `other`，不再报成 hostkey）。子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p4q-hello-unavailable**（2026-09-27，NET2 合并那一拍）：协议 —— `hello.unavailable` 真填（按这台机器现算，含 unix 权限位一维 `unix_mode_unavailable`）;
/// 中转在飞上界挪到宿主 `relay/listen.rs`。子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p4r-ccm-is-backend**（2026-09-27，E2 合并那一拍）：`ccm` 就是后端本体（本机远端各一个文件 `~/.cc-monitor/bin/ccm`，shim / 副本 / `backendPath` 删）·
/// `ccm [交给 claude 的…] -- [ccm 自己的…]`（按最后一个 `--` 切，后端词只紧跟打头的 `--`；后端调自己一律带 `--`）· `--ccm-print` 归 ccm、帧命令 ＋1 `ccm-probe` ·
/// 载荷先看用户自设的 `ANTHROPIC_BASE_URL`。
///
/// ★★★ **p4s-wave-b-train**（2026-09-27，B 段合并列车）：子命令 ＋3 `--files-extract`（解压，挡 zip-slip）· `--files-read-chunk`（按字节寻址分块读）·
/// `--resync`（对齐：与起步初扫同一个 `reconcile_sessions`）。＋ 行为 / 协议：hello additive `uncancellable` · 身份标签在 pidfile 重写与 tmux 探测到达时对账 ·
/// seq 跨截断换代（`session_file_reread` ＋ `SeqCounter::restart`）· `Gap.to_seq` 可缺 · A6 退休前补读 · 注入噪声规则一份（`search-core::user_text`）·
/// 搜索索引起来就后台建、常驻 64 MiB 上界 · `--stdin-line` 扩到 argv 一族 · ack 带 `jump_fingerprints` · ccm resume 在跑就接上 · 全景 `--probe` 形状代号 · 凭据模板说明进文案表。
///
/// ★★★ **p4t-ccm-new-right**（2026-09-27，E2 续合并那一拍）：行为 —— `new` 只能写在 `--` 右边第一个（`ccm new` 整行交 claude）·
/// 路由不看 argv0（只有「打头 `--` ＋ 后端词」进后端，其余走 ccm；入口②「`<bin> ccm …`」删）。子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p4u-resync-offer**（2026-09-27，RESYNC 续合并那一拍）：子命令 −1 `--resync`（CLI 面摘，只留帧面）· `resync` 应答带当下的
/// `unavailable` / `uncancellable`（与 hello 同一个函数）· `resync{sid}` 顺手从游标补读 jsonl（tab「重新读取」）· SIGUSR1 按 `watcher::LIVE` 一张名单戳。
///
/// ★★★ **p4v-resume-running-elsewhere**（2026-09-27，FIX 续合并那一拍）：行为 —— ccm resume 时那个会话在跑但不在 ccm 起的 tmux 里 ⇒ 拒并说 pid
/// （判活与 watcher 起步初扫同一份，由 `main` 注入）。子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p4w-no-fallback**（2026-09-27，DEL 合并那一拍）：子命令 −3 `--relay` · `--relay-ensure` · `--relay-status`（帧命令 −2）· 远端只剩常驻（流模式回落删，
/// 非 unix 远端明说不支持）· 中转只住常驻后端进程内 · tee 只剩 tap 口（NDJSON 落点删）。
///
/// ★★★ **p4x-relay-state-in-process**（2026-09-27，DEL 续合并那一拍）：子命令 −2（`--apikey-routing` · `--launch-endpoint` 只留帧面）·
/// 「我们的中转在不在」读常驻后端进程内的监听状态（差分 HTTP 探针删）· 非 unix 远端归「永久不支持」、流收工不再按退避重连。
///
/// ★★★ **p4y-no-raw-keys**（2026-09-27，RST 续合并那一拍）：协议 —— 后端 `launch` 的 `send-keys-raw` mode 删（之后零生产调用者）；
/// `launch` 只剩 `create-or-attach` / `send-into` 两个 mode。子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p4z-assets-in-backend**（2026-09-27，MIG-3a 前半合并那一拍）：子命令 ＋7（MCP 增删 · MCP 同步三问 · skill 装卸），后端新模块 `assets/`；
/// `assets-sync` 多认只给 `origin` 的调用（查握手登记的可达表）。MCP 编辑 / 同步、skill 装卸的判定与写都在被写那台。
///
/// ★★★ **p5a-resync-caught-up**（2026-09-28，REREAD 续合并那一拍）：协议 —— `resync` 应答 ＋1 格 `caught_up`（本次从游标补读出的行数合计），
/// 给「重新读取」按台说补读了几条。子命令没变，照 p1v 先例不加历史行。
///
/// ★★★ **p5b-launch-in-backend**（2026-09-28，MIG-2 合并那一拍）：子命令 ＋2、帧命令 ＋3 —— 起会话渲染核心进后端 `control/launch_render/`
/// （`launch-render-cli` · `launch-render-payload` · `launch-local`）；`launch-endpoint` 答 `{baseUrl}` 或拒 `relay_down`；㊴ `exit-policy-read` / `-set` 出成品 `said`。
///
/// ★★★ **p5c-aliases-hub-in-backend**（2026-09-28，MIG-3a 后半合并那一拍）：别名整族进 `assets/aliases/`（方言按这台机器自己判）·
/// skill 接入面与 SKILLS 表进 `agents/claudecode/` · MCP 推拉 / skill 装改本机后端当枢纽（`assets/hub.rs`，写前再核来源）· cc-bus 由后端装、装卸账复用 skill 装记录。
///
/// ★★★ **p5d-deploy-plan-in-backend**（2026-09-28，MIG-3b 合并那一拍）：新帧命令 `hooks-diag` · `deploy-plan`（纯判定在共享 crate `deploy-core`）·
/// 任务推送走 `session-tasks` 流（`tasks_changed{sid}`）· 删会话 / 分叉界面直接经通道说后端（`files-delete-session` · `session-fork`）。
///
/// ★★★ **p5e-session-ledger-in-backend**（2026-09-28，MIG-1 合并那一拍）：会话 / tmux 账本进后端 `observe/session_ledger.rs`（出 `session_state` 帧；9 个会话事件并进 `session-lines`，起停格不吃 credit）·
/// `~/.ssh/config` / `ssh -G` 解读进 `dial/ssh_config.rs` · 端口转发进 `dial/forwards.rs`（流没起也按配置拨）· 测试连接 `remote-probe` · `tmux-list` 出成品 · `tmux_sessions` / `tmux_session_closed` 两帧删（aterm 不读）。
///
/// ★★★ **p5f-dial-resolve-one-home**（2026-09-28，MIG-1 收尾合并那一拍）：拨号请求只在后端 `dial/machine.rs::resolve` 组（monitor 只交原样配置）·
/// ack ＋3 格 `winner` · `strict` · `jump_strict` · `remote-probe` 只留帧面、经 `probe {ticket, cell}` 帧逐段推进度（hello `EMITS` ＋ `probe`）。
///
/// ★★★ **p5g-acct-iso-in-backend**（2026-09-28，MIG-3a 续合并那一拍）：新帧命令 `acct-iso-install`（字节随后端走，vendored 目录挪到 `src/shared/cc-acct-iso/`）· `files-link`（写面 `land_link` 的帧面入口）·
/// 枢纽远端那一跳按原码回（`stale` / `refused` …）· 文件窗口首屏由窗口进程自己问、回一行。
///
/// ★★★ **p5h-footprint-panorama-in-backend**（2026-09-28，MIG-3b 续合并那一拍）：足迹申报与判定进后端 `footprint/`（Claude 布局那部分在 `agents/claudecode/footprint.rs`），`footprint-report` 替 `footprint-probe` ·
/// `panorama-edit` 进后端（CAS 落盘）· `pubkey-push` / `authorized-keys-add` · `deploy-plan` 首连指纹交 monitor 固化 · `files` 链路 `stat` 删。
///
/// ★★★ **p5i-footprint-two-beats**（2026-09-28，MIG-3b 收口合并那一拍）：协议 —— `footprint-report` 入参 `{client?: {home, agentHome, path?}}`、应答即整份报告（`clientAsks` 删）；`HostScope::Client` 由本机后端按 monitor 交的环境自己 stat。子命令没变。
///
/// ★★★ **p5j-late-server-reconnectable**（2026-09-28，MIG-1 续四合并那一拍）：行为 —— 会话账本每一份可观测 tmux 快照都推「没报过的」可重连（后端先起、tmux server 后起不再漏报）。子命令没变。
///
/// ★★★ **p5k-records-in-backend**（2026-09-28，MOD 合并那一拍）：Claude 记录抽取进后端 `agents/claudecode/`，会话正文四条走通道、前端只收成品 · 窄探针 ＋ `end_turn` 子串闸（40 万行 1706 → 59 ms）·
/// 顶层归 `assets/` `history/` `faces/` `stream/`（`remote_ask` 进 `stream/`）· 文件模块只依 platform / common / 基础设施 · argv 只在 `main` 读一次、退出口只 `main` 与 `exit_after_drain`。
///
/// ★★★ **p5l-relay-codes-arrival**（2026-09-28，FIX3 合并那一拍）：中转状态码 —— 我们拒的 4xx（`/t/` 未登记 404）· 上游超时 504 / 其余 502 · `WriteFailed` 回 502 · 中转自答的响应带 `X-Cc-Monitor-Reason`；
/// `live` 格带 `rbind_token`（起会话「真成功」认它）· 本机 ccm 探针补 `at=`。子命令没变。
///
/// ★★★ **p5m-terminal-ssh-in-backend**（2026-09-28，FIX4 合并那一拍）：新帧命令 `terminal-ssh`（远端开终端的 ssh 外壳 / PowerShell 载荷由本机后端渲）· `tmux-name-mint`（tmux 名只在后端派生）· `panorama-uninstall`；
/// `kill` 应答 ＋ `bus: {removed, failed, unread}`。⑬「待迁」清空。
///
/// ★★★ **p5n-files-toctou**（2026-09-28，FIX5 合并那一拍）：行为 —— 文件管理写面闭 TOCTOU：不覆盖改名 `platform::fs::rename_noreplace`（Linux `renameat2`）· 复制与新建先写旁名再不覆盖上位 · 第三层开文件全程 `O_NOFOLLOW`；文案扫描补 CSS `content:` / 入口 HTML 与 R1 认形状。子命令没变。
///
/// ★★★ **p5o-files-grep-search-merge**（2026-09-28，J15 ＋ FILES3 合并那一拍）：新帧命令 `history-search-merge`（多机搜索合并排序进本机后端，`search-core::sort_by_recency`）·
/// `files-grep`（文件管理器按内容搜：不跟链接、不跨文件系统、命中文件数与读字节有上界、可撤；另有 CLI 面）· 删无调用者的 `stream_source::winner_address`。
///
/// ★★★ **p5p-probe-said-link-fallback**（2026-09-28，FIX5 续合并那一拍）：行为 / 协议 —— 盘不认 `RENAME_NOREPLACE` 时普通文件走 `link` ＋ `unlink`、目录拒并出声；
/// `remote-probe` 结局那一行改人话（版本 · 能用 / 做不到几项 · 往返毫秒）＋ `backendGaps`。子命令没变。
///
/// ★★★ **p5q-panorama-self-report**（2026-09-29，PANO 合并那一拍）：协议 —— `panorama` / `panorama-edit` 请求必带 `shape`（期望的小程序形状由发起方带，后端比 `--probe` 自报的）；
/// op 表 · 期限档 · 写表（算 op → 写成之后那一步）都由小程序自报，后端零引擎常量；`panorama-edit` 的 `op` 改成「算」op 名；op `subgraph` 删、`neighborhood` 加（跳数由小程序给，前端不算图）。形状代号换代 ⇒ 各台首次打开全景重放一次小程序字节。
///
/// ★★★ **p5r-link-fixes-win3**（2026-09-29，WF2 合并那一拍，WIN3 上一趟读数）：行为 / 协议 —— russh 副本压缩循环 CZ2（撑满就扩、吃光且冲刷完才收工：压缩链路上大块不可压数据不再断）·
/// `deploy-plan` 多 `leftovers`（部署残件每次连上照删）· 拨号 ack 多 `open_refused`（对端拒端口转发 ⇒ 停止重拨、明说）· `--list-projects` 没有记录树时出错行带码 `no_record_tree`（本机后端认它当零个项目）。子命令没变。
///
/// ★★★ **p5s-win-shell-fixes**（2026-09-29，WF1 合并那一拍，WIN3 上一趟读数）：行为 / 协议 —— PowerShell 单引号字面量只留一个出口（认全五个引号字符）· 流进 Windows 开终端那条路的远端命令不带双引号 ·
/// 执行策略挡住别名块时明说、给标准做法（新帧命令 `powershell-policy-set`，只在用户确认后执行）· 写用户 PATH 保持注册表原类型 · 后端家目录收成 `platform` 一个家 · Windows 上本机 ccm 也现测。
///
/// ★★★ **p5t-monitor-thin**（2026-09-29，THIN 合并那一拍）：monitor 里残留的共享判定全部进后端 —— 远端常驻换不换（`resident-verdict`）· 那台要哪一格字节（`deploy-slot`）· 远端旧 `~/.local/bin/ccm` 认不认得出、删不删（`deploy-retired`）· agent 卡型与判活词表进 `agents/claudecode/cards.rs`（assistant 成品带 `toolCards`，`tmux-list` 每行带 `agent`）· Gate 1 前检删、gate-core 收成后端模块 · branch-core 进 `agents/claudecode/` · acct-core 夹具挪到 feature 后面；共享 crate 只放契约，monitor 只许链契约类。
///
/// ★★★ **p5u-one-data-home**（2026-09-29，DATA-HOME 合并那一拍）：行为 —— monitor 数据目录默认 `~/.cc-monitor/`（与后端的 `bin/` 等并排；API 号凭据 `apikey-credentials.json` 在它根上，两侧经同一个 `monitor_data_dir` 推，认 `CCM_DATA_DIR`）· monitor 日志在 `logs/monitor/` · 数据目录建出来只给本人 · PowerShell 别名块模板 v5（旧块判旧、提示重装）。子命令没变。
///
/// ★★★ **p5v-plain-copy**（2026-09-29，COPY-R 合并那一拍）：文案 —— 用户看得到的文字只说现在是什么、能做什么：去掉演进叙事、内部名挪进句末括号或换成人话、删重复解释（文案表改 282 条；写进用户文件的别名块头注去掉版本沿革）· 后端 IO 错误按种类说人话（`files::io_kind_said`）。子命令没变。
///
/// ★★★ **p5w-readme-shots**（2026-09-29，SHOTS 合并那一拍）：行为 / 协议 —— 足迹行的现状多一档 `expected_absent`（旧版遗留认出就删的那一类不在 ＝ 该有的样子，由后端 `footprint/rows.rs::read_absence` 给结论）· 设置 → 机器那张表补样式 · README 配图 `docs/screenshots/`（合成数据渲染）。子命令没变。
///
/// ★★★ **p5x-win-rtt-oem**（2026-09-29，P2 合并那一拍）：行为 —— Windows 上读得到连接往返时间（`SIO_TCP_INFO`，压缩判准不变：局域网远端不再压）· PATH 状态探针直写 UTF-8 字节 · 加 / 撤 PATH 失败时报错按控制台代码页解 · `ssh_config` 走统一的家目录 · 中转删无用参数 · 门读盘那一半只从宿主调有判据。子命令没变。
///
/// ★★★ **p5y-panorama-upstream-asks**（2026-09-29，P7 合并那一拍）：协议 —— 代码全景接上游四条：邻域跳数由引擎给 · 符号 id 结构化（`SymbolRef`）· 线上类型由上游导出、前端生成 · 建索引带进度（`panorama` 请求多 `ticket`，新出方向帧 `progress{ticket, cell}`，界面显示「阶段：已做 / 总数」）；re-vendor 到上游 `7eafe64`，形状代号换代 ⇒ 各台首次打开全景重放一次小程序。子命令没变。
///
/// ★★★ **p5z-panorama-zoom-data-page**（2026-09-29，P3 合并那一拍）：行为 —— 全景图可缩放 / 拖拽、按视口适配 · 列宽变了重算行高 · 文案取文口按值在值与汉字之间补 / 去空格（C-L5，TS 与 Rust 同一套）· 数据位置页列出后端住在 `~/.cc-monitor` 里的全部东西（名字进 `relay_route_core` 契约）· CI 的 eslint 改成会拦。子命令没变。
///
/// ★★★ **p6a-terminal-prelude-home**（2026-09-29，P5 合并那一拍）：协议 / 行为 —— 开终端的令牌握手前奏整段由后端渲（新帧命令 `terminal-local`；`terminal-ssh` 多收 `rbindToken`），monitor 只开窗 · 家目录规则收成 `creds-core` 唯一一个函数、两侧共用（Windows `USERPROFILE` → `HOME`，其余 `HOME` → `USERPROFILE`）。
///
/// ★★★ **p6b-deploy-contract**（2026-09-30，P1 合并那一拍）：协议 / 行为 —— `deploy-core` 拆成契约 `deploy-contract` ＋ 后端 `control/deploy_plan.rs` 的判定；本机后端自举改问手上那份字节自己（新一次性子命令 `--place-verdict` · 帧 `place-verdict`）· `deploy-retired` 多入参形 `{text}`（monitor 不再判「是不是我们放的」）· search-core 拆进后端（`agents/claudecode/text.rs` · `observe/search_rules.rs`）· 起会话事实只剩后端一个家（`Adapter.launch`），monitor 画像代码删。
///
/// ★★★ **p6c-filewin-package**（2026-09-30，P4 合并那一拍）：结构 —— 文件窗口独立成包 `src/frontend/filewin/`（`cc-monitor-filewin`，只说 call / subscribe；开终端经通道交 monitor、命令由后端渲）· 新契约 / 宿主 crate `chan-core` · `host-core` · `filewin-contract` · 壳里平台形态全部收进 `platform/`（阶段 H 收完）· 足迹里两个写点住址跟着新住址。行为不变。
///
/// p6d-multiline-args：ccm 收的透传给 agent 的参数与登记备注可以跨行（只拒 NUL / CR），多行初始任务起得来。
///
/// p6e-accounts-native：账号库由后端直接管理（建库 · 加号 · 删号 · 设默认 · 核对 · 修复 · 隔离 · 回滚 · 别名），不再调外部工具。
///
/// p6f-ext-page：skill 与 MCP 收成一张跨机器的表，一套「装到 / 卸载」；密钥不出来源机。
///
/// p6g-child-runs：子 agent 的流归各自的运行，主 tab 上每个在跑的子运行一行；通用层只认运行，各家的形状住适配层。
///
/// p6h-runs-panel：子 agent 不进主 tab，只在 agent 面板里列；收场以派出它的那一方为准；扩展页的项目目录由它所属的那台判。
///
/// p6i-accounts-mcp：这台各账号共用一份用户级 MCP，三方对照同步，只改各号 .claude.json 的 mcpServers 那一键。
///
/// p6j-no-panorama：cc-monitor 不再带代码全景；code-picture 只作为扩展（skill ＋ MCP）。
///
/// p6k-ext-targets：扩展页装到哪由用户选、扩展可带备注、cc-bus 归扩展、有账号库时 MCP 能装到全局；机器页「工具」栏改为「终端」。
///
/// p6l-agent-strict：没写或写空用默认那一家，写错直接报错并列出认得的几家；cc-spawn 不再按启动器名猜。
///
/// p6m-project-dir：tab 标题取会话启动时的项目目录；报错不再露出内部错误码。
///
/// p6n-relay-optin：机器页「终端」栏给出让用户自己贴进 settings.json 的 env 片段，cc-monitor 只读这份文件。
///
/// p6o-accounts-home：账号库住进 ~/.cc-monitor/accounts/，后端只在这里读写；去掉另指账号库位置的环境变量与 --accts-dir。
///
/// p6p-launch-one：monitor 的每条起会话路径只交一行 ccm；环境、中转地址、身份标记由那台的 ccm 在启动那一刻定。
///
/// ★★★ **p6q-terminals-tabs**（4.0.6）：行为 / 协议 —— 「这个会话由哪个 tmux 会话在跑」只在后端判（sessions-tmux），单个与批量的停 / 起走 sessions-stop / sessions-start；远端 ↗ 改成问那台此刻连着会话的终端（session-terminals）、本机认连接的进程链（terminal-processes），删掉窗口标题与起会话令牌两套（含 terminal-local、session_added.rbind_token、能力 rbind-token）；别名读回带归组 / 账号表 / 指纹、写入要指纹、别名多 restTo、ccm 多 --cwd-if；files-find 收原样搜索词与序号、files-stat 多回 owner / link_target；开文件窗口带主题。
///
/// p6r：4.1.0 —— 信任框不再自动答、中转按目标账号；别名表单互转交后端；谁说的一处判；本机那一份按家定门牌、钥匙只交文件；运行表不挂僵尸；活卡不挤正在流的主运行；↗ 精确到窗口；文件窗口不撑破 / 不截断 / 缩放；第二个前端的契约与终端 L1 三条命令。
///
/// p6s：4.1.1 —— 起法请求、历史成品与搜索行带 agent，恢复按会话那一家；会话事实多上下文上限三项；--resident-ensure 刚起的常驻回空钥匙（每次起换新钥匙）；按 sid 找窗格（送字 · 抓屏 · 结束落在挂着它的窗格）。
///
/// p6t-quota-ledger：中转按号记下回包里的额度头（5h / 7d 用量、重置时刻、状态），账号域落 quota.json；新帧命令 quota-read、变了推 quota_changed；订阅号令牌由账号域续期。
///
/// p6u-inbound-split：帧命令表按族拆成目录（命令集合不变）；hello / resync 的 uncancellable 按字母排。
///
/// p6v-comms-crates：中转与通道各立一个 crate（comms-outward · comms-inward），行为与命令集合不变。
///
/// p6w-terminal-naming：session_added.container 改成 {host, terminal} 对象；capture-pane · tmux-list 删，tmux-name-mint → terminal-name-mint，sessions-tmux → sessions-where。
///
/// p6x-own-version：身份戳界标与能力表改从 deploy-contract 取（字节同形），命令集合不变。
///
/// p6y-child-primitive：后端起子进程收成一个原语（期限必填、杀整组、无条件清环境）；新命令级码 child_timed_out。
///
/// p6z-command-budget：每条阻塞档命令一个总期限（短于界面等待），命令集合不变。
///
/// p7a-launch-account：起会话用哪个号由会话所在那台判（follow / account_unavailable），上次用的号由那台记；失败应答可带 data。
///
/// p7b-session-restart：新帧命令 session-restart（换号重启整条在会话所在那台做完）。
///
/// p7c-fork-launch：分叉之后起的推断由后端出（session-fork 回复带 launch），sessions-start 收 fresh_terminal / fork_of；named 只剩名字。
///
/// p7d-quota-rotation：额度满了自动换号、不重启（中转 retry 口 ＋ 账号域轮换）；新帧命令 rotation-* 五条与推送 rotation_changed。
///
/// p7e-codex-who：Codex 记录「谁说的」在适配层认对；通用层按家取、不取第一家（命令集合不变）。
///
/// p7f-win-fixes：起会话叫 ccm 入口绝对路径；终端 ssh 认 cc-monitor 的主机钥匙；accounts-list meta 多 unsupported；Windows 上 ccm-print 按 PowerShell 写（命令集合不变）。
///
/// p7g-quota-show：额度显示态由后端出成品（quota-read / rotation-* 只加格），命令集合不变。
///
/// p7h-autostart：自动起算 —— 号的 5h 窗口空着时那台后端替它发一句开始计时；+ autostart-read / autostart-set（只走流）· 推送 autostart_changed。
///
/// p7i-autostart-out：自动起算挪出后端（成独立 skill quota-warm）；-autostart-read / -autostart-set · -autostart_changed。只留按工作目录藏 ~/.cc-monitor/autostart 的会话。
///
/// p7j-writers：会话事实多 writers（此刻持着这条会话的活进程）；起会话遇到已有活进程在写那条会话就拒；cc-bus 超时说实际等了多久。
///
/// p7k-atlimit：轮换多 atLimit（continue 软阈值 · stop 硬上限回那一家自己认得的用满回包）；被拒而阈值以下无号可接 ⇒ 取首个没被拒的。
///
/// p7l-interrupts：界面地基 ＋ 只读命令 session-interrupts（进行中的轮次 · 在跑的子 agent · 进行中的任务），界面「会打断什么」一问读它。
///
/// p7m-within：请求信封多 within_ms（发起方期限），阻塞档命令的总期限按它减余量装、后端只留上限。
///
/// p7n-limit-reply：硬上限的用满回包恒带 representative-claim（说不出按 T 远近）＋ retry-after = T − 此刻。
///
/// p7o-files-sort：files-find 后端排（sort · desc · scope；命中带 location · size · mtime_secs · marks）；files-ls 多 link_to · total 与三个打不开码；files-grep 每条 rel · lines。
///
/// p7p-quota-text：额度与账号界面（A7）＋ CLI 面修饰词 --text（只给 quota-read）；文件窗口删除那一问。
pub const BUILD_ID: &str = "p7p-quota-text";

// 身份戳的两个界标住契约 crate（`deploy_contract::STAMP_OPEN` / `STAMP_CLOSE`）：monitor 扫字节用的是同一份。

pub const BUILD_STAMP_LEN: usize =
    deploy_contract::STAMP_OPEN.len() + BUILD_ID.len() + deploy_contract::STAMP_CLOSE.len();

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
    let open = deploy_contract::STAMP_OPEN.as_bytes();
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
    let close = deploy_contract::STAMP_CLOSE.as_bytes();
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
/// 搜 `deploy_contract::STAMP_OPEN` 就问得出它是谁（扫法 `deploy_contract::identity_of_bytes`）。消费者：
/// monitor 构建期（内嵌字节自报的 id 就是 monitor 的「我这一版」）· monitor 运行期（推远端之前的见证）·
/// 本 crate 的部署计划（落点那一份是谁）与 `build_id_guard` 的自扫判据。
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
    // 改账号库那几条帧命令（`inbound::REGISTRY` 的 `accounts-*`，本体 `accounts/manage/`）**自动派生**的 CLI 面。
    // 登记理由同下面那几族：`is_query_mode` 那道闸门读本表，不在表里 ⇒ 当未知 flag 静默进流模式。
    // `--accounts-add` 的入参（API 号的 key 在内）**从 stdin 读**，不收 argv。⚠ 加这几行会逼出一次 `BUILD_ID` bump。
    "--accounts-add",
    "--accounts-init",
    "--accounts-isolate",
    "--accounts-login-cmd",
    // 各号共用的用户级 MCP 那三条（`accounts-mcp-*`）派生的 CLI 面；`remove` / `pick` 的入参从 stdin 读。⚠ 逼出 `BUILD_ID` bump，本路不 bump。
    "--accounts-mcp-pick",
    "--accounts-mcp-read",
    "--accounts-mcp-remove",
    "--accounts-remove",
    "--accounts-repair",
    "--accounts-rollback",
    "--accounts-set-default",
    "--accounts-verify",
    // 只读查询面那八条帧命令**自动派生**出来的 CLI 面
    //（`cli_control::cli_exposed`）。登记在这里的理由与下面那几族逐字相同 ——
    // `is_query_mode` 那道闸门读的就是本表；不在表里 ⇒ 当未知 flag ⇒ 静默进流模式。
    // ⚠ 它们与 `--list-accounts` / `--session-accounts` 是**同一个函数的两个宿主**，
    //   不是第二份实现（理由整段在 `inbound::REGISTRY` 那一段）。
    "--accounts-list",
    "--accounts-sessions",
    // 帧命令 `accounts-trust` 自动派生出来的 CLI 面（与 `--account-trust` / `--account-trust-zero`
    //   是同一个函数的两个宿主）。⚠ 逼出一次 `BUILD_ID` bump —— 本路不 bump，合并那一拍统一做。
    "--accounts-trust",
    // 帧面 `backend-log`（这台后端的 stderr 诊断文件尾部）自动派生的 CLI 面。
    //   **是新子命令** ⇒ `build_id_guard` 红是预期的，BUILD_ID 由合并那一拍统一 bump（本路不 bump）。
    "--backend-log",
    // ── P4d：控制面的 CLI 面。**它们不在这里各写一条实现** ——
    // 分派臂按 `cli_control::spec_for` 派生（见下面那条臂），实现落在 `inbound::REGISTRY`。
    // 登记在这张表里是因为 `is_query_mode` 与 `argv_table_guard` 都读它，
    // 而且 `build_id_guard` 的指纹也取自它 ⇒ 加在这里会**逼出一次 BUILD_ID bump**，那正是要的。
    // P4f：cc-bus 的两条基础命令。⚠ **不加这两行的后果是静默的** ——
    // 分派臂是派生的（认得出来），但 `is_query_mode` 这道**闸门**读的是本表：
    // 不在表里 ⇒ 当成未知 flag ⇒ 打一行 warn 之后**照常进流模式**，
    // CLI 面看上去"存在"却永远调不到（08-13 实测到了这个形状）。
    // ⇒ 现由 `cli_control::tests::every_cli_exposed_command_is_in_the_query_mode_gate` 钉住。
    // 广播（`inbound::REGISTRY` 的 `bus-broadcast`）：原是 monitor 里的组合（列名单 ＋ 逐个发），
    //   界面改经通道直接说后端之后收进后端。登记理由同下面那几条；⚠ 加这一行逼出一次 `BUILD_ID` bump
    //   （`build_id_guard`）—— 本路**不 bump**，合并那一拍统一做。
    "--bus-broadcast",
    // 帧面 `bus-inbox` 自动派生的 CLI 面（只读看收件箱尾巴）。⚠ 加这一行逼出 `BUILD_ID` bump，本路不 bump。
    "--bus-inbox",
    "--bus-kill",
    "--bus-list",
    "--bus-send",
    // 派生协作 agent（`inbound::REGISTRY` 的 `bus-spawn`）。登记理由同上面那三条。
    "--bus-spawn",
    // `K-R113`：cc-bus 的**具名读命令**（名单 ＋ spawn 台账一次回全）。
    // 登记在这里的理由与上面那三条逐字相同 —— `is_query_mode` 那道闸门读的就是本表。
    "--bus-state",
    // 部署计划（帧面 `deploy-plan` 的 CLI 面，自动派生）。
    "--deploy-plan",
    // 远端常驻后端 hello 的新旧（帧面 `resident-verdict` 的 CLI 面，自动派生）。⇒ `build_id_guard` 红是预期的（本路不 bump）。
    "--resident-verdict",
    // 那台旧入口的去向（帧面 `deploy-retired` 的 CLI 面，自动派生）。⇒ `build_id_guard` 红是预期的（本路不 bump）。
    "--deploy-retired",
    // 本机那一份放不放（帧面 `place-verdict` 的 CLI 面，自动派生；monitor 自举时跑手上那份字节问它）。⇒ `build_id_guard` 红是预期的（本路不 bump）。
    "--place-verdict",
    // 这台后端的漂移账（帧面 `drift-report` 的 CLI 面，自动派生）。⇒ `build_id_guard` 红是预期的（本路不 bump）。
    "--drift-report",
    // 公钥推送两条（帧面 `pubkey-push` / `authorized-keys-add` 的 CLI 面，自动派生；远端那台被 `remote_ask::ask_json` 走的就是后一条）。
    "--authorized-keys-add",
    "--pubkey-push",
    // 〔条 66〕「退出行为」那个值的两条命令（`inbound::REGISTRY` 的 `exit-policy-*`）自动派生的 CLI 面。
    // 登记理由与上面那几族逐字相同 —— `is_query_mode` 那道闸门读本表，不在表里 ⇒ 当未知 flag 静默进流模式。
    // ⚠ 加这两行会逼出一次 `BUILD_ID` bump（`build_id_guard`）—— 本路**不 bump**，合并那一拍统一做。
    "--exit-policy-read",
    "--quota-read",
    // 换号那一族（`inbound::REGISTRY` 的 `rotation-*`）自动派生的 CLI 面；除 `--rotation-read` 外入参从 stdin 读。⚠ 逼出 `BUILD_ID` bump，本路不 bump。
    "--rotation-read",
    "--rotation-set",
    "--rotation-session-read",
    "--rotation-session-set",
    "--rotation-switch",
    "--exit-policy-set",
    // 上游选择那份凭据文件在这台机器上的两条命令（`inbound::REGISTRY` 的 `apikey-*`）
    // 自动派生的 CLI 面。⚠ `--apikey-key-set` 的入参（含 key）**从 stdin 读**（`takes_input: true`），
    // 不收 argv —— argv 在同机任何用户的 `ps` 里都看得见。加这两行会逼出一次 `BUILD_ID` bump，本路不 bump。
    "--apikey-key-set",
    "--apikey-read",
    // `--apikey-routing` / `--launch-endpoint` 摘了（`cli_control::STREAM_ONLY`：一次性进程里没有中转，答「不在」是假话）。⚠ 逼出 `BUILD_ID` bump，本路不 bump。
    // `--relay-ensure` / `--relay-status` 随帧面那两条删了。⚠ 逼出 `BUILD_ID` bump，本路不 bump。
    // 起会话那一行 `ccm …` 的渲染（`inbound::REGISTRY` 的 `launch-render-cli`，纯函数）自动派生的 CLI 面，入参从 stdin 读。
    //   `launch-local` 不上 CLI 面（`STREAM_ONLY`）。`--launch-render-payload` 随载荷那条删了 ⇒ 逼出 `BUILD_ID` bump，本路不 bump。
    "--launch-render-cli",
    // 「足迹」出成品（`inbound::REGISTRY` 的 `footprint-report`，替掉 `--footprint-probe`）派生的 CLI 面。只读。
    "--footprint-report",
    // 帧命令 `ccm-print` 的 CLI 面删了：`--ccm-*` 这族名字归 ccm 的诊断口，二进制叫 `ccm` 时
    //   按本表分流会把 `ccm --ccm-print` 抢进后端（`cli_control::cli_exposed` 排除 ccm 的词）。逼出 `BUILD_ID` bump，本路不 bump。
    // MCP 资产同步的判定（`inbound::REGISTRY` 的 `mcp-sync-plan`）派生的 CLI 面。只读，入参从 stdin 读。
    // 加这一行会逼出一次 `BUILD_ID` bump（`build_id_guard`）—— 本路**不 bump**，合并那一拍统一做。
    // 帧面 `mcp-read` 自动派生的 CLI 面（MCP 列表成品）。⚠ 逼出 `BUILD_ID` bump，本路不 bump。
    "--mcp-read",
    "--mcp-sync-plan",
    // MCP 写两条 ＋ 推拉三条（`inbound::REGISTRY` 派生的 CLI 面）。⚠ 逼出 `BUILD_ID` bump，本路不 bump。
    "--mcp-server-put",
    "--mcp-server-remove",
    "--mcp-sync-source",
    "--mcp-sync-preview",
    "--mcp-sync-apply",
    "--cc-bus-install",
    "--files-link",
    "--cc-bus-install-state",
    "--skill-install-apply",
    "--aliases-render",
    "--aliases-read",
    "--aliases-install",
    "--aliases-block-render",
    "--aliases-block-install",
    "--aliases-block-remove",
    // 别名表单两向（`inbound::REGISTRY` 的 `aliases-to-form` / `aliases-from-form`，纯）派生的 CLI 面，入参从 stdin 读。
    // ⚠ 加这两行会逼出一次 `BUILD_ID` bump（`build_id_guard`）—— 本路**不 bump**，合并那一拍统一做。
    "--aliases-to-form",
    "--aliases-from-form",
    // 帧面 `powershell-policy-set` 派生的 CLI 面。⚠ 逼出 `BUILD_ID` bump，本路不 bump。
    "--powershell-policy-set",
    // 资产目录那两条（`inbound::REGISTRY` 的 `assets-catalog` / `assets-catalog-merge`）派生的 CLI 面。
    // 加这两行会逼出一次 `BUILD_ID` bump（`build_id_guard`）—— 本路**不 bump**，合并那一拍统一做。
    "--assets-catalog",
    "--assets-catalog-merge",
    // `assets-sync` 是真异步命令 ⇒ 按派生规则上 CLI 面（`cli_control::cli_exposed`：非内建即上）。
    // ⚠ 一次性进程没有常驻那一个的连接池与可达表：它自己新拨一条、对那一台做一趟，扇出恒为零台。
    "--assets-sync",
    // skill「装到这台」那两条（`skill-read` / `skill-install-plan`）派生的 CLI 面。只读，入参从 stdin 读。
    "--skill-read",
    "--skill-install-plan",
    // 装记录那一条（`skill-install-record`）派生的 CLI 面。
    "--skill-install-record",
    // 扩展页「从这台卸」那两条（`ext-uninstall-preview` / `ext-uninstall-apply`）派生的 CLI 面。⚠ 逼出 `BUILD_ID` bump，本路不 bump。
    "--ext-uninstall-preview",
    "--ext-uninstall-apply",
    // 扩展页写备注那一条（`ext-note-set`）派生的 CLI 面。
    "--ext-note-set",
    // 可达表登记（`inbound::REGISTRY` 的 `remote-reach`）派生的 CLI 面，入参从 stdin 读。
    // ⚠ 一次性进程的可达表随进程退出就空 —— 真正的用法是常驻后端的帧面。加这一行会逼出一次 `BUILD_ID` bump，本路**不 bump**。
    "--remote-reach",
    // 测试连接（`remote-probe`）那一行 CLI 面摘了：进度格改走本连接的应答通道（硬臂，只在帧面，理由见 `cli_control_tests::NOT_ON_CLI`）。
    // 历史注解那三条（`inbound::REGISTRY` 的 `history-annotate` / `-forget` / `-last-accounts`）派生的 CLI 面。
    // 文件住家里，一次性进程与常驻那一个按家推出同一份。加这三行会逼出一次 `BUILD_ID` bump，本路**不 bump**。
    "--history-annotate",
    "--history-forget",
    "--history-last-accounts",
    "--backend-probe",
    // `--dial`（拨号代理，`K-P6b` / C2）**从本表摘掉了**：拨号挪进本机那一个常驻后端、
    // 经流上的链路（`link-*` 四条，`dial/link.rs`）做，不再每条链路起一个进程。
    // 〔墓碑 —— 那一行原来的理由要点：「**常驻**（起来就一直搬字节，不返回），配置走**环境变量**
    //  `CCM_DIAL_REQUEST`，argv 与 stdin 都不走」。〕⚠ 摘这一行会让 `build_id_guard` 红 —— 本路**不 bump**，合并那一拍统一做。
    // 〔步 `24f` 第二刀 09-20〕`files-read` 这一族的 CLI 面。登记在这里的理由与上面
    // 那几条逐字相同 —— `is_query_mode` 那道**闸门**读的就是本表；不在表里 ⇒ 当未知 flag
    // ⇒ 打一行 warn 之后照常进流模式，调用方拿到的是一堆 jsonl 行而不是它要的应答。
    // ⚠ 线上名用 `-` 不用 `.`（能力名仍是 `files.ls` 那一套）：理由整段在
    // `files::answer_wire` 的头注 —— 一句话是 `"--files.ls"` 会被本仓两个 token
    // 取词器**静默丢弃**，那等于把这四条从三条判据底下同时抽走而三条都照常报绿。
    // 〔步 `24f` 第三刀 09-21〕同族的第五、第六条。登记在这里的理由
    // 与上面那四条逐字相同 —— `is_query_mode` 那道闸门读的就是本表。
    // 🔴 〔波 5 ㈠〕**本族第一条会往盘上写的命令。**
    //
    // 它落在这张表里**不是选择，是派生的必然**：`cli_control::cli_exposed` 逐字是
    // `!matches!(spec.run, Run::Builtin)` —— 只要一条命令进了 `inbound::REGISTRY`
    // 且不是那个硬臂，CLI 面就**自动**认得它；而本表是 `is_query_mode` 那道闸门。
    // ⇒ 不加这一行的后果与上面那几条逐字相同（当未知 flag、照常进流模式）。
    // ⚠ **如实登记一条代价**：因此「写这一面只从帧面进来」这句话**做不到**，
    //   两个宿主共用同一个 `run`（「一份代码两种宿主」正是这一形）。
    //   写面的「窄」因此**不靠命令面**，而靠 `readonly_guard` 第三层那条
    //   「只从声明过的那一面来」—— 它判的是**谁引用得到那个模块**，不是谁发得出命令。
    "--files-create",
    // 〔波 5 ㈡ 09-23〕同一面的另外五条。登记理由与上一行逐字相同。
    "--files-chmod",
    "--files-delete",
    "--files-mkdir",
    "--files-rename",
    "--files-write-text",
    // 同一面第七条（同根内复制）。登记理由与上面逐字相同。
    "--files-copy",
    // 解压。**是新子命令** ⇒ `build_id_guard` 红是预期的，BUILD_ID 由合并那一拍统一 bump。
    "--files-extract",
    // 上传的提交。登记理由同上面写面那几条（CLI 面从 `REGISTRY` 派生）。
    "--files-commit-upload",
    // 存盘装不进一行时的两步（逐块进暂存区 ＋ 读回拼起来原地覆盖）。登记理由同上。
    "--files-stage-chunk",
    "--files-commit-text",
    // 用户文件的读改写 ＋ 删历史会话。登记理由同上面写面那几条（CLI 面从 `REGISTRY` 派生）。
    "--files-delete-session",
    "--files-peek",
    "--files-put",
    "--files-browse",
    "--files-find",
    "--files-index-rebuild",
    "--files-index-status",
    "--files-ls",
    "--files-stat",
    // 同族第七、第八条。登记理由与上面那几条逐字相同。
    "--files-home",
    "--files-read-text",
    // 读族第十条（按字节寻址分块读回）。**是新子命令** ⇒ `build_id_guard` 红是预期的。
    "--files-read-chunk",
    // 读族第九条（算目录大小）。**是新子命令** ⇒ `build_id_guard` 红是预期的，BUILD_ID 由合并那一拍统一 bump。
    "--files-size",
    // 读族第十一条（按内容搜）。**是新子命令** ⇒ `build_id_guard` 红是预期的。
    "--files-grep",
    // 会话内查找（Ctrl+F）。**是新子命令** ⇒ `build_id_guard` 红是预期的，
    // BUILD_ID 由合并那一拍统一 bump（本路不 bump）。
    "--find-in-session",
    "--fork-session",
    // 帧面 `session-fork` 自动派生的 CLI 面（读 stdin）。刻意不与上一行同名：那一条是对 aterm 冻结的
    //   argv 形。⚠ 加这一行逼出一次 `BUILD_ID` bump（`build_id_guard`）—— 本路**不 bump**，合并那一拍统一做。
    "--session-fork",
    // 同上一段：`history-*` 六条帧命令的 CLI 面。
    // +2：`history-index` / `history-user-inputs`（骨架索引与大纲清单上帧面）的 CLI 面，
    //   登记理由同上 —— `is_query_mode` 那道闸门读本表。⚠ 逼出 `BUILD_ID` bump，本路不 bump。
    // `history-facts`（会话事实出成品）的 CLI 面（从 `REGISTRY` 派生，`is_query_mode` 那道闸门读本表）。
    //   **是新子命令** ⇒ `build_id_guard` 红是预期的，BUILD_ID 由合并那一拍统一 bump（本路不 bump）。
    "--history-facts",
    "--history-find",
    "--history-index",
    // `history-lines`（按行号取回）的 CLI 面。**是新子命令** ⇒ `build_id_guard` 红是预期的，
    //   BUILD_ID 由合并那一拍统一 bump（本路不 bump）。
    "--history-lines",
    // `history-page`（按字节分页出记录行）的 CLI 面。**是新子命令** ⇒ `build_id_guard` 红是预期的（本路不 bump）。
    "--history-page",
    "--history-projects",
    "--history-read",
    // `history-record` 的 CLI 面（CLI 面从 `REGISTRY` 派生，`is_query_mode` 那道闸门读本表）。
    // **是新子命令** ⇒ `build_id_guard` 红是预期的，BUILD_ID 由合并那一拍统一 bump（本路不 bump）。
    "--history-record",
    // 按运行读一个子运行的记录（替掉按目录与描述挑的那一条）。**子命令换了** ⇒ `build_id_guard` 红是预期的（本路不 bump）。
    "--history-run",
    "--history-search",
    "--history-sessions",
    "--history-tail",
    "--history-user-inputs",
    // 帧面 `hooks-diag` 自动派生的 CLI 面。⚠ 逼出 `BUILD_ID` bump，本路不 bump。
    "--hooks-diag",
    "--kill",
    "--launch",
    "--list-accounts",
    // 大纲的数据源：「你说过的话」清单。**是新子命令** ⇒
    // `build_id_guard` 红是预期的，BUILD_ID 由合并那一拍统一 bump（本路不 bump）。
    "--list-user-inputs",
    "--list-projects",
    "--list-sessions",
    "--ping",
    "--read-session",
    "--read-session-from-offset",
    "--read-session-tail",
    // `--relay`（独立的中转进程）删了：中转只住常驻后端进程里。⚠ 逼出 `BUILD_ID` bump，本路不 bump。
    // 远端常驻后端的起 · 找 · 停（`control/resident.rs`；monitor 经链路 capture 跑）。
    // ⚠ 新子命令 ⇒ `build_id_guard` 红是预期的，本路不 bump。
    "--resident-ensure",
    "--resident-stop",
    "--resolve",
    // `--resync` 摘了（`cli_control::STREAM_ONLY`：一次性进程里没有 watcher，答 `watchers: 0` 是假话）。⚠ 逼出 `BUILD_ID` bump，本路不 bump。
    "--search",
    "--session-accounts",
    "--session-interrupts",
    // `ssh-config-*` 三条帧命令自动派生的 CLI 面（理由同 `--tasks-list`）。⚠ 逼出 `BUILD_ID` bump，本路不 bump。
    "--ssh-config-aliases",
    "--ssh-config-import",
    "--ssh-config-resolve",
    // `tasks-list` 帧命令**自动派生**出来的 CLI 面（`cli_control::cli_exposed`），
    // 登记理由同 `C1` 那一段：不在表里 ⇒ `is_query_mode` 当未知 flag ⇒ 静默进流模式。
    // ⚠ 新子命令 ⇒ `build_id_guard` 红是预期的，BUILD_ID 由合并那一拍统一 bump（本路不 bump）。
    "--tasks-list",
    // 终端管理 L1 三条帧命令自动派生的 CLI 面（stdin 一段 JSON 当 `args`）。⚠ 新子命令 ⇒ `BUILD_ID` 合并那一拍统一 bump。
    "--terminal-input",
    "--terminal-preview",
    "--terminals-list",
    "--tmux-notify",
];

// ── argv 那一族 ＋ 四个贴着 `main.rs` 的测试模块 ─────────────
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
//   真正的分派（那个 `match`）仍然留在 `main.rs`，规格。

/// F66（#58③）：本构建**声明支持的能力 token**（hello 帧 `capabilities` 字段）。值住契约 crate
/// （`deploy_contract::STREAM_CAPABILITIES`），monitor 认 token 读的是同一份。
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
// ⚠ **排序照字典序**（不是按加入时间）：`capability_ledger_guard::the_stream_flag_list_keeps_its_own_narrow_semantics`
// 拿汇总那侧（排过序）与本表**逐项相等**。
pub const CAPABILITIES: &[&str] = deploy_contract::STREAM_CAPABILITIES;

// ══════════════════ 步 `8a`：能力清单的**汇总** —— 第 2 层 ══════════════════
//
// 🔴 **这一段填的是 `files/mod.rs` 头注自己登记的那个缺口**，逐字：
//   「`CAPABILITIES` 的汇总没接。第 2 层要求『能力清单从实现派生，
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
// # 这一层买到什么 · 买不到什么（那张三层表逐字）
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

/// 一个能力**属于哪一类**。那条射程要求的兑现处。
///
/// # 🔴 为什么这个枚举必须有两个成员（不是分类癖）
///
///
/// > **清单的射程要能装下「协议级能力」** —— 不只是「能管哪几类资产」。
/// > ⇒ **清单的条目 ＝「我能管哪几类资产」＋「我认不认这条协议帧」两类**，
/// > 第 2 层那条派生要把后者也派生进来；否则接上来的时候，
/// > 它要协商的东西在清单里找不到住址。
///
/// ⇒ 两个成员就是那两类。**而且两类今天都有真成员**（不是「留着以后用」）：
/// `files-read` 那六条是 [`CapabilityKind::Asset`]，`stream-flags` 那两条是
/// [`CapabilityKind::Protocol`] —— 后者正是「对面认不认这条」那一形。
/// 「两个成员各有人用」由 `capability_ledger_guard` 钉住：只剩一类时这条射程就是空话。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CapabilityKind {
    /// **我能管哪几类资产** —— 文件 / 会话 / 插件那一轴。
    Asset,
    /// **我认不认这条协议上的东西** —— 流 flag / 帧 / 命令那一轴。
    Protocol,
}

/// 编译 target —— 那条跨 target 对拍的人群。
///
/// # 🔴 它为什么住在这一层（第 2 层逐字「**每个能力面**声明自己在哪些
/// target 上有实现」）
///
/// 这个轴原先住 `files/mod.rs`（那时只有一个能力面）。汇总一接上，声明 target 的面
/// 就不止一个 ⇒ 轴留在任何**一个**面里，其余的面都得从一个兄弟那里引它。
/// ⇒ 轴住汇总这一层，`files` 再导出（`files::Target` 原样可用，那一族一个字没改）。
///
/// # 🔴 **本轴判「做得到」，不判「编得过」**
///
/// 上一版本轴判的是「编不编得过」，并逐字论证过「ccm 声明了 `tmux` 却在 Windows 上
/// 做不到」**不算假声明**，因为它编得过。**用户把这条判法推翻了。**
///
/// 他那一拍是三句，合起来是一条完整裁决：
/// ① 判「**做得到**」；② 「**除非暂时不做**」；③ 「**Windows 用 Windows 自己的后台服务，后面再做**」。
/// ③ 的**机制**那半已收回：Windows 上「会话活在前端之外」用哪种机制
/// （甲 · 控制台窗口本身就是容器 / 乙 · 常驻后端用 ConPTY 托管 / 丙 · 真 Windows 服务）都先不做、**机制未定**
/// ⇒ [`TARGET_GAPS`] 的理由串不许替用户选；①② 两句照旧。
///
/// ⇒ 于是本轴的形状变了三处：
/// 1. `targets` 那一栏的意思从「编得过」变成「**这一面在这个 target 上真的做得到**」；
/// 2. 做不到的那几条要**逐条登记**成 [`TARGET_GAPS`]，每条写明**为什么做不到**
///    与**将来怎么办**（「暂时不做」也是一种答复，但必须写出来）；
/// 3. 「编不编得过」**仍然是一条真性质**，但它不再由本轴承担 —— 它由跨 target
///    编译门禁（第 1 层）承担，而那一层**本仓今天只有两格**（两条 `-gnu`），
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

/// 一个**能力面**的登记 —— 第 2 层那条派生的**人群**。
///
/// ⚠ [`CapabilityFace::declares`] 是一个**函数指针**，不是又抄一份名单。
/// 那是本结构最要紧的一栏：它让「这一面有哪些能力」**只有一个住址**
///（那一族自己的声明表），汇总侧连一个可以漂开的副本都没有。
pub struct CapabilityFace {
    /// 这一面的族名。那种标题。
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
    /// 空着等于「抄了个 `TARGETS` 上去」，而管那叫**假声明**。
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
                        （同一个 launcher 在 `--agent claude` 那趟 `EXIT=0`）。\
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

/// 一条 [`TargetGap`] 属于哪一档 —— 「差异」要分两档登记，**不许合成一档**。
///
/// # 为什么非分不可
///
/// 两档要的**下一步动作相反**：结构上没有的那一格，正确的结局是**永远留在表里**；
/// 欠着的那一格，正确的结局是**有一天被删掉**（`P19` 删 `agent` 那一行就是这一形）。
/// 混成一档 ⇒ 读表的人分不清「这一行该不该有人去还」，而「欠着」会被读成「本来就这样」——
/// 那正是与 `no_timer_guard` 各栽过一次的静默缩水。
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
/// 本轴一旦从「编得过」改成「**做得到**」，
/// `ccm-launcher` 那一面就**不可能**再对四个 target 整面成立 —— Windows 上没有 tmux。
/// ⇒ 要么把整面从 Windows 上摘掉（**那会连 12 条真做得到的一起摘掉，是假的**），
/// 要么逐条登记豁免。**只有后者说的是真话。**
///
/// ⚠ **「暂时不做」是一种合法答复，但必须写出来** —— 用户那一拍逐字
/// 「**做得到. 除非暂时不做. windows用windows自己的后台服务. 后面在做**」。
/// ⇒ [`TargetGap::rationale`] 要同时答两件：**今天为什么做不到** ＋ **将来怎么办**。
/// 「将来怎么办」那一半**不许预设机制**：「用哪种机制」未定（见 [`Target`] 头注）。
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
    ///
    /// 字段名 `why` → `rationale`：它是**设计登记的理由散文**（只给判据读，从不上界面），
    /// 而普查的字段出口按字段名认文案（`why:` / `reason:` / `what:` …）—— 叫 `why` 就被数成了对外文案。
    /// 改名让字段说实话（第 5 类），不是给它开豁免。
    pub rationale: &'static str,
}

/// 🔴 **全部逐能力豁免 —— 唯一住址。**
///
/// **20 → 18**：帧面 `capture-pane` 与 CLI 面 `--capture-pane` × Windows 两行摘了（抓屏只走 `terminal-preview`）。
///
/// **14 → 16**：`files-chmod` × Windows（帧面 ＋ CLI 面各一行，档 = 结构），
/// 由 `no_unix_mode` 码 × [`unix_mode_bits_on`] 现推出来（表尾那一段）。
///
/// **8 → 14**：命令面并进第 3 层之后，帧面 3 条 ＋ CLI 面 3 条（`capture-pane` /
/// `kill` / `launch` × Windows）被那条横向两向相等**现推出来**、逐条登记在表尾。下面那段账说的是
/// `ccm-launcher` 那 8 条：
///
/// 那 8 条都在 `ccm-launcher` × Windows 这一格上：
/// **6 条**是读源码推出来的（tmux 那一族），**另 2 条是真机现打补的**
/// （`bus-register` / `ccm-sid`）—— 上一版的账把它们记成「做得到」，
/// 🔴 **那两格是错的，不是缺的**。逐条读数住。
///
/// 🔴 **9 → 8：`agent` 那一条删了，而它是这张表里第一条被「做掉」的。**
/// 前两次改这张表都是**往里加**（读源码推的 6 条 → 真机现打补 3 条），本轮第一次**往外减**
/// ⇒ 减法的判准与加法不同，写在那一行的墓碑里（`capability` 那一栏搜 `P19`）：
/// 加一条要的是「现打出它做不到」，**减一条要的是「那条『做不到』的根因在源码里没了」＋
/// 一句如实登记「真机那一维这次买不到」**。别把它读成「真机上复验过了」。
pub const TARGET_GAPS: &[TargetGap] = &[
    // ── `ccm-launcher` × Windows：tmux 那一族 6 条 ─────────────────
    //
    // 🔴 **将来的答复是用户给的，不是我们挑的**：Windows 上会话放后台用哪种机制（甲 · 乙 · 丙）都先不做，
    //    **机制未定**（**不是**去 Windows 上装个 tmux，也**不是**自己写一个终端复用器）。
    //    下面各行的理由只说「等 Windows 后台机制」，不替用户选机制；判据 `target_parity_guard::no_gap_rationale_picks_the_windows_mechanism`。
    TargetGap {
        family: "ccm-launcher",
        capability: "tmux",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "Windows 上没有 tmux（也不打算装）。「会话活在前端之外」在 Windows 上用哪种机制，\
              Windows 后台机制未定 ⇒ **暂时不做**；\
              将来选定机制的那一拍一并给出等价物。",
    },
    TargetGap {
        family: "ccm-launcher",
        capability: "attach",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "「接回一个还活着的会话」今天的实现是 `tmux attach`。Windows 上那条路不存在 \
              ⇒ 等 Windows 后台机制选定（未定）时一并给出等价物，**暂时不做**。",
    },
    TargetGap {
        family: "ccm-launcher",
        capability: "detach",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "「把会话留在后台」今天是 `tmux detach`。同 `attach`，等 Windows 后台机制选定，**暂时不做**。",
    },
    TargetGap {
        family: "ccm-launcher",
        capability: "tmux-size",
        target: Target::Windows,
        kind: GapKind::Structural,
        rationale: "给 tmux 那个窗格定尺寸。没有 tmux 就没有这一格；Windows 后台机制将来不论选哪种，\
              「会话的终端多大」都是另一种形状，**不照搬这一条** ⇒ 这一条本身不跨过去；\
              那边的尺寸若要有，是**另一条**能力、另立一行〔PR1 分档：结构〕。",
    },
    TargetGap {
        family: "ccm-launcher",
        capability: "tmux-base",
        target: Target::Windows,
        kind: GapKind::Structural,
        rationale: "tmux 的窗格编号基数（`base-index`）。它是 tmux 自己的配置面，\
              Windows 上连对应概念都没有 ⇒ **不是推后，是这一条本身不该跨过去**（Windows 后台机制选哪种都一样）。",
    },
    // ── `ccm-launcher` × Windows：**真机现打补上的三条** ───────
    //
    // 🔴 上面那六条是**读源码**推出来的（「Windows 上没有 tmux」）。
    //    这三条不是 —— 它们是在本机那台 Win11 虚拟机上**真跑 `ccm.exe`** 量出来的，
    //    而上一版的账把它们记成「做得到」。⇒ **那三格是错的，不是缺的。**
    //
    // ⚠ 如实写清本表买不到什么：`why` 里那几句是**读数的转述**，
    //    而「这条能力在 Windows 上做不到」这件事**本 crate 的判据买不到**
    //    （要真机）。判据能守的只有「名字真存在」「答了将来怎么办」「不是整面豁免」。
    //    真机那一维的住址是（跑法、命令、读数逐条在那儿）。
    TargetGap {
        family: "ccm-launcher",
        capability: "bus-register",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "**硬链在 tmux 上，不是自己做不到**：`argv.rs` 两道闸串着 —— \
              `--bus-register` 要 `--detach`，而 `--detach` 要 `--tmux`。\
              真机现打（Win11）把两个 bus 脚本都种齐、排掉「脚本缺失」这个变量之后，\
              仍然 `EXIT=4 no_tmux` ⇒ **唯一闸门就是 tmux**。\
              ⇒ 等 Windows 后台机制选定（未定）时，「把会话登记到总线上」要重新回答一次，\
              **暂时不做**。",
    },
    TargetGap {
        family: "ccm-launcher",
        capability: "ccm-sid",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "`--ccm-sid` 写的是 tmux 会话上的意图标（`@ccm_sid_expect`，kill / 送键的身份门据它确认事实）。\
              Windows 上没有 tmux ⇒ 没有可写的地方；直路上它不起作用，拉前终端也不靠它（点 ↗ 时现查连着的终端）。\
              等 Windows 后台机制选定（未定）时一并回答，**暂时不做**。",
    },
    // ── 🔴 〔散文墓碑〕**`agent` 那一条豁免删了，原话留在这里** ──────
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
    //   判准住 `agents/mod.rs::LaunchFace` 的 `needs_bus_id` 那一格的头注。
    TargetGap {
        family: "ccm-launcher",
        capability: "base-url-across-tmux",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "把中转地址跨 tmux 会话传下去。载体没了这一条就没了；\
              将来 Windows 后台机制选定时要自己回答「地址怎么传给它起的会话」，**暂时不做**。",
    },    // ── 命令面 × Windows：**不是新裁的，是第一次被看见** ────────────
    //
    // 这六行在 PR1 之前就是真的（`K-P4` 那一拍 `unavailable_from` 在 Windows 上就会列出这三条），
    // 只是没有任何东西把命令面放进 target 这根轴里对 —— PR1 把帧面 / CLI 面并进第 3 层的人群，
    // 那条两向相等当场点出它们，于是逐条登记。**差异是现推出来的**（命令自己声明的 `no_tmux` 码 ×
    // Windows 平台档），这里只补理由与档。
    // 档：六条都是**欠着** —— 「起会话 / 杀会话 / 看一眼画面」与 tmux 无关，是 Windows 该有的；
    // 谁来还：挂在 Windows 后台机制上（机制未定）。
    TargetGap {
        family: "wire-commands",
        capability: "kill",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "结束会话今天是起一次 `tmux kill-session`（命令自己声明了 `no_tmux` 码）。\
              Windows 上没有 tmux ⇒ 平台默认做不到。将来 Windows 后台机制选定（未定）时\
              一并管会话的生死，**暂时不做**。",
    },
    TargetGap {
        family: "wire-commands",
        capability: "launch",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "起会话今天是在 tmux 里开一个新会话（命令自己声明了 `no_tmux` 码）。\
              Windows 上没有 tmux ⇒ 平台默认做不到。「会话活在前端之外」用的 Windows 后台机制\
              未定，**暂时不做**。",
    },
    TargetGap {
        family: "wire-commands",
        capability: "terminal-preview",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "终端管理 L1 的抓一屏：这一版宿主只有 tmux（起 `tmux capture-pane`，声明了 `no_tmux`）。\
              形状与宿主无关，Windows 后台机制选定（未定）时换实现、不换形状，**暂时不做**。",
    },
    TargetGap {
        family: "wire-commands",
        capability: "terminal-input",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "终端管理 L1 的送字送键：这一版宿主只有 tmux（`send-keys`，声明了 `no_tmux`）。\
              同 `terminal-preview`：Windows 后台机制选定（未定）时换实现、不换形状，**暂时不做**。",
    },
    TargetGap {
        family: "cli-subcommands",
        capability: "--terminal-preview",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "与帧面 `terminal-preview` 那一行是同一条实现（CLI 面派生）⇒ 同一个理由（Windows 后台机制未定），同拍还，**暂时不做**。",
    },
    TargetGap {
        family: "cli-subcommands",
        capability: "--terminal-input",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "与帧面 `terminal-input` 那一行是同一条实现（CLI 面派生）⇒ 同一个理由（Windows 后台机制未定），同拍还，**暂时不做**。",
    },
    TargetGap {
        family: "cli-subcommands",
        capability: "--kill",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "与帧面 `kill` 那一行是**同一条实现**（CLI 面经 `cli_control::spec_for` \
              派生到同一条登记）⇒ 同一个理由（Windows 后台机制未定），将来与帧面那一行同拍还，**暂时不做**。",
    },
    TargetGap {
        family: "cli-subcommands",
        capability: "--launch",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "与帧面 `launch` 那一行是**同一条实现**（CLI 面经 `cli_control::spec_for` \
              派生到同一条登记）⇒ 同一个理由（Windows 后台机制未定），将来与帧面那一行同拍还，**暂时不做**。",
    },
    // ── `files-chmod` × Windows：待拍 3 ────────────
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
        rationale: "改的是 **unix 权限位**（低 12 位），命令自己声明了 `no_unix_mode` 码。Windows 上没有这套位\
              （那边是 ACL ＋ 只读属性），`change_mode` 在那里回 `no_unix_mode`、一个字节不动。\
              ⇒ 这一条本身**不该跨过去**；Windows 那边若要「改访问权限」，是另一条能力、另立一行〔FW5 分档：结构〕。",
    },
    TargetGap {
        family: "cli-subcommands",
        capability: "--files-chmod",
        target: Target::Windows,
        kind: GapKind::Structural,
        rationale: "与帧面 `files-chmod` 那一行是**同一条实现**（CLI 面经 `cli_control::spec_for` \
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

/// 🔴🔴 **汇总本体** —— 第 2 层那份「由它们汇总而来」的清单。
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

// ══════════════ PR1：第 3 层的**人群** —— 每个 target 上做得到的那一份 ══════════════
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
//   而那句「没有字节的 target，第 3 层是在对空集断言」对本段**如实成立**：
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

/// 编译 target → 那个平台**有没有 unix 权限位**（`files-chmod` 那一格的载体）。
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
/// 声明了 [`NO_UNIX_MODE`] 且那个 target 没有 unix 权限位的）。
///
/// tmux 那一维读的就是生产里填 `hello.unavailable` 的那一个函数（[`unavailable_from`]），不另写判准；
/// unix 权限位那一维同形：**谁声明会回那个码，谁就依赖那个机制**，从 `codes` 现推，不抄名单。
fn wire_commands_unavailable_on(t: Target) -> Vec<String> {
    unavailable_from(tmux_by_platform(t))
        .into_iter()
        .chain(unix_mode_unavailable(unix_mode_bits_on(t)))
        .map(|u| u.command)
        .collect()
}

/// `ccm-launcher`：载体是 tmux 的那几条，在平台确证没有 tmux 的 target 上摘掉。
///
/// 「哪几条载体是 tmux」住 `control::ccm::CCM_TMUX_CARRIED`（紧挨着那一面的 `CAPABILITIES`，
/// 一条能力一个住址）；这里只做「× 平台档」那一步。
fn ccm_launcher_on(t: Target) -> Vec<&'static str> {
    ccm_launcher_with(tmux_platform_of(t))
}

/// 上面那一步的内核：平台档是**入参**（同 [`tmux_present`] 的做法），
/// 于是「能力账」（按 [`Target`] 问）与「`ccm --ccm-probe` 自报」（按本二进制的 [`TMUX_PLATFORM`] 问）
/// 走的是**同一个函数**，不另写名单。
pub(crate) fn ccm_launcher_with(p: TmuxPlatform) -> Vec<&'static str> {
    let no_tmux = tmux_present(p, None) == Some(false);
    control::ccm::CAPABILITIES
        .iter()
        .copied()
        .filter(|c| !(no_tmux && control::ccm::CCM_TMUX_CARRIED.contains(c)))
        .collect()
}

/// 帧面命令：`inbound::REGISTRY` 减去这个 target 上平台默认做不到的那几条。
fn wire_commands_on(t: Target) -> Vec<&'static str> {
    let gone = wire_commands_unavailable_on(t);
    stream::inbound::REGISTRY
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
    stream::inbound::REGISTRY.iter().map(|s| s.name).collect()
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
        declared_in: "stream/inbound/mod.rs",
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
    // 会话账本的成品（可重连 / 已结束，`observe::session_ledger` 经 watcher 的 sink 真发，登记 = 承诺真发）。
    // 旧 monitor / 仓外 aterm 不认 ⇒ 忽略（additive）。⚠ hello 字节变了 ⇒ 合并那一拍 bump `BUILD_ID`。
    "session_state",
    "overflow",
    "turn_end", // backend-09：process_jsonl 已发 TurnEnd（登记=承诺真发，已接线）
    // `tmux_sessions` / `tmux_session_closed` 两格删了：tmux 快照只喂这台的会话账本、不上线（`wire.rs` 那一处墓碑）。
    // 账号清单变了（watcher 盯 manifest 所在目录，登记 = 承诺真发，已接线）。
    "accounts_changed",
    // 这台的额度账显示得出来的那几格变了（中转记账那一路真发，走 tap 那条可丢的通道；登记 = 承诺真发）。
    "quota_changed",
    // 某个会话的轮换 / 「账号」格变了（换号那一路与帧面改轮换那一路真发，走 tap 那条可丢的通道；登记 = 承诺真发）。
    "rotation_changed",
    // 某个会话的任务清单变了（watcher 盯 `<agent 家>/tasks/`，登记 = 承诺真发，已接线）。
    "tasks_changed",
    // 活会话清单报完了（watch_loop Phase 1 走完那一刻发一次，登记 = 承诺真发，已接线）。
    // 固定复活的 tab 靠它分「说不清」与「已结束」。
    "sessions_replayed",
    // 活会话的记录文件不见了 / 被改过已从头重读（`process_jsonl` 真发，登记 = 承诺真发，已接线）。
    // 旧 monitor / 仓外 aterm 不认 ⇒ 忽略（additive）。
    "session_file_gone",
    "session_file_reread",
    // 链路的下行字节与收尾（`dial/link.rs` 的两台泵真发，登记 = 承诺真发）。
    // 只在 monitor 开了链路之后才出现；旧 monitor / 仓外 aterm 不认这两个 kind ⇒ 忽略（additive）。
    "link_data",
    "link_end",
    // 一趟传输的进度与终局（`control/transfer.rs` 的转发任务真发，登记 = 承诺真发）。
    // 只在客户端 `transfer-start` 之后才出现；旧客户端不认 ⇒ 忽略（additive）。
    "transfer",
    // 测试连接那一趟的进度与结局（`dial/probe.rs` 真发，登记 = 承诺真发）。只在 `remote-probe` 在跑时出现；
    // 旧客户端不认 ⇒ 忽略（additive）。⚠ hello 字节变了 ⇒ 合并那一拍 bump `BUILD_ID`。
    "probe",
    // 中转抄出来的 SSE 事件（`tap::attach` 的接收端经 `writer_task` 真发，登记 = 承诺真发）。
    // 只有进程里住着中转的那个后端（本机常驻）才会有；旧客户端不认 ⇒ 忽略（additive）。
    "tap",
    // 一个会话的运行表（watcher 读子运行记录、表变了真发，登记 = 承诺真发）。⚠ hello 字节变了 ⇒ 合并那一拍 bump `BUILD_ID`。
    "session_runs",
];

/// `--stream`：「我是流模式后端」的**显式词**。二进制叫 `ccm` 时零参数是「起会话」，
/// 起流那几发（monitor 远端流 / 测试连接探针 / 本机宿主 / 远端常驻子进程）一律带它打头，由 `control::ccm::intercept` 按本表分流。
/// 它不对应任何能力（老后端不认 ⇒ 按未知旗标照常进流模式，同 U6b-2 的降级）。
pub const STREAM_FLAG_EXPLICIT: &str = "--stream";

/// ① 流模式 flag：出现即剥离并置位，**不影响模式判定**。
///
/// `--with-pid`：客户端显式索要 `session_added` 上的 `pid`（本机 ↗ 按它找父 PowerShell）。
/// 只有本机那条流发它（本机后端与 monitor 同一份构建），不对应能力 token；默认关 ⇒ 别的客户端收到的字节不变。
/// `--with-raw`：客户端显式索要 `line` 上的 `raw`（那一行记录的原文）。第二个前端自己解析记录、要它；
/// 同 `--with-pid` 不对应能力 token（老后端把它当未知旗标忽略、照常起流）；默认关 ⇒ 没索要的客户端字节不变。
pub const STREAM_FLAGS: &[&str] = &[
    STREAM_FLAG_EXPLICIT,
    "--with-bg",
    "--tail-only",
    "--with-pid",
    "--with-raw",
];

/// 一条流的客户端索要了什么（流模式旗标剥出来的那几位）。全关 ＝ 默认：没索要的客户端收到的字节不变。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StreamWants {
    /// `--with-bg`：放行 bg 会话。
    pub with_bg: bool,
    /// `--tail-only`：不重放历史。
    pub tail_only: bool,
    /// `--with-pid`：`session_added` 带 `pid`。
    pub with_pid: bool,
    /// `--with-raw`：`line` 带 `raw`。
    pub with_raw: bool,
}

/// **「只读一行 stdin」的入口**：跟在子命令后面（`--assets-catalog-merge --stdin-line`）。
///
/// 为什么要它：远端那一跳（`remote_ask::DialRemote`，capture）**不关远端的 stdin** ⇒ 默认那种「读到 EOF」会一直等；
/// 此前的出路是把载荷拼进命令行 `printf '%s\n' '<json>' | …`，那要求远端登录 shell 认 POSIX 单引号与管道 ——
/// fish 一类不认（`'…\\…'` 在 fish 的单引号里会被当转义吃掉一个反斜杠，JSON 就坏了），而且一趟受 `sh -c` 那一个参数的上限。
/// 有了它，命令行里只剩后端路径与两个旗标（不含载荷），载荷经 capture 写进远端进程的 stdin，本入口读到换行就动手。
/// 上限同默认那一形（`control/cli_control.rs::MAX_CLI_STDIN`，超了拒、不截断）。
///
/// 住这里（argv 三分表旁边）而不住 `cli_control`：它是 [`SUBCOMMAND_OPTIONS`] 的一员；发它的一方（`asset_sync`）
/// 只该认得这个字面量，不该因此在引用图上连到 CLI 面的分派口（`target_parity_guard` 那条「够不够得着 tmux」按文件级引用图走）。
pub const STDIN_LINE_FLAG: &str = "--stdin-line";

/// **「给人看」那一形**：跟在 `--quota-read` 后面（`--quota-read --text`）⇒ 同一份回包排成每号一段的字（`control/quota_text.rs`）。
/// 只给这一条；别的子命令带它 ⇒ `bad_args`。缺省仍是 JSON 进 JSON 出。住这里同 [`STDIN_LINE_FLAG`]：它是 [`SUBCOMMAND_OPTIONS`] 的一员。
pub const TEXT_FLAG: &str = "--text";

/// 帧命令名 → 它的 CLI 子命令（`launch` → `--launch`）。**唯一一处拼法**：本进程的 CLI 面（`control/cli_control.rs::flag_of`）
/// 与问远端那台 CLI 面的那一跳（`remote_ask::ask_json`）都经它 —— 住这里而不住 `cli_control`，是为了让
/// `remote_ask` 不必引 `control/`（引了，按文件画的引用图就把问远端的几条命令连到 tmux 上）。
pub fn cli_flag(name: &str) -> String {
    format!("--{name}")
}

/// ③ 子命令自己的选项：只在某条 [`SUBCOMMANDS`] 之后才有意义，backend 顶层不解释它们。
pub const SUBCOMMAND_OPTIONS: &[&str] = &[
    "--after-ms",
    // `--list-user-inputs` 的增量起点（字节偏移，传上次尾行的 `end`）。
    "--from",
    // `--resident-stop` 的宽限期（秒；必须大于退出排空上限）。
    "--grace",
    "--include-tools",
    // 〔骨架〕`--read-session-from-offset` 的两个选项（出骨架索引 / 右端收口）。
    // 刻意是**选项**不是新子命令：新子命令会逼出 `BUILD_ID` bump，本轮不许 —— 理由与老后端上的
    // 降级形状住 `observe::history_query::FromOffsetOpts` 的头注。
    "--index",
    "--limit",
    // `--find-in-session` 的查询串（选项值，不是位置参数：查询本身可能以 `--` 起头）。
    "--query",
    // `--resident-ensure` 的「先停口上那一位再起」（只升不降由 monitor 按 hello 判）。
    "--replace",
    "--scope",
    // CLI 控制面那一族（`--<帧命令>`）的「只读一行 stdin」修饰词。
    STDIN_LINE_FLAG,
    // `--quota-read` 的「给人看」那一形（只给这一条）。
    TEXT_FLAG,
    "--until",
];

/// 从 argv 剥离流模式 flag，返回（剩余参数, 这条流索要了什么）。
///
/// **必须在一次性查询模式判定之前调用**（INVARIANT §26）。
pub fn split_stream_flags(mut args: Vec<String>) -> (Vec<String>, StreamWants) {
    let has = |f: &str| args.iter().any(|a| a == f);
    let wants = StreamWants {
        with_bg: has("--with-bg"),
        tail_only: has("--tail-only"),
        with_pid: has("--with-pid"),
        with_raw: has("--with-raw"),
    };
    args.retain(|a| !STREAM_FLAGS.contains(&a.as_str()));
    (args, wants)
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

// ── 第四面：本机到底缺什么（tmux 可用性）＋ `EMITS` ───────────
// 同上一段的理由：它们不是「分派」，是 hello 帧的**内容**。in-process 那条路一样要回答
// 「这台机器有没有 tmux」「本构建会发哪些帧」。`main_fourth_face_tests` 用 `super::` 引它们。
/// `K-P4`（09-04）：命令级 code —— **「这台机器上没有 tmux」**。
///
/// ⚠ 这是这个字面量在仓里的第 N 份，但它**不是第 N 个源头**：真相是
/// `inbound::REGISTRY` 里各条命令自己登记的 `codes`，本常量只拿它去**查那张表**。
/// 查不到就红（`the_declared_code_is_one_the_registry_already_declares`）⇒
/// 谁把那边的拼写改了，这边不会静默跟丢。
pub const NO_TMUX: &str = "no_tmux";

/// 命令级 code —— **「这个平台没有 unix 权限位」**。
///
/// 只有 `files-chmod` 声明它（`control/files_write.rs::change_mode` 在非 unix 上回它）。
/// 与 [`NO_TMUX`] 同形：真相是 `inbound::REGISTRY` 里命令自己登记的 `codes`，本常量只拿去查那张表；
/// target 轴（[`unix_mode_bits_on`] × 这个码）由此现推「Windows 上没有 `files-chmod`」
/// —— 待拍 3 那一格（「得让它的声明带一个『这个平台没有』的码」）。
///
/// 运行期那条轴（`hello.unavailable`）读的是同一个判准：[`unix_mode_unavailable`]（接线那一拍加进来）。
pub const NO_UNIX_MODE: &str = "no_unix_mode";

/// unix 权限位那一维：这台（或那个 target）没有 unix 权限位 ⇒ 声明会回 [`NO_UNIX_MODE`] 的命令做不到。
/// 与 tmux 那一维同形：谁声明会回那个码，谁就依赖那个机制，从 `inbound::REGISTRY` 的 `codes` 现推。
pub fn unix_mode_unavailable(unix_mode_bits: bool) -> Vec<stream::wire::Unavailable> {
    if unix_mode_bits {
        return Vec::new();
    }
    stream::inbound::REGISTRY
        .iter()
        .filter(|s| s.codes.contains(&NO_UNIX_MODE))
        .map(|s| stream::wire::Unavailable {
            command: s.name.to_string(),
            code: NO_UNIX_MODE.to_string(),
        })
        .collect()
}

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
pub fn unavailable_from(tmux: Option<bool>) -> Vec<stream::wire::Unavailable> {
    let mut out = Vec::new();
    if tmux == Some(false) {
        for spec in stream::inbound::REGISTRY {
            if spec.codes.contains(&NO_TMUX) {
                out.push(stream::wire::Unavailable {
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

/// `K-P4`：生产入口 —— hello 那一帧填的就是它（真填：`main.rs::build_hello`）。
/// 两维：tmux（[`unavailable_from`]）· unix 权限位（[`unix_mode_unavailable`]，这份二进制是不是 unix）。
///
/// 🔴 **这一趟探测的结果会被用很久。**
/// `build_hello` 在分档**之前**只调一次，那一帧随后交给两条载体；常驻那条（`listen.rs`）
/// 服务**不限次**的「只读 hello 就走」⇒ **同一帧被这个进程后续的所有连接共用**。
/// ⇒ 探测本身必须**便宜且挂不住**（所以 `tmux_in` 是纯 `stat` 扫 `PATH`，不是真 exec 一次），
/// 而消费侧必须把它当**提示**（`wire.rs` 那个字段头注的口径③）。
pub fn unavailable_here() -> Vec<stream::wire::Unavailable> {
    let mut out = unavailable_from(tmux_present(
        TMUX_PLATFORM,
        std::env::var_os("PATH").as_deref(),
    ));
    out.extend(unix_mode_unavailable(cfg!(unix)));
    out
}
