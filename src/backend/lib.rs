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

// 测试段可以直接问系统钟（量具、夹具的时刻）；生产段只许 `common::time::now`（`clippy.toml`）。
#![cfg_attr(test, allow(clippy::disallowed_methods))]
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
#[cfg(test)]
#[path = "../../tests/backend/allocator_guard.rs"]
mod allocator_guard; // 远端 musl 版的全局分配器是 mimalloc，本机 glibc 版与 Windows 版不换（整体 #[cfg(test)]）
pub mod assets; // 后端代管的用户资产（别名 · MCP · skill）：D 组的计算与判定，写经本进程的文件管理面
#[cfg(test)]
#[path = "../../tests/backend/build_features_tests.rs"]
mod build_features; // 判据构建与发布构建的 serde_json 特性集相同（`cargo tree --frozen` 现取）
#[cfg(test)]
#[path = "../../tests/backend/build_id_guard.rs"]
mod build_id_guard; // E77：加了子命令必须 bump BUILD_ID（内部整体 #[cfg(test)]，生产构建为空）
#[cfg(test)]
#[path = "../../tests/backend/cc_bus_boundary_guard.rs"]
mod cc_bus_boundary_guard; // P4f-Y2：backend 不许碰 cc-bus 的数据布局（整体 #[cfg(test)]）
pub mod ccm_line; // ccm 这一行的词与解析（旗标 · 解析 · 自己叫什么 · 配置的记号）：起会话 · 账号域 · 别名那一族都往下引它
pub mod common; // U2：两边都要、又不含平台原语的纯工具（§0.5-6 打掉了「三分够用」那个判断）
pub mod control; // U3：控制面 —— 会改变世界（写盘 / 改 tmux server / 发信号），或产出改变世界的计划
pub mod dial; // K-P6b / C2 /：SSH 的一切 —— 握手 · 连接池 · 链路（**只此一处**，判据在它自己的测块）
pub mod faces; // 帧面宿主：read_face · feature_face · fork_face · resync_face（薄壳，本体在 observe/ · control/）
pub mod files; // 步 24f：`files-read` 这一族（**只读**）—— 常驻文件名索引 ＋ 四条只读能力
#[path = "../../tests/backend/flow_scripts_guard.rs"]
mod flow_scripts_guard; // flow3：流程工具（门禁入口 · 挑格 · 重活隔离 · 推候选分支）在仓里、能跑、不写家目录（整体 #[cfg(test)]）
pub mod footprint; // 〔RM1a → MIG-3b 续〕「足迹」：帧面 `footprint-report`（申报表 ＋ 判定 ＋ 这台的 stat，出整份成品；只读）
#[cfg(test)]
#[path = "../../tests/backend/golden.rs"]
mod golden; // 金样判据只比不写；现打的落 `.build/golden/`，由 `tests/scripts/bless-golden.sh` 拷回（仅测试构建）
pub mod history; // 历史清单与注解（history_list · history_annotations）
#[cfg(test)]
#[path = "../../tests/backend/layering_guard.rs"]
mod layering_guard; // U3：§1.1 第二条解耦线的机器判据（observe↔control 方向与条数）
pub mod lineage; // 会话血缘：谁起的谁（中转那一处认，只存 `~/.cc-monitor/lineage.json` 一份）
#[cfg(test)]
#[path = "../../tests/backend/no_timer_guard.rs"]
mod no_timer_guard; // P6：零定时器护栏（内部整体 #[cfg(test)]，生产构建为空）
pub mod observe; // U3：观测面 —— 读，不改变世界
pub mod plan; // 计划（planned-build）的读面：找 pb、跑 `pb dump`、加工成界面排版的成品、盯计划仓推 `changed {plan}`（只读，一个字节都不写）
pub mod platform; // U2：唯一允许平台原语与平台 cfg 的层（§1.1 第一条解耦线）
pub mod plugin; // K-W1A：插件通用调用口 —— 找它 / 传 argv 起它 / 问它会什么（方向由 layering_guard 钉）
#[cfg(test)]
#[path = "../../tests/backend/plugin_walk_fixture.rs"]
mod plugin_walk_fixture; // K-W2E：最小假插件走通全流程（夹具 + 判据，整个文件级 cfg(test)，生产构建为空）
#[cfg(test)]
#[path = "../../tests/backend/protocol_doc_gen.rs"]
mod protocol_doc_gen; // 协议参考 IPC-COMMANDS.md 由它从代码生成；判据 = 重新生成 == 仓里那份
#[cfg(test)]
#[path = "../../tests/backend/protocol_doc_guard.rs"]
mod protocol_doc_guard; // U6a：IPC-PROTOCOL.md 与真实协议面的对拍
#[cfg(test)]
#[path = "../../tests/backend/ratchet_guard.rs"]
mod ratchet_guard; // K-P1 KPY7：动过的那几张登记表，**断言那几行**逐字没动（整体 #[cfg(test)]）
#[cfg(test)]
#[path = "../../tests/backend/readonly_guard.rs"]
mod readonly_guard;
#[cfg(test)]
#[path = "../../tests/backend/tmux_sandbox.rs"]
mod tmux_sandbox; // 判据用的私有 tmux server（`-L` ＋ 私有 TMUX_TMPDIR，碰不到用户的默认 server；仅测试构建）
#[cfg(test)]
#[path = "../../tests/backend/ts_export_guard.rs"]
mod ts_export_guard; // 第 8 刀：ts-rs 生成物只落在登记的目录、同名只许一个出处 // F08a：backend 只读机器护栏（内部整体 #[cfg(test)]，生产构建为空）
#[cfg(test)]
pub(crate) use ssh_dial::refusing_port; // 拨了必被拒、又不会被别人捡走的本机口（测试用；住拨号核心，经它的 `test-support`）
pub mod relay; // 中转的宿主：绑口 · 钥匙 · 起中转（中转本身是通信层 crate `comms_outward`）
#[cfg(test)]
#[path = "../../tests/backend/runs_guard.rs"]
mod runs_guard; // 子运行：通用层只认「运行」（扫描 ＋ 假适配层与 Claude Code 两套形状跑同一批判据）
#[cfg(test)]
#[path = "../../tests/backend/single_stream_guard.rs"]
mod single_stream_guard; // K-P1 KPY8：多客户按连接各一份 —— 「源码里恰好一份、按连接实例化」那几处的触发器（整体 #[cfg(test)]）
pub mod stderr_log; // 脱离常驻那条载体的 stderr 落进一份有上限、滚动的文件（宿主交 `CCM_BACKEND_STDERR_LOG` 才接；后端自有状态，经 `Own`，写口只从 main.rs 进）
pub mod stream; // 进后端的口（最上层）：命令表 · 帧面与 CLI 面的分派 · 门 · 期限 · 出口声明 · tap（谁都不引它，只 main / lib 接）
#[cfg(test)]
#[path = "../../tests/backend/test_home_guard.rs"]
mod test_home_guard; // 每个有判据的 Rust crate 都引了 `src/common/test-home`（测试档的家一开始就是沙箱）
pub mod wire; // 帧面的协议零件（最底下那一层，只引 platform · common · agents · relay）：帧 · typed 命令 · 登记形状 · 复制详情 · 主题表 · 监听判定 · 收场闸

/// Streaming wire-protocol major version, reported as `v` in the `Hello` frame.
/// Bump ONLY on a breaking wire change; additive forward-compatible frame kinds
/// (e.g. `Overflow`, #32) do NOT bump it — old parsers skip unknown kinds.
/// The monitor negotiates against its own `EXPECTED_PROTO_V` (#33).
pub const PROTO_VERSION: u32 = 1;

/// 后端身份（`Hello` 帧的 `build_id`，也烤进二进制的身份戳）：monitor 拿它比内嵌那份的，不同 ⇒ 那台该换后端。
/// 后端发出去的东西变了（子命令集 · 冻结格 · 形状指纹 · 行为）就打一个新的：`tests/scripts/bump-build-id.sh`，再 re-embed。
pub const BUILD_ID: &str = "p10s-session-open";

// 身份戳的两个界标住契约 crate（`deploy_contract::STAMP_OPEN` / `STAMP_CLOSE`）：monitor 扫字节用的是同一份。

pub const BUILD_STAMP_LEN: usize =
    deploy_contract::STAMP_OPEN.len() + BUILD_ID.len() + deploy_contract::STAMP_CLOSE.len();

/// 编译期把 `<开>` ＋ `BUILD_ID` ＋ `<关>` 拼成一段**定长字节**。
///
/// 🔴 **为什么必须是 `static [u8; N]` 而不是一个 `&str` 常量** —— 这一条是这里的支点，
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
    // 改账号库那几条帧命令（`inbound::REGISTRY` 的 `accounts-*`，本体 `accounts/manage/`）**自动派生**的 CLI 面。
    // 登记理由同下面那几族：`is_query_mode` 那道闸门读本表，不在表里 ⇒ 当未知 flag 静默进流模式。
    // `--accounts-add` 的入参（API 号的 key 在内）**从 stdin 读**，不收 argv。⚠ 加这几行会逼出一次 `BUILD_ID` bump。
    "--accounts-add",
    "--accounts-init",
    "--accounts-login-cmd",
    // 各号共用的用户级 MCP 那四条（`accounts-mcp-*`）派生的 CLI 面；`remove` / `pick` / `sync` 的入参从 stdin 读。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--accounts-mcp-pick",
    "--accounts-mcp-read",
    "--accounts-mcp-remove",
    "--accounts-mcp-sync",
    "--accounts-remove",
    "--accounts-repair",
    "--accounts-rollback",
    "--accounts-set-default",
    "--accounts-verify",
    // 会话外的需手动清单（`manual-list`，typed）派生的 CLI 面：无入参。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--manual-list",
    // 只读查询面那八条帧命令**自动派生**出来的 CLI 面
    //（`stream::cli::cli_exposed`）。登记在这里的理由与下面那几族逐字相同 ——
    // `is_query_mode` 那道闸门读的就是本表；不在表里 ⇒ 当未知 flag ⇒ 静默进流模式。
    //   `--accounts-list` 不在：它的徽章读本进程的中转监听状态，一次性进程答不出真话（`stream::cli::STREAM_ONLY`）。
    "--accounts-sessions",
    // 会话级答题 hook 的客户端（`control/asks/hook.rs`）：那一家的 hook 叫它，它经本机常驻后端问人、把答案交回去。⚠ 新子命令 ⇒ `build_id_guard` 红是预期的（打版本号时统一 bump）。
    "--answer-hook",
    "--machine-interrupts",
    // 帧命令 `sessions-needs`（这台上需手动的会话清单）自动派生出来的 CLI 面。⚠ 逼出一次 `BUILD_ID` bump（打版本号时统一做）。
    "--sessions-needs",
    // 各台需手动清单合成一个先后（`sessions-needs-merge`，纯计算）派生的 CLI 面。⚠ 新子命令 ⇒ BUILD_ID 打版本号时统一 bump。
    "--sessions-needs-merge",
    // 帧命令 `accounts-trust` 自动派生出来的 CLI 面。⚠ 逼出一次 `BUILD_ID` bump（打版本号时统一做）。
    "--accounts-trust",
    // 帧面 `backend-log`（这台后端的 stderr 诊断文件尾部）自动派生的 CLI 面。
    //   **是新子命令** ⇒ `build_id_guard` 红是预期的，BUILD_ID 打版本号时统一 bump。
    "--backend-log",
    // ── P4d：控制面的 CLI 面。**它们不在这里各写一条实现** ——
    // 分派臂按 `stream::cli::spec_for` 派生（见下面那条臂），实现落在 `inbound::REGISTRY`。
    // 登记在这张表里是因为 `is_query_mode` 与 `argv_table_guard` 都读它，
    // 而且 `build_id_guard` 的指纹也取自它 ⇒ 加在这里会**逼出一次 BUILD_ID bump**，那正是要的。
    // P4f：cc-bus 的两条基础命令。⚠ **不加这两行的后果是静默的** ——
    // 分派臂是派生的（认得出来），但 `is_query_mode` 这道**闸门**读的是本表：
    // 不在表里 ⇒ 当成未知 flag ⇒ 打一行 warn 之后**照常进流模式**，
    // CLI 面看上去"存在"却永远调不到（08-13 实测到了这个形状）。
    // ⇒ 现由 `stream::cli::tests::every_cli_exposed_command_is_in_the_query_mode_gate` 钉住。
    // 广播（`inbound::REGISTRY` 的 `bus-broadcast`）：原是 monitor 里的组合（列名单 ＋ 逐个发），
    //   界面改经通道直接说后端之后收进后端。登记理由同下面那几条；⚠ 加这一行逼出一次 `BUILD_ID` bump
    //   （`build_id_guard`）—— 这里**不 bump**，打版本号时统一做。
    "--bus-broadcast",
    // 帧面 `bus-inbox` 自动派生的 CLI 面（只读看收件箱尾巴）。⚠ 加这一行逼出 `BUILD_ID` bump（打版本号时统一做）。
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
    // 本机那一份放不放（帧面 `place-verdict` 的 CLI 面，自动派生；monitor 自举时跑手上那份字节问它）。⇒ `build_id_guard` 红是预期的（打版本号时统一 bump）。
    "--place-verdict",
    // 这台后端的漂移账（帧面 `drift-report` 的 CLI 面，自动派生）。⇒ `build_id_guard` 红是预期的（打版本号时统一 bump）。
    "--drift-report",
    // 公钥推送两条（帧面 `pubkey-push` / `authorized-keys-add` 的 CLI 面，自动派生；远端那台被 `remote_ask::ask_json` 走的就是后一条）。
    "--authorized-keys-add",
    "--pubkey-push",
    // 「退出行为」那个值的两条命令（`inbound::REGISTRY` 的 `exit-policy-*`）自动派生的 CLI 面。
    // 登记理由与上面那几族逐字相同 —— `is_query_mode` 那道闸门读本表，不在表里 ⇒ 当未知 flag 静默进流模式。
    // ⚠ 加这两行会逼出一次 `BUILD_ID` bump（`build_id_guard`）—— 这里**不 bump**，打版本号时统一做。
    "--exit-policy-read",
    "--quota-read",
    // 计划读面三条（`inbound::REGISTRY` 的 `plan-*`）自动派生的 CLI 面，入参从 stdin 读。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--plan-list",
    "--plan-read",
    "--plan-cell-view",
    // 计划审面三条（`plan-ack` · `plan-unack` · `plan-return`）派生的 CLI 面，入参从 stdin 读。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--plan-ack",
    "--plan-unack",
    "--plan-return",
    // 代敲 pb 的用户命令（`plan-command`）派生的 CLI 面，入参从 stdin 读。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--plan-command",
    // 文件窗口反查（`plan-files`）派生的 CLI 面，入参从 stdin 读。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--plan-files",
    // 校验 pb 路径（`plan-pb-check`）派生的 CLI 面，入参从 stdin 读。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--plan-pb-check",
    // 用某个号查一次额度（`inbound::REGISTRY` 的 `quota-probe`）派生的 CLI 面，入参从 stdin 读。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--quota-probe",
    // 换号那一族（`inbound::REGISTRY` 的 `rotation-*`，轮换第五版，typed）自动派生的 CLI 面；入参从 stdin 读。
    // ⚠ 第三版那 15 条（配置 · 会话副本 · 推演 · 现在就换）随第五版删了，换成下面这些 ⇒ BUILD_ID 打版本号时统一 bump。
    "--rotation-facts",
    "--rotation-try",
    "--rotation-strategies",
    "--rotation-view",
    "--rotation-settings",
    "--rotation-save",
    "--rotation-revert",
    "--rotation-use",
    "--rotation-for-new",
    "--rotation-temp",
    "--rotation-use-now",
    "--rotation-strategy-add",
    "--rotation-strategy-copy",
    "--rotation-strategy-rename",
    "--rotation-strategy-delete",
    "--rotation-strategy-tz",
    "--exit-policy-set",
    // 上游选择那份凭据文件在这台机器上的写口（`inbound::REGISTRY` 的 `apikey-key-set`）自动派生的 CLI 面。
    // ⚠ 入参（含 key）**从 stdin 读**（`takes_input: true`），不收 argv —— argv 在同机任何用户的 `ps` 里都看得见。
    "--apikey-key-set",
    // `--relay-ensure` / `--relay-status` 随帧面那两条删了。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    // 起会话那一行 `ccm …` 的渲染（`inbound::REGISTRY` 的 `launch-render-cli`，纯函数）自动派生的 CLI 面，入参从 stdin 读。
    //   `launch-local` 不上 CLI 面（`STREAM_ONLY`）。`--launch-render-payload` 随载荷那条删了 ⇒ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--launch-render-cli",
    // 「足迹」出成品（`inbound::REGISTRY` 的 `footprint-report`，替掉 `--footprint-probe`）派生的 CLI 面。只读。
    "--footprint-report",
    // 「文件与数据」那一份成品（`data-report`，同一份足迹重排）派生的 CLI 面。只读。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--data-report",
    // 换 agent 家目录存之前那一问（`agent-home-check`）派生的 CLI 面。只读。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--agent-home-check",
    // 「待办」记下一个选择（`chores-mark`，写后端自己的 `~/.cc-monitor/chores.json`）派生的 CLI 面。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--chores-mark",
    // 几台的「待办」件数合成一份（`chores-merge`，纯计算）派生的 CLI 面。⚠ 新子命令 ⇒ BUILD_ID 打版本号时统一 bump。
    "--chores-merge",
    // 用户级 PATH 那一格（`ccm-user-path`，typed）派生的 CLI 面；入参从 stdin 读。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--ccm-user-path",
    // 离线那台的上次值（`last-seen-read` / `last-seen-write`，读写后端自己的 `~/.cc-monitor/last-seen.json`）派生的 CLI 面。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--last-seen-read",
    "--last-seen-write",
    // 首次运行「开始用」那份数（`first-run`）派生的 CLI 面。只读。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--first-run",
    // 帧命令 `ccm-print` 的 CLI 面删了：`--ccm-*` 这族名字归 ccm 的诊断口，二进制叫 `ccm` 时
    //   按本表分流会把 `ccm --ccm-print` 抢进后端（`stream::cli::cli_exposed` 排除 ccm 的词）。逼出 `BUILD_ID` bump（打版本号时统一做）。
    // 帧面 `mcp-read` 自动派生的 CLI 面（MCP 列表成品）。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--mcp-read",
    // MCP 推拉三条（`inbound::REGISTRY` 派生的 CLI 面）。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--mcp-sync-source",
    "--mcp-sync-preview",
    "--mcp-sync-apply",
    "--cc-bus-install",
    "--cc-bus-install-state",
    "--skill-install-apply",
    "--aliases-read",
    "--aliases-block-render",
    "--aliases-block-install",
    "--aliases-block-remove",
    "--profiles-read",
    "--profiles-resolve",
    "--profiles-from-line",
    "--profiles-impact",
    "--profiles-bases",
    "--profiles-write",
    // 这台的偏好（`prefs-read` / `prefs-set`，第 7 批第一题）派生的 CLI 面。⚠ 新子命令 ⇒ BUILD_ID 打版本号时统一 bump。
    "--prefs-read",
    "--prefs-set",
    // 这一端的机器表（`machines-read` / `-try` / `-set`，第 7 批第 4 件）派生的 CLI 面。⚠ 新子命令 ⇒ BUILD_ID 打版本号时统一 bump。
    "--machines-read",
    "--machines-try",
    "--machines-set",
    // 帧面 `powershell-policy-set` 派生的 CLI 面。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--powershell-policy-set",
    // 资产目录那两条（`inbound::REGISTRY` 的 `assets-catalog` / `assets-catalog-merge`）派生的 CLI 面。
    // 加这两行会逼出一次 `BUILD_ID` bump（`build_id_guard`）—— 这里**不 bump**，打版本号时统一做。
    "--assets-catalog",
    "--assets-catalog-merge",
    // `assets-sync` 是真异步命令 ⇒ 按派生规则上 CLI 面（`stream::cli::cli_exposed`：非内建即上）。
    // ⚠ 一次性进程没有常驻那一个的连接池与可达表：它自己新拨一条、对那一台做一趟，扇出恒为零台。
    "--assets-sync",
    // skill「装到这台」那两条（`skill-read` / `skill-install-plan`）派生的 CLI 面。只读，入参从 stdin 读。
    "--skill-read",
    "--skill-install-plan",
    // 装记录那一条（`skill-install-record`）派生的 CLI 面。
    "--skill-install-record",
    // 扩展页「从这台卸」那两条（`ext-uninstall-preview` / `ext-uninstall-apply`）派生的 CLI 面。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--ext-uninstall-preview",
    "--ext-uninstall-apply",
    // 扩展页那张表的本机那一半（`ext-list-here`）派生的 CLI 面：只有这台一列，一格都不问可达表
    //   （跨机那一整张仍只在帧面的 `ext-list`）。登记理由同上面那几族 —— `is_query_mode` 那道闸门读本表。
    // ⚠ 是新子命令 ⇒ `build_id_guard` 红是预期的，BUILD_ID 打版本号时统一 bump。
    "--ext-list-here",
    // 扩展页写备注那一条（`ext-note-set`）派生的 CLI 面。
    "--ext-note-set",
    // 可达表登记（`inbound::REGISTRY` 的 `remote-reach`）派生的 CLI 面，入参从 stdin 读。
    // ⚠ 一次性进程的可达表随进程退出就空 —— 真正的用法是常驻后端的帧面。加这一行会逼出一次 `BUILD_ID` bump（打版本号时统一做）。
    "--remote-reach",
    // 测试连接（`remote-probe`）那一行 CLI 面摘了：进度格改走本连接的应答通道（硬臂，只在帧面，理由见 `stream::cli` 判据的 `NOT_ON_CLI`）。
    // 历史注解那三条（`inbound::REGISTRY` 的 `history-annotate` / `-forget` / `-last-accounts`）派生的 CLI 面。
    // 文件住家里，一次性进程与常驻那一个按家推出同一份。加这三行会逼出一次 `BUILD_ID` bump（打版本号时统一做）。
    "--history-annotate",
    "--history-forget",
    "--history-last-accounts",
    "--backend-probe",
    // `--dial`（拨号代理，`K-P6b` / C2）**从本表摘掉了**：拨号挪进本机那一个常驻后端、
    // 经流上的链路（`link-*` 四条，`dial/link.rs`）做，不再每条链路起一个进程。
    //  ⚠ 摘这一行会让 `build_id_guard` 红 —— 这里**不 bump**，打版本号时统一做。
    // `files-read` 这一族的 CLI 面。登记在这里的理由与上面
    // 那几条逐字相同 —— `is_query_mode` 那道**闸门**读的就是本表；不在表里 ⇒ 当未知 flag
    // ⇒ 打一行 warn 之后照常进流模式，调用方拿到的是一堆 jsonl 行而不是它要的应答。
    // ⚠ 线上名用 `-` 不用 `.`（能力名仍是 `files.ls` 那一套）：理由整段在
    // `files::answer_wire` 的头注 —— 一句话是 `"--files.ls"` 会被本仓两个 token
    // 取词器**静默丢弃**，那等于把这四条从三条判据底下同时抽走而三条都照常报绿。
    // 同族的第五、第六条。登记在这里的理由
    // 与上面那四条逐字相同 —— `is_query_mode` 那道闸门读的就是本表。
    // 🔴 **本族第一条会往盘上写的命令。**
    //
    // 它落在这张表里**不是选择，是派生的必然**：`stream::cli::cli_exposed` 逐字是
    // `!spec.run.on_frame_arm()` —— 只要一条命令进了 `inbound::REGISTRY`
    // 且不是那个硬臂，CLI 面就**自动**认得它；而本表是 `is_query_mode` 那道闸门。
    // ⇒ 不加这一行的后果与上面那几条逐字相同（当未知 flag、照常进流模式）。
    // ⚠ **如实登记一条代价**：因此「写这一面只从帧面进来」这句话**做不到**，
    //   两个宿主共用同一个 `run`（「一份代码两种宿主」正是这一形）。
    //   写面的「窄」因此**不靠命令面**，而靠写句柄：`Face` 只递给命令表文件管理那一族（typed 的 `Facing` 那一档），
    //   判的是**谁拿得到句柄**，不是谁发得出命令。
    "--files-create",
    // 同一面的另外五条。登记理由与上一行逐字相同。
    "--files-chmod",
    "--files-delete",
    "--files-mkdir",
    "--files-rename",
    "--files-write-text",
    // 同一面第七条（同根内复制）。登记理由与上面逐字相同。
    "--files-copy",
    // 撤一组复制 · 删除 · 改权限。登记理由同上（CLI 面从 `REGISTRY` 派生）。
    "--files-batch-stop",
    // 解压。**是新子命令** ⇒ `build_id_guard` 红是预期的，BUILD_ID 发版时统一 bump。
    "--files-extract",
    // 上传的提交。登记理由同上面写面那几条（CLI 面从 `REGISTRY` 派生）。
    "--files-commit-upload",
    // 存盘装不进一行时的两步（逐块进暂存区 ＋ 读回拼起来原地覆盖）。登记理由同上。
    "--files-stage-chunk",
    "--files-commit-text",
    // 删历史会话。登记理由同上面写面那几条（CLI 面从 `REGISTRY` 派生）。
    "--files-delete-session",
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
    // 读族第九条（算目录大小）。**是新子命令** ⇒ `build_id_guard` 红是预期的，BUILD_ID 发版时统一 bump。
    "--files-size",
    // 读族第十一条（按内容搜）。**是新子命令** ⇒ `build_id_guard` 红是预期的。
    "--files-grep",
    // 会话内查找（Ctrl+F）。**是新子命令** ⇒ `build_id_guard` 红是预期的，
    // BUILD_ID 打版本号时统一 bump。
    // 历史读面 · 搜索 · 账号读面 · 分叉那 13 条手写子命令（`--read-session*` · `--search` · `--list-*` · `--session-accounts` ·
    //   `--find-in-session` · `--account-trust*` · `--fork-session`）删了（第 7 批第二题：帧面早有同义的帧命令、生产里没人调）。
    //   ⚠ 子命令集变了 ⇒ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    // 帧面 `session-fork` 自动派生的 CLI 面（读 stdin）。
    "--session-fork",
    // 同上一段：`history-*` 六条帧命令的 CLI 面。
    // +2：`history-index` / `history-user-inputs`（骨架索引与大纲清单上帧面）的 CLI 面，
    //   登记理由同上 —— `is_query_mode` 那道闸门读本表。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    // `history-facts`（会话事实出成品）的 CLI 面（从 `REGISTRY` 派生，`is_query_mode` 那道闸门读本表）。
    //   **是新子命令** ⇒ `build_id_guard` 红是预期的，BUILD_ID 打版本号时统一 bump。
    // 主线外清单的冷读（手机端不发帧命令，从 CLI 面问）。⚠ 新子命令 ⇒ BUILD_ID 发版时统一 bump。
    "--history-branch",
    "--history-facts",
    "--history-find",
    "--history-index",
    // `history-lines`（按行号取回）的 CLI 面。**是新子命令** ⇒ `build_id_guard` 红是预期的，
    //   BUILD_ID 打版本号时统一 bump。
    "--history-lines",
    // `history-list`（历史页的平铺清单）的 CLI 面：远端那一支问的就是它（`raw`）。**是新子命令** ⇒ `build_id_guard` 红是预期的（打版本号时统一 bump）。
    "--history-list",
    // `history-page`（按字节分页出记录行）的 CLI 面。**是新子命令** ⇒ `build_id_guard` 红是预期的（打版本号时统一 bump）。
    "--history-page",
    "--history-read",
    // `history-record` 的 CLI 面（CLI 面从 `REGISTRY` 派生，`is_query_mode` 那道闸门读本表）。
    // **是新子命令** ⇒ `build_id_guard` 红是预期的，BUILD_ID 打版本号时统一 bump。
    "--history-record",
    // 按运行读一个子运行的记录（替掉按目录与描述挑的那一条）。**子命令换了** ⇒ `build_id_guard` 红是预期的（打版本号时统一 bump）。
    "--history-run",
    "--history-search",
    // 各台搜索结果合成一份（`history-search-merge`，纯计算）派生的 CLI 面：原先「只有界面逐台问完才有得合」，第二个前端同样逐台问 ⇒ 放出。
    // ⚠ 新子命令 ⇒ BUILD_ID 打版本号时统一 bump。
    "--history-search-merge",
    "--history-tail",
    // `history-turns`（一轮的摘要，主窗口第 2 批）的 CLI 面。**是新子命令** ⇒ `build_id_guard` 红是预期的（打版本号时统一 bump）。
    "--history-turns",
    "--history-user-inputs",
    // 帧面 `hooks-diag` 自动派生的 CLI 面。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--hooks-diag",
    "--kill",
    "--launch",
    "--ping",
    // `--relay`（独立的中转进程）删了：中转只住常驻后端进程里。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    // 远端常驻后端的起 · 找 · 停（`control/resident.rs`；monitor 经链路 capture 跑）。
    // ⚠ 新子命令 ⇒ `build_id_guard` 红是预期的（打版本号时统一 bump）。
    // 远端那台的小中继（连常驻后端的套接字、原样对拷；monitor 经链路 stream 跑）。⚠ 新子命令 ⇒ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--resident-attach",
    "--resident-ensure",
    "--resident-stop",
    // 帧面 `resolve` 派生的 CLI 面（手写那一臂删了，信封与别的命令同一形）。
    "--resolve",
    // `--resync` 摘了（`stream::cli::STREAM_ONLY`：一次性进程里没有 watcher，答 `watchers: 0` 是假话）。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    // 起新会话框那三问 ＋ tab 栏多选的批量停 / 起 / 问样子（`inbound::REGISTRY` 的 `session-new*` / `sessions-*`）
    //   自动派生的 CLI 面：六条的事实**全在这台**（tmux 名单现探 · 记录在不在 · 铸名经同一张会话快照 ——
    //   `SessionSnapshot::query` 没有「只读缓存」那条路，一次性进程问它一次它就重探一次），
    //   一格都不读本进程的可达表 / 监听状态 / 在飞表 ⇒ 不进 `stream::cli::STREAM_ONLY`，照派生规则上 CLI 面。
    //   登记在本表的理由与上面那几族逐字相同 —— `is_query_mode` 那道闸门读的就是本表，不在表里 ⇒ 当未知 flag 静默进流模式。
    // ⚠ 是新子命令 ⇒ `build_id_guard` 红是预期的，BUILD_ID 打版本号时统一 bump。
    "--session-new",
    "--session-new-dir",
    "--session-new-facts",
    // 起会话 · 恢复 · 接回一个请求（`session-open`）派生的 CLI 面，理由同上。⚠ 新子命令 ⇒ BUILD_ID 打版本号时统一 bump。
    "--session-open",
    // 往会话说一句（`session-say`）派生的 CLI 面（stdin 一段 JSON 当 `args`）。⚠ 新子命令 ⇒ BUILD_ID 打版本号时统一 bump。
    "--session-commands",
    "--session-model",
    "--session-permission",
    "--session-resume",
    "--session-say",
    "--sessions-start",
    "--sessions-stop",
    "--sessions-where",
    // `ssh-config-import` 帧命令自动派生的 CLI 面（理由同 `--tasks-list`）。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    "--ssh-config-import",
    // `tasks-list` 帧命令**自动派生**出来的 CLI 面（`stream::cli::cli_exposed`），
    // 登记理由同 `C1` 那一段：不在表里 ⇒ `is_query_mode` 当未知 flag ⇒ 静默进流模式。
    // ⚠ 新子命令 ⇒ `build_id_guard` 红是预期的，BUILD_ID 打版本号时统一 bump。
    "--tasks-list",
    // 终端管理 L1 三条帧命令自动派生的 CLI 面（stdin 一段 JSON 当 `args`）。⚠ 新子命令 ⇒ `BUILD_ID` 发版时统一 bump。
    "--terminal-input",
    // 铸终端名 · 开终端那一串（`terminal-name-mint` / `terminal-ssh`）派生的 CLI 面：原先在 `stream::cli` 那张「只有界面用得着」的表里，
    //   复核后放出（铸名经会话快照现探；那一串由入参与这台的 ssh 客户端算出）。⚠ 新子命令 ⇒ BUILD_ID 打版本号时统一 bump。
    "--terminal-name-mint",
    "--terminal-preview",
    "--terminal-ssh",
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

// ══════════════════ 步 `8a`：能力清单的**汇总** —— 第 2 层 ══════════════════
//
// 🔴 **这一段填的是 `files/mod.rs` 头注自己登记的那个缺口**，逐字：
//   「`CAPABILITIES` 的汇总没接。第 2 层要求『能力清单从实现派生，
//    `CAPABILITIES` 由它们汇总而来』。本族把自己那一份声明成了**数据**，但**没有**
//    把它汇进流旗标那张能力表（10-10 随流旗标删了）—— 那一处的语义当时是『会在一次性查询判定前
//    剥离对应 flag 的**流**能力』，本族六条都不是那种东西，硬塞进去会当场红，
//    **而且会是红对了**。⇒ 汇总要先有第 2 层那个派生机制，那是另一件活。」
//
// 🔴 **「硬塞进去会当场红」这句话实测过，成立**：把 `"files.ls"` 加进上面那个
//   流旗标那张能力表之后 `backend` 套 `769 passed / 1 failed`，**只红一条**，
//   而且是当时那条「每个能力 token 都有剥离分支」的判据逐字点名
//   「无 flag 映射 …… 否则埋 §26 死循环」。⇒ 那一处的语义**保持不动**，
//   它在本汇总里是**一个面**（`stream-flags`），不是汇总本身。（10-10 那张表与 hello 的 `capabilities` 一起删了。）
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
/// 他那次是三句，合起来是一条完整裁决：
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
        target_basis: "本族目录下**零**平台 `cfg`（实测：`src/backend/files/` 递归 \
                       `grep cfg(target_os|windows|unix|target_family` 零命中）\
                       ⇒ 四个 target 同一份源码。另有本族自己那条更强的：每条能力的 \
                       `Capability::targets` 与 `TARGETS` 做**集合相等**（边界②）。",
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
                        〔⚠ 这句被订正的原话**刻意在源码里连着写**：写的时候差点栽在自己的 grep 上 —— \
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
/// ⚠ **「暂时不做」是一种合法答复，但必须写出来** —— 用户那次逐字
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
/// 🔴 **减一条的判准与加一条不同**：加一条要的是「现打出它做不到」，**减一条要的是「那条『做不到』的根因在源码里没了」＋
/// 一句如实登记「真机那一维这次买不到」**。别把减掉的读成「真机上复验过了」。
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
              将来选定机制时一并给出等价物。",
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
    // 这六行在 PR1 之前就是真的（`K-P4` 那时 `unavailable_from` 在 Windows 上就会列出这三条），
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
        capability: "terminal-follow",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "终端实时预览：这一版宿主只有 tmux（起 tmux 控制模式客户端收输出事件，声明了 `no_tmux`）。\
              与 `terminal-preview` 同一个理由（Windows 后台机制未定），同拍还，**暂时不做**。",
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
        family: "wire-commands",
        capability: "plan-return",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "计划退回送字走的就是 `terminal-input` 的本体（tmux `send-keys`，声明了 `no_tmux`）⇒ 同那一行的理由，\
              Windows 后台机制定了、`terminal-input` 还上时一起还，**暂时不做**。",
    },
    TargetGap {
        family: "cli-subcommands",
        capability: "--plan-return",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "与帧面 `plan-return` 那一行是同一条实现（CLI 面派生）⇒ 同一个理由（Windows 后台机制未定），同拍还，**暂时不做**。",
    },
    TargetGap {
        family: "wire-commands",
        capability: "session-resume",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "恢复再附着：同 `session-say` 终端没了那一支（在 tmux 里起）⇒ 同一个理由（Windows 后台机制未定），同拍还，**暂时不做**。",
    },
    TargetGap {
        family: "wire-commands",
        capability: "session-say",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "往会话说一句：送字那一半就是 `terminal-input` 那一条（`send-keys`，声明了 `no_tmux`）⇒ 同一个理由（Windows 后台机制未定），同拍还，**暂时不做**。",
    },
    TargetGap {
        family: "wire-commands",
        capability: "session-permission",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "会话开着时切权限档：按键走 `terminal-input` 那一条、等重画用 tmux 控制模式客户端（声明了 `no_tmux`）⇒ 同那两条的理由（Windows 后台机制未定），同拍还，**暂时不做**。",
    },
    TargetGap {
        family: "wire-commands",
        capability: "session-model",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "会话开着时换模型：送那一行走 `terminal-input` 那一条（`send-keys`，声明了 `no_tmux`）⇒ 同那一条的理由（Windows 后台机制未定），同拍还，**暂时不做**。",
    },
    TargetGap {
        family: "cli-subcommands",
        capability: "--session-model",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "与帧面 `session-model` 那一行是同一条实现（CLI 面派生）⇒ 同一个理由（Windows 后台机制未定），同拍还，**暂时不做**。",
    },
    TargetGap {
        family: "cli-subcommands",
        capability: "--session-permission",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "与帧面 `session-permission` 那一行是同一条实现（CLI 面派生）⇒ 同一个理由（Windows 后台机制未定），同拍还，**暂时不做**。",
    },
    TargetGap {
        family: "cli-subcommands",
        capability: "--session-resume",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "与帧面 `session-resume` 那一行是同一条实现（CLI 面派生）⇒ 同一个理由（Windows 后台机制未定），同拍还，**暂时不做**。",
    },
    TargetGap {
        family: "cli-subcommands",
        capability: "--session-say",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "与帧面 `session-say` 那一行是同一条实现（CLI 面派生）⇒ 同一个理由（Windows 后台机制未定），同拍还，**暂时不做**。",
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
        rationale: "与帧面 `kill` 那一行是**同一条实现**（CLI 面经 `stream::cli::spec_for` \
              派生到同一条登记）⇒ 同一个理由（Windows 后台机制未定），将来与帧面那一行同拍还，**暂时不做**。",
    },
    TargetGap {
        family: "cli-subcommands",
        capability: "--launch",
        target: Target::Windows,
        kind: GapKind::Owed,
        rationale: "与帧面 `launch` 那一行是**同一条实现**（CLI 面经 `stream::cli::spec_for` \
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
        rationale: "与帧面 `files-chmod` 那一行是**同一条实现**（CLI 面经 `stream::cli::spec_for` \
              派生到同一条登记）⇒ 同一个理由：unix 权限位这一条**不该跨过去**。",
    },
];

/// [`CAPABILITIES`] 的名单，包成 [`CapabilityFace::declares`] 要的形状。
///
/// ⚠ 它**不是**第二份名单，是同一个 const 的一次借用：这一族的能力名住址仍然只有
/// [`CAPABILITIES`] 那一行。
/// `control::ccm::CAPABILITIES` 的名单。
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

/// 编译 target → 那一格在「tmux 在不在」这件事上**平台这一维**给的档（[`platform::tmux::TmuxPlatform`]）。
///
/// # 为什么它是本段的一根柱子
///
/// 本仓今天所有「这个 target 上做不到」的声明，归根到底只有一种机制：**载体是 tmux，
/// 而那个 target 没有 tmux**。[`platform::tmux::TmuxPlatform`] 已经把「Windows 上确证没有」这句话
/// 做成了编译期的值（`K-P4` 那时，头注逐字「这一句在**编译期**就成立，不需要探针去证」）。
/// ⇒ 本函数只把 [`Target`] 这根轴接到那根轴上，**不另立一份平台知识**。
///
/// ⚠ 两根轴对不对得上，由 `target_parity_guard::the_target_axis_agrees_with_the_host_tmux_platform`
/// （本机那一格，运行期）与下面那条 `#[cfg(windows)]` 编译期断言（Windows 那一格）各钉一半。
pub const fn tmux_platform_of(t: Target) -> platform::tmux::TmuxPlatform {
    match t {
        Target::Windows => platform::tmux::TmuxPlatform::AbsentUnlessExeOnPath,
        Target::LinuxGnu | Target::LinuxMusl | Target::MacOs => {
            platform::tmux::TmuxPlatform::AskThePath
        }
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

/// ★ 只在 Windows 编译时存在：[`tmux_platform_of`] 给 Windows 的档 == 那份二进制真编进去的 [`platform::tmux::TMUX_PLATFORM`]。
///
/// 同 `TMUX_PLATFORM` 旁边那条：本机（Linux）门禁上它**不存在**，开口的时刻是
/// `winchk-backend` 那一格的 Windows 编译。
#[cfg(windows)]
const _: () = assert!(
    tmux_platform_of(Target::Windows) as u8 == platform::tmux::TMUX_PLATFORM as u8,
    "Target 轴说 Windows 是一档，TMUX_PLATFORM 说是另一档 —— 两根平台轴分叉了"
);

/// 在 target `t` 上，**不看任何一台机器**、只凭平台就成立的 tmux 结论。
///
/// 入参 `PATH` 刻意给 `None`：unix 那几档因此答「不知道」（`None`）——
/// 按 [`unavailable_from`] 头注那条三态处置，「不知道」**不许**压成「做不到」；
/// Windows 那一档答「确证没有」（`Some(false)`）—— 那是它的平台默认，不靠探针。
fn tmux_by_platform(t: Target) -> Option<bool> {
    platform::tmux::tmux_present(tmux_platform_of(t), None)
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

/// 上面那一步的内核：平台档是**入参**（同 [`crate::platform::tmux::tmux_present`] 的做法），
/// 于是「能力账」（按 `crate::Target` 问）与「`ccm --ccm-probe` 自报」（按本二进制的 [`crate::platform::tmux::TMUX_PLATFORM`] 问）
/// 走的是**同一个函数**，不另写名单。
pub(crate) fn ccm_launcher_with(p: crate::platform::tmux::TmuxPlatform) -> Vec<&'static str> {
    let no_tmux = crate::platform::tmux::tmux_present(p, None) == Some(false);
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
            stream::cli::spec_for(flag).is_none_or(|s| !gone.iter().any(|g| g == s.name))
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
        target_basis: "`SUBCOMMANDS` 四个 target 上逐字同一张表；经 `stream::cli::spec_for` 派生到 \
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
///   开门要改那份门表与 `files` 那一族，PR1 没动它们。
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

/// ① 流模式 flag：出现即剥离，**不影响模式判定**。只剩「我是流模式」那一个词；这条流要什么由声明说（`StreamWants::view`）。
pub const STREAM_FLAGS: &[&str] = &[wire::cli_words::STREAM_FLAG_EXPLICIT];

/// 看的那一台的时区（[`TZ_FLAG`] · 请求信封 `tz` 读出来的那一格）。
pub use common::time::Tz;
/// 一条分页应答 / 一帧 `session_branch` 至多多少字节（写出方向写完这么多应答就先让实时帧走：`main.rs::writer_task`）。
pub use wire::REPLY_PAGE_BYTES;

/// 一行线上字里复制详情那一行时刻的空位，按看的那一台的时区填好（流的写者对每一行调它；本体 `common::time::fill_at`）。
pub fn stamp_detail_slots<'a>(line: &'a str, tz: &Tz) -> std::borrow::Cow<'a, str> {
    common::time::fill_at(line, tz)
}

/// ③ 子命令自己的选项：只在某条 [`SUBCOMMANDS`] 之后才有意义，backend 顶层不解释它们。
pub const SUBCOMMAND_OPTIONS: &[&str] = &[
    // `--after-ms` · `--include-tools` · `--limit` · `--scope`（全文搜索的选项词）第 8 批删了：帧面 `history-search` 的入参直接从类型收（`faces/search_face.rs`）。
    // `--from` · `--until` · `--query`（`--list-user-inputs` · `--read-session-from-offset` · `--find-in-session` 的选项）随那几条子命令删了。
    // CLI 控制面那一族（`--<帧命令>`）的 argv 形载荷口（与 `--stdin-line` 二选一）。⚠ 进指纹的 `#options` 段 ⇒ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    wire::cli_words::ARGS_B64_FLAG,
    // `--resident-stop` 的宽限期（秒；必须大于退出排空上限）。
    "--grace",
    // `--resident-ensure` 的「先停口上那一位再起」（只升不降由 monitor 按 hello 判）。
    "--replace",
    // CLI 控制面那一族（`--<帧命令>`）的「只读一行 stdin」修饰词。
    wire::cli_words::STDIN_LINE_FLAG,
    // CLI 控制面那一族的「给人看」那一形（所有命令通用）。
    wire::cli_words::TEXT_FLAG,
    // 看的那一台的时区（同帧面请求信封的 `tz`；流模式起参也认它，剥在 `stream::cli::split_stream_flags`）。⚠ 进指纹的 `#options` 段 ⇒ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    wire::cli_words::TZ_FLAG,
    // CLI 控制面那一族（`--<帧命令>`）的声明口（同帧面请求信封的 `view`）。⚠ 进指纹的 `#options` 段 ⇒ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    wire::cli_words::VIEW_FLAG,
    // CLI 控制面那一族（`--<帧命令>`）的期限口（同帧面请求信封的 `within_ms`）。⚠ 进指纹的 `#options` 段 ⇒ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    wire::cli_words::WITHIN_MS_FLAG,
];

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
#[path = "../../tests/backend/one_clock_guard.rs"]
mod one_clock_guard;

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

// ── 第四面：本机到底缺什么（tmux 可用性） ───────────
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
/// 运行期那条轴（`hello.unavailable`）读的是同一个判准：[`unix_mode_unavailable`]（接线时加进来）。
pub const NO_UNIX_MODE: &str = "no_unix_mode";

/// unix 权限位那一维：这台（或那个 target）没有 unix 权限位 ⇒ 声明会回 [`NO_UNIX_MODE`] 的命令做不到。
/// 与 tmux 那一维同形：谁声明会回那个码，谁就依赖那个机制，从 `inbound::REGISTRY` 的 `codes` 现推。
pub fn unix_mode_unavailable(unix_mode_bits: bool) -> Vec<wire::Unavailable> {
    if unix_mode_bits {
        return Vec::new();
    }
    stream::inbound::REGISTRY
        .iter()
        .filter(|s| s.codes.contains(&NO_UNIX_MODE))
        .map(|s| wire::Unavailable {
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
pub fn unavailable_from(tmux: Option<bool>) -> Vec<wire::Unavailable> {
    let mut out = Vec::new();
    if tmux == Some(false) {
        for spec in stream::inbound::REGISTRY {
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

/// `K-P4`：生产入口 —— hello 那一帧填的就是它（真填：`main.rs::build_hello`）。
/// 两维：tmux（[`unavailable_from`]）· unix 权限位（[`unix_mode_unavailable`]，这份二进制是不是 unix）。
///
/// 🔴 **这一趟探测的结果会被用很久。**
/// `build_hello` 在分档**之前**只调一次，那一帧随后交给两条载体；常驻那条（`listen.rs`）
/// 服务**不限次**的「只读 hello 就走」⇒ **同一帧被这个进程后续的所有连接共用**。
/// ⇒ 探测本身必须**便宜且挂不住**（所以 `tmux_in` 是纯 `stat` 扫 `PATH`，不是真 exec 一次），
/// 而消费侧必须把它当**提示**（`wire.rs` 那个字段头注的口径③）。
pub fn unavailable_here() -> Vec<wire::Unavailable> {
    let mut out = unavailable_from(platform::tmux::tmux_present(
        platform::tmux::TMUX_PLATFORM,
        std::env::var_os("PATH").as_deref(),
    ));
    out.extend(unix_mode_unavailable(cfg!(unix)));
    out
}

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本 crate 的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
