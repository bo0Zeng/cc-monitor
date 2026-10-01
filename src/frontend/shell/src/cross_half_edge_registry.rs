//! F20：**两半之间的编译期边**（`include_str!` / `include_bytes!` 跨到对面那一半）。
//!
//! # C2 的反面
//!
//! 定框 C2 说「backend 不是 monitor 的外部依赖，是 monitor 自己的一半」。
//! 它的**反面**是：两半不该在**编译期**互相咬住 —— 一半的源码布局变了，
//! 另一半就编不过，那两半其实是一个不可分割的整体，「一份代码两种承载」只是说法。
//!
//! # 摸底把这件的前提**证伪了一半**
//!
//! 08-04 架构重估报的是「10 条跨 crate `include_str!` 边，其中后端读 monitor 那个
//! 60KB 平铺 `tmux.rs` ⇒ **拆那个文件会让后端编不过**，而后端自称可在目标机原生构建」。
//!
//! 逐条量 + 一次决定性实验（把两条 backend→monitor 的路径打断，再分别跑两条命令）：
//!
//! | 命令 | 打断后 | 说明 |
//! |---|---|---|
//! | `cargo build` | **exit=0，0 error** | ★ **部署路径完全不受影响** —— 目标机原生构建走的就是这条 |
//! | `cargo test --no-run` | exit=101，两条边都被逐字点名 | 判据路径才咬住 |
//!
//! ⇒ **`10` 这个数当时对得上**（monitor→backend 8 · backend→monitor 2），
//!
//! ⚠ **08-06 订正：真实是 11 条**（monitor→backend **9** · backend→monitor 2）。
//! 多出来的那条是 `tmux.rs → observe/watcher.rs`，它**从一开始就在**，
//! 只是路径藏在 `macro_rules! backend_watcher_src` 里，而抽取器只认 `(` 之后紧跟的引号。
//! ⇒ 「10 对得上」这个结论当时是**用一个看不见它的量具**得出的 ——
//! 量具与被量对象一起决定了那个数，而结论只写了数。
//! 但「编不过」说的是 **`cargo test`**，不是 `cargo build`。
//! backend 的 `Cargo.toml` 逐字写的是「Standalone crate, intentionally NOT part of a
//! workspace」—— 那句**没有假**：它讲的是 workspace 成员身份与构建。
//! 真实的代价是另一句、而且**以前谁都没写下来**：
//! **backend 的 `cargo test` 需要旁边那棵 `src/frontend/shell/` 树在。**
//!
//! # 于是这件钉的不是「有几条边」，是**边只许长在判据里**
//!
//! 这些边**无一例外**都是**跨轨对拍**（一侧的判据去读另一侧的源码，断言两侧同形）——
//! （不在散文里写条数：条数会长，而没有任何判据读这句话 —— 那正是「停滞式腐坏」的载体。
//!   真值只有一个家 = 下面的 `CROSS_EDGES` 表，由 `..._sees_both_trees_and_actually_parses` 核对。）
//! 那是本仓最有价值的一类判据，砍掉它们等于砍掉「两侧不许偷偷漂开」这条保障。
//! 所以本模块**不减少边**，它钉两件事：
//!
//! 1. **每条边都要登记**，写清「谁读谁 · 为什么必须编译期读」（发现机制是**遍历两棵树**）；
//! 2. ★★ **没有一条边长在生产段** —— 那才是「部署路径不受影响」这句话的机检形态。
//!    生产段一旦出现跨半边的 `include_str!`，`cargo build` 就真的咬住了，本条当场红。
//!
//! ⚠ **刻意不做的一件**：不拆 monitor 那个 60KB 的 `tmux.rs`。
//! 「文件大」不是拆的理由 —— 那条边读它是为了找一个 12 字符的 needle
//! （`format!("={target}:")`，F01 那条「裸 `-t` 会打到兄弟会话上」的两侧同形）。
//! 真要拆，理由得是具体的架构病，不是行数。已进。

/// 两半之间**每一条**编译期边的登记：`(方向, 读者文件, 被读的文件, 为什么必须编译期读)`。
///
/// # 为什么按「读者 → 被读」这一**对**做键，而不是行号
///
/// 行号是最易腐的键（`ssh_source.rs` 六千行，动一处上面全移位）。
/// 实测每一对都**两两不同**，所以「文件对」是够用且稳定的键。
#[cfg(test)]
const CROSS_EDGES: &[(&str, &str, &str, &str)] = &[
    // ── monitor → backend：monitor 的判据去读后端的源码 ─────────────────
    // `tmux_tests.rs → observe/watcher.rs` 那一条（`TMUX_LS_FMT` 双写点对拍，路径藏在 `backend_watcher_src`〔散文墓碑〕 宏里）
    //   出列：列会话的解析整族搬进后端，格式串只剩后端一个家，那条对拍与那个宏随之删了。
    (
        "monitor→backend",
        "tests/frontend/shell/backend_kill_tests.rs",
        "src/backend/control/kill.rs",
        "创建路径不许铸出后端 kill 形状门拒的名字：字符集的来源必须从后端 `parse_name` 现抠 \
         （本侧手抄一份就成了两侧同源的恒等）。原理由「拒绝文案两侧逐字同形」随 monitor 的杀会话发送端迁到界面退役；\
         同拍删掉的三条边（`backend_launch_tests.rs` → `inbound.rs` · `backend_send_keys_tests.rs` → `launch.rs` · \
         `inbound_client_tests.rs` → `launch.rs`）守的「发出去的字段 / mode 名 == 后端解析器认的」改由跨语言金样 \
         `tests/__fixtures__/tmux-control.golden.json` 钉（后端侧让请求样例过生产解析器，界面侧逐字断言发的就是那一份）",
    ),
    // `mcp_sync_tests.rs` 那两条边随 monitor 那份推拉编排一起删了（编排进了被写那台后端）。
    // 足迹的申报表与判据进了后端 ⇒ 原先 `tool_registry_tests.rs` → 后端 `assets/*`（五条装 / 卸口对拍）·
    //   `config_surface_tests.rs` → `assets/mcp_edit.rs` · `footprint_remote_tests.rs` → `inbound.rs` 那七条成了后端 crate 内部的读，摘了；
    //   反过来长出几条 backend→monitor（申报表去钉 monitor 那一侧真放字节的口，见下）与 monitor→backend（monitor 的判据读申报表）。
    (
        "backend→monitor",
        "tests/backend/footprint/registry_tests.rs",
        "src/frontend/shell/src/sftp.rs",
        "申报表 `backend` / `ccm` 那几行的装 / 卸口在 monitor 放字节那一侧（部署远端后端 · 卸载）：申报的「可装 / 可卸」\
         必须与那两个真有的口逐字签名一致（`KR63D1`，搬家前是 monitor→backend 读 `assets/*` 的那一族，方向跟着申报表翻过来）",
    ),
    (
        "backend→monitor",
        "tests/backend/footprint/registry_tests.rs",
        "src/frontend/shell/src/local_backend.rs",
        "申报的本机 `ccm` 落点末段要盖得住 monitor 真放下去的那个名字（`CCM_ENTRY_WORD` ＋ 可执行后缀），\
         词住 monitor（本机那份由它释放）⇒ 读它的源码现抠（本侧抄一份就成了两侧同源的恒等）",
    ),
    (
        "backend→monitor",
        "tests/backend/footprint/rows_tests.rs",
        "src/frontend/shell/src/sftp.rs",
        "足迹里远端落点那几行必须钉在 monitor 真写的那一处（`declared_destinations_are_pinned_to_the_real_writers`），随判据从 monitor 搬来",
    ),
    // `tests/backend/footprint/rows_tests.rs` → `relay-route-core` 那一条删了：共享 crate 搬出 monitor 包、住 `src/common/`，
    //   不再是「两半之间」的边（共享 crate 两侧都依赖，本表只登记半边对半边）。
    (
        "monitor→backend",
        "tests/frontend/shell/ccm_legacy_tests.rs",
        "src/backend/footprint/registry.rs",
        "旧入口清理（`ccm_legacy`，monitor 放字节那一侧）认的那个落点，足迹里恰有一行、且是「远端 · 旧版放的认出才删」 \
         —— 申报表进了后端，读它的源码才对得上（本侧抄一份就成了两侧同源的恒等）",
    ),
    (
        "monitor→backend",
        "tests/frontend/shell/write_site_registry_tests.rs",
        "src/backend/footprint/registry.rs",
        "写点表里申报成「安装动作」的每一格，点名的工具 id 必须真在足迹申报表里（本表那一半）",
    ),
    (
        "monitor→backend",
        "tests/frontend/shell/write_site_registry_tests.rs",
        "src/backend/agents/claudecode/footprint.rs",
        "同上一条，申报表落在 Claude 布局里的那一半（适配层）",
    ),
    (
        "monitor→backend",
        "tests/frontend/shell/sftp_tests.rs",
        "src/backend/assets/aliases/block.rs",
        "「别名块的真相不住 `sftp.rs`」的正控：同一把尺子量真相今天的住处（那台后端的别名块模块），量不出 ⇒ 尺子瞎了",
    ),
    (
        "monitor→backend",
        "tests/frontend/shell/bind_tests.rs",
        "src/backend/platform/shell/rbind-token-bind.ps1.tpl",
        "令牌握手的写侧（本机后端的开终端前奏模板）写进 await 文件的三个键 == 读侧 `bind.rs::AwaitRequest` 认的：\
         键名只在模板里、结构体只在 monitor（读侧的形状不进共享 crate），两侧对上只能读模板那份文字",
    ),
    (
        "monitor→backend",
        "tests/frontend/shell/quote_singleton_guard_tests.rs",
        "src/backend/platform/shell/dialect.rs",
        "「monitor 零 PowerShell 引号器」那把零命中尺子的正控：同一把尺子量后端那唯一的出口（`ps_literal`）要恰好量出四个引号字符，量不出 ⇒ 尺子瞎了",
    ),
    // `tests/frontend/shell/remote_relay_tests.rs → src/backend/stream/inbound.rs` 那一条退役：远端「用到才起」的脱离中转一族删了（中转只住常驻后端里）。
    // `tests/frontend/shell/apikey_remote_tests.rs → src/backend/stream/inbound.rs` 那一条退役：命令名常量随写臂删了、那条对拍判据随之退役
    //   （monitor 里零处叫得出那条帧命令，由 `creds_store_tests::hx2_the_monitor_names_no_plaintext_key_on_the_way_to_the_backend` 钉零命中）。
    (
        "monitor→backend",
        "tests/frontend/shell/config_tests.rs",
        "src/backend/platform/lock.rs",
        "`config.json` 的跨进程锁（monitor `platform::fs::hold_dir_lock`）与后端第四层那把（`platform/lock.rs::hold`）\
         是两个 crate 各一份的**同一种锁**（两个 crate 没有能放平台原语的共享落点）——「同一种」只能同时读两份源码对拍：\
         unix 都锁目录、Windows 互斥量名字的拼法逐字相同；只读一侧就成了自己跟自己比",
    ),
    (
        "monitor→backend",
        "tests/frontend/shell/creds_store_tests.rs",
        "src/backend/stream/inbound.rs",
        "写 key 改走通道之后，「monitor 生产段零处叫得出明文 key 的写口」那条零命中判据的**正控**要落在真命令表上 —— \
         同一根针（帧命令名 `apikey-key-set`）在后端 `inbound.rs` 的登记里数得到，才说明零命中不是针瞎了",
    ),
    (
        "monitor→backend",
        "tests/frontend/shell/creds_store_tests.rs",
        "src/backend/accounts/upstream_select/file_face.rs",
        "本机凭据文件的写者换成本机常驻后端之后，「monitor 生产段零处够写半边」那条零命中判据的**正控** \
         要落在真写者身上 —— 同一把针在后端那一份写口里数得到，才说明零命中不是针瞎了（合成样本证不了针对准了真写口）",
    ),
    (
        "monitor→backend",
        "tests/frontend/shell/inbound_client_tests.rs",
        "src/backend/stream/inbound.rs",
        "入方向帧的种类与错误码两侧同形",
    ),
    (
        "monitor→backend",
        // 〔步 7c 剖分 2026-09-19〕住址跟着判据搬：
        // 那条 `include_str!` 一直长在 `polling_registry` 的**测试段**里，
        // 而测试段这一轮搬进了 `tests/frontend/shell/polling_registry_tests.rs`。
        // **边的条数一格没变**（16 → 16），只是左端的住址换了。
        "tests/frontend/shell/polling_registry_tests.rs",
        "src/backend/control/tmux_hook.rs",
        "C14 那条登记在案的例外（预信任的等信任框以 shell 字符串形态产出）真实存在的证据 —— \
         它是「零轮询」那条零命中守卫的反向锚点",
    ),
    // 🔴 **〔条 67〕`tool_registry.rs → sidecars/codepicture/acquire.rs`
    //    这条跨半边删了** —— 右边那份源码随 `sidecars/` 整棵走了（2 008 行）。
    //    ⚠ 它当初记的那条道理别丢：「光在闭集里加一行是**申报**，申报会在那一层被掏空之后
    //    照样绿着」⇒ 下次再有「app 自带某个二进制」这类申报，右边仍要去钉真源码。
    // 载荷内核搬进后端：原先 `tests/frontend/shell/payload_tests.rs → identity_tag.rs`（令牌变量名双写点）
    //   与 `tests/frontend/shell/history_tests.rs → ccm/argv.rs · ccm/plan.rs`（本机接回那一句真读得懂）三条边成了后端 crate 内部的读，摘了；
    //   接回那一句的牙换成 `launch_render/launch_cli_parity_tests.rs::every_rendered_ccm_line_is_accepted_by_the_ccm_argv`。
    (
        "backend→monitor",
        "tests/backend/control/launch_render/payload_tests.rs",
        "src/frontend/shell/src/platform/terminal.rs", // 开窗那一跳从 `launch.rs` 搬进壳的平台层
        "「谁给 agent 进程定 env」那张人群闭表（`the_population_that_renders_env_prefixes_for_the_agent_process_is_enumerated`）\
         跨两半：串级那三处住后端（载荷内核 · 本机起会话 · `ccm` 容器路），进程级那一处（开窗那一跳的 `.env(k, v)`）留在 monitor。\
         多一个决定点就多一个能各自答错「这次走不走中转」的地方 —— 只有同时数两半才验得了。",
    ),
    (
        "monitor→backend",
        "tests/frontend/shell/ssh_source_emits_parity.rs",
        // 🔴 对端从 `main.rs` 改成 `lib.rs` —— `EMITS` 那张表按
        // 前置 1 搬进了后端库面。本行是**盘上现打出来的答案**：
        //    搬家当天这条判据逐字报「盘上有 `…/lib.rs`、登记里是 `…/main.rs`」。
        "src/backend/lib.rs",
        "backend 的启动契约（身份清单 / hello）两侧同形",
    ),
    // `ssh_source_f032_idle_tests.rs → src/backend/stream/wire.rs`（`RemovalCause` 字面量双写点）那一条随那份判据删了：
    //   monitor 不再读 `session_removed.cause`（去向由后端会话账本裁成 `session_state`）。
    (
        "monitor→backend",
        "tests/frontend/shell/ssh_source_stream_flag_gate_tests.rs",
        "src/backend/lib.rs",
        "★★〔09-23 新增〕**monitor 发的每一条流模式 flag，         后端都必须认得并剥离** —— `the_stream_flags_monitor_sends_are_all_strippable`。         失效方向是本仓栽过的 §26：老后端把**不认识**的 `--flag` 当成一次性查询、         处理完就退出 ⇒ 无 hello ⇒ monitor 重连 ⇒ **死循环**。远端只剩常驻一形之后，「monitor 发的那几个字面量」住 `remote_resident::attach_line`（attach 行，远端 `listen::attach_flags` 认不得就整条拒）、         「后端认得哪几个」住 `backend::STREAM_FLAGS` —— 两处各写一份，         只有同时读两侧源码才验得了。         ⚠ 如实写它买不到什么：**文本级**（从生产函数体里抠 `push` 的那几个串），         不是真起一个老后端看它会不会退出。",
    ),
    (
        "monitor→backend",
        "tests/frontend/shell/remote_resident_tests.rs",
        "src/backend/control/resident.rs",
        "〔替 HX1 那一行〕**两个期限的先后**：那台停一次的最长时间（`resident::STOP_GRACE_MS ＋ KILL_WAIT_MS`，一次性子命令 `--resident-stop` 里）\
         与 monitor 等那一趟的期限（`dial_host::ONE_SHOT_DEADLINE`）。monitor 等得比它短 ⇒ 那台其实停成了、界面却说超时，**不会报错**；\
         只有同时读两侧才验得了（现抠后端生产段那两个字面量，各恰好一处）。宽限期与排空上限的先后在后端一侧同 crate 钉（`resident_tests`）。",
    ),
    (
        "monitor→backend",
        "tests/frontend/shell/logging_tests.rs",
        "src/backend/stderr_log.rs",
        "**跨 crate 字面量对拍**：宿主交给脱离常驻那条载体的 `CCM_BACKEND_STDERR_LOG` 与后端读的那一个，\
         两边各声明一份；漂了**不会报错** —— 后端当作「没交」、stderr 照旧进 `/dev/null`，设置页那一行永远是「还没有」。\
         ⇒ 只能同时读两侧的源码才验得了（形状同下一行 `CCM_LISTEN_PORT` 那一条）。",
    ),
    (
        "monitor→backend",
        "tests/frontend/shell/local_backend_host_tests.rs",
        "src/backend/stream/listen.rs",
        "★**跨 crate 字面量对拍**：常驻监听口那两个 env 名\
         （`CCM_LISTEN_PORT` / `CCM_LISTEN_TOKEN`）宿主与后端各声明一份，\
         而两边漂了**不会报错** —— backend 会把它当成「没设」走 stdio 那条路，\
         宿主则等在一个永远没人 bind 的口上，日志里只有一句「连不上」。\
         ⇒ 只能同时读两侧的源码才验得了（形状抄 `the_local_origin_is_the_same_string_on_both_sides`）。",
    ),
    // `tests/frontend/shell/sftp_tests.rs` → `tests/backend/build_id_guard.rs` 那一行摘了：序键随部署判定搬进共享 crate（序键今天住契约 crate `deploy-contract`，只升不降那条判定住后端 `control/deploy_plan.rs`），
    //   读历史表的那一格（`hx2_every_build_id_ever_shipped_has_an_order_and_the_history_climbs`）挪到后端 `deploy_plan_tests.rs` ——
    //   序键实现（后端依赖的共享 crate）与历史表同在后端那一半，这条边不再跨。
    // `tests/frontend/shell/search_kou_jing_guard.rs` → `src/backend/observe/search_query.rs` 那一行摘了：口径的家从共享 crate
    //   `search-core` 拆进后端（`observe/search_rules.rs` · `agents/claudecode/text.rs`），「恰一份 · 搜索那一侧只调它」那两道随家搬进
    //   后端 `search_rules_tests.rs`，monitor 那一侧只剩「自己零处」—— 这条边不再跨。
    // 这里原有四条边（`session_outline_tests.rs` / `session_find_tests.rs` 各两条：
    //   大纲清单与会话内查找的**线上词**、monitor 造的 **argv** 两侧同形）。monitor 那一侧的读者（核头尾、造 argv）
    //   随两条命令改走通道一起删了 ⇒ 四条边没有读者了。两侧同形的牙换到了**帧面成品**上：
    //   后端 `read_face_tests::the_frame_products_carry_exactly_the_rows_the_cli_arm_prints`（成品 == CLI 臂中段）
    //   ＋ 跨语言金样（`tests/__fixtures__/session-reads.golden.json`，后端写、TS 解码器读）—— 那两条都不是编译期边。
    (
        "monitor→backend",
        "tests/frontend/shell/dial_host_tests.rs",
        "src/backend/dial/mod.rs",
        "★〔C2 · 09-24 新增 · 改判〕**本侧写的可选格后端真读** —— \
         `dial_host::tests::the_request_hands_over_the_machine_as_is`。写侧是 monitor 的 `dial_host::request` \
         （这台原样的配置 ＋ `command` / `agent_sock` 等可选格），可选格的读侧是后端 `dial::DialRequest` 的字段。\
         失效方向**很安静**：serde 默认忽略未知字段 ⇒ 本侧写错一个键名，后端照样读得动、那一项悄悄变成缺省，两侧各自的判据全绿。",
    ),
    (
        "monitor→backend",
        "tests/frontend/shell/dial_host_tests.rs",
        "src/backend/dial/machine.rs",
        "★〔「一个判定一个家」〕**这台原样的配置交过去的那几格两侧同形** —— \
         `dial_host::tests::the_request_hands_over_the_machine_as_is`。写侧是 monitor 的 `dial_host::request`（`machine` · `saved` · \
         `jump` · `prefer` · `use`），读侧是后端 `dial/machine.rs::resolve`（组拨号请求只在那里）。失效方向同上一行：写错一格名， \
         后端读成缺席（`saved` 缺 ⇒ 固化之后的重连照旧 TOFU · `prefer` 缺 ⇒ 上次赢的那条不排首），两侧各自的判据全绿。",
    ),
    (
        "monitor→backend",
        "tests/frontend/shell/link_mux_tests.rs",
        "src/backend/dial/link.rs",
        "★〔SR1a 09-24 新增〕**monitor 的上行步长 == 后端一块的上限、窗口落在后端肯收的区间里** —— \
         `link_mux::tests::the_chunk_cap_is_the_same_number_on_both_sides`。两侧各写一个数（`LINK_STEP` / \
         `LINK_CHUNK_BYTES`，两棵依赖树，共享常量要一条新依赖）；失效方向：monitor 的块比后端的大 ⇒ \
         后端对每一块回 `invalid_args`、上行整条断；窗口越出区间 ⇒ 后端拒开每一条链路。",
    ),
    (
        "monitor→backend",
        "tests/frontend/filewin/transfer_tests.rs",
        "src/backend/dial/pool.rs",
        "★〔SR1b 09-24 新增〕**窗口一趟拖入起几件，与本机后端那条连接的传输车道 / 通道闸对得上** —— \
         窗口传输那份判据里的 `one_windows_burst_fits_the_transfer_lane_and_never_fills_the_connection`。\
         两个数住两棵依赖树（窗口的 `WINDOW_TRANSFER_LANES` · 后端的 `TRANSFER_LANE_CAP` / `SESSION_CHANNEL_CAP`）；\
         失效方向：窗口起的件数超过车道 ⇒ 多挂的订阅只是白排队；够着整条连接的通道闸 ⇒ 一个窗口把会话与查询饿死。",
    ),
    (
        "monitor→backend",
        "tests/frontend/filewin/cross_copy_tests.rs",
        "src/backend/control/files_commit.rs",
        "★**窗口清 B 那头暂存件用的暂存区 == 后端的暂存区** —— \
         `cross_copy_tests.rs::the_staging_dir_is_the_backend_one`。两个 crate 互相引不到，两侧各写一份 \
         `.cc-monitor/staging`；失效方向：两份漂开 ⇒ 跨机复制半路失败时删错地方，B 那头的暂存件只能等孤儿扫。",
    ),
    (
        "monitor→backend",
        "tests/frontend/filewin/lossy_pull_tests.rs",
        "src/backend/files/mod.rs",
        "★〔V152 新增〕**窗口一块读多少 == 后端 `files-read-chunk` 的上限** —— \
         `lossy_pull_tests.rs::the_chunk_is_the_backend_cap`。两侧各写一个数；失效方向：窗口的块比后端的上限大 ⇒ \
         每一块都被 `bad_args` 拒，非 UTF-8 名永远下不下来。",
    ),
    (
        "monitor→backend",
        "tests/frontend/filewin/chunk_upload_tests.rs",
        "src/backend/control/transfer.rs",
        "★**窗口据以换路的收场码 == 传输台发的那个** —— \
         `chunk_upload_tests.rs::the_mismatch_code_is_the_backend_one`。两侧各写一份 `sftp_home_mismatch`；\
         失效方向：两份漂开 ⇒ SFTP 起始目录不是后端 home 的那台机器上，窗口认不出那一码、不改走块形，上传永远做不成。",
    ),
    // `tests/frontend/shell/subagent_tests.rs` → 后端 `history_query.rs` 那一条边删了：它钉的是本机 exec 那条路
    //   「退出 2 ＋ `unknown argument`」的认法（`local_failure_kind`〔散文墓碑〕），那条路改走 `<local>` 长连接之后
    //   「老后端」由长连接的 `accepts` 当场判（与远端同一个判定），不再读后端 stderr 的措辞。
    // `tests/frontend/shell/history_title_coverage.rs` → 后端 `history_query.rs` 那一条边出列：人群（带 `*title` 字段的 `JsonlRecord`
    //   变体）随记录解释进了后端（`agents/claudecode/schema.rs`），两侧同在后端一棵树里 ⇒ 判据挪去
    //   `tests/backend/agents/claudecode/schema_title_coverage.rs`，不再是跨半边的边。
    // ── backend → monitor（3 → 4 · 4 → 2：两条对拍随「只剩一份」出列 · −1：vendored cc-acct-iso 挪去 `src/shared/` · 2 → 0）：backend 的判据去读 monitor ──
    // 别名块的 PowerShell 模板随别名块进了那台后端：模板挪去 `src/shared/cc.ps1.tpl`（两棵树都不属于、两侧读同一份，
    //   同 `src/shared/ccm-aliases.sh`）⇒ 不是跨半边的边，不登记。
    // `iso_tests.rs` → vendored `cc-acct-iso` 那一条出列：账号库今天由后端自己建（`src/backend/accounts/manage/`），
    //   那份 vendored 工具整棵删了 ⇒ 没有这条边。
    // 这里原先还有两条 backend → monitor：`control/gate_tests.rs` 读 monitor 那份 `gate2-golden.tsv`（金表挪去
    //   `tests/__fixtures__/`：两棵树都不属于、后端与 e2e 两侧读同一份 ⇒ 不是跨半边的边）· `control/launch_tests.rs` 抠 monitor
    //   `tmux.rs` 的 `={target}:`（那份跨轨锚点随 monitor 侧的 Gate 残留删了，精确匹配形只剩后端一个家）⇒ 两条出列（2 → 0）。
    // 这里原先还有两条 backend → monitor：`relay/route_tests.rs` 抠 `payload.rs` 的路由样例 /
    //   凭据文件那一家 / 登记了默认上游的 agent（`KH2B4` 那一族）· `relay/door_tests.rs` 抠 `payload.rs` 的钥匙文件路径（RK1）。
    //   两样都不再有第二份：路由语法与门牌进了共享 crate `relay-route-core`（目标），决策表进了后端上游选择 ⇒ 两条边出列（4 → 2）。
];

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/cross_half_edge_registry_tests.rs"]
mod tests;
