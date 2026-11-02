//! `cc-monitor-backend` 的**库面**。
//!
//! 🔴 **它为什么存在**（前置 1、2）：目标形态是
//! 「`backend-core` 一份，两个宿主 link 同一份」—— 本机 GUI **进程内**直接跑它，
//! 远端仍走那个薄 `main` 壳。今天 link 它的只有那个壳（`main.rs`，`Cargo.toml` 的 `[lib]`）；
//! 本机 GUI 进程内 link 它那一步还没做（monitor 经子进程 / 监听口说话）。
//!
//! 模块声明、身份那一块（`BUILD_ID` ＋ 戳）、argv 的选项词与流旗标住这里；进程入口住 `stream/entry.rs`。
//! 库面对外只有 [`run`]（`main.rs` 调）· [`b64_encode`]（bench 读）· `BUILD_ID` · 身份戳；模块一律私有，
//! 零读者的项 rustc 当场报 `dead_code`（门禁 `deadcode-backend` 那一格数的就是它）。
//!
//! 🔴 **`BUILD_ID` 必须住这里，不能留在 `main.rs`** —— in-process 那条路**没有
//! 那个 `main.rs`**，身份会跟着消失。读它源码的只有发版核对那一侧（字节自报 == 这里的 `BUILD_ID`）：
//!   · `.github/workflows/release.yml`（内嵌校验 · 版本对账）
//!   · `tests/scripts/re-embed.sh`（`--check`）
//!   · `tests/backend/build_id_guard.rs`（真身份住址的逐条核对）
//! monitor 不读它：monitor 的「我这一版」是手上那份内嵌字节自报的 id。**别在第四处写它的住址。**

// 测试段可以直接问系统钟（量具、夹具的时刻）；生产段只许 `common::time::now`（`clippy.toml`）。
#![cfg_attr(test, allow(clippy::disallowed_methods))]
mod accounts; // 账号域：上游选择（`resolve` 那张决策表 ＋ 表 ＋ 凭据 ＋ 热重载）＋ `iso`。**不是中转**，不住 relay/
#[cfg(test)]
#[path = "../../tests/backend/agent_boundary_guard.rs"]
mod agent_boundary_guard; // S1：通用层不许知道任何 agent 的名字与文件格式（整体 #[cfg(test)]）
#[cfg(test)]
#[path = "../../tests/backend/agent_locality_guard.rs"]
mod agent_locality_guard; // S2：agent 的格式知识只许住 agents/<名>/ + 通用层零个 agent 名字面量（整体 #[cfg(test)]）
mod agents; // S2/S3：agent 适配层——每个 agent 一份，装它专属的知识（codex + claudecode）
#[cfg(test)]
use test_meters::alloc_probe; // 线程级内存量具（`VmHWM` 是进程级的，会把邻居测试算进来；住 dev 侧的量具包）
#[cfg(test)]
#[path = "../../tests/backend/allocator_guard.rs"]
mod allocator_guard; // 远端 musl 版的全局分配器是 mimalloc，本机 glibc 版与 Windows 版不换（整体 #[cfg(test)]）
mod assets; // 后端代管的用户资产（别名 · MCP · skill）：D 组的计算与判定，写经本进程的文件管理面
#[cfg(test)]
#[path = "../../tests/backend/build_features_tests.rs"]
mod build_features; // 判据构建与发布构建的 serde_json 特性集相同（`cargo tree --frozen` 现取）
#[cfg(test)]
#[path = "../../tests/backend/build_id_guard.rs"]
mod build_id_guard; // E77：加了子命令必须 bump BUILD_ID（内部整体 #[cfg(test)]，生产构建为空）
mod ccm_line; // ccm 这一行的词与解析（旗标 · 解析 · 自己叫什么 · 配置的记号）：起会话 · 账号域 · 别名那一族都往下引它
use backend_common as common; // U2：两边都要、又不含平台原语的纯工具（§0.5-6 打掉了「三分够用」那个判断；独立的包 `backend-common`）
#[cfg(test)]
#[path = "../../tests/backend/common/cross_tests.rs"]
mod common_cross_tests; // 纯工具那一层判据里引到上层的那几条（那一层是独立的包、够不到上层）
mod control; // U3：控制面 —— 会改变世界（写盘 / 改 tmux server / 发信号），或产出改变世界的计划
mod dial; // SSH 的一切 —— 握手 · 连接池 · 链路（**只此一处**，判据在它自己的测块）
mod faces; // 帧面宿主：read_face · feature_face · fork_face · resync_face（薄壳，本体在 observe/ · control/）
use backend_files as files; // 文件管理那一块：只读族（常驻文件名索引 ＋ 列 · 读 · 找 · grep）＋ 写面（独立的包 `backend-files`：只引纯工具与平台两层）
#[cfg(test)]
#[path = "../../tests/backend/cross/files.rs"]
mod cross_files; // 文件管理那一块的判据里引到上层的那几条（那一块是独立的包、够不到上层）
#[path = "../../tests/backend/flow_scripts_guard.rs"]
mod flow_scripts_guard; // flow3：流程工具（门禁入口 · 挑格 · 重活隔离 · 推候选分支）在仓里、能跑、不写家目录（整体 #[cfg(test)]）
mod footprint; // 〔RM1a → MIG-3b 续〕「足迹」：帧面 `footprint-report`（申报表 ＋ 判定 ＋ 这台的 stat，出整份成品；只读）
#[cfg(test)]
use test_meters::golden; // 金样判据只比不写；现打的落 `.build/golden/`，由 `tests/scripts/bless-golden.sh` 拷回（住 dev 侧的量具包）
mod history; // 历史清单与注解（history_list · history_annotations）
mod host; // 宿主口：终端住在哪种宿主里（tmux · 托管），起 tmux 进程只在 `host/tmux.rs`；control · observe 都引它，它不引那两层
#[cfg(test)]
#[path = "../../tests/backend/layering_guard.rs"]
mod layering_guard; // U3：§1.1 第二条解耦线的机器判据（observe↔control 方向与条数）
mod lineage; // 会话血缘：谁起的谁（中转那一处认，只存 `~/.cc-monitor/lineage.json` 一份）
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
#[path = "../../tests/backend/accounts/manage/side_name_exclusion_tests.rs"]
mod side_name_exclusion_tests; // 平台层造的旁名进不了账号库的共享集（账号域拆成包时挪到 `accounts/manage/layout.rs` 底下）
#[cfg(test)]
use platform::tmux_sandbox; // 判据用的私有 tmux server（`-L` ＋ 私有 TMUX_TMPDIR，碰不到用户的默认 server；住平台层的 `test-support`）
#[cfg(test)]
#[path = "../../tests/backend/ts_export_guard.rs"]
mod ts_export_guard; // ts-rs 生成物只落在登记的目录、同名只许一个出处（内部整体 #[cfg(test)]，生产构建为空）
#[cfg(test)]
pub(crate) use ssh_dial::refusing_port; // 拨了必被拒、又不会被别人捡走的本机口（测试用；住拨号核心，经它的 `test-support`）
mod relay; // 中转的宿主：绑口 · 钥匙 · 起中转（中转本身是通信层 crate `comms_outward`）
#[cfg(test)]
#[path = "../../tests/backend/runs_guard.rs"]
mod runs_guard; // 子运行：通用层只认「运行」（扫描 ＋ 假适配层与 Claude Code 两套形状跑同一批判据）
#[cfg(test)]
#[path = "../../tests/backend/single_stream_guard.rs"]
mod single_stream_guard; // 多客户按连接各一份 —— 「源码里恰好一份、按连接实例化」那几处的触发器（整体 #[cfg(test)]）
mod stderr_log; // 脱离常驻那条载体的 stderr 落进一份有上限、滚动的文件（宿主交 `CCM_BACKEND_STDERR_LOG` 才接；后端自有状态，经 `Own`，写口只从 main.rs 进）
mod stream; // 进后端的口（最上层）：进程入口 · 命令表 · 帧面与 CLI 面的分派 · 门 · 期限 · 出口声明 · tap（谁都不引它，只 lib 接）
pub use stream::entry::run; // 二进制那一行（`main.rs`）：进程的整个身子
pub use wire::b64_encode; // bench `s7_history_read` 读它（crate 外读者）
#[cfg(test)]
#[path = "../../tests/backend/test_home_tests.rs"]
mod test_home_tests; // `src/common/test-home` 自己的行为（换家 · 摘会话变量 · 临时目录照旧）：子进程那几条要真测试二进制
mod wire; // 帧面的协议零件（最底下那一层，只引 platform · common · agents · relay）：帧 · typed 命令 · 登记形状 · 复制详情 · 主题表 · 监听判定 · 收场闸

/// 后端身份（`Hello` 帧的 `build_id`，也烤进二进制的身份戳）：monitor 拿它比内嵌那份的，不同 ⇒ 那台该换后端。
/// 后端发出去的东西变了（子命令集 · 冻结格 · 形状指纹 · 行为）就打一个新的：`tests/scripts/bump-build-id.sh`，再 re-embed。
pub const BUILD_ID: &str = "p11g-terminals-end";

// 身份戳的两个界标住契约 crate（`deploy_contract::STAMP_OPEN` / `STAMP_CLOSE`）：monitor 扫字节用的是同一份。

pub(crate) const BUILD_STAMP_LEN: usize =
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
pub(crate) const fn build_stamp() -> [u8; BUILD_STAMP_LEN] {
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

/// 🔴 **这一份后端二进制自己带着的身份**。
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
pub(crate) const STREAM_FLAGS: &[&str] = &[wire::cli_words::STREAM_FLAG_EXPLICIT];

/// 看的那一台的时区（[`TZ_FLAG`] · 请求信封 `tz` 读出来的那一格）。
pub(crate) use common::time::Tz;
/// 一条分页应答 / 一帧 `session_branch` 至多多少字节（写出方向写完这么多应答就先让实时帧走：`stream/writer.rs::writer_task`）。
pub(crate) use wire::REPLY_PAGE_BYTES;

/// 一行线上字里复制详情那一行时刻的空位，按看的那一台的时区填好（流的写者对每一行调它；本体 `common::time::fill_at`）。
pub(crate) fn stamp_detail_slots<'a>(line: &'a str, tz: &Tz) -> std::borrow::Cow<'a, str> {
    common::time::fill_at(line, tz)
}

/// ② 子命令自己的选项：只在某条子命令（[`stream::cli::is_subcommand`]）之后才有意义，backend 顶层不解释它们。
pub(crate) const SUBCOMMAND_OPTIONS: &[&str] = &[
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
#[path = "../../tests/backend/one_clock_guard.rs"]
mod one_clock_guard;

#[allow(clippy::items_after_test_module)]
#[cfg(test)]
#[path = "../../tests/backend/main_stream_flag_tests.rs"]
mod stream_flag_tests;

// ── 第四面：本机到底缺什么（tmux 可用性） ───────────
// 它们不是「分派」，是 hello 帧的**内容**。`main_fourth_face_tests` 用 `super::` 引它们。
/// 命令级 code —— **「这台机器上没有 tmux」**。
///
/// ⚠ 这是这个字面量在仓里的第 N 份，但它**不是第 N 个源头**：真相是
/// `inbound::REGISTRY` 里各条命令自己登记的 `codes`，本常量只拿它去**查那张表**。
/// 查不到就红（`the_declared_code_is_one_the_registry_already_declares`）⇒
/// 谁把那边的拼写改了，这边不会静默跟丢。
pub(crate) const NO_TMUX: &str = "no_tmux";

/// 命令级 code —— **「这个平台没有 unix 权限位」**。
///
/// 只有 `files-chmod` 声明它（`files/write.rs::change_mode_reporting` 在非 unix 上回它）。
/// 与 [`NO_TMUX`] 同形：真相是 `inbound::REGISTRY` 里命令自己登记的 `codes`，本常量只拿去查那张表；
/// `hello.unavailable` 经 [`unix_mode_unavailable`] 读它。
pub(crate) const NO_UNIX_MODE: &str = "no_unix_mode";

/// unix 权限位那一维：这台（或那个 target）没有 unix 权限位 ⇒ 声明会回 [`NO_UNIX_MODE`] 的命令做不到。
/// 与 tmux 那一维同形：谁声明会回那个码，谁就依赖那个机制，从 `inbound::REGISTRY` 的 `codes` 现推。
pub(crate) fn unix_mode_unavailable(unix_mode_bits: bool) -> Vec<wire::Unavailable> {
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

/// 这台机器上**做不到**的命令 —— **纯判定那一半**（不碰世界 ⇒ 可拿合成读数驱动）。
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
pub(crate) fn unavailable_from(tmux: Option<bool>) -> Vec<wire::Unavailable> {
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

/// 生产入口 —— hello 那一帧填的就是它（真填：`stream/entry.rs::build_hello`）。
/// 两维：tmux（[`unavailable_from`]）· unix 权限位（[`unix_mode_unavailable`]，这份二进制是不是 unix）。
///
/// 🔴 **这一趟探测的结果会被用很久。**
/// `build_hello` 在分档**之前**只调一次，那一帧随后交给两条载体；常驻那条（`listen.rs`）
/// 服务**不限次**的「只读 hello 就走」⇒ **同一帧被这个进程后续的所有连接共用**。
/// ⇒ 探测本身必须**便宜且挂不住**（所以 `tmux_in` 是纯 `stat` 扫 `PATH`，不是真 exec 一次），
/// 而消费侧必须把它当**提示**（`wire.rs` 那个字段头注的口径③）。
pub(crate) fn unavailable_here() -> Vec<wire::Unavailable> {
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
