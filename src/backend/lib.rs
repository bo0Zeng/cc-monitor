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
#[path = "../../tests/backend/alloc_probe.rs"]
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
pub const BUILD_ID: &str = "p10t-batch-core";

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
    // 忘掉一台的主机指纹（`host-keys-forget`）派生的 CLI 面。⚠ 新子命令 ⇒ BUILD_ID 打版本号时统一 bump。
    "--host-keys-forget",
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
    // 部署那一趟：暂存区里新传上去的那一份自己换进落点（`control/place_self.rs`）。⚠ 新子命令 ⇒ BUILD_ID 打版本号时统一 bump。
    "--place-self",
    // 卸载那一趟：暂存区里传上去的那一份删掉落点那一份（同一个文件）。
    "--unplace-self",
    "--resident-attach",
    "--resident-ensure",
    "--resident-stop",
    // 帧面 `resolve` 派生的 CLI 面（手写那一臂删了，信封与别的命令同一形）。
    "--resolve",
    // `--resync` 摘了（`stream::cli::STREAM_ONLY`：一次性进程里没有 watcher，答 `watchers: 0` 是假话）。⚠ 逼出 `BUILD_ID` bump（打版本号时统一做）。
    // 起新会话框那两问 ＋ tab 栏多选的批量停 / 问样子（`inbound::REGISTRY` 的 `session-open-*` / `sessions-*`）
    //   自动派生的 CLI 面：四条的事实**全在这台**（tmux 名单现探 · 记录在不在 · 铸名经同一张会话快照 ——
    //   `SessionSnapshot::query` 没有「只读缓存」那条路，一次性进程问它一次它就重探一次），
    //   一格都不读本进程的可达表 / 监听状态 / 在飞表 ⇒ 不进 `stream::cli::STREAM_ONLY`，照派生规则上 CLI 面。
    //   登记在本表的理由与上面那几族逐字相同 —— `is_query_mode` 那道闸门读的就是本表，不在表里 ⇒ 当未知 flag 静默进流模式。
    // ⚠ 是新子命令 ⇒ `build_id_guard` 红是预期的，BUILD_ID 打版本号时统一 bump。
    "--session-open-dir",
    "--session-open-facts",
    // 起会话 · 恢复 · 接回一个请求（`session-open`）派生的 CLI 面，理由同上。⚠ 新子命令 ⇒ BUILD_ID 打版本号时统一 bump。
    "--session-open",
    // 往会话说一句（`session-say`）派生的 CLI 面（stdin 一段 JSON 当 `args`）。⚠ 新子命令 ⇒ BUILD_ID 打版本号时统一 bump。
    "--session-commands",
    "--session-model",
    "--session-permission",
    "--session-say",
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
    // 开终端那一串（`terminal-ssh`）派生的 CLI 面：原先在 `stream::cli` 那张「只有界面用得着」的表里，
    //   复核后放出（那一串由入参与这台的 ssh 客户端算出）。⚠ 新子命令 ⇒ BUILD_ID 打版本号时统一 bump。
    "--terminal-preview",
    "--terminal-ssh",
    "--terminals-list",
    "--tmux-notify",
];

/// `ccm --ccm-probe` 自报的能力：没有 tmux 的平台档上摘掉载体是 tmux 的那几条（`control::ccm::CCM_TMUX_CARRIED`）。
/// 平台档是入参，本机是 Linux 也能算出「Windows 那一份会吐什么」。
pub(crate) fn ccm_launcher_with(p: crate::platform::tmux::TmuxPlatform) -> Vec<&'static str> {
    let no_tmux = crate::platform::tmux::tmux_present(p, None) == Some(false);
    control::ccm::CAPABILITIES
        .iter()
        .copied()
        .filter(|c| !(no_tmux && control::ccm::CCM_TMUX_CARRIED.contains(c)))
        .collect()
}

#[allow(clippy::items_after_test_module)]
#[cfg(test)]
#[path = "../../tests/backend/target_parity_guard.rs"]
mod target_parity_guard;

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
// 它们不是「分派」，是 hello 帧的**内容**。`main_fourth_face_tests` 用 `super::` 引它们。
/// `K-P4`（09-04）：命令级 code —— **「这台机器上没有 tmux」**。
///
/// ⚠ 这是这个字面量在仓里的第 N 份，但它**不是第 N 个源头**：真相是
/// `inbound::REGISTRY` 里各条命令自己登记的 `codes`，本常量只拿它去**查那张表**。
/// 查不到就红（`the_declared_code_is_one_the_registry_already_declares`）⇒
/// 谁把那边的拼写改了，这边不会静默跟丢。
pub const NO_TMUX: &str = "no_tmux";

/// 命令级 code —— **「这个平台没有 unix 权限位」**。
///
/// 只有 `files-chmod` 声明它（`control/files_write.rs::change_mode_reporting` 在非 unix 上回它）。
/// 与 [`NO_TMUX`] 同形：真相是 `inbound::REGISTRY` 里命令自己登记的 `codes`，本常量只拿去查那张表；
/// `hello.unavailable` 经 [`unix_mode_unavailable`] 读它。
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
