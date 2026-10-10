// 🔴 〔搬树 2026-09-18〕**ts-rs 的 `export_to` 是三级 `../`，不是两级。**
// 本 crate 从 `<repo>/src-tauri` 搬到 `<repo>/src/frontend/shell` ⇒ 到仓根多了一级。
// 漏改的症状**不是编译错**：ts-rs 会把全部类型写进一个叫 `<repo>/src/src/generated`
// 的**文件**（因为那个目录不存在），而 `src/frontend/ui/generated/*.ts` 那 86 份**从此不再更新**
// ⇒ CI 那条「生成物必须最新」会红，而本机什么都不响。
// 现打：32 个文件 / 84 处属性，全在 `src/frontend/shell/src/*.rs` 同一层。
//
// 🔴 **结尾那个 `/` 也是承重的**：ts-rs 12 把不带斜杠的 `export_to` 当**文件路径**。
// 重组那一趟把两件都改坏了：① 少一级 `..`；② **结尾斜杠被吃掉**
// （重组前逐字是 `"../../src/frontend/ui/generated/"`）。
// 两个错叠在一起恰好**不报错**：`src/src/generated` 那个目录不存在 ⇒ ts-rs 安静地
// 建了一个同名**文件**，把全部类型塞进去。只补 `..` 不补斜杠则当场 `Is a directory`（84 条红）。
// ⇒ 这一格的教训：**"改完能编过"不等于改对了** —— 这两处都不是编译期能看见的。
//! 库 crate 根：模块声明 + Tauri 应用装配。
//!
//! `run()` 在 `tauri::Builder` 之前先 `logging::init`（tracing 全局 dispatcher 必须最先 init），
//! 然后注册 single-instance plugin（须为链上第一个）、`setup()` 里起本机内容消费者 / 各后台线程
//! 并 `app.manage` 所有 Arc-shared State，最后注册 `invoke_handler`（IPC 命令清单）。
//! State 都在 `setup()` 里 `app.manage`；漏 `manage` 不会被 cargo check 抓住（INVARIANT § 8）。

// `acct_iso_deploy`〔散文墓碑〕删了：账号库今天由那台后端自己管（`src/backend/accounts/manage/`），不再部署外部工具。
// `adapter`（monitor 那一份 agent 适配表 ＋ 画像生成器，`adapter/claude_code.rs` · `adapter/codex.rs`）删了：
//   起会话事实只住后端适配层（`src/backend/agents/<名>/resume.rs`，注册表 `Adapter.launch`），monitor 读它生成的
//   `src/frontend/ui/generated/agent-profile-table.ts`（起前清洗那一格，[`nested_env_markers`]）。
mod asset_sync; // 资产目录同步：连上那一刻把「怎么够到那台」交给本机常驻后端 `assets-sync`（零判定；看机器页那一问界面直问）
mod auto_launch;
// 🔴 `origin` 归一的地基：「这一趟问的是哪台机器」的唯一类型。
mod backend_policy;
// 通信层面 A 是 crate `comms_inward`（住 `src/comms/inward/`）：`origin` · 分流 · SSH 链路读应答这里再导出、路径不变；
//   `origin` 那份判据（含 monitor 这一侧的「origin 归一」棘轮）照旧挂在 monitor 里（`origin_tests` 见下）。
use crate::detail::Said;
use comms_inward::{backend_route, origin, ssh_link};
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/backend_route_senders_tests.rs"]
mod backend_route_senders_tests; // 每个走后端的发送端都经那一份分流规则（遍历 monitor 源码树）
#[cfg(test)]
#[path = "../../../../tests/comms/inward/origin_tests.rs"]
mod origin_tests; // P2s（C8）：每台机一份后端策略（生效值住内存，持久化归前端）
                  // 🔴 〔归属 2026-09-19〕**它搬不进 `backend/`** —— `backend_policy_tests.rs::
                  //    the_supervisor_itself_never_records_a_death` 逐字：「`backend/` 的生产段里
                  //    出现了 `record_death(` ⇒ 判与记该在**宿主层**，`backend/` 那半**只搬证据**」。
                  //    而 `record_death` 的唯一定义就在本模块里。⇒ 这是**解耦**的活，不是改名一刀能搬的。
mod bind;
mod ui_contract;
// 要让用户知道的出错：壳推给界面只这一种（码 ＋ 文案键 ＋ 那句话 ＋ 复制详情），日志行不上屏。
mod ui_error;
// 通信层面 A 的第一个进程外客户端那条路（末尾「面 A 的第一个外部客户端：通道」）。
// `pub` 同 `filewin`：它的客户端那一半给另一个二进制（外部前端）经 `monitor_lib::chan` 用。
mod cc_bus_deploy; // PS1：把内嵌的 cc-bus 装到 <claude_dir>/skills/（U10b 裁「开」后落地；只读铁律第 7 条例外）
pub mod chan;
mod config;
// `mod config_surface;` · `mod footprint_remote;` · `mod tool_registry;`〔散文墓碑〕（足迹的申报表 ＋ 判定 ＋ 远端事实两趟问法）整族进了后端
//   （`src/backend/footprint/`，帧命令 `footprint-report`）；monitor 只剩它自己那台那几行的事实。
mod data_paths;
mod footprint_client; // 「足迹」里 monitor 自己那台那几行（`HostScope::Client`）只有 monitor 知道的事实：它自己进程的家目录 · agent 家 · PATH（stat 在本机后端）
                      // U-CC1：数据面漂移记账 —— 把「CC 变了」从不可观测变成看一眼就知道。只记账，零行为变化。
mod app_restart; // 设置窗「现在重启」：重起 cc-monitor 自己
mod clipboard; // 写系统剪贴板那一条命令（回真成败）：全产品的复制只这一口
mod desktop_notify; // 系统通知那一条命令（平台那一半在 platform/notify.rs）
mod diagnostics_report; // 日志页「复制诊断信息」：一个命令出整段诊断文本
mod drift_ledger;
mod event_replay;
// 🔴原生文件管理窗口。进程形态＝**同进程**、
// egui 事件循环住次线程；住址为什么是 monitor 的一个模块而不是新 crate——两条理由
// （同进程要链进这个二进制 · 门禁 `cargo` 格把包数恒等钉在 9）逐条写在它的头注里。
// ⚠ **`pub` 是刻意的**：本 crate 的 `mod` 全是私有的，而私有模块里没人调的 `pub fn`
//   会被 `dead_code` 记一笔 —— 门禁 `deadcode` 那一格把 `never used` **恒等钉在 36**（34 → 36，逐条住 gate.sh 那一格上方）。
//   这棵树今天的消费者只有它自己的判据（窗口还没接到界面上），`pub` 让它在
//   rlib 的公开面上可达 ⇒ 不往那个 34 上加数。**等窗口真被界面调起来，这里可以收回私有。**
pub mod filewin;
mod history;
// `hooks_diag`〔散文墓碑〕（cc-bus 钩子诊断）进了后端：帧命令 `hooks-diag`（`src/backend/observe/cc_bus_hooks.rs`），界面经通道直问那台。
// U8a-2a：monitor 侧的入方向发送端（往那条长连接的写半边发命令 + 按 id 收应答）。
// 「hello 之前不许写」在这里是类型上的事实：ParkedWriter 身上没有任何写方法。
// `apikey_remote`〔散文墓碑〕删了：它最后只剩发送口，唯一的调用方（起会话那一侧问 `launch-endpoint`）随本机起会话搬进后端。
// `backend/` 这个目录从壳里消失了（从前住着 monitor 侧「后端边界」的九份：它们是 monitor 自己的代码，名字却叫 backend）。
//   逐份回真住址（都在壳根，宿主无关 ＋ 平台无关两道判据照旧看着它们：`backend_client_guard_tests.rs`）：
//   调后端的客户端 —— `inbound_client`（长连接入方向的 wire 客户端）· `frame_query`（只读查询发送端）· `backend_route`（通信层成员，
//   分流器，住 `src/comms/inward/`）；Tauri 命令层 —— `backend_control`（起 / 停 / 状态）· `cc_bus`（两句说法与 id 规则）；
//   宿主 —— `local_backend`（本机后端的起与看住）。跨轨对拍锚点 `agent_profile_parity`〔散文墓碑〕 随 monitor 那份适配表删了（对拍随家进了后端 `agents_tests.rs`）。
//   Gate 1 前检 `tmux` 与 monitor 那一轨 `gate2_parity` 删了（子步 1：门只在后端）。
mod backend_control;
mod cc_bus;
mod frame_query;
mod inbound_client;
mod local_backend;
// 标识符放行判定的生成物（`src/frontend/ui/generated/judgment-rules.ts`）与共用金样（`INVARIANTS §47` ①）：只读共享 crate 的常量。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/payload_judgment_rules.rs"]
mod judgment_rules;
// 「创建路径不许铸出主路杀不掉的名字」与它的发现口径（原挂 `backend/control/` 那一层的测试段，改挂壳根）。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/backend_kill_creation_detect.rs"]
pub(crate) mod creation_detect;
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/backend_kill_tests.rs"]
mod kill_name_tests;
// 从前 `backend/` 目录那两道判据（宿主无关 · 平台无关）改看上面那一组文件（逐个点名，不再按目录）。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/backend_client_guard_tests.rs"]
mod backend_client_guard;
// monitor 生产段只许依赖契约类共享 crate。
mod byte_table; // 全仓唯一的取字节口：一台机器要哪一份可执行字节，按它的 (OS, arch) 查表
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/contract_crate_guard_tests.rs"]
mod contract_crate_guard;
mod copy_table; // 对外文案表的 Rust 读口（与前端 `copyText` 同一份 `src/shared/copy/table.json`）
mod creds_store; // 第三方 API key 那份文件：monitor 这一侧零读零写零交路径（本机常驻后端按家推、写、答）—— 只剩判据
pub(crate) mod detail; // 壳这一端写的「复制详情」那几行 ＋ 壳命令失败的那一形 `Said`
#[cfg(test)]
mod guard_support; // 住址唯一源（仓根/源码树/测试树）——头注写着它为什么存在
mod launch;
mod local_backend_host; // P2s（C8）：本机后端的生命周期（起/停/状态）——命令不能与 IPC 命令清单同模块，理由见该模块头注
mod local_origin_registry;
mod logging;
mod machine_state; // 每台机器的状态成品（连着 · 没连上与原因码 · 版本关系 · 修法），`backend_status` 的 `machine` 一格
                   // `stop_grace`〔散文墓碑〕删：「请它收尾 → 等 → 强杀」搬进那台机器上的一次性子命令 `--resident-stop`（后端 `control/resident.rs`）。
                   // `mod messages;` · `mod parser;` · `mod codex_record;`〔散文墓碑〕（记录解析）整族搬进了后端
                   //   `agents/claudecode/`（`schema` · `parse`）与 `agents/codex/record.rs`：monitor 只把后端给的成品原样转交。
mod platform; // C10：平台相关的 fs 原语的唯一住址，注入给平台无关的 backend
              // `plugins` 模块（P8a 的 marketplace 只读枚举，`list_plugin_marketplaces`〔散文墓碑〕）删了：
              //   后端 `plugins-marketplaces` 直接出成品，界面经通道问（`src/frontend/ui/settings/plugins-section.ts::fetchSurvey`）。
mod profile_installer;
// `pubkey`〔散文墓碑〕（F50 公钥推送）进了本机后端：帧命令 `pubkey-push`（`src/backend/assets/pubkey.rs`），界面经通道直问。
// 分叉的 monitor 这一侧（整个模块）删了：界面经通道直说那台后端 `session-fork`（`src/frontend/ui/session-writes.ts`）。
// `mod remote_history;`〔散文墓碑〕删：最后一个函数（按名字核「这台配置过」）随子 agent 那条命令退役。
mod remote_resident; // 远端常驻后端：起 · 找（`--resident-ensure`）→ 隧道 → 握手；停（`--resident-stop`）
mod remote_write_registry; // devbench F10c：远端写面登记（接三张表各自划出去、然后没人接的那道缝）
                           // `mod search;` 删了：本机全文搜索也问本机后端（`history-search`），monitor 进程内那份内存索引〔散文墓碑〕随之退役。
                           // 会话起停的成品缓存（后端裁、monitor 只转交 ＋ F5 重放）。替掉 `session_map` 本机活会话表〔散文墓碑〕
                           //   与 `session_facts` 容器账〔散文墓碑〕：两本账的裁决与记账都搬进了那台后端（`observe/session_ledger.rs`）。
mod probe_relay; // 本机后端的 `probe` 帧（测试连接的进度格）原样转给界面订的 `probe-progress/<票>`
mod session_book;
mod session_tap;
mod terminal_screen_relay; // 本机 / 远端后端的终端实时预览帧原样转给界面订的 `terminal-screen/<票>` // 本机后端的 `tap` 帧（中转抄出来的 SSE 事件）原样转给前端 `session-tap`
                           // 起子进程的**唯一出口**（三个策略都没有 Default）。
                           // 住宿主知识层是硬的：平台原语进不了 `backend/`（那侧的禁针 + 递减棘轮），
                           // `backend/` 的两个落点收注入参数（`ManagedSpawn`）。
mod spawn_managed;
// devbench F02：skill 接入面（一份声明 + 通用宿主）。
// ⚠ **今天零生产消费者**（UI 归 F03）—— 照 `tool_registry` 的先例如实登记并写处置条件：
// F03 接上之后删掉那个模块级 `#[allow(dead_code)]`；若 F03 收工时它仍零消费者，
// 就该删掉整个模块，而不是让它当装饰。
mod sftp_pool;
// `verified_write`〔散文墓碑〕模块删了：它的判定只剩 `fenced_block::apply`〔散文墓碑〕一个调用方，
//   那个序列删了之后零调用方；用户文件的回读比对只住后端 `files_write.rs::put_text`，部署物按字节比住 `sftp::verify_readback`。
// SS-D 统一 SFTP 写层（issue #29 自动部署 F08；后续 F11/F10 复用）。
mod sftp;
// SSH-remote Phase 0 (issue #15)：从 setup() 调用 —— 当 config.json 的
// `remote.enabled = true` 时，stream_source::run 作为**附加**数据源与本机那条流
// 并行跑（aggregate：本地 + 远端 session 同时显示为 Tab）。本机会话内容也经本模块的
// `LineIntake` / `consume_local` 走同一个出口（flush_lines → batch_to_payloads → on_line_batch_awaited）；
// 远端行带 origin=host 标签。
mod ccm_probe;
mod stream_source;
// 拨号应答的客户端（通信层面 A 的 SSH 链路那一段）。
// `inproc_dial`（界面进程里最后一份 russh 拨号，只剩 SFTP 一个用户）删了：
//   SFTP 进了本机常驻后端（`src/backend/dial/sftp.rs`），**界面进程零 SSH**（用户）。
// 〔C2 → SR1a〕拨号的宿主：配置 → 请求 · 经本机常驻后端开链路 · 链路交给 `ssh_link`。
mod dial_host;
// 链路的 monitor 这一侧：在本机后端那条流上多路复用到各远端的字节流（`link-*`）。
mod link_mux;
// 本机会话内容的入口通道：本机两条读循环把后端的内容帧送进来，交给与远端同一个 `stream_source::LineIntake`。
mod local_lines;
// 三条读帧循环共用的丢帧账（认不出的帧 · 非 UTF-8 行：计数、按 2 的幂次说、流结束出总账）。
mod frame_tally;
// T01：结构性扫描的可复用形式（枚举+逐个断言+计数自检+钉死逃生口）。
// **只在测试期编译**——它的消费者全在 `#[cfg(test)]` 里（`sftp.rs` 的 tmux 目标守卫、
// `tool_registry.rs` 的字段纪律）。这是测试支撑模块，不是被闲置的生产代码；
// 加 `cfg(test)` 就是把这件事写进类型系统，顺带消掉 5 条 dead_code 警告。
// `agent_dispatch_registry`〔散文墓碑〕（桌面侧「通用层认得出某个 adapter」逐条登记 ＋ 递减棘轮）退役：桌面侧没有适配器了，
//   登记的人群清零；「通用层不按名字够某一家」那一条住后端 `agent_locality_guard`。
/// U1a：`shared/ccm` 的强度契约（仅测试构建）。U9 迁移后由同一份 `measure()` 对拍新构造点。
///
/// ⚠ **插在这里、不要插在上面那条注释与 `#[cfg(test)]` 之间。** U1a 初版就插错了位置，
/// 把属性与 `mod structural_scan;` 的配对拆开 —— `structural_scan` 当场变成无条件编译，
/// 上面注释里逐字写着的「顺带消掉 5 条 dead_code 警告」被原样撤销，而 CI 的 `cargo build`
/// 没有 `-D warnings` ⇒ **不会红**。是 Phase D 审计数出「dead_code 正好 +5」才发现的。
#[cfg(test)]
mod ccm_cli_contract;
mod cross_half_edge_registry; // F20：两半之间的编译期边（跨半边 include_str! 逐条登记 + ★ 一条都不许长在生产段）
mod doc_claim_registry; // F11：耐久文档里「描述当下」的字段与代码对拍（状态列逐格登记 + ★ 判据从文档里读那个数，代码里不留第二份）
#[cfg(test)]
mod fixture_guard; //：`tests/__fixtures__/` 里的夹具不许掉光引用变成孤儿（α3 刀 D 那个没红的读数；整体 cfg(test)）
mod frame_cadence_guard; // F01：帧节奏说法的零命中守卫（P5 后后端零定时器；被禁措辞见模块头注）
mod gate_singleton_guard; // F03：§34 Gate 2 的身份判定在 Rust 侧只许有一个家（后端 `control/gate_rules.rs`）

#[cfg(test)]
mod atomic_replace_registry; // audit-0805 F13：原子替换的两套 Win32 语义，谁用哪一套
#[cfg(test)]
mod bus_identity_registry; // cc-bus：拿 id 点名 tmux 前必须核身份（整体 #[cfg(test)]）
mod byte_cap_registry; // audit-0805 F06：字节上限登记表（管什么量 + 超限怎么办 + 跨 crate 对拍）
mod capability_registry;
#[cfg(test)]
mod comm_boundary_registry; // 通信层的边界判据：成员 ＝ 通信层那两个 crate（`cargo metadata` 现取）＋ C1–C5 / X1–X6
#[cfg(test)]
mod dial_home_registry; // K-R74：「解耦干净」改述成三样可判的东西 —— 终点二值旗（russh 在不在界面 manifest 里）+ 过程递减棘轮（还没搬走的拨号处数）+ 拨号锚点的唯一住址（整体 #[cfg(test)]）
mod e2e_gate_registry; // audit-0805 08-08：每一套 e2e 要么进门禁要么登记为什么不进
mod exec_site_registry;
// F10（出口④）：本机读面清账 + 递减棘轮。
// ⚠ 注释写在上一行而不是行尾：行尾放不下，而 `rustfmt`
//   会把跟在带行尾注释那一行后面的注释块**缩进对齐到那一列**
//   （形状同下面 `plugin_class_registry` 那一段）—— 09-10
//   fmt 那道门当场判过一次，别再拉回顶格。
// 〔订正 2026-09-10（v3.7.0）—— 本行原话逐字：
//  「F10（出口④）：本机读面清账 + 递减棘轮（**正题被 F05b 挡着**）」。
//  括号里那句今天不成立：F05b 已经做完并随 v3.7.0 发出去了
//  （09-10 干净 win11 现打，PM：装出来那份跑着 2 个
//  `cc-monitor-backend.exe`、裸 `monitor.exe` 那份 0 个）。
//  今天挡着正题的是后端侧一批有名有姓的缺口，逐条写在
//  那张表**自己每一行**里 —— 这里刻意不抄第二份（定框 E12）。〕
#[cfg(test)]
mod command_home_registry; // Tauri 命令两张封闭表：monitor 自己的事 / 待迁（整体 cfg(test)）
mod local_read_surface_registry;
#[cfg(test)]
mod lockfile_conflict_guard; // audit-0805 F16：两份 lock 的真冲突必须为空（超集不算）
#[cfg(test)]
mod needle_anchor_registry; // audit-0805 F24：匹配单位不许比事实小（F23 的兄弟族）
mod parity_ledger; // L5：本地/远端平价对账表（§40 的机制那半；内部整体 cfg(test)）
                   // EF01（plugin-split）：`E4` 的四候选 × 两轴分类表落成会红的登记表（整体 `#[cfg(test)]`）。
                   // ⚠ 注释刻意写在上一行而不是行尾：本模块有一条判据要断言「生产段里没人消费这张表」，
                   //   而 `lib.rs` 的这行声明是它存在的方式、不是消费 —— 那条判据按**整行相等**放行它。
mod plugin_class_registry;
mod polling_registry; // U7-P：前端 + shared/ccm 的周期唤醒清账（backend 那条零定时器护栏点名要「单独论证」的那半）
mod quote_singleton_guard; // U8c-2b-0：POSIX 单引号 quote 在 Rust 侧只许有一个实现（账本 S5）
mod rust_timer_registry; // F09：monitor **Rust 侧**周期唤醒清账（`polling_registry` 明确留下的那半）
mod scanning_guard_registry; // audit-0805 F23：扫描型判据不许裸遍历（自匹配这一族的收口）
mod session_name_registry; // U11 摸底：会话名产出点清账 + 递减棘轮（账本 S12 的落地形态）
                           // `sftp_move_ledger`〔散文墓碑〕（K-R78：那 14 处 SFTP 拨号今天各自卡在哪 —— 乙为什么没搬 + 甲的四条挡路石）
                           //   **退役**：那 14 处全搬了（界面进程零 SFTP），底账要记的那件事做完了；它的挡路石各自怎么被拆的写在 SR1b 的记录里。
#[cfg(test)]
mod pub_reader_registry; // 后端 · 壳 · 共享 crate 的 `pub` 项要有产品读者（只被测试读算零）
#[cfg(test)]
mod shared_crate_registry; // U8c-1：新增共享 crate 时 CI 三样都要补 —— 从散文变机检
mod shell_lint_registry; // audit-0805 08-08：每个 shell 脚本要么进 shellcheck 要么登记豁免
#[cfg(test)]
mod structural_scan;
// `mod subagent;`〔散文墓碑〕删：子 agent 那一份（列 ＋ 挑 ＋ 读 ＋ 解析）由那台后端按运行读（`history-run`）出成品，界面经通道直问。
// 业务路径零裸吞：每一处 `let _ =` / 语句级 `.ok();` 登记为什么可以丢（整体 #[cfg(test)]）。
#[cfg(test)]
mod swallow_registry;
// TL3（审计 F 🔴-2）：同步 IPC 命令的调用闭包里零 `block_on` / 零同步连后端（`INVARIANTS §10`；整体 cfg(test)）。
//   注释写在上一行：行尾注释会让 rustfmt 把下一行的注释块缩进对齐过去（同 `local_read_surface_registry` 那一段）。
#[cfg(test)]
mod sync_command_registry;
// `mod session_skeleton;`〔散文墓碑〕删：按偏移 / 按行号取正文由那台后端出记录行（`history-page` · `history-lines`），界面经通道直问。
// 大纲清单与会话内查找两个模块（`session_outline` / `session_find`）删了：
//   界面经通道直接说帧命令 `history-user-inputs` / `history-find`，后端出成品（`src/frontend/ui/session-reads.ts`）。
// 远端流断线重连后，旁路快照从续点接着拉（不再从第 0 行整份重拉）。
mod snapshot_resume;
// `tasks` 模块（本机任务 notify ＋ `task-update`）删了：监视进后端，界面经通道订 `changed/tasks`。
mod tmux_backend_gate_guard; // U10 裁决：backend 侧没有身份守卫之前，send-keys/kill 不许改走 backend
                             // `tmux_reconcile`（tmux 存活对账的纯决策）〔散文墓碑〕搬进后端会话账本（`src/backend/observe/session_ledger.rs`）。
mod utils;
mod write_site_registry; // audit-0805 08-07：每个会写用户机器的落点都要申报（关掉 §5 4b 一半） // audit-0805 08-07：每处远端执行都要申报命令来历 // audit-0805 08-08：webview 能力清单 = 三张登记表的共同前提

use crate::copy_table::copy_text;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Listener, Manager};

/// issue #24：清掉从宿主 shell 继承的 claude 嵌套标记。
///
/// Windows 子进程默认继承全部环境。若 monitor 是从「Claude Code 会话内的 shell」
/// 启动的（开发者跑 `run.ps1 dev` 很常见），这些标记会沿 monitor → wt.exe →
/// powershell → `claude --resume` 一路传下去，resume 出的 claude 被嵌套检测判成
/// **子会话** → 不注册 `sessions/<PID>.json`、不写会话 jsonl（对话只活在内存、
/// 关窗即丢）→ monitor 永远不出 Tab。启动时单点清洗，之后 spawn 的一切子进程
/// 都干净。**保留 `CLAUDE_CONFIG_DIR`**（monitor 自己消费它解析数据目录）。
/// 正常启动路径这些变量本就不存在 → no-op 零回归。
///
/// ⚠ 勿把上面"子会话不注册 pidfile"泛化：CC 2.1.x 的 backend **后台任务**
/// (--fork-session) 会写 pidfile（kind:"bg" + jobId）——那类由后端的 kind 交互性过滤处理
/// （Batch6-F21；本机也是本机后端那一份，藏不藏在 `stream_source::local_hides`），与本处嵌套环境清洗无关。
/// 完整排查：src/doc/DEVELOPMENT.md 常见问题节。
///
/// 返回实际清掉的 key（供 caller 在 logging 就绪后留痕——本函数必须在任何线程
/// spawn 之前调用，那时 logging 还没初始化、不能直接打 log）。
fn scrub_env_vars(keys: &[&str]) -> Vec<String> {
    let mut removed = Vec::new();
    for &k in keys {
        if std::env::var_os(k).is_some() {
            std::env::remove_var(k);
            removed.push(k.to_string());
        }
    }
    removed
}

/// 起会话事实的生成物（值的家在后端 `agents/<名>/resume.rs`，`npm run gen:types` 重生成，门禁 `generated` 那一格盯着）。
const AGENT_PROFILE_TABLE_TS: &str = include_str!("../../ui/generated/agent-profile-table.ts");

/// 起前要清的嵌套会话标记：画像表生成物里**每一家**的 `nestedEnvVars`（并集，按出现序去重）。
/// 从前取 monitor `adapter::active()` 那一家（恒是 Claude）；今天不认 agent，每一家的都清（Codex 那一格今天是空的，结果与从前逐字相同）。
/// 只认生成器写出的那一形（`    nestedEnvVars: ["A", "B"],` 一行一格）；认不出一格就少清一格 —— 判据按金样两向钉住它读全了。
fn nested_env_markers() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for line in AGENT_PROFILE_TABLE_TS.lines() {
        let Some(inner) = line
            .trim()
            .strip_prefix("nestedEnvVars: [")
            .and_then(|r| r.strip_suffix("],"))
        else {
            continue;
        };
        for item in inner.split(", ") {
            if let Some(name) = item.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
                if !out.contains(&name) {
                    out.push(name);
                }
            }
        }
    }
    out
}

/// ST1：设置窗的标签（`open_settings_window` 建它时用的同一个串）。
pub(crate) const SETTINGS_WINDOW_LABEL: &str = "settings";
/// 主窗的标签（`tauri.conf.json` 里那一个）。
pub(crate) const MAIN_WINDOW_LABEL: &str = "main";

/// ST1「关窗改隐藏」的生命周期缝：**`destroyed` 这个窗口刚销毁、`alive` 是此刻还在的窗口，
/// 要跟着 destroy 掉哪几个。**
///
/// 设置窗关窗是隐藏（它永不自己销毁）⇒ 主窗销毁时它必须跟着走，否则一个看不见的窗口
/// 会把进程吊住。别的组合一律不动：viewer 窗看得见、关得掉；设置窗自己销毁不牵连谁。
pub(crate) fn windows_to_destroy_after<'a>(destroyed: &str, alive: &[&'a str]) -> Vec<&'a str> {
    if destroyed != MAIN_WINDOW_LABEL {
        return Vec::new();
    }
    alive
        .iter()
        .copied()
        .filter(|l| *l == SETTINGS_WINDOW_LABEL)
        .collect()
}

/// 那扇窗此刻所在的显示器的工作区（类型与判定住 `host_core`，问 Tauri 这一下留在宿主）；问不到 ⇒ `None`（不夹，照原样开）。
pub(crate) fn work_area_of(w: &tauri::WebviewWindow) -> Option<host_core::WorkArea> {
    let m = w.current_monitor().ok().flatten()?;
    let a = m.work_area();
    Some(host_core::WorkArea {
        x: a.position.x,
        y: a.position.y,
        w: a.size.width,
        h: a.size.height,
    })
}

/// 照 [`host_core::fit_into_work_area`] 把一扇 Tauri 窗夹进它所在显示器的工作区（开窗之后调一次；问不到尺寸 ⇒ 不动）。
pub(crate) fn fit_window_to_work_area(w: &tauri::WebviewWindow) {
    let (Some(work), Ok(pos), Ok(outer), Ok(inner)) = (
        work_area_of(w),
        w.outer_position(),
        w.outer_size(),
        w.inner_size(),
    ) else {
        tracing::info!("窗口 {} 问不到尺寸或工作区 —— 不夹", w.label());
        return;
    };
    let Some(((iw, ih), (x, y))) = host_core::fit_into_work_area(
        (pos.x, pos.y),
        (outer.width, outer.height),
        (inner.width, inner.height),
        work,
    ) else {
        return;
    };
    tracing::info!(
        "窗口 {} 夹进工作区 {work:?}：内框 {}×{} → {iw}×{ih}，外框左上 ({},{}) → ({x},{y})",
        w.label(),
        inner.width,
        inner.height,
        pos.x,
        pos.y
    );
    if let Err(e) = w.set_size(tauri::PhysicalSize::new(iw, ih)) {
        tracing::warn!("窗口 {} 缩不进工作区：{e}", w.label());
    }
    if let Err(e) = w.set_position(tauri::PhysicalPosition::new(x, y)) {
        tracing::warn!("窗口 {} 挪不进工作区：{e}", w.label());
    }
}

/// 主窗 / 设置窗：每次起都摆在所在显示器工作区正中（[`host_core::center_in_work_area`]），尺寸照样夹进去。
/// 不摆的话位置交给系统的默认落点，Windows 每开一扇新窗就挪一格 ⇒ 重启一次左移 26 px。问不到 ⇒ 不动。
pub(crate) fn center_window_in_work_area(w: &tauri::WebviewWindow) {
    let (Some(work), Ok(pos), Ok(outer), Ok(inner)) = (
        work_area_of(w),
        w.outer_position(),
        w.outer_size(),
        w.inner_size(),
    ) else {
        tracing::info!("窗口 {} 问不到尺寸或工作区 —— 不摆", w.label());
        return;
    };
    let ((iw, ih), (x, y)) = host_core::center_in_work_area(
        (outer.width, outer.height),
        (inner.width, inner.height),
        work,
    );
    tracing::info!(
        "窗口 {} 摆进工作区 {work:?} 正中：内框 {}×{} → {iw}×{ih}，外框左上 ({},{}) → ({x},{y})",
        w.label(),
        inner.width,
        inner.height,
        pos.x,
        pos.y
    );
    if (iw, ih) != (inner.width, inner.height) {
        if let Err(e) = w.set_size(tauri::PhysicalSize::new(iw, ih)) {
            tracing::warn!("窗口 {} 缩不进工作区：{e}", w.label());
        }
    }
    if let Err(e) = w.set_position(tauri::PhysicalPosition::new(x, y)) {
        tracing::warn!("窗口 {} 摆不到正中：{e}", w.label());
    }
}

// 这里先前是 `D7 阻-3` 那条缝（`ExitShutdownSinks` ＋ 它的收口点）：退出臂按现问的「退出行为」
// 收掉 monitor 另起的那个本机中转。中转并进本机常驻后端之后，本机固定两个进程（monitor ＋ 常驻后端），
// 中转随后端按「退出行为」留或退⇒ 退出臂里不再有第三个进程要收，那条缝连同它的判据一起删掉。

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 启动 perf 测量起点
    let t0 = std::time::Instant::now();

    // issue #24：第一件事就是清嵌套标记——必须在任何线程 spawn 之前
    // （std::env::remove_var 修改进程级环境，单线程窗口内调用才稳妥；
    // 下面 logging::init 就会起 non_blocking writer 线程）。
    let scrubbed_env = scrub_env_vars(&nested_env_markers());

    // v2.0.0 (issue #4)：tracing 初始化提前到 Builder 之前 —— 一旦 init 全局
    // dispatcher 锁死，且我们要捕获 setup() 期间的所有 log。
    //
    // logging 模块内部把所有复杂度（rolling file appender / non_blocking writer /
    // ErrorEmitterLayer / EnvFilter reload）封死，对外只暴露 init + state。
    //
    // **monitor_data_dir 必须能解析**：家目录按两侧同一条规矩（`creds_core::store::home_dir`）推，不依赖任何
    // 配置（避免 log 初始化跟 config 初始化循环依赖）；推不出退到临时目录。
    let monitor_data_dir = config::resolve_monitor_data_dir()
        .unwrap_or_else(|| std::env::temp_dir().join("cc-monitor-fallback"));
    // 数据目录就是 `~/.cc-monitor`（后端的家），本进程里头一个碰它的是下面的日志 ⇒ 先按「只给本人」建好；
    //   建不了不拦启动（`INVARIANTS §15`），日志那一步自己会出声。
    let born_private = platform::fs::ensure_private_dir(&monitor_data_dir);
    let logging_state = logging::init(&monitor_data_dir);
    if let Err(e) = born_private {
        tracing::warn!("{}（{}）", e.said, e.raw());
    }
    tracing::info!(
        "[perf] T+{}ms cc-monitor starting (data_dir={}, log_dir={})",
        t0.elapsed().as_millis(),
        monitor_data_dir.display(),
        logging_state.log_dir().display()
    );
    if !scrubbed_env.is_empty() {
        // issue #24：留痕——宿主 shell 带嵌套标记（从 claude 会话内启动的）。
        // 没这行，"清洗是否真的发生过"无法事后验证。
        tracing::info!(
            "scrubbed inherited claude nested-session env markers: {}",
            scrubbed_env.join(", ")
        );
    }

    // setup 闭包是 FnOnce + 'static，必须 move-capture。把 logging_state shadow 进闭包，
    // 闭包内同时 install_error_emitter（&self 借用）+ app.manage(clone)
    let logging_state = logging_state;

    // issue #9：single-instance lock。**必须是第一个 plugin**（第二个实例在别的插件起来之前就退）。各平台都注册
    // （Windows / macOS：`tauri-plugin-single-instance`；Linux：自己占会话总线上的名字，连同激活令牌一起交 —— `platform/single_instance.rs`）。
    // 第二个 cc-monitor 实例启动 → 本回调在第一个实例里跑 → 主窗口还原 · 显示 · 拉前（带令牌）→ 第二个实例立即退出。详 src/doc/INVARIANTS.md § 16。
    let mut builder = tauri::Builder::default();
    builder = builder.plugin(crate::platform::single_instance::plugin(|app, second| {
        // 第二个实例若带 --background（开机自启竞态下偶发）→ 只 show 不抢焦点；普通双击照常置前。
        let background = second.args.iter().any(|a| a == "--background");
        tracing::info!(
            "second cc-monitor instance detected (background={background}, token={})",
            second.token.is_some()
        );
        crate::platform::window::raise_main(app, second.token, !background);
    }));
    // Windows 那一件（WebView2 最大化 / 全屏后内容错位的修复）住壳的平台层：别处原样返回。
    builder = crate::platform::window::desktop_fixes(builder);

    // ST1「关窗改隐藏」的另一半：设置窗关窗 = **隐藏**，永不自己销毁
    // ⇒ 它会把进程吊住（Tauri 是「最后一个窗口销毁才退出」）。主窗一销毁，就把它一起 destroy 掉，
    //    让「最后一个**看得见**的窗口关掉 ⇒ 进程退出」照旧成立。决策在纯函数里，这里只执行。
    // ⚠ 刻意**不** `app.exit(0)`：那会改变「主窗关了、viewer 窗还开着」时的行为，
    //   退出行为整体搬家另算，这里不预先改它的语义。
    builder = builder.on_window_event(|window, event| {
        if !matches!(event, tauri::WindowEvent::Destroyed) {
            return;
        }
        use tauri::Manager;
        let app = window.app_handle();
        // 窗口没了 ⇒ 它的订阅整份作废（终端画面流顺手替它向那台退订）。
        if let Some(replay) = app.try_state::<Arc<event_replay::EventReplay>>() {
            replay.drop_webview(window.label());
        }
        let alive: Vec<String> = app.webview_windows().keys().cloned().collect();
        let alive: Vec<&str> = alive.iter().map(String::as_str).collect();
        for label in windows_to_destroy_after(window.label(), &alive) {
            if let Some(w) = app.get_webview_window(label) {
                if let Err(e) = w.destroy() {
                    tracing::warn!("跟着主窗收掉 {label} 窗口失败：{e}");
                }
            }
        }
    });

    // 本机能力（↗ · shell 方言 · ccm 缓存）：每个 webview 起页时注入 `window.__CCM_HOST__`，判定只住 `platform/host_facts.rs`。
    builder = builder.plugin(
        tauri::plugin::Builder::<tauri::Wry>::new("host-facts")
            .js_init_script(crate::platform::host_facts::init_script())
            .build(),
    );
    // 页面重载（开发者工具刷新 · 窗口重建同一个 webview）：旧页面的订阅整份作废 —— 重载后编号从头来，
    //   没被重订到的旧订阅（尤其终端画面流：后端那张票占着一个 tmux 客户端）不留成孤儿。
    builder = builder.on_page_load(|webview, payload| {
        if payload.event() != tauri::webview::PageLoadEvent::Started {
            return;
        }
        use tauri::Manager;
        if let Some(replay) = webview.try_state::<Arc<event_replay::EventReplay>>() {
            replay.drop_webview(webview.label());
        }
    });

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            // **重放缓冲与本机内容消费者先于本机后端就位。** 本机会话内容从此是
            // 本机后端的 `line` 帧（`local_lines` 头注）；后端一接上就开始宣告、发行 ⇒ 接住它们的那一头
            // 必须先在。原来 `EventReplay` 造在下面 watcher 那一段（本机 watcher 已删）。
            let replay = Arc::new(event_replay::EventReplay::new());
            // 会话内容经通道 `subscribe` 交给 webview（`chan/webview.rs` 头注）：出口先装上。
            replay.attach_sink(Arc::new(chan::webview::WebviewSink(app.handle().clone())));
            local_lines::install(app.handle().clone(), replay.clone());

            // F05a（定框 C7：没有 daemonless）：起并看住**本机后端进程**。
            // 〔`K-R59` 09-11：`C7` 的第二格今天补上了 —— 远端那个 `daemonless`
            //  每机开关整格删除（定框 `K35`），从此**没有「没有后端」这回事**。〕
            //
            // ⚠ **走哪一支取决于用户手里是哪一份产物**〔订正 2026-09-10，v3.7.0〕。
            //
            // 〔墓碑 —— 本行原话逐字：「⚠ 今天恒走「诚实降级」那一支 —— 安装包里还没有
            //  local_backend（`externalBin` 归 F05b）。」它记的是 F05b 之前的世界，**今天不成立**。〕
            //
            // 证伪它的读数：09-10 干净 win11 虚拟机上现打（PM，真安装包 + 真裸 exe 各一趟）——
            // **装出来那份** `C:\Program Files\cc-monitor\` 下 `cc-monitor-backend.exe`
            // **2 个进程在跑**；**裸 `monitor.exe`** 那份 **0 个**。
            // F05b 已经做完并随 v3.7.0 发出去了：`externalBin` 配在
            // `src/frontend/shell/tauri.sidecar.conf.json`，发版那一步用
            // `npx tauri build --config src/frontend/shell/tauri.sidecar.conf.json` 注入。
            // ⚠ 它**刻意不进基础 `tauri.conf.json`** —— 进了会让 `cargo test` 也要求当前
            // target 的那份二进制存在。⇒ **「基础配置里没有」≠「没配」，别再把这两句写成一句。**
            //
            // ⇒ 降级那一支仍然在（裸 exe · 开发树走的就是它），只是**不再是常数**。
            // 它**不是**「接线没做」：接线在这里，是**这一份产物里没带**，两者的区别就在那个
            // tagged 返回值上。它**刻意不扫仓库 dev 产物** —— backend 一起来就无条件往
            // tmux server 装三条全局 hook 且没有开关，扫到 dev 产物就起它 = 去改用户真实
            // tmux 的状态（F05 摸底 §2.5）。
            {
                use local_backend::Resolved;
                // P2z（定框 C10）：exe 旁边没有本机后端时，把**已内嵌**的那份释放到本机再起 ——
                // 「单 exe 也能起后端进程」那句话的落点。
                //
                // 两样宿主知识在这里给（backend 层不认识它们）：
                //   · 落点 `~/.cc-monitor/bin`：与远端自部署同一个目录，但**文件名带 build_id**
                //     ⇒ 与远端那份结构上不可能撞（理由见 `extract_embedded_to` 头注的 D1 段）。
                //   · 这台机器的 (OS, arch)：`byte_table::choose` 按它挑内嵌字节；取不到时交进去那句拒绝的话，
                //     函数把它接在「旁边没有」后面、不伪造理由。
                use local_backend_host::StartOutcome;
                match local_backend_host::start_local_backend() {
                    StartOutcome::Started(p) => {
                        tracing::info!("本机后端 local_backend: {}", p.display())
                    }
                    StartOutcome::AlreadyRunning => {
                        tracing::info!("本机后端已经在跑（启动路径不重复起）")
                    }
                    StartOutcome::Failed { reason, looked_at } => {
                        // ★★ `K-P1-D1` `重-2`：**「对不上就出声并拒绝」那句话，
                        //    在这条路上不许只进日志。**
                        //
                        // 两条起法只有手动那条会让用户看见（`crate::backend_control::backend_start`
                        // 回 `Err` ⇒ 前端 toast）。而这一条 —— 用户每天真正走的那条 ——
                        // 回修前是 `tracing::info!`，连 `warn` 都不是。
                        //
                        // 触发场景不是理论：口按家目录算死，`hello_verdict` 逐字比
                        // 「我这一版」（`byte_table::my_backend_id`）⇒ **升级 monitor 之后上一次脱离留下的那个
                        // backend 还在听同一个口** ⇒ `Stranger` ⇒ `Adopt::Refused`
                        // ⇒ 本机后端起不来，而界面上什么都不说。
                        // ⚠ 这是**常驻带来的新场景**：翻面之前 backend 153ms 就死了。
                        //
                        // ⚠ 分两档，因为这两件事不是一回事：
                        //   · **拒绝**（口上有东西、接不上）= 一件用户能动手解决的事 ⇒ 说到眼前；
                        //   · 别的失败（**这一份产物里没带 local_backend** —— 裸 exe / 开发树，
                        //     或释放内嵌那一份也失败了）= **诚实降级**，
                        //     每次启动都弹一次就成了噪音 ⇒ 仍走日志。
                        //     〔订正 2026-09-10（v3.7.0）—— 原话逐字：「别的失败（安装包里
                        //      还没有 local_backend…）= 今天的**诚实降级**」。括号里那句今天不成立：
                        //      09-10 干净 win11 现打（PM）装出来那份跑着 2 个
                        //      `cc-monitor-backend.exe`、裸 `monitor.exe` 那份 0 个。
                        //      ⚠ **分档标准本身一格没动**，换掉的只是它举的那个例子 ——
                        //      「诚实降级」这一档今天仍然有人（裸 exe · 开发树 · 释放失败）。〕
                        //   分档的依据是 `local_backend_host` 里那条**只在真的被拒绝时才写下**的记录，
                        //   **不是**去 `reason` 串里认字（那是 `KPY5` 治的那种假信号）。
                        match local_backend_host::take_start_refusal() {
                            Some(next_step) => {
                                tracing::warn!(
                                    "本机后端未启动: {}（{}）；找过 {looked_at:?}",
                                    reason.said,
                                    reason.raw()
                                );
                                if let Err(e) = crate::platform::notify::show(
                                    app.handle(),
                                    &copy_text("rsLib.run.localBackendDown", &[]),
                                    &next_step,
                                ) {
                                    // 通知发不出去也要留痕，别让「说出口」这件事静默失败。
                                    tracing::warn!(
                                        "本机后端拒绝的通知发不出去（{e}）：{next_step}"
                                    );
                                }
                            }
                            None => {
                                tracing::info!(
                                    "本机后端未启动: {}（{}）；找过 {looked_at:?}",
                                    reason.said,
                                    reason.raw()
                                )
                            }
                        }
                    }
                }
            }

            // 主窗的初始尺寸（`tauri.conf.json`）夹进工作区（小屏上底边别压在任务栏下）、摆在正中（重启不漂）。
            if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                center_window_in_work_area(&window);
            }

            // Debug build 自动开 DevTools(CCM_NO_DEVTOOLS=1 抑制——远程实测/E2E 时省半屏)
            #[cfg(debug_assertions)]
            if std::env::var("CCM_NO_DEVTOOLS").is_err() {
                if let Some(window) = app.get_webview_window("main") {
                    window.open_devtools();
                }
            }

            // 窗口 config `focus=false` → 创建时不激活、不抢前台（cc 集成 auto-launch 带
            // `--background` 启动时正好不打断当前终端）。但**手动**启动（双击 exe，无该参数）
            // 仍应置前，这里补一次 set_focus 还原默认体验。
            if !std::env::args().any(|a| a == "--background") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.set_focus();
                }
            }

            // agent 数据目录不在这里解析：那台后端自己解析（设置里的覆盖由起本机后端那一处显式交过去）。
            // 这里原来还算 `sessions_dir`（`<claude_dir>/sessions`，喂 monitor 自己那份判活）——
            //   本机判活改由本机后端的帧来，monitor 不再需要知道 pidfile 住哪。
            // 这里原来还算 `tasks_dir`（喂 monitor 自己那条任务 notify）—— 监视进了后端，monitor 不再需要知道任务住哪。

            // monitor 自己的数据目录：~/.cc-monitor
            let monitor_data_dir = config::resolve_monitor_data_dir().ok_or("no data dir")?;
            tracing::info!("monitor_data_dir: {}", monitor_data_dir.display());

            // v2.0.0：把 AppHandle 注入给 ErrorEmitterLayer（之前一直是 None，
            // setup 期间的 ERROR 只写 log；从这里开始 ERROR 才会弹前端 toast）
            logging_state.install_error_emitter(app.handle().clone());

            // v1.7.1：把当前 exe 路径记到 auto-launch.json，让 cc function 能在用户启用
            // auto-launch 时主动启动 monitor（不硬编码安装路径）
            auto_launch::update_monitor_path_on_startup(&monitor_data_dir);

            // 握手表（Linux bash / zsh 接入块那一份）：监听 ps-await/*.tty → 按标题找窗口 → 写 ps-registry/。
            //   ↗ 点那一刻现读进程链（`bind.rs`），不缓存会话 ↔ 窗口。
            let bind_registry = bind::BindRegistry::spawn(monitor_data_dir.clone());

            // 会话起停的成品（后端裁：活 / 状态灯 / 可重连 / 已结束 / 清单报完了）由两条流交 `session_book`，
            //   这里装它的出口：一条有序通道，下面那**一个** emitter 收（本机远端同一个）。
            //   〔LOC1b 之前这里起 monitor 自己的判活（`SessionMap::load_with_changes`〔散文墓碑〕）；MIG-1 之前是两个 emitter 各自裁。〕
            let (book_tx, book_rx) = std::sync::mpsc::channel::<session_book::Out>();
            session_book::install_sink(book_tx);

            // 本机会话内容**不再**由 monitor 自己 watch：它是本机后端的 `line` 帧，
            // 经 `local_lines` → `stream_source::consume_local` → 与远端同一个 `LineIntake`。
            // 原来这里起 monitor 自己的 jsonl watcher（`watcher.rs`，已删：第二套游标与 seq）、
            // 还有那条「会话后到 ⇒ 强制重扫」的兜底通道 —— 后端宣告会话时先 prime、历史走旁路快照，那个竞态不在了。

            // `session_tap` 的出口：本机后端的 `tap` 帧交给会话流的句柄（`EventReplay::on_tap`），
            //   订了 `session-tap` 的那几条订阅按 credit 收（一条帧路 ＋ `subscribe`，不开裸事件）。
            {
                let replay = replay.clone();
                crate::session_tap::install_sink(move |payload| replay.on_tap(payload));
            }
            {
                let replay = replay.clone();
                crate::probe_relay::install_sink(move |ticket, cell| {
                    replay.on_probe(&ticket, cell)
                });
            }
            {
                // 画面流撤掉 ⇒ 替界面向那台退订（后端那张票的寿命跟着这条流走，重载时界面没人能发）。
                replay.on_screen_dropped(crate::terminal_screen_relay::unfollow);
                let replay = replay.clone();
                crate::terminal_screen_relay::install_sink(move |origin, ticket, cell| {
                    replay.on_terminal_screen(origin, ticket, cell)
                });
            }
            // host key 自动固化 / 各地址不一 ⇒ 经既有的 `remote-health` 告知（机器页据此刷新）。
            {
                let handle = app.handle().clone();
                crate::dial_host::install_host_key_notice(move |n| {
                    let payload = ui_contract::RemoteHealthPayload {
                        origin: n.origin,
                        kind: n.kind.to_string(),
                        message: n.message,
                        detail: String::new(),
                    };
                    if let Err(e) = handle.emit(ui_contract::events::REMOTE_HEALTH, payload) {
                        tracing::warn!("emit remote-health(host key) failed: {e}");
                    }
                });
            }
            // 每台机器的状态成品一变就推一帧（`machine-state`：`{origin, machine}`，与 `backend_status` 那一格同形）。
            {
                let handle = app.handle().clone();
                crate::machine_state::install_out(move |origin| {
                    let machine = backend_control::machine_now(origin);
                    let payload = serde_json::json!({ "origin": origin, "machine": machine });
                    if let Err(e) = handle.emit(ui_contract::events::MACHINE_STATE, payload) {
                        tracing::warn!("emit machine-state failed: {e}");
                    }
                });
            }
            // 会话成品的**唯一**出口线程（本机远端同一个）：monitor 自己那几样副作用（拉前绑定）＋
            //   原样交会话流（`EventReplay::on_lifecycle`：`subscribe(origin, "session-lines")` 里的格，不吃 credit、不丢）。
            //   不裁决（可重连 / 已结束由那台后端裁）；原先这里是本机 / 远端两个 emitter，各自裁、发 9 个 Tauri 事件。
            {
                let replay = replay.clone();
                let spawned = std::thread::Builder::new()
                    .name("session-book-emitter".into())
                    .spawn(move || {
                        while let Ok(out) = book_rx.recv() {
                            replay.on_lifecycle(out.origin(), out.frames());
                        }
                    });
                if let Err(e) = spawned {
                    tracing::error!(
                        "failed to spawn session-book-emitter thread: {e}; \
                         会话起停事件将丢失，Tab 不会自动归档 / 新会话可能丢首屏"
                    );
                }
            }

            // 远端是在本机那条流之外**额外**的数据源（本地 + 远端会话同时显示）；机器表里每台要连的起一条。
            // 远端那几条流：启动时与之后每次改机器表（`remote_reconcile`）走同一条对齐路。
            if REMOTE_CTX
                .set((replay.clone(), app.handle().clone()))
                .is_err()
            {
                tracing::warn!("remote stream context already set");
            }
            let done = reconcile_remote_streams();
            if !done.started.is_empty() {
                tracing::info!("remote data source(s) started: {:?}", done.started);
            }

            // 焦点同步功能已移除：Windows 11 默认 WT 是单进程多窗口架构，
            // GetForegroundWindow 永远返回 WT 主进程 PID，OS 无法区分 tab/window。
            // 旧 focus.rs / lookup_by_foreground_pid / focus-switch IPC 都已删。
            // Tab 切换走手动点击或 Ctrl+Tab 快捷键。

            // 任务变更的监视进了后端（`changed {tasks}` 帧 ⇒ 通道 `subscribe(origin, "changed/tasks")`），
            //   monitor 这边那条 notify 线程与 `task-update` 事件删了；本机远端同形。

            // 这里原来起「历史全文搜索索引」那条后台线程（延迟 1.5 s 扫 projects/**/*.jsonl 建内存索引）。
            //   本机搜索改问本机后端（与远端同一条 `history-search`），这条线程与那份索引一起删了。

            // 前端 ready 事件 → replay all。
            //
            // 这里原来先 10ms 一拍地等本机 watcher「首扫完成」（10 s 上限）才 replay ——
            // 那是 v2.4 修首次启动乱序的办法。P5.4 之后前端按 seq 排序，而远端流从来就不等；
            // 本机内容改走后端的帧之后与远端同形：没到的行 ready 之后照样实时发、按 seq 落位，不需要等。
            {
                let replay = replay.clone();
                let t0_capture = t0;
                app.listen(ui_contract::events::FRONTEND_READY, move |event| {
                    // Batch5-F19：payload 携带用户上次所在 tab（localStorage 记忆），
                    // replay 按 session 分组、该 tab 的块先发。缺省/解析失败 → None
                    // （行为同 F19 前；viewer 等旧调用方不带 payload 也安全）。
                    // 契约定义在 ui_contract.rs（单一来源，G 验收纠偏）。
                    let priority_sid =
                        serde_json::from_str::<ui_contract::FrontendReadyPayload>(event.payload())
                            .ok()
                            .and_then(|p| p.priority_sid);
                    let replay = replay.clone();
                    let listen_recv_at = t0_capture.elapsed().as_millis();
                    tauri::async_runtime::spawn(async move {
                        tracing::info!(
                            "[perf] T+{}ms frontend-ready received, starting replay",
                            listen_recv_at
                        );
                        // 就绪点：主界面在发 `frontend-ready` 之前已经订好了各台机器的会话流（`chan.subscribe`），
                        //   这里按 credit 交完留存；起停的成品（骨架在行前、终局在行后，`session_book::Book::replay`）在同一条流里原位交，
                        //   不吃 credit。〔从前这一段在就绪点前后各发一轮 Tauri 事件：重宣告 · 容器 · 可重连 · 对账补发已结束 / 说不清 · 清单。〕
                        replay.ready_point(priority_sid.as_deref()).await;
                    });
                });
            }

            // 给 Tauri 命令暴露 state。
            //
            // **v1.7.4 修回归**：v1.6.7 撤 bring_terminal_to_front 时把
            // `app.manage(session_map.clone())` 也删了，但当年的历史清单命令也接
            // `State<Arc<SessionMap>>`，导致历史浏览器打不开，报"state not managed
            // for field `map`"。这里补回去。那两条命令退役了（清单归本机后端），`SessionMap` 仍有别的命令接。
            // `app.manage(session_map)`〔散文墓碑〕那一行删了：本机活会话表是进程级的一张（`session_map::local()`），命令直接读它。
            app.manage(replay.clone());
            app.manage(bind_registry.clone());
            // v2.0.0 (issue #4)：logging state 也要 manage，IPC handler 才能拿到
            app.manage(logging_state.clone());

            tracing::info!(
                "[perf] T+{}ms setup() completed (watchers spawned, state managed)",
                t0.elapsed().as_millis()
            );

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 「退出行为」问 / 交写那两条退役：设置页经通道说 `exit-policy-read` / `exit-policy-set`。
            backend_control::backend_machines,
            backend_control::backend_status,
            backend_control::backend_start,
            backend_control::backend_stop,
            config::load_config,
            config::patch_config,
            config::machine_table_try,
            // K-H2a：apikey 表那把 key 的写（`KS10`）走通道（`apikey-key-set`）；「表里有没有行」并进账号清单的徽章（`accounts-list`）。
            // 「起会话那一发注入哪个中转地址」与全量注入开关都归起 agent 那台的 `ccm` 自己定（`relay_all_sessions_switch`〔散文墓碑〕删了）。
            // 别名六条（`aliases_*`〔散文墓碑〕）进了那台机器的后端（`assets/aliases/`），界面经通道直问 `aliases-*`。
            //   留下的只有「这台已握手的终端数」—— 它住本进程的 `BindRegistry`，不是那台盘上的事实。
            // F87(#50+#51): MCP 管理——读跨 scope 展示 / 写只项目 .mcp.json（SS-14）
            // B03 批一：cc-bus 驾驶舱的两条读命令退役 —— 界面经通道直接问那台后端 `bus-state` / `bus-inbox`
            // B04：钩子只读诊断（本机 + 远端）。**没有任何写命令**——用户定调不改 settings.json
            // 足迹成品由那台后端出（界面经通道问 `footprint-report`）；这里只答 monitor 自己那台那几行的事实。
            footprint_client::footprint_client_facts,
            drift_ledger::drift_ledger_report,
            diagnostics_report::diagnostics_report,
            app_restart::restart_app,
            desktop_notify::notify_desktop,
            clipboard::clipboard_write,
            // `ccm …` 调用行 · 载荷渲染两条退役：那台后端的帧命令 `launch-render-cli` / `launch-render-payload`。
            // MCP 读写（`mcp::*` 六条）与推 / 拉两条退役：界面经通道问那台后端
            //   （`mcp-read` · `mcp-server-put` / `-remove` · `mcp-sync-source` / `-preview` / `-apply`，`src/frontend/ui/mcp-reads.ts` · `src/frontend/ui/mcp-sync-reads.ts`）。
            //   列远端配置标签那一条是 monitor 自己的配置，挪进 `config.rs`。
            config::list_remote_mcp_origins,
            // 资产目录同步（`assets_sync`）与 skill 装 / 卸三条退役：界面经通道直问那台后端
            //   （`assets-sync` 问 `<local>` · `skill-read` / `skill-install-plan` / `skill-install-apply` / `skill-uninstall-apply`，
            //   `src/frontend/ui/assets-sync-reads.ts` · `src/frontend/ui/skill-install-reads.ts`）。
            forget_session,
            // issue #10: 独立只读窗口（多窗口 / 双屏）
            open_session_in_new_window,
            // F82a(#56+#47): 设置独立窗口
            open_settings_window,
            remote_reconcile,
            bring_terminal_to_front,
            // 远端 Tab ↗ 拉前对应本地终端窗口（界面问过那台与本机后端，交来对上的窗口）
            bring_remote_terminal_to_front,
            // issue #23: 红绿灯快照（启动/F5 初始收敛；增量走 activity 格 事件）
            // v2.4 issue #2: 用户在终端输入时可选拉前 monitor 自身
            bring_monitor_to_front,
            // 🔴 `K-R135` / `R85`：用户级 PATH 那一格（现在状态 · 加 · 撤）。
            //    `R87` 裁定它住 Tauri 命令 —— 与别名块那几条同族（今天是 `aliases_block_*`，从前叫
            //    `cc_integration_*`〔散文墓碑〕）
            //    （「往用户的 shell profile 里写」与「往用户级 PATH 里写一段」是同一族动作，
            //    而前者已经在这儿了；再给同一族动作另起一条路本身就违反 `K33`）。
            ccm_user_path_status,
            ccm_user_path_add,
            ccm_user_path_remove,
            cc_get_auto_launch,
            cc_set_auto_launch,
            // v2.0.0 (issue #4): 诊断 / log
            frontend_perf_log,
            get_diagnostics_config,
            set_diagnostics_config,
            get_log_file_info,
            open_log_file,
            open_log_dir,
            // 历史清单住本机常驻后端（`history-list`，远端那一支代问）；注解与上次的号跟着会话住在那台（`history-annotate` / `history-last-accounts` 问那台），
            //   界面经通道问（`history-list-reads.ts` · `history-reads.ts`）。
            // 会话正文四条（整份读 · 子 agent · 按偏移 · 按行号）退役：那台后端出记录行，
            //   界面经通道直问（`src/frontend/ui/record-reads.ts`：`history-page` · `history-subagent` · `history-lines`）。
            // 接上骨架的会话，重放缓冲只留尾巴
            // 远端装 / 卸别名块那两条命令并进上面 `aliases_block_install` / `_remove`（带 `origin`），删。
            // F08c：部署 / 卸载远端后端（SFTP 写 ~/.cc-monitor/bin，SS-G 部署写豁免）。
            // 部署那一条同时放 `ccm` 入口 —— 「部署后端」只有一个动作。
            sftp::deploy_remote_backend,
            sftp::uninstall_remote_backend,
            // 本机 / 远端各两条合成两条带 origin 的。
            // `probe_session_record`（resume 之前问记录还在不在）退役：界面经通道问 `history-record`。
            // 本机起会话三条（resume · 新起 · 接回那一句）退役：计划与渲染问本机后端 `launch-local`；
            //   monitor 只剩「开一个终端窗口跑这串」。
            launch::open_local_terminal,
            // A2 那两条账号清单（远端 `list_remote_accounts` · 本机 `list_local_accounts`）与
            //   换号前的信任预检（`check_account_trust`）退役：前端经通道说 `accounts-list` / `accounts-trust`，后端出成品。
            // 「某会话属于哪个账号」那两条（本机 E79 · 远端 A2）退役：
            //   本机与远端同一条路 —— 前端经通道 `chan_call` 说 `accounts-sessions`。
            launch::open_terminal_window,
            // 设置页「终端」那一行的事实（挑终端的判定住平台层）。
            launch::terminal_choices,
            launch::terminal_dial,
            // 池子那十二条 Tauri 命令〔散文墓碑〕随老面板与窗口改走通道一起删了；
            // 最后一条（零流量复制）随门禁那一格退役一起删了 ⇒ 池子零条 Tauri 命令。
            // 🔴 `24e` 第二刀（第三段）：**原生文件管理窗口的入口。**
            //    它不是「又一条 sftp 命令」—— 它开的是那个 egui 窗口（同进程、次线程，
            //    进程形态见 `filewin/mod.rs` 头注）。先真的列一趟目录，列不出来就带原文报错，
            //    **不静默开一个空窗**（理由逐条住 `filewin/entry.rs` 头注）。
            //    ⚠ 界面上点得到它的地方是旧 SFTP 面板的表头 —— 那块面板按 `§6.6 C`
            //    要退役，而在这个窗口真能替代它之前删掉旧的等于把功能拿走 ⇒ 这一刀不删。
            filewin::entry::open_file_window,
            // 远端 `ccm` 探针那条命令退役：渲染进了那台后端，能力问它自己。
            // 🔴 `K-R69` / `KR69D2`：本机 `ccm` 这一格（我们那一份 · PATH 上那一份 · 判词）。
            ccm_probe::local_ccm_entry_status,
            // skill 接入面三条（收件箱的列 / 读 / 写）退役：界面经通道直问那台后端
            //   （`skill-host-list` / `-read` / `-write`，声明与围栏住后端 `agents/claudecode/skill_host.rs`）。
            // cc-bus 装 / 三态进了本机后端（`cc-bus-install` / `-state`）；留下装前那道本机 `ccm` 预检。
            cc_bus_deploy::cc_bus_ccm_precheck,
            // **主界面说 `call` 的那一跳**：webview ⇒ 通道 ⇒ 注入的后端句柄。
            chan::webview::chan_call,
            // 〔「撤单不许回退」〕撤单过 webview 那一跳（带编号撤那一问）。
            chan::webview::chan_cancel,
            // 会话内容经通道的 `subscribe`（本地撤单 · credit）。
            chan::webview::chan_offer,
            chan::webview::chan_subscribe,
            chan::webview::chan_want,
            chan::webview::chan_stop,
            // issue #6: 历史全文搜索那三条命令（本机索引）删了：本机远端都经通道说 `history-search`。
            // v2.3.0 issue #3 (A 透明化): 设置面板「数据」区列出所有持久路径
            data_paths::get_data_paths,
            // issue #15 Tier 1: SSH 连接 UX —— 测试连接 + 指纹固化。`~/.ssh/config` 导入那三条搬进后端（`ssh-config-*`）。
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        // F05a：**退出前收掉本机后端。** 被监护的后端对「stdin 写端关闭」刻意不敏感
        // ⇒ 不显式 `stop()` 它就活过 monitor，成游魂进程。
        //
        // ⚠ 这一段是 **clippy 的 dead_code 抓出来的**：`stop()` / `attempts()` / `current_pid()`
        // 三个方法全被报「never used」，也就是说我把句柄存进了 `LOCAL_BACKEND` 却从没用过它 ——
        // 而模块头注里逐字写着「不杀就成了游魂进程」。**注释说了、代码没做，靠一条告警才发现。**
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                // P2s（C8②③）：**杀不杀由这台机自己的值说了算**，缺省不杀。
                //
                // 〔条 66〕那个值住后端所在那台机器上 ⇒ 这里**在决定那一刻现问**
                // 本机后端一次（`kill_on_exit_now`），不再读一张启动时推进来的表（那张表删了）。
                // 问不到就按缺省（不结束）办并出声。**只问一次**。
                //
                // ⚠ 常驻（脱离）那条起法**不在这里收** —— 它自己就是读那个值的人：
                //   这个 monitor 的那条流一断，它现读、选了「结束」就退（后端 `main.rs::serve_listening`）。
                //   这里替它再决定一次 = 两个决策处，且违背 `§3.3b ⑥`「不是起我的那个 monitor 退了」。
                let kill = crate::backend_policy::kill_on_exit_now(&crate::origin::Origin::local());
                // ── 起法 ①：被监护的子进程，句柄在 `LOCAL_BACKEND` 里 ──
                // ⚠ 锁在这里取、句柄不克隆：`SuperviseHandle` 刻意不是 `Clone`
                // （克隆出去的那份 `stop()` 谁都能调，就没有「一个句柄一条命」这回事了）。
                // 🔴 它为什么留在臂里而不进缝：`local_backend_tests.rs::the_exit_path_really_stops_the_local_backend`
                // 逐条要求这一臂**体内**恰好一处 `.stop()` + 恰好一处现问 + 问在前 + 中间那个 `if` 判的就是那个答案。
                {
                    let guard = local_backend_host::LOCAL_BACKEND.lock();
                    if let Some(h) = guard.as_ref().ok().and_then(|g| g.as_ref()) {
                        tracing::info!(
                            "退出：本机后端 pid={:?}（起过 {} 次）kill_on_exit={kill}",
                            h.current_pid(),
                            h.attempts()
                        );
                        if kill {
                            h.stop();
                        }
                    }
                }
                // 本机中转住在上面那个后端进程里 ⇒ 这里**没有**第三个进程要收
                //   （`the_exit_arm_collects_no_relay` 的零命中守卫数着这件事）。
            }
        });
}

/// Batch7-F24：读 config.json 顶层 `showBgSessions`（默认 true）。**OnceLock 缓存
/// 首读**——本地 scan 过滤与远端 exec 参数（含每次重连）拿到同一个值，双端统一
/// "重启生效"语义（审计 D：不缓存则远端在重连时活切换、与本地/文案不一致）。
pub(crate) fn load_show_bg_sessions() -> bool {
    static CACHE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHE.get_or_init(|| {
        config::load_config()
            .ok()
            .and_then(|v| v.get("showBgSessions").and_then(|b| b.as_bool()))
            .unwrap_or(true)
    })
}

/// SSH-remote（issue #15 / 多机 #30）：从 monitor 的 config.json 读 `remote` 段，构造
/// **0..N 个** [`stream_source::RemoteConfig`]。**空 Vec = 本地模式**（与历史 bit-for-bit
/// 一致）：config.json 不存在 / 解析失败 / 无 `remote` 键 / `enabled != true` / 无任何
/// 合法 host → 空 Vec。
///
/// config.rs 是 schema-agnostic（只透传 serde_json::Value），所以这里直接读
/// `config::resolve_config_path()` 的文件，自己取 `remote` 子对象。读法对齐
/// `config.rs::claude_dir_override`（同一个 config.json，同样的 best-effort 容错）。
///
/// remote 段 schema（S6/S7 的设置 UI 负责写）：
/// ```json
/// "remote": {
///   "enabled": true,
///   "hosts": [
///     { "label": "pi", "host": "raspberrypi.local", "port": 22, "user": "pi",
///       "keyPath": "C:\\Users\\me\\.ssh\\id_ed25519",
///       "backendPath": "/home/pi/cc-monitor-backend",
///       "hostKeyFingerprint": "SHA256:..." }
///   ]
/// }
/// ```
/// 每台缺必填字段(host/user/backendPath) 则跳过 + warn；`label` 重复则后缀化 ` (#2)`（保证 by-label 选台 key 唯一）。
/// 〔「不为旧配置留兼容」〕旧单对象形态（`"remote": { "enabled": true, "host": …, … }`，
/// 没有 `hosts` 数组）**不再认**：[`parse_remote_hosts`] 回 `Err`，这里照原样落一条 `error!` 日志、不连任何远端 ——
/// 不再把它悄悄当成一台，也不装作「没配远端」（D4）。
/// `remote-health` 事件的出口：窗口把手只在这一层，交给 `stream_source`（它不认识 GUI 宿主）的是一个闭包。
pub(crate) fn remote_health_out(app: tauri::AppHandle) -> stream_source::HealthOut {
    Arc::new(move |payload| {
        app.emit(ui_contract::events::REMOTE_HEALTH, payload)
            .map_err(|e| e.to_string())
    })
}

/// 起远端流要的两样上下文（启动时放进来，热加载时取用）。
static REMOTE_CTX: std::sync::OnceLock<(Arc<event_replay::EventReplay>, tauri::AppHandle)> =
    std::sync::OnceLock::new();

/// 主窗口那一侧的应用把手（起来之后才有；通道上由 monitor 自己接的那几条要发界面事件时取它）。
pub(crate) fn main_app() -> Option<tauri::AppHandle> {
    REMOTE_CTX.get().map(|(_, app)| app.clone())
}

/// 一台远端的重起闭包：每次调用起一条新的 `stream_source::run`。
fn remote_respawn(
    cfg: stream_source::RemoteConfig,
    replay: Arc<event_replay::EventReplay>,
    app: tauri::AppHandle,
) -> backend_control::Respawn {
    Box::new(move || {
        let cfg = cfg.clone();
        let replay = replay.clone();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let label = cfg.origin_label();
            let connected = Arc::new(std::sync::atomic::AtomicBool::new(true));
            if let Err(e) = stream_source::run(cfg, replay, remote_health_out(app), connected).await
            {
                tracing::error!("stream_source::run [{label}] exited: {e}");
            }
        })
    })
}

/// 照 config.json 的机器表对齐远端流（要连的才连；连接参数变了的重起；其余不动）。
fn reconcile_remote_streams() -> backend_control::Reconciled {
    let Some((replay, app)) = REMOTE_CTX.get() else {
        return backend_control::Reconciled::default();
    };
    let wanted = load_remote_configs()
        .into_iter()
        .map(|cfg| backend_control::WantedRemote {
            origin: cfg.origin_label(),
            cfg_key: serde_json::to_string(&cfg).unwrap_or_default(),
            respawn: remote_respawn(cfg, replay.clone(), app.clone()),
        })
        .collect();
    let out = backend_control::reconcile_remotes(wanted);
    // 停下的那几台（删了 / 停用了）状态成品作废；还在表里的由读点按「连接这台」说停用。
    for origin in &out.stopped {
        machine_state::forget(origin);
    }
    out
}

/// 机器表改了（增删改 · 某台的「连接这台」）：当场对齐，不要重启 cc-monitor。回这一趟起了 / 停了 / 重起了哪几台。
#[tauri::command]
async fn remote_reconcile() -> Result<backend_control::Reconciled, Said> {
    let r: Result<backend_control::Reconciled, Said> = async move {
        Ok(
            tauri::async_runtime::spawn_blocking(reconcile_remote_streams)
                .await
                .map_err(Said::crashed)?,
        )
    }
    .await;
    r.map_err(|s| s.named("remote_reconcile"))
}

/// 机器表里**要连的**那几台（某台 `"connect": false` ⇒ 不在里面）。
pub(crate) fn load_remote_configs() -> Vec<stream_source::RemoteConfig> {
    load_all_remote_configs()
        .into_iter()
        .filter(|(_, connect)| *connect)
        .map(|(cfg, _)| cfg)
        .collect()
}

/// 机器表里的每一台，连同它的「连接这台」（缺省 = 连）。
pub(crate) fn load_all_remote_configs() -> Vec<(stream_source::RemoteConfig, bool)> {
    let Some(cfg_path) = config::resolve_config_path() else {
        return Vec::new();
    };
    if !cfg_path.exists() {
        return Vec::new();
    }
    let Ok(raw) = std::fs::read_to_string(&cfg_path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };
    let Some(remote) = value.get("remote").and_then(|v| v.as_object()) else {
        return Vec::new();
    };

    match parse_remote_entries(remote) {
        Ok(cfgs) => cfgs,
        Err(why) => {
            tracing::error!("{} 的 remote 段：{why}", cfg_path.display());
            ui_error::tell(ui_error::UiError::MachinesUnreadable {
                path: cfg_path.display().to_string(),
                why: why.to_string(),
            });
            Vec::new()
        }
    }
}

/// `remote` 段没有 `hosts` 数组时那句话（旧单对象写法、或者 `hosts` 写成了别的类型）。
pub(crate) const REMOTE_HOSTS_UNRECOGNIZED: &str =
    "认不出：没有 hosts 数组（旧的单台写法不再认），远端一台都不连；在设置里重新添加这台机器";

/// 把 `remote` 对象解析成 host 列表（抽出供单测直接喂 JSON 对象）。**只认 `hosts` 数组**；
/// 没有它 ⇒ `Err`（旧单对象那一支删了）。重复 label 后缀化去重。
/// 每台连同它的「连接这台」（`"connect": false` ⇒ 不连；缺省 = 连）。
fn parse_remote_entries(
    remote: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<(stream_source::RemoteConfig, bool)>, &'static str> {
    let Some(arr) = remote.get("hosts").and_then(|v| v.as_array()) else {
        return Err(REMOTE_HOSTS_UNRECOGNIZED);
    };
    let host_objs: Vec<&serde_json::Map<String, serde_json::Value>> =
        arr.iter().filter_map(|v| v.as_object()).collect();

    let mut out: Vec<(stream_source::RemoteConfig, bool)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for obj in host_objs {
        let Some(mut cfg) = parse_host_obj(obj) else {
            continue; // parse_host_obj 已 warn
        };
        // label 去重：重复则后缀化 " (#2)"、" (#3)"…，保证 by_label 选台唯一。
        if !seen.insert(cfg.label.clone()) {
            let base = cfg.label.clone();
            let mut n = 2u32;
            let unique = loop {
                let cand = format!("{base} (#{n})");
                if seen.insert(cand.clone()) {
                    break cand;
                }
                n += 1;
            };
            tracing::warn!("remote label 重复，'{base}' 改为 '{unique}'");
            cfg.label = unique;
        }
        let connect = obj.get("connect").and_then(|v| v.as_bool()) != Some(false);
        out.push((cfg, connect));
    }
    Ok(out)
}

/// 解析单个 host JSON 对象 → RemoteConfig；缺必填字段(host/user) → None+warn。
fn parse_host_obj(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> Option<stream_source::RemoteConfig> {
    let str_field = |k: &str| {
        obj.get(k)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
    };

    // `backendPath` 那一格删了（落点恒是那台的 `~/.cc-monitor/bin/ccm`）；盘上旧值不读。
    let (host, user) = match (str_field("host"), str_field("user")) {
        (Some(h), Some(u)) => (h.to_string(), u.to_string()),
        _ => {
            tracing::warn!("remote host 缺必填字段(host/user)，跳过该台");
            return None;
        }
    };

    let label = str_field("label")
        .map(str::to_string)
        .unwrap_or_else(|| host.clone());
    let port = obj
        .get("port")
        .and_then(|v| v.as_u64())
        .and_then(|p| u16::try_from(p).ok())
        .unwrap_or(22);
    let key_path = str_field("keyPath").map(str::to_string);
    let host_key_fingerprint = str_field("hostKeyFingerprint").map(str::to_string);
    // Batch14-F56：跳板 label（指向另一台已配置主机的 origin_label）。
    let jump = str_field("jump").map(str::to_string);
    // 🔴 `K-R59`（定框 `K35`）：这里原来读 `daemonless`（per-host 降级开关）。
    //    那个键今天**故意不读** —— 盘上还留着 `true` 的旧配置由界面侧
    //    （`src/frontend/ui/remote-config.ts` 的 `LEGACY_NO_BACKEND_KEY`）认出来、指名告知一次，
    //    后端这一侧一律按「有后端」走，不再有第二条路。
    // Batch14-F45：备用地址。前端下发数组（addresses: string[]）；也容忍换行文本（历史/手填）。
    let addresses: Vec<String> = match obj.get("addresses") {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        Some(serde_json::Value::String(s)) => s
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    };

    Some(stream_source::RemoteConfig {
        label,
        host,
        port,
        user,
        key_path,
        host_key_fingerprint,
        addresses,
        jump,
    })
}

/// 按 label 选台（停用连接的那台也选得到：卸载 · 测试连接照样要它的参数）。
pub(crate) fn load_remote_config_by_label(label: &str) -> Option<stream_source::RemoteConfig> {
    load_all_remote_configs()
        .into_iter()
        .map(|(c, _)| c)
        .find(|c| c.origin_label() == label)
}

/// 把后端帧里来的一批 `JsonlLine` 组成可 emit 的 `JsonlLinePayload`。
///
/// v2.4.2 issue #2 抽出的最小 seam。今天它**只有一个**生产调用方：`stream_source::flush_lines`
/// （远端流 · 本机流 · 旁路快照三路的行都从那里出去）。
///
/// **这里不解释记录**：这一行在界面里是什么（`record`）、进不进界面（有没有 `record`）、
/// 它自己的 `cwd`，都是那台后端给的成品（`agents/claudecode/`）；本函数只组载荷、记「连着的不可显示那一段」，`seq` 透传。
///
/// `origin`：数据来源。载荷上的 `origin` 字段由它派生：本机 ⇒ 不带（前端 Tab 标题不加前缀，与历史一致）；远端 ⇒ 那台的名字
/// （issue #15，前端 Tab 标题加 `[host]` 前缀以区分本地/远端）。
pub(crate) fn batch_to_payloads(
    lines: Vec<stream_source::JsonlLine>,
    origin: &crate::origin::Origin,
    runs: &mut SkipRuns,
) -> Vec<ui_contract::JsonlLinePayload> {
    let label = origin.host_name().map(str::to_string);
    let mut payloads = Vec::with_capacity(lines.len());
    for line in lines {
        let skipped = runs.pending(&line.session_id, line.seq);
        match line.record {
            Some(record) => {
                runs.saw(&line.session_id, line.seq, None);
                payloads.push(ui_contract::JsonlLinePayload {
                    session_id: line.session_id,
                    cwd: line.cwd,
                    path: line.path.to_string_lossy().into_owned(),
                    // P5.1：后端给每行编行号（`--tail-only` 下与快照同一个行号空间）；前端按 seq 排到 timeline
                    seq: line.seq,
                    origin: label.clone(),
                    record,
                    skipped_from: skipped,
                    rid: line.rid,
                });
            }
            None => {
                // 不进界面（后端没给成品）：照占号、不出 payload —— 记进「连着的不可显示那一段」。
                runs.saw(
                    &line.session_id,
                    line.seq,
                    Some(skipped.unwrap_or(line.seq)),
                );
            }
        }
    }
    payloads
}

/// 每个会话**最近见过的那一行**（不论可不可显示）＋ 它之后连着的不可显示那一段从哪起。
/// 只在行号**连着**（这一行 == 上一行 + 1）时才认那一段 —— 中间丢过行（通道满 / 快照与实时交错）就不认，宁少记不多记。
/// 一个收口（`LineIntake`）一份、一次旁路快照一份；会话走了就摘（`forget`）。
#[derive(Debug, Default)]
pub(crate) struct SkipRuns(std::collections::HashMap<String, (u64, Option<u64>)>);

impl SkipRuns {
    /// 这一行之前连着的不可显示那一段的起点（这一行紧接着上一行才有）。
    fn pending(&self, sid: &str, seq: u64) -> Option<u64> {
        self.0
            .get(sid)
            .filter(|(last, _)| last.checked_add(1) == Some(seq))
            .and_then(|(_, from)| *from)
    }

    fn saw(&mut self, sid: &str, seq: u64, run_from: Option<u64>) {
        match self.0.get_mut(sid) {
            Some(slot) => *slot = (seq, run_from),
            None => {
                self.0.insert(sid.to_string(), (seq, run_from));
            }
        }
    }

    pub(crate) fn forget(&mut self, sid: &str) {
        self.0.remove(sid);
    }
}

// 墓碑：这里从前是 Tauri 命令 `write_apikey_credentials_key`〔散文墓碑〕（`K-H2a` 从界面配一把 key；
// 按 origin 交那台机器的后端；本机那一臂先核路径）。常驻后端身份带上数据目录之后（`local_backend_host::hello_verdict`
//   比 hello 的 `host_env`），核路径那一问由连接本身答 ⇒ 界面经通道直接发 `apikey-key-set`（`src/frontend/ui/apikey-reads.ts::writeApikeyKey`），
//   账号 id 由后端推（`acct_core::apikey_account_id_of_dir`）。monitor 里从此没有明文 key 的具名绑定。
//   `KH2C1` 前端那一侧的机检（它的旧名 `the_ui_never_derives_the_account_id_itself`〔散文墓碑〕）照旧在 `accounts-section.vitest.ts`。

// 别名一族六条（`aliases_render` / `_read` / `_install` / `aliases_block_*`〔散文墓碑〕）退役：
//   规则 · 方言 · 围栏住那台机器的后端（`src/backend/assets/aliases/`），界面经 `chan.call(origin, "aliases-*")` 直问（`src/frontend/ui/alias-reads.ts`）。

#[tauri::command]
fn forget_session(
    session_id: String,
    replay: tauri::State<'_, Arc<event_replay::EventReplay>>,
) -> Result<(), Said> {
    let r: Result<(), Said> = (move || -> Result<(), Said> {
        replay.forget(&session_id);
        // 它的会话成品也忘掉（F5 不再重放一个用户关掉了的已结束 tab）。
        session_book::book().write().forget(&session_id);
        Ok(())
    })();
    r.map_err(|s| s.named("forget_session"))
}

/// issue #10：把某 session 在一个独立 WebviewWindow（`viewer-<sid>`）里打开，
/// 加载 `viewer.html?viewer=<sid>` —— 独立入口 `src/frontend/ui/entry-viewer.ts`（三入口拆分）。
/// 窗口已存在则前置聚焦（不重复开）。双屏 / 并排查看用。
///
/// `run` 有 ⇒ 开的是这个会话里**一个子运行**的窗口（agent 窗口）：窗口名按「会话 ＋ 运行」定（`viewer-agent-<sid>-<运行>`），
/// URL 多带 `run=<运行>`；同一个子运行至多一个窗口（已在 ⇒ 前置聚焦）。没给落点时错开叠放：从另一个 agent 窗口开的
/// 在它右下错开 [`AGENT_WINDOW_STEP`]；从别的窗口开的，按此刻已开着几个 agent 窗口依次错开。
///
/// **必须 `async`**：Tauri 2 同步 `fn` 命令在**主线程**执行，而
/// `WebviewWindowBuilder::build()` 要把窗口创建派发到主线程并阻塞等待 —— 同步命令
/// 就是在主线程里等主线程 → 死锁（表现：新窗口白屏 + 整个 app 卡死连 X 都点不了）。
/// async 命令跑在 async runtime（非主线程）→ build() 派发给空闲主线程 → 正常建窗。
///
/// 拖拽撕离（tear-off）：`x` / `y` 为可选的**逻辑屏幕坐标**（CSS px），来自前端
/// mouseup 的 `e.screenX/screenY`。两者皆 `Some` 时新窗口在该落点打开（双屏拖出体验）；
/// 任一为 `None`（右键菜单 / Ctrl+Shift+N 老调用方）则维持默认居中行为，不破坏旧路径。
#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn open_session_in_new_window(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    session_id: String,
    origin: crate::origin::Origin,
    title: String,
    x: Option<f64>,
    y: Option<f64>,
    run: Option<String>,
) -> Result<(), Said> {
    let r: Result<(), Said> = async move {
        use tauri::Manager;
        // 独立窗口自己订 `session-lines/<sid>`（`subscribe(origin, kind)`：origin 是唯一寻址键）
        //   ⇒ 窗口要知道这个会话在哪台机器上；随 URL 交过去（百分号编码：本机那个 `<local>` 有尖括号）。
        origin.route("open_session_in_new_window")?;
        let origin_q = pct_encode(origin.as_wire_str());
        let run = run.filter(|r| !r.is_empty());
        let label = match &run {
            Some(r) => agent_window_label(&session_id, r),
            None => format!("viewer-{session_id}"),
        };
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            let _ = w.request_user_attention(Some(tauri::UserAttentionType::Informational));
            return Ok(());
        }
        let run_q = run
            .as_deref()
            .map(|r| format!("&run={}", pct_encode(r)))
            .unwrap_or_default();
        let url = tauri::WebviewUrl::App(
            format!("viewer.html?viewer={session_id}&origin={origin_q}{run_q}").into(),
        );
        let mut builder = tauri::WebviewWindowBuilder::new(&app, &label, url)
            .title(if title.is_empty() {
                "cc-monitor"
            } else {
                &title
            })
            .inner_size(900.0, 720.0)
            .min_inner_size(480.0, 360.0)
            // Batch7-F23B：与主窗口 backgroundColor 一致——合成间隙露底为主题深色
            // 而非 WebView2 默认白（tauri.conf.json 主窗口同款 #2b2a27）
            .background_color(tauri::window::Color(0x2b, 0x2a, 0x27, 0xff));
        // 落点定位：仅当 x/y 都给出时按逻辑坐标摆放（Tauri 2 builder 取 LogicalPosition）。
        let at = match (x, y) {
            (Some(x), Some(y)) => Some((x, y)),
            _ if run.is_some() => {
                let open = app
                    .webview_windows()
                    .keys()
                    .filter(|l| l.starts_with(AGENT_WINDOW_PREFIX))
                    .count();
                agent_window_spot(&window, open)
            }
            _ => None,
        };
        if let Some((x, y)) = at {
            builder = builder.position(x, y);
        }
        let w = builder
            .build()
            .map_err(|e| Said::with_raw(copy_text("rsShellCmd.viewer.openFailed", &[]), e))?;
        fit_window_to_work_area(&w);
        Ok(())
    }
    .await;
    r.map_err(|s| s.named("open_session_in_new_window"))
}

/// agent 窗口名的前缀（`viewer-agent-<sid>-<运行>`：落在查看窗那一组窗口名里，同一套权限）。
const AGENT_WINDOW_PREFIX: &str = "viewer-agent-";
/// agent 窗口错开叠放的一步（逻辑 px）。
const AGENT_WINDOW_STEP: f64 = 32.0;
/// 从别的窗口依次错开时，至多错开几步就绕回第一格。
const AGENT_WINDOW_STEPS: usize = 8;

/// 一个子运行的窗口名：`viewer-agent-<sid>-<运行>`。窗口名只许字母数字与 `-` `/` `:` `_`，别的字节写成 `_xx`（十六进制）。
fn agent_window_label(sid: &str, run: &str) -> String {
    let mut out = String::from(AGENT_WINDOW_PREFIX);
    for (i, part) in [sid, run].into_iter().enumerate() {
        if i > 0 {
            out.push('-');
        }
        for b in part.bytes() {
            // 运行那一段连 `-` 也写成 `_2d`：会话那一段带 `-`，两段之间的 `-` 才不会认错。
            if b.is_ascii_alphanumeric() || (b == b'-' && i == 0) {
                out.push(char::from(b));
            } else {
                out.push_str(&format!("_{b:02x}"));
            }
        }
    }
    out
}

/// 新 agent 窗口的落点（逻辑坐标）：从 agent 窗口开 ⇒ 它右下一步；从别的窗口开 ⇒ 那扇窗左上角往里错开
/// `1 + 已开着几个 agent 窗口`（绕着 [`AGENT_WINDOW_STEPS`]）步。读不出那扇窗的位置 ⇒ `None`（交给系统摆）。
fn agent_window_spot(from: &tauri::WebviewWindow, open: usize) -> Option<(f64, f64)> {
    let scale = from.scale_factor().ok()?;
    let p = from.outer_position().ok()?.to_logical::<f64>(scale);
    let steps = if from.label().starts_with(AGENT_WINDOW_PREFIX) {
        1
    } else {
        1 + open % AGENT_WINDOW_STEPS
    };
    let d = AGENT_WINDOW_STEP * steps as f64;
    Some((p.x + d, p.y + d))
}

/// 设置窗（单例 `settings`，关窗只是藏起来）：已在 ⇒ show ＋ 聚焦；不在 ⇒ 建（加载 `settings.html`）。
/// `target` 是一份不透明的目的地 JSON（页 · 机器 · 栏 · 锚点，前端 `open-settings.ts` 定义与解析）：
/// 已在的窗收 `settings-target` 事件，新建的窗由初始化脚本带进 `window.__CCM_SETTINGS_TARGET__`。
/// **必须 `async`**（同步命令建窗会死锁）。默认 960×740、最小 640×480，夹进工作区正中。
#[tauri::command]
async fn open_settings_window(app: tauri::AppHandle, target: Option<String>) -> Result<(), Said> {
    let r: Result<(), Said> = async move {
        use tauri::Manager;
        let label = SETTINGS_WINDOW_LABEL;
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            if let Some(t) = target {
                let to = tauri::EventTarget::webview_window(label);
                if let Err(e) = app.emit_to(to, "settings-target", t) {
                    tracing::warn!("settings target not delivered: {e}");
                }
            }
            return Ok(());
        }
        let url = tauri::WebviewUrl::App("settings.html".into());
        let mut builder = tauri::WebviewWindowBuilder::new(&app, label, url)
            .title(&copy_text("rsLib.settings.windowTitle", &[]))
            .inner_size(960.0, 740.0)
            .min_inner_size(640.0, 480.0)
            .background_color(tauri::window::Color(0x2b, 0x2a, 0x27, 0xff));
        if let Some(t) = target {
            let lit = serde_json::to_string(&t).map_err(Said::crashed)?;
            builder =
                builder.initialization_script(format!("window.__CCM_SETTINGS_TARGET__ = {lit};"));
        }
        let w = builder
            .build()
            .map_err(|e| Said::with_raw(copy_text("rsShellCmd.settings.openFailed", &[]), e))?;
        center_window_in_work_area(&w);
        Ok(())
    }
    .await;
    r.map_err(|s| s.named("open_settings_window"))
}

/// URL 查询串里的一格：非「字母数字 `-` `_` `.` `~`」一律 `%XX`（RFC 3986 unreserved 之外全编）。
fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

// 独立窗口的定向重放（`replay_session_to_window`〔散文墓碑〕）退役：独立窗口自己订
//   `session-lines/<sid>`（`src/frontend/ui/entry-viewer.ts`），留存由那条订阅当场交。

/// v2.4 (issue #2)：把 monitor 自己的主窗口拉到最前 + unminimize + 抢焦点。
/// 两个平台臂（Windows 的 AttachThreadInput 那套 · 别处 Tauri 自己的 `set_focus`）住 `platform/window.rs::bring_to_front`，头注随之。
#[tauri::command]
async fn bring_monitor_to_front(app: tauri::AppHandle) -> Result<(), Said> {
    let r: Result<(), Said> =
        async move { Ok(crate::platform::window::bring_to_front(app).await?) }.await;
    r.map_err(|s| s.named("bring_monitor_to_front"))
}

// `list_session_activity`〔散文墓碑〕· `list_active_sessions`〔散文墓碑〕两条命令退役：本机活会话的骨架与初始灯
//   是会话流里的 `live` / `activity` 成品（就绪点按成品缓存重放），与远端同一条路。

/// 拉本机会话的终端窗口：点那一刻现走那个 claude 往上的进程链（`bind::bring_local_window`，与远端那一格同一条规则），
/// 校验、拉前。会话账里没有它的进程号（不在活集里）⇒ 「没登记」。回的是结局族（`bind::FrontOutcome`），句子在界面的文案表。
///
/// **必须 async + spawn_blocking** 隔离 Win32 sync 调用（v1.6.5 的教训）。
#[tauri::command]
async fn bring_terminal_to_front(
    session_id: String,
    bind_state: tauri::State<'_, Arc<bind::BindRegistry>>,
) -> Result<bind::FrontOutcome, Said> {
    let r: Result<bind::FrontOutcome, Said> = async move {
        let bind = bind_state.inner().clone();
        Ok(tokio::task::spawn_blocking(move || {
            let pid = session_book::book()
                .read()
                .live_pid(&crate::origin::Origin::local(), &session_id);
            match pid {
                Some(pid) => bind::bring_local_window(pid, &bind),
                None => bind::front_refusal().unwrap_or(bind::FrontOutcome::Unbound),
            }
        })
        .await
        .map_err(Said::crashed)?)
    }
    .await;
    r.map_err(|s| s.named("bring_terminal_to_front"))
}

/// 拉对应**远端** Tab 的本地终端窗口，两问各一次（界面按顺序调）：
/// ① 交那台 `session-terminals` 的 `terminals` 原样 ⇒ 按窗口标签找（`bind::bring_labeled_window`）：对上了回那一次的结局，
///    没有标签 / 对不上回 `None`（界面接着问第二问）；
/// ② 交本机后端 `terminal-processes` 的成品 `chain` ⇒ 沿链每一级先问它显示在哪个窗口（Windows 问控制台 · Linux 查握手表）、再看属主的窗口，
///    校验、拉前（`bind::bring_chain_window`），回结局。两样都空 ⇒ `None`。
/// **必须 async + spawn_blocking** 隔离 Win32 sync 调用（INVARIANT § 10）。
#[tauri::command]
async fn bring_remote_terminal_to_front(
    terminals: Option<Vec<serde_json::Value>>,
    chain: Option<Vec<bind::ChainLink>>,
    bind_state: tauri::State<'_, Arc<bind::BindRegistry>>,
) -> Result<Option<bind::FrontOutcome>, Said> {
    let r: Result<Option<bind::FrontOutcome>, Said> = async move {
        let bind = bind_state.inner().clone();
        Ok(tokio::task::spawn_blocking(move || {
            if let Some(t) = terminals.filter(|t| !t.is_empty()) {
                return bind::bring_labeled_window(&t, &bind);
            }
            chain
                .filter(|c| !c.is_empty())
                .map(|c| bind::bring_chain_window(&c, &bind))
        })
        .await
        .map_err(Said::crashed)?)
    }
    .await;
    r.map_err(|s| s.named("bring_remote_terminal_to_front"))
}

// === v1.7：PowerShell profile cc 集成 IPC ===
//
// 这里原来是「终端集成」那五条命令（`cc_integration_*`〔散文墓碑〕：状态 · 扫一份 · 预览 · 装 · 卸）
// 与它们的三个出参类型。它们办的是**别名块**，并进了别名同一族命令面；
// 那一族今天住那台机器的后端（`assets/aliases/`，帧命令 `aliases-*`）。
// 「v1.7.0-1.7.1 装错位置」那一段遗留扫描随之删了 —— 每份候选都带块的现状，块装在哪几份照实说。

/// Batch13-F40:前端 perf 仪表落盘。webview 无 devtools(生产/CCM_NO_DEVTOOLS)时
/// console 取证不能,前端把启动管线 timeline/建卡计数经此写进 monitor 日志。
#[tauri::command]
fn frontend_perf_log(lines: String) {
    for line in lines.lines().take(40) {
        // 行数 + 单行长度双封顶(任意前端字符串进日志,防日志膨胀)
        let capped: String = line.chars().take(2000).collect();
        tracing::info!(target: "fe_perf", "{capped}");
    }
}

/// 读 auto-launch.json：UI 显示当前 toggle 状态 + 记录的 exe 路径。
#[tauri::command]
fn cc_get_auto_launch() -> Result<auto_launch::AutoLaunchConfig, Said> {
    let r: Result<auto_launch::AutoLaunchConfig, Said> =
        (move || -> Result<auto_launch::AutoLaunchConfig, Said> {
            let dir = auto_launch::data_dir()
                .ok_or_else(|| copy_text("rsShellCmd.autoLaunch.readNoDir", &[]))?;
            Ok(auto_launch::get_config(&dir))
        })();
    r.map_err(|s| s.named("cc_get_auto_launch"))
}

/// UI toggle 改变时调：写 auto_launch_enabled。
#[tauri::command]
fn cc_set_auto_launch(enabled: bool) -> Result<(), Said> {
    let r: Result<(), Said> = (move || -> Result<(), Said> {
        let dir = auto_launch::data_dir()
            .ok_or_else(|| copy_text("rsShellCmd.autoLaunch.writeNoDir", &[]))?;
        Ok(auto_launch::set_enabled(&dir, enabled)?)
    })();
    r.map_err(|s| s.named("cc_set_auto_launch"))
}

// ===== 🔴 `K-R135`（`R85` / `R87` / `R88`）：用户级 PATH 那一格 =====
//
// 用户 `R85` 逐字：「**应该让用户手动点击加，也能管理删除。就像是 log 数据管理一样。**」
// ⇒ 三样：**现在状态（现算不缓存）· 一个按钮加 · 一个按钮撤**，形状照
// `src/frontend/ui/settings/diagnostics-section.ts`（用户点名的那个范式，它现打也全走 `commands.*`）。
//
// 实现一律住 `profile_installer` 那一族（`R87` 裁定：同一族动作不许另起一条路）；
// 这三条只是**包装层**，一行业务逻辑都不许写在这里。
// 三条都走 `spawn_blocking`：它们底下要起一趟 `powershell.exe`（`R88`），不许占住 async 执行器。

/// `KR135D1` ①：**现在状态** —— `~/.cc-monitor\bin` 在不在**用户级** PATH 上。
///
/// **现算，不缓存**：每调一次真跑一趟探针。界面只在「打开那一格 / 点刷新」时调它
/// （起一趟 PowerShell 几百 ms ⇒ 不许轮询）。
/// 🔴 探不动时回的是 `error` 非空、`on_user_path = false` —— 前端**必须**把 `error` 显示出来，
/// 不许把「问不出来」静默成「没装」。
#[tauri::command]
async fn ccm_user_path_status() -> Result<profile_installer::UserPathStatus, Said> {
    let r: Result<profile_installer::UserPathStatus, Said> = async move {
        Ok(
            tokio::task::spawn_blocking(profile_installer::user_path_status)
                .await
                .map_err(Said::crashed)?,
        )
    }
    .await;
    r.map_err(|s| s.named("ccm_user_path_status"))
}

/// `KR135D1` ②：**一个按钮加**。跑的就是界面上显示给用户看的那段字节
/// （`render_user_path_setup_command`）—— 点按钮与自己复制去跑**逐字同一份**。
#[tauri::command]
async fn ccm_user_path_add() -> Result<(), Said> {
    let r: Result<(), Said> = async move {
        Ok(
            tokio::task::spawn_blocking(profile_installer::user_path_add)
                .await
                .map_err(Said::crashed)??,
        )
    }
    .await;
    r.map_err(|s| s.named("ccm_user_path_add"))
}

/// `KR135D1` ③：**一个按钮撤**。**只摘自己那一格**（整格比，不碰用户 PATH 里别的东西）。
#[tauri::command]
async fn ccm_user_path_remove() -> Result<(), Said> {
    let r: Result<(), Said> = async move {
        Ok(
            tokio::task::spawn_blocking(profile_installer::user_path_remove)
                .await
                .map_err(Said::crashed)??,
        )
    }
    .await;
    r.map_err(|s| s.named("ccm_user_path_remove"))
}

// ===== v2.0.0 (issue #4): 诊断 / log IPC =====

/// 读当前 diagnostics 配置。设置面板打开时调一次。
#[tauri::command]
fn get_diagnostics_config(
    state: tauri::State<'_, Arc<logging::LoggingState>>,
) -> Result<logging::DiagnosticsConfig, Said> {
    let r: Result<logging::DiagnosticsConfig, Said> =
        (move || -> Result<logging::DiagnosticsConfig, Said> { Ok(state.config()) })();
    r.map_err(|s| s.named("get_diagnostics_config"))
}

/// 应用新 diagnostics 配置。日志级别 + error_toast 立即生效；
/// log_enabled / max_files 改了返回 `NeedsRestart` 让前端提示用户重启。
#[tauri::command]
fn set_diagnostics_config(
    cfg: logging::DiagnosticsConfig,
    state: tauri::State<'_, Arc<logging::LoggingState>>,
) -> Result<logging::RestartHint, Said> {
    let r: Result<logging::RestartHint, Said> =
        (move || -> Result<logging::RestartHint, Said> { Ok(state.update_config(cfg)?) })();
    r.map_err(|s| s.named("set_diagnostics_config"))
}

/// 返回 log 目录 + 当前 log 文件 + 全部 .log 文件列表（path / size / mtime）。
/// 设置面板用来显示路径 + 文件大小，让用户一眼看到 log 状态。
#[tauri::command]
fn get_log_file_info(
    state: tauri::State<'_, Arc<logging::LoggingState>>,
) -> Result<logging::LogFileInfo, Said> {
    let r: Result<logging::LogFileInfo, Said> =
        (move || -> Result<logging::LogFileInfo, Said> { Ok(state.log_file_info()) })();
    r.map_err(|s| s.named("get_log_file_info"))
}

/// 用系统默认编辑器打开当前 log 文件（rolling::daily 写入的 mtime 最新那个）。
/// 失败常见原因：log_enabled=false 还没生成过 log 文件 → Err 让前端 alert 提示。
#[tauri::command]
async fn open_log_file(state: tauri::State<'_, Arc<logging::LoggingState>>) -> Result<(), Said> {
    let r: Result<(), Said> = async move {
        let path = state
            .current_log_file()
            .ok_or_else(|| copy_text("rsLib.log.none", &[]))?;
        let path_str = path.to_string_lossy().into_owned();
        Ok(tokio::task::spawn_blocking(move || open_with_os(&path_str))
            .await
            .map_err(Said::crashed)??)
    }
    .await;
    r.map_err(|s| s.named("open_log_file"))
}

/// 用资源管理器打开 log 目录。
#[tauri::command]
async fn open_log_dir(state: tauri::State<'_, Arc<logging::LoggingState>>) -> Result<(), Said> {
    let r: Result<(), Said> = async move {
        let dir = state.log_dir();
        // 目录可能还不存在（log_enabled=false 时不创建）
        if !dir.exists() {
            std::fs::create_dir_all(&dir).map_err(|e| {
                Said::with_raw(
                    copy_text(
                        "rsShellCmd.logDir.createFailed",
                        &[("why", &copy_core::reason::io_reason(e.kind()))],
                    ),
                    format!("{}\n{e}", dir.display()),
                )
            })?;
        }
        let dir_str = dir.to_string_lossy().into_owned();
        Ok(tokio::task::spawn_blocking(move || open_with_os(&dir_str))
            .await
            .map_err(Said::crashed)??)
    }
    .await;
    r.map_err(|s| s.named("open_log_dir"))
}

/// 跨平台调系统默认 opener。Windows 用 `cmd /C start ""` 兜 path 中的空格。
/// 复用 tauri-plugin-opener 也行（前端就是走它），但这里在 Rust 端直接调更直接。
fn open_with_os(path_or_dir: &str) -> Result<(), String> {
    use crate::spawn_managed::{spawn_managed, ConsolePolicy, Lifetime, StderrSink};
    // 「用哪个程序打开」是平台差异（住壳的平台层）；「怎么起它」三条策略走唯一出口。
    //
    // ★ 这一处走的是 `spawn_managed(bin, args, …)` 那个**五参数形态**（
    //   逐字写的那个签名），而不是它的内层 `spawn_managed_cmd` —— 因为这一跳**真的
    //   只有「一个二进制 ＋ 一串 argv」**：不设 env、不设 cwd、三根 stdio 一根都不碰。
    //   ⚠ 别把这读成「别处偷懒了」：别处要 env / cwd / stdin / stdout，那些不属于
    //   那三条策略，硬塞进这个签名只会长出第七、第八个参数。
    // 「用哪个程序打开」按平台选（`cmd /C start ""` · `open` · `xdg-open`）住 `platform/proc.rs::os_opener`。
    let (bin, args) = crate::platform::proc::os_opener(path_or_dir);
    // 三条策略：
    // · `Hidden` —— 这一跳只是转交给系统默认 opener，**先前那个 `CREATE_NO_WINDOW`
    //   就是这一条**（设计稿逐字点名它是「仓里有、却用在最不需要的那处」的那一份）；
    // · `Detached` —— fire-and-forget：我们不等它，也不该在自己退出时把用户刚打开的
    //   文件管理器一起收掉 ⇒ **绝不能是 `JobKillOnClose`**（那会在本函数返回、
    //   句柄一丢的瞬间把它杀掉）；
    // · `Inherit` —— 它的抱怨跟着界面进程的 stderr 走。接进滚动日志要多一条泵，
    //   而这一跳失败时用户当场就看得见（东西没打开）。
    spawn_managed(
        std::path::Path::new(bin),
        &args,
        ConsolePolicy::Hidden,
        Lifetime::Detached,
        StderrSink::Inherit,
    )
    .map(|_| ())
    .map_err(|e| format!("{bin} failed: {e}"))
}

// 搜索口径那道守卫原挂在 `search.rs` 下；那份文件删了，挂到这里。今天只剩「monitor 这一侧零处」（「恰一份」那两道随家进了后端）。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/search_kou_jing_guard.rs"]
mod search_kou_jing_guard;

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/lib_env_scrub_tests.rs"]
mod env_scrub_tests;

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/lib_batch_tests.rs"]
mod batch_tests;

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/agent_window_label_tests.rs"]
mod agent_window_label_tests;

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/lib_mod_decl_hygiene_tests.rs"]
mod mod_decl_hygiene_tests;

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/lib_remote_config_tests.rs"]
mod remote_config_tests;

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/lib_window_lifecycle_tests.rs"]
mod window_lifecycle_tests;

// Windows 应用清单进了本包每一个链接产物（`build.rs::manifest_for_every_artifact`），测试程序读回自己那一份。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/lib_app_manifest_tests.rs"]
mod app_manifest_tests;

// `INVARIANTS §47` / `§49` 的人群判据（盘上全集派生，与登记表两向相等）。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/lib_invariant_population_tests.rs"]
mod invariant_population_tests;
