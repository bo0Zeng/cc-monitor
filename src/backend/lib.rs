//! `cc-monitor-backend` 的**库面**。
//!
//! 🔴 **它为什么存在**（前置 1、2）：目标形态是
//! 「`backend-core` 一份，两个宿主 link 同一份」—— 本机 GUI **进程内**直接跑它，
//! 远端仍走那个薄 `main` 壳。今天 link 它的只有那个壳（`main.rs`，`Cargo.toml` 的 `[lib]`）；
//! 本机 GUI 进程内 link 它那一步还没做（monitor 经子进程 / 监听口说话）。
//!
//! 模块声明与烤进字节的身份戳（[`CC_MONITOR_BUILD_STAMP`]）住这里；进程入口住 `stream/entry.rs`；
//! 身份本身（`BUILD_ID` · 名字 · 版本）住 `common/id.rs`。
//! 库面对外只有 [`run`]（`main.rs` 调）· [`b64_encode`]（bench 读）· 身份戳；模块一律私有，
//! 零读者的项 rustc 当场报 `dead_code`（门禁 `deadcode-backend` 那一格数的就是它）。

// 测试段可以直接问系统钟（量具、夹具的时刻）；生产段只许 `common::time::now`（`clippy.toml`）。
// 测试段收尾时丢结果不写理由（清临时目录 · 停替身进程）；生产段丢一个 `Result` 写 `.discard(Why::…)`。
#![cfg_attr(
    test,
    allow(
        clippy::allow_attributes,
        clippy::allow_attributes_without_reason,
        clippy::disallowed_methods,
        clippy::let_underscore_must_use,
        clippy::unused_result_ok,
        reason = "测试段放开禁用名单与吞结果那两条：夹具自己定期限、读写盘、起进程、收尾时丢结果；用不用得到随判据增减，写成 `expect` 会在用不到时报 ⇒ 整包唯一许写 `allow` 的一行"
    )
)]
use backend_accounts as accounts; // 账号域：上游选择 · 账号库 · 登录 · 额度与轮换 · oauth。**不是中转**，不住 relay/（独立的包 `backend-accounts`：只引历史清单 · agents · 宿主口 · 中转宿主 · 帧面零件 · 纯工具 · 平台七层）
#[cfg(test)]
#[path = "../../tests/backend/agent_boundary_guard.rs"]
mod agent_boundary_guard; // S1：通用层不许知道任何 agent 的名字与文件格式（整体 #[cfg(test)]）
#[cfg(test)]
#[path = "../../tests/backend/agent_locality_guard.rs"]
mod agent_locality_guard; // S2：agent 的格式知识只许住 agents/<名>/ + 通用层零个 agent 名字面量（整体 #[cfg(test)]）
use backend_agents as agents; // S2/S3：agent 适配层——每个 agent 一份，装它专属的知识（codex + claudecode；独立的包 `backend-agents`：只引纯工具与平台两层）
#[cfg(test)]
#[path = "../../tests/backend/cross/accounts.rs"]
mod cross_accounts; // 账号域判据里引到上层的那几条（那一层是独立的包、够不到上层）
#[cfg(test)]
#[path = "../../tests/backend/cross/agents.rs"]
mod cross_agents; // agent 适配层判据里引到上层的那几条（那一层是独立的包、够不到上层）
#[cfg(test)]
use test_meters::alloc_probe; // 线程级内存量具（`VmHWM` 是进程级的，会把邻居测试算进来；住 dev 侧的量具包）
#[cfg(test)]
#[path = "../../tests/backend/allocator_guard.rs"]
mod allocator_guard; // 远端 musl 版的全局分配器是 mimalloc，本机 glibc 版与 Windows 版不换（整体 #[cfg(test)]）
use backend_assets as assets; // 后端代管的用户资产（别名 · MCP · skill）：D 组的计算与判定，写经本进程的文件管理面（独立的包 `backend-assets`：只引账号域 · agents · 宿主口 · 拨号 · 历史清单 · 帧面零件 · 纯工具 · 平台八层）
#[cfg(test)]
#[path = "../../tests/backend/build_features_tests.rs"]
mod build_features; // 判据构建与发布构建的 serde_json 特性集相同（`cargo tree --frozen` 现取）
#[cfg(test)]
#[path = "../../tests/backend/build_id_guard.rs"]
mod build_id_guard; // E77：加了子命令必须 bump BUILD_ID（内部整体 #[cfg(test)]，生产构建为空）
#[cfg(test)]
#[path = "../../tests/backend/cap_over_guard.rs"]
mod cap_over_guard; // 读上限的 `over` 与调用处对得上（硬报错不被吞 · 丢弃＋带身份报告的每一臂都说话）
#[cfg(test)]
#[path = "../../tests/backend/cross/assets.rs"]
mod cross_assets; // 资产域判据里引到上层的那几条（那一层是独立的包、够不到上层）
use backend_common as common; // U2：两边都要、又不含平台原语的纯工具（§0.5-6 打掉了「三分够用」那个判断；独立的包 `backend-common`）
use backend_host::ccm_line; // ccm 这一行的词与解析（旗标 · 解析 · 自己叫什么 · 配置的记号）：起会话 · 账号域 · 别名那一族都往下引它（住宿主口那个包 `backend-host`）
#[cfg(test)]
#[path = "../../tests/backend/common/cross_tests.rs"]
mod common_cross_tests; // 纯工具那一层判据里引到上层的那几条（那一层是独立的包、够不到上层）
mod control; // U3：控制面 —— 会改变世界（写盘 / 改 tmux server / 发信号），或产出改变世界的计划
#[cfg(test)]
#[path = "../../tests/backend/cross/dial.rs"]
mod cross_dial; // 拨号那一侧判据里引到上层的那几条（那一层是独立的包、够不到上层）
#[cfg(test)]
#[path = "../../tests/backend/cross/host.rs"]
mod cross_host; // 宿主口与 ccm 行判据里引到上层的那几条（那一层是独立的包、够不到上层）
use backend_dial as dial; // SSH 拨号的后端那一侧 —— 钥匙 · known_hosts · 转发的账 · 问远端后端 · 开终端（独立的包 `backend-dial`：只引帧面零件 · 纯工具 · 平台三层）
mod faces; // 帧面宿主：read_face · feature_face · fork_face · resync_face（薄壳，本体在 observe/ · control/）
use backend_files as files; // 文件管理那一块：只读族（常驻文件名索引 ＋ 列 · 读 · 找 · grep）＋ 写面（独立的包 `backend-files`：只引纯工具与平台两层）
#[cfg(test)]
#[path = "../../tests/backend/cross/files.rs"]
mod cross_files; // 文件管理那一块的判据里引到上层的那几条（那一块是独立的包、够不到上层）
#[path = "../../tests/backend/flow_scripts_guard.rs"]
mod flow_scripts_guard; // flow3：流程工具（门禁入口 · 挑格 · 重活隔离 · 推候选分支）在仓里、能跑、不写家目录（整体 #[cfg(test)]）
mod footprint; // 〔RM1a → MIG-3b 续〕「足迹」：帧面 `footprint-report`（申报表 ＋ 判定 ＋ 这台的 stat，出整份成品；只读）
use backend_history as history; // 历史清单那一族：项目 / 会话清单 · 注解 · 历史页那一列 · 组 · 会话输入队 · 判活那张表（独立的包 `backend-history`：只引 agents · dial · host · 帧面零件 · 纯工具 · 平台六层）
use backend_host as host; // 宿主口：终端住在哪种宿主里（tmux · 托管），起 tmux 进程只在 `host/tmux.rs`（独立的包 `backend-host`：只引 agents · 帧面零件 · 纯工具 · 平台四层）
#[cfg(test)]
use test_meters::golden; // 金样判据只比不写；现打的落 `.build/golden/`，由 `tests/scripts/bless-golden.sh` 拷回（住 dev 侧的量具包）
#[cfg(test)]
#[path = "../../tests/backend/layering_guard.rs"]
mod layering_guard; // U3：§1.1 第二条解耦线的机器判据（observe↔control 方向与条数）
#[cfg(test)]
use backend_history::lineage; // 会话血缘：谁起的谁（中转那一处认，只存 `~/.cc-monitor/lineage.json` 一份；住历史清单那个包 `backend-history`；生产段的读者都在账号域里，主 crate 只剩判据读它）
#[cfg(test)]
#[path = "../../tests/backend/no_timer_guard.rs"]
mod no_timer_guard; // P6：零定时器护栏（内部整体 #[cfg(test)]，生产构建为空）
mod observe; // U3：观测面 —— 读，不改变世界
mod plan; // 计划（planned-build）的读面：找 pb、跑 `pb dump`、加工成界面排版的成品、盯计划仓推 `changed {plan}`（只读，一个字节都不写）
use backend_platform as platform; // U2：唯一允许平台原语与平台 cfg 的层（§1.1 第一条解耦线；独立的包 `backend-platform`）
#[cfg(test)]
#[path = "../../tests/backend/platform/cross_tests.rs"]
mod platform_cross_tests; // 平台层判据里引到上层的那几条（平台层是独立的包、够不到上层）
#[cfg(test)]
#[path = "../../tests/backend/platform/fs/own_bytes_golden.rs"]
mod platform_own_bytes_golden; // 经上层各写口写几份自有文件、盘上字节与金样逐字比（平台层的 `Own` 那一套）
use backend_plugin as plugin; // 插件通用调用口 —— 找它 / 传 argv 起它 / 问它会什么（独立的包 `backend-plugin`：只引平台层）
#[cfg(test)]
#[path = "../../tests/backend/plugin_walk_fixture.rs"]
mod plugin_walk_fixture; // 最小假插件走通全流程（夹具 + 判据，整个文件级 cfg(test)，生产构建为空）
#[cfg(test)]
#[path = "../../tests/backend/protocol_doc_gen.rs"]
mod protocol_doc_gen; // 协议参考 IPC-COMMANDS.md 由它从代码生成；判据 = 重新生成 == 仓里那份
#[cfg(test)]
#[path = "../../tests/backend/protocol_doc_guard.rs"]
mod protocol_doc_guard; // U6a：IPC-PROTOCOL.md 与真实协议面的对拍
#[cfg(test)]
#[path = "../../tests/backend/ratchet_guard.rs"]
mod ratchet_guard; // 动过的那几张登记表，**断言那几行**逐字没动（整体 #[cfg(test)]）
#[cfg(test)]
#[path = "../../tests/backend/readonly_guard.rs"]
mod readonly_guard;
#[cfg(test)]
use platform::tmux_sandbox; // 判据用的私有 tmux server（`-L` ＋ 私有 TMUX_TMPDIR，碰不到用户的默认 server；住平台层的 `test-support`）
#[cfg(test)]
#[path = "../../tests/backend/ts_export_guard.rs"]
mod ts_export_guard; // ts-rs 生成物只落在登记的目录、同名只许一个出处（内部整体 #[cfg(test)]，生产构建为空）
use backend_relay as relay;
#[cfg(test)]
pub(crate) use ssh_dial::refusing_port; // 拨了必被拒、又不会被别人捡走的本机口（测试用；住拨号核心，经它的 `test-support`） // 中转的宿主：绑口 · 钥匙 · 起中转（中转本身是通信层 crate `comms_outward`；独立的包 `backend-relay`：只引纯工具与平台两层）
#[cfg(test)]
#[path = "../../tests/backend/cross/relay.rs"]
mod cross_relay; // 中转宿主判据里引到上层的那几条（那一层是独立的包、够不到上层）
#[cfg(test)]
#[path = "../../tests/backend/runs_guard.rs"]
mod runs_guard; // 子运行：通用层只认「运行」（扫描 ＋ 假适配层与 Claude Code 两套形状跑同一批判据）
#[cfg(test)]
#[path = "../../tests/backend/single_stream_guard.rs"]
mod single_stream_guard; // 多客户按连接各一份 —— 「源码里恰好一份、按连接实例化」那几处的触发器（整体 #[cfg(test)]）
use backend_stderr_log as stderr_log; // 脱离常驻那条载体的 stderr 落进一份有上限、滚动的文件（宿主交 `CCM_BACKEND_STDERR_LOG` 才接；后端自有状态，经 `Own`，写口只从 main.rs 进）
mod stream; // 进后端的口（最上层）：进程入口 · 命令表 · 帧面与 CLI 面的分派 · 门 · 期限 · 出口声明 · tap（谁都不引它，只 lib 接）
pub use stream::entry::run; // 二进制那一行（`main.rs`）：进程的整个身子
pub use wire::b64_encode; // bench `s7_history_read` 读它（crate 外读者）
#[cfg(test)]
#[path = "../../tests/backend/test_home_tests.rs"]
mod test_home_tests; // `src/common/test-home` 自己的行为（换家 · 摘会话变量 · 临时目录照旧）：子进程那几条要真测试二进制
use backend_wire as wire; // 帧面的协议零件：帧 · typed 命令 · 登记形状 · 复制详情 · 主题表 · 监听判定 · 收场闸（独立的包 `backend-wire`：只引 agents · relay · 纯工具 · 平台四层）
#[cfg(test)]
#[path = "../../tests/backend/cross/wire.rs"]
mod cross_wire; // 帧面零件判据里引到上层的那几条（那一层是独立的包、够不到上层）

/// 🔴 **这一份后端二进制自己带着的身份**（`<开>` ＋ `common::id::BUILD_ID` ＋ `<关>`，拼法 [`common::id::build_stamp`]）。
///
/// 拿到一份字节（内嵌的 / 装出来的 / 推到远端的那份都算），**不看它旁边任何文件**，
/// 搜 `deploy_contract::STAMP_OPEN` 就问得出它是谁（扫法 `deploy_contract::identity_of_bytes`）。消费者：
/// monitor 构建期（内嵌字节自报的 id 就是 monitor 的「我这一版」）· monitor 运行期（推远端之前的见证）·
/// 本 crate 的部署计划（落点那一份是谁）与 `build_id_guard` 的自扫判据。
///
/// 住主 crate 而不进纯工具那一层：它是这份二进制的属性，不靠「库里的 `#[used]` 一定被链进来」。
/// `#[used]` ＋ `#[no_mangle]`：前者挡「没人读它就优化掉」，后者让它在符号表里也留个名
/// （`strip` 之后符号没了，**数据还在** —— 判据扫的是数据不是符号）。
#[used]
#[no_mangle]
pub static CC_MONITOR_BUILD_STAMP: [u8; common::id::BUILD_STAMP_LEN] = common::id::build_stamp();

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

#[cfg(test)]
#[path = "../../tests/backend/target_parity_guard.rs"]
mod target_parity_guard;

/// 看的那一台的时区（[`TZ_FLAG`] · 请求信封 `tz` 读出来的那一格）。
pub(crate) use common::time::Tz;

/// 一行线上字里复制详情那一行时刻的空位，按看的那一台的时区填好（流的写者对每一行调它；本体 `common::time::fill_at`）。
pub(crate) fn stamp_detail_slots<'a>(line: &'a str, tz: &Tz) -> std::borrow::Cow<'a, str> {
    common::time::fill_at(line, tz)
}

#[cfg(test)]
#[path = "../../tests/backend/main_fourth_face_tests.rs"]
mod fourth_face_tests;

#[cfg(test)]
#[path = "../../tests/backend/one_clock_guard.rs"]
mod one_clock_guard;

#[cfg(test)]
#[path = "../../tests/backend/main_stream_flag_tests.rs"]
mod stream_flag_tests;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本 crate 的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
