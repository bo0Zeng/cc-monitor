// 🔴 〔搬树 2026-09-18〕**ts-rs 的 `export_to` 是三级 `../`，不是两级。**
// 本 crate 从 `<repo>/src-tauri` 搬到 `<repo>/src/bridge` ⇒ 到仓根多了一级。
// 漏改的症状**不是编译错**：ts-rs 会把全部类型写进一个叫 `<repo>/src/src/generated`
// 的**文件**（因为那个目录不存在），而 `src/generated/*.ts` 那 86 份**从此不再更新**
// ⇒ CI 那条「生成物必须最新」会红，而本机什么都不响。
// 现打：32 个文件 / 84 处属性，全在 `src/bridge/src/*.rs` 同一层。
//
// 🔴 **结尾那个 `/` 也是承重的**：ts-rs 12 把不带斜杠的 `export_to` 当**文件路径**。
// 重组那一趟把两件都改坏了：① 少一级 `..`；② **结尾斜杠被吃掉**
// （重组前逐字是 `"../../src/generated/"`）。
// 两个错叠在一起恰好**不报错**：`src/src/generated` 那个目录不存在 ⇒ ts-rs 安静地
// 建了一个同名**文件**，把全部类型塞进去。只补 `..` 不补斜杠则当场 `Is a directory`（84 条红）。
// ⇒ 这一格的教训：**"改完能编过"不等于改对了** —— 这两处都不是编译期能看见的。
//! 库 crate 根：模块声明 + Tauri 应用装配。
//!
//! `run()` 在 `tauri::Builder` 之前先 `logging::init`（tracing 全局 dispatcher 必须最先 init），
//! 然后注册 single-instance plugin（须为链上第一个）、`setup()` 里 spawn watcher / 各后台线程
//! 并 `app.manage` 所有 Arc-shared State，最后注册 `invoke_handler`（IPC 命令清单）。
//! State 注册矩阵见 src/doc/STATE-MATRIX.md；漏 `manage` 不会被 cargo check 抓住（INVARIANT § 8）。

mod account_aliases; // K-R49：加了账号就给那条命令落盘——写的是 monitor 自己那份别名文件，不是用户的 rc
mod accounts; // A2：多账号（cc-acct-iso）只读查询——账号=一个 CLAUDE_CONFIG_DIR
mod acct_iso_deploy; // F5：一键部署 vendored cc-acct-iso 到远端 + 存在性检测
mod adapter;
mod auto_launch;
// 🔴 〔步 12 · 09-19〕`origin` 归一的地基：「这一趟问的是哪台机器」的唯一类型。
mod backend_policy;
mod origin; // P2s（C8）：每台机一份后端策略（生效值住内存，持久化归前端）
            // 🔴 〔步 8 · 归属 2026-09-19〕**它搬不进 `backend/`** —— `backend_policy_tests.rs::
            //    the_supervisor_itself_never_records_a_death` 逐字：「`backend/` 的生产段里
            //    出现了 `record_death(` ⇒ 判与记该在**宿主层**，`backend/` 那半**只搬证据**」。
            //    而 `record_death` 的唯一定义就在本模块里。⇒ 这是**解耦**的活，不是改名一刀能搬的。
mod bind;
mod bridge;
// 通信层面 A 的第一个进程外客户端那条路（`设计/05` 末尾「面 A 的第一个外部客户端：通道」）。
// `pub` 同 `filewin`：它的客户端那一半给另一个二进制（外部前端）经 `monitor_lib::chan` 用。
mod cc_bus_deploy; // PS1：把内嵌的 cc-bus 装到 <claude_dir>/skills/（U10b 裁「开」后落地；只读铁律第 7 条例外）
pub mod chan;
mod claude_data_fence; // 步 H2：Claude 自己的数据不许我们写 —— `INVARIANTS §1` 的 F47/F03b 两段澄清共用的那一个判定（用户 09-21 裁「拆」）
mod codex_record; // Phase 2 · F2a：Codex rollout 记录防御式分类器（keystone 第一块）
mod config;
mod config_surface; // T02：配置面审计视图（遍历 tool_registry，只读、不轮询）
mod data_paths;
mod footprint_remote; // 〔RM1a〕「足迹」的远端那一栏：问那台后端要路径事实（footprint-probe），判定走同一份 build_rows
                      // U-CC1：数据面漂移记账 —— 把「CC 变了」从不可观测变成看一眼就知道。只记账，零行为变化。
mod drift_ledger;
mod event_replay;
mod fenced_block; // T04 第二步：围栏块配对判定（本机+远端 profile 共用最强那一档）
                  // 🔴〔步 24e · 2026-09-20〕原生文件管理窗口（`设计/60 §4 戊`）。进程形态＝**同进程**、
                  // egui 事件循环住次线程；住址为什么是 monitor 的一个模块而不是新 crate——两条理由
                  // （同进程要链进这个二进制 · 门禁 `cargo` 格把包数恒等钉在 9）逐条写在它的头注里。
                  // ⚠ **`pub` 是刻意的**：本 crate 的 `mod` 全是私有的，而私有模块里没人调的 `pub fn`
                  //   会被 `dead_code` 记一笔 —— 门禁 `deadcode` 那一格把 `never used` **恒等钉在 36**（〔F7c 收尾〕34 → 36，逐条住 gate.sh 那一格上方）。
                  //   这棵树今天的消费者只有它自己的判据（窗口还没接到界面上），`pub` 让它在
                  //   rlib 的公开面上可达 ⇒ 不往那个 34 上加数。**等窗口真被界面调起来，这里可以收回私有。**
pub mod filewin;
mod history;
mod hooks_diag; // B04：cc-bus 钩子在 settings.json 里的只读诊断 + 生成待贴文本（绝不写入）
                // U8a-2a：monitor 侧的入方向发送端（往那条长连接的写半边发命令 + 按 id 收应答）。
                // 「hello 之前不许写」在这里是类型上的事实：ParkedWriter 身上没有任何写方法。
mod apikey_remote; // 〔RM1a〕那份文件**按机器**读写：本机进 creds_store，远端交那台机器的后端
mod backend; // P4a（§1.4b）：monitor 侧的后端边界 —— 读/控制两条能力线，宿主无关
mod creds_store; // K-H2a：第三方 API key 那份文件的**写侧**（monitor 独占）+ 读侧只回掩码
#[cfg(test)]
mod guard_support; // 住址唯一源（仓根/源码树/测试树）——头注写着它为什么存在
mod launch;
mod local_accounts; // L3a：本机多账号枚举（只读）——`accounts.rs` 的本地对侧
mod local_backend_host; // P2s（C8）：本机后端的生命周期（起/停/状态）——命令不能与 IPC 命令清单同模块，理由见该模块头注
mod local_origin_registry;
mod logging;
mod mcp; // F87（#50+#51）：MCP 管理（读跨 scope 展示 / 写只项目 .mcp.json，SS-14）
mod messages;
mod panorama;
mod panorama_bytes; // 〔RM1c · 第四波〕全景小程序的字节从哪来：按 (OS, arch) 选内嵌的那一份（推上去归 F08 部署路 / SR1b）
mod panorama_call; // 〔RM1c · 第四波〕代码全景经那台机器的后端走（V108 选 B）：`panorama_call(origin, op, repo, args)`
mod panorama_seam_registry; // P7c-2 第一刀：引擎住哪一侧要可换（整体 #[cfg(test)]）
mod parser;
mod paths;
mod platform_fs; // C10：平台相关的 fs 原语的唯一住址，注入给平台无关的 backend
mod plugins; // P8a：Claude Code marketplace 面的只读枚举（**不声称安装/启用**，见模块头注）
mod port_forward;
mod profile_installer;
mod pubkey;
mod remote_branch; // G6：远端分叉（经 ssh 调 backend `--fork-session`）——写面故与只读的 remote_history 分家
mod remote_history;
mod remote_relay; // 〔RM1a〕中转（层 1）按机器：本机由 monitor 监护，远端问 / 交那台机器的后端
mod remote_write_registry; // devbench F10c：远端写面登记（接三张表各自划出去、然后没人接的那道缝）
mod search;
mod session_facts; // 〔U4b〕两条后端流交来、要送前端的会话事实（容器 · 本机可重连落已结束）的一个口
mod session_map;
mod shell_dialect; // AL1c（第四波 4B）：`设计/71 §4.4` 那组 shell 方言接口 —— POSIX 与 PowerShell 各一份实现，通用层零 shell 文本
                   // `15 §5.1 A3` / `00 §1.5.2`：起子进程的**唯一出口**（三个策略都没有 Default）。
                   // 住宿主知识层是硬的：平台原语进不了 `backend/`（那侧的禁针 + 递减棘轮），
                   // `backend/` 的两个落点收注入参数（`ManagedSpawn`）。
mod spawn_managed;
// devbench F02：skill 接入面（一份声明 + 通用宿主）。
// ⚠ **今天零生产消费者**（UI 归 F03）—— 照 `tool_registry` 的先例如实登记并写处置条件：
// F03 接上之后删掉那个模块级 `#[allow(dead_code)]`；若 F03 收工时它仍零消费者，
// 就该删掉整个模块，而不是让它当装饰。
mod sftp_pool;
mod skill_host;
mod user_files; // RW1（第四波）：monitor 够用户文件的唯一开口 —— 读·算·交给那台机器的后端，自己一个字节不落盘
mod verified_write; // T01：统一的「备份→写→读回比对→回滚」；本机侧从长度比对升级为内容比对
                    // SS-D 统一 SFTP 写层（issue #29 自动部署 F08；后续 F11/F10 复用）。
mod sftp;
// SSH-remote Phase 0 (issue #15)：从 setup() 调用 —— 当 config.json 的
// `remote.enabled = true` 时，ssh_source::run 作为**附加**数据源与本地 jsonl-watcher
// 并行跑（aggregate：本地 + 远端 session 同时显示为 Tab），走相同的
// batch_to_payloads → replay.on_line_batch 出口；远端行带 origin=host 标签。
// remote off（默认）时本模块不被调用，本地路径 bit-for-bit 不变。
mod ccm_probe;
mod ssh_source;
// 〔C2 · `设计/05 §13`〕拨号应答的客户端（通信层面 A 的 SSH 链路那一段）。
mod ssh_link;
// 〔SR1b · 2026-09-24〕`inproc_dial`（界面进程里最后一份 russh 拨号，只剩 SFTP 一个用户）删了：
//   SFTP 进了本机常驻后端（`src/backend/dial/sftp.rs`），**界面进程零 SSH**（用户 V89）。
// 〔C2 → SR1a〕拨号的宿主：配置 → 请求 · 经本机常驻后端开链路 · 链路交给 `ssh_link`。
mod dial_host;
// 〔SR1a〕链路的 monitor 这一侧：在本机后端那条流上多路复用到各远端的字节流（`link-*`）。
mod link_mux;
// T01：结构性扫描的可复用形式（枚举+逐个断言+计数自检+钉死逃生口）。
// **只在测试期编译**——它的消费者全在 `#[cfg(test)]` 里（`sftp.rs` 的 tmux 目标守卫、
// `tool_registry.rs` 的字段纪律）。这是测试支撑模块，不是被闲置的生产代码；
// 加 `cfg(test)` 就是把这件事写进类型系统，顺带消掉 5 条 dead_code 警告。
mod agent_dispatch_registry; // K-W1B D2：桌面侧「通用层认得出某个 adapter」的地方逐条登记 + 递减棘轮（整体 #[cfg(test)]；刻意不叫 agent_boundary_guard —— backend 侧已有同名异职模块，来历见该模块头注）
mod arch_doc_shape_guard; // F19：顶层架构文档的结构性存在钉（必须覆盖 backend 边界 / 零轮询 / 两条链）+ 形状钉（逐文件模块表不许长回来）
/// U1a：`shared/ccm` 的强度契约（仅测试构建）。U9 迁移后由同一份 `measure()` 对拍新构造点。
///
/// ⚠ **插在这里、不要插在上面那条注释与 `#[cfg(test)]` 之间。** U1a 初版就插错了位置，
/// 把属性与 `mod structural_scan;` 的配对拆开 —— `structural_scan` 当场变成无条件编译，
/// 上面注释里逐字写着的「顺带消掉 5 条 dead_code 警告」被原样撤销，而 CI 的 `cargo build`
/// 没有 `-D warnings` ⇒ **不会红**。是 Phase D 审计数出「dead_code 正好 +5」才发现的。
#[cfg(test)]
mod ccm_cli_contract;
mod cross_half_edge_registry; // F20：两半之间的编译期边（跨半边 include_str! 逐条登记 + ★ 一条都不许长在生产段）
#[cfg(test)]
mod design_doc_registry; // `99 §4.17` · `Q9` 乙：设计篇索引不许与各篇自己的作废声明漂（整体 cfg(test)）
mod doc_claim_registry; // F11：耐久文档里「描述当下」的字段与代码对拍（状态列逐格登记 + ★ 判据从文档里读那个数，代码里不留第二份）
#[cfg(test)]
mod fixture_guard; // `99 §4.8.3 P15`：`tests/__fixtures__/` 里的夹具不许掉光引用变成孤儿（α3 刀 D 那个没红的读数；整体 cfg(test)）
mod frame_cadence_guard; // F01：帧节奏说法的零命中守卫（P5 后后端零定时器；被禁措辞见模块头注）
mod gate_singleton_guard; // F03：§34 Gate 2 的身份判定在 Rust 侧只许有一个家（`gate-core`）

#[cfg(test)]
mod atomic_replace_registry; // audit-0805 F13：原子替换的两套 Win32 语义，谁用哪一套
#[cfg(test)]
mod bus_identity_registry; // cc-bus：拿 id 点名 tmux 前必须核身份（整体 #[cfg(test)]）
mod byte_cap_registry; // audit-0805 F06：字节上限登记表（管什么量 + 超限怎么办 + 跨 crate 对拍）
mod capability_registry;
#[cfg(test)]
mod comm_boundary_registry; // 设计/05 §8 步 1：通信层的边界登记表 ＋ C1–C5 / X1–X6 十一条判据（人群非空之后绿的理由是三方相等，不是「扫不到」；份数的唯一住址在那个模块的头注里，这里刻意不抄第二份；整体 #[cfg(test)]）
#[cfg(test)]
mod dial_home_registry; // K-R74：「解耦干净」改述成三样可判的东西 —— 终点二值旗（russh 在不在界面 manifest 里）+ 过程递减棘轮（还没搬走的拨号处数）+ 拨号锚点的唯一住址（整体 #[cfg(test)]）
#[cfg(test)]
mod doc_copy_registry; // audit-0805 F18：散文里的数字副本清账（E12 的第二条路变成机检）
mod e2e_gate_registry; // audit-0805 08-08：每一套 e2e 要么进门禁要么登记为什么不进
mod exec_site_registry;
#[cfg(test)]
mod launcher_identity_registry; // K-P5b：起会话方身份落点清账 + 递减棘轮（人群 = 所有起会话方；整体 cfg(test)）
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
mod local_read_surface_registry;
#[cfg(test)]
mod lockfile_conflict_guard; // audit-0805 F16：两份 lock 的真冲突必须为空（超集不算）
#[cfg(test)]
mod needle_anchor_registry; // audit-0805 F24：匹配单位不许比事实小（F23 的兄弟族）
#[cfg(test)]
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
                           // 〔SR1b · 2026-09-24〕`sftp_move_ledger`〔散文墓碑〕（K-R78：那 14 处 SFTP 拨号今天各自卡在哪 —— 乙为什么没搬 + 甲的四条挡路石）
                           //   **退役**：那 14 处全搬了（界面进程零 SFTP，V89），底账要记的那件事做完了；它的挡路石各自怎么被拆的写在 SR1b 的记录里。
#[cfg(test)]
mod shared_crate_registry; // U8c-1：新增共享 crate 时 CI 三样都要补 —— 从散文变机检
mod shell_lint_registry; // audit-0805 08-08：每个 shell 脚本要么进 shellcheck 要么登记豁免
#[cfg(test)]
mod structural_scan;
mod subagent;
// 〔`设计/10` 骨架 · 子步 3〕monitor 侧「从偏移读」：骨架索引 ＋ 按偏移取一段正文。
mod session_skeleton;
// 〔SE1 · `设计/10 §2.2b ⑥`〕大纲的数据源：问后端要「你说过的话」清单。
mod session_outline;
// 〔SE2 · `设计/10 §6 步 6`〕会话内查找（Ctrl+F）：问后端要这一份会话里的命中。
mod session_find;
// 〔C2 · U3 第 3 件〕远端流断线重连后，旁路快照从续点接着拉（不再从第 0 行整份重拉）。
mod snapshot_resume;
mod tasks;
mod tmux_backend_gate_guard; // U10 裁决：backend 侧没有身份守卫之前，send-keys/kill 不许改走 backend
mod tmux_reconcile;
mod tool_registry; // T01：受管工具声明（只声明，不改各工具行为）
mod utils;
mod watcher;
mod write_site_registry; // audit-0805 08-07：每个会写用户机器的落点都要申报（关掉 §5 4b 一半） // audit-0805 08-07：每处远端执行都要申报命令来历 // audit-0805 08-08：webview 能力清单 = 三张登记表的共同前提

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
/// (--fork-session) 会写 pidfile（kind:"bg" + jobId）——那类由 session_map /
/// 远端后端的 kind 交互性过滤处理（Batch6-F21），与本处嵌套环境清洗无关。
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

// F-MA:CC 嵌套会话 env 清单移到 adapter/claude_code.rs（走 adapter.nested_env_to_scrub()）。

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Batch7-F23A：nudge skip 判定的纯函数对（单测钦定，见 MASTERPLAN §6）。
///
/// `pack_nudge_state`：终态物理尺寸 + fullscreen 位打包成一个可比较状态值。
/// fullscreen 占 bit 63（F11 borderless 全屏与 maximize 在"自动隐藏任务栏"下
/// inner 尺寸可能相同——状态位保证这类 #4095 高危过渡不被 skip）；宽度截 31 位
/// （物理像素远小于 2^31，不损失信息）。
pub(crate) fn pack_nudge_state(w: u32, h: u32, fullscreen: bool) -> u64 {
    ((fullscreen as u64) << 63) | (((w as u64) & 0x7FFF_FFFF) << 32) | h as u64
}

/// skip 当且仅当：曾经 nudge 过（last != 0）且 (尺寸+全屏态) 与上次执行完的
/// nudge 完全一致——典型即"最小化→恢复"。0 是安全哨兵：真实窗口尺寸非零，
/// pack 结果不可能为 0（0×0 在事件入口与 settle 双重滤除）。
pub(crate) fn nudge_should_skip(last_nudged: u64, packed: u64) -> bool {
    last_nudged != 0 && last_nudged == packed
}

/// 〔U2 · 第三波〕远端会话一宣告就起的那条 `remote-bind-scan` 线程（每 ~0.6s 扫一次
/// `ccm-rbind-<sid>` 标题、最多 ~9s）**要不要起**。带启动令牌的会话**不起**：
///
/// - ↗ 对它先走 `令牌 → HWND`（`bind::resolve_remote_front` 的第一条路），用不着标题；
/// - 不在 tmux 里的令牌会话**根本没有谁去设** `ccm-rbind-<sid>` 这个标题
///   （设它的是 tmux 外层的 `set-titles-string`）⇒ 那 15 次扫描注定白跑 9 秒；
/// - 在 tmux 里的令牌会话也不必预扫：令牌那扇窗关了、退到标题路时，点 ↗ 那一刻
///   `try_bind_with_retry` 会现扫（`ON_DEMAND_BIND_*`，最多 4s）—— 预扫只省那一次现扫的等待。
///
/// 没令牌（老后端 / 不是 cc-monitor 启动的）⇒ 照旧预扫：标题路是它唯一的路。
pub(crate) fn wants_title_prescan(rbind_token: Option<&str>) -> bool {
    rbind_token.is_none()
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

// 〔RL1 · V107〕这里先前是 `D7 阻-3` 那条缝（`ExitShutdownSinks` ＋ 它的收口点）：退出臂按现问的「退出行为」
// 收掉 monitor 另起的那个本机中转。中转并进本机常驻后端之后，本机固定两个进程（monitor ＋ 常驻后端），
// 中转随后端按「退出行为」留或退（`设计/01 §3.3b`）⇒ 退出臂里不再有第三个进程要收，那条缝连同它的判据一起删掉。

pub fn run() {
    // 启动 perf 测量起点
    let t0 = std::time::Instant::now();

    // issue #24：第一件事就是清嵌套标记——必须在任何线程 spawn 之前
    // （std::env::remove_var 修改进程级环境，单线程窗口内调用才稳妥；
    // 下面 logging::init 就会起 non_blocking writer 线程）。
    let scrubbed_env = scrub_env_vars(adapter::active().nested_env_to_scrub());

    // v2.0.0 (issue #4)：tracing 初始化提前到 Builder 之前 —— 一旦 init 全局
    // dispatcher 锁死，且我们要捕获 setup() 期间的所有 log。
    //
    // logging 模块内部把所有复杂度（rolling file appender / non_blocking writer /
    // ErrorEmitterLayer / EnvFilter reload）封死，对外只暴露 init + state。
    //
    // **monitor_data_dir 必须能解析**：这里用 dirs::home_dir 兜底，不依赖任何
    // 配置（避免 log 初始化跟 config 初始化循环依赖）。
    let monitor_data_dir = paths::resolve_monitor_data_dir()
        .unwrap_or_else(|| std::env::temp_dir().join("cc-monitor-fallback"));
    let logging_state = logging::init(&monitor_data_dir);
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

    // issue #9：single-instance lock。**必须是第一个 plugin**（Tauri 官方 plugin 要求）。
    // 第二个 cc-monitor 实例启动 → 触发本回调（在第一个实例里跑）→ 把主窗口
    // unminimize + show + set_focus → 第二个实例立即退出（plugin 内部处理）。
    // 详 src/doc/INVARIANTS.md § 16。
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();
    #[cfg(windows)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // 第二个实例若带 --background（cc auto-launch 竞态下偶发）→ 只 show 不抢焦点；
            // 普通双击拉起第二个实例则照常置前（用户显式想看）。
            let background = args.iter().any(|a| a == "--background");
            tracing::info!("second cc-monitor instance detected (background={background})");
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.unminimize();
                let _ = win.show();
                if !background {
                    let _ = win.set_focus();
                }
            }
        }));

        // WebView2 maximize / 全屏后内容错位修复（v2.14.0 引入，F12 加固重写）。
        // 根因是 WebView2 Runtime 内部（浏览器进程）在 maximize / restore / 全屏切换后
        // 丢失/挂起对宿主 bounds 更新的处理（WebView2Feedback #4095 族，微软未修）：
        // 宿主侧 put_Bounds 成功、容器 HWND 已是全尺寸，但合成层（"Intermediate D3D
        // Window"）停在旧尺寸 → 内容不铺满、周围留白。DOM 之下，前端 reflow 够不着。
        //
        // v2.14 的手段（±1px webview.set_size 抖动）机制上生效但对 Runtime 内部 bug
        // 不可靠（1px 差值可能被 Runtime 合并/丢弃），F12 升级为 controller 级三板斧
        // （with_webview 闭包内直接 COM 调用）：
        //   1. 双 rect SetBounds（h-1 → h）：让 Runtime 看到「变化后的 rect」重新 put_Bounds
        //   2. NotifyParentWindowPositionChanged：微软文档明示的宿主位置变化通知
        //   3. SetIsVisible(false→true) 翻转：强制重建/重挂合成 visual，对 #4095 族最有效；
        //      仅 maximize/fullscreen 时做（普通拖拽 resize 不翻，避免理论上的闪烁）
        //
        // ⚠ 最小化守卫（F12，修"restore 后数秒点不了"）：tao 0.35 在 WM_SIZE(SIZE_MINIMIZED)
        // 时发 Resized(0,0)（不过滤），而 wry 自己的 subclass 明确跳过 SIZE_MINIMIZED——
        // 最小化时把 controller bounds 打成 0×0 会让 renderer 视口归零、进入挂起态，
        // restore 后画面先回、输入 hit-test 层数秒才重建。入口按 0×0 早退 + 线程动作前
        // 二次守卫（去抖 60ms 期间可能又被最小化），对齐 wry 的保护语义。
        //
        // 去抖：resize 期间（含拖拽）每个事件 bump 一个 generation；后台线程等到连续 60ms
        // 没有新事件（= 过渡稳定）再动手。nudge_pending 保证一个突发 resize 只有一个
        // 去抖线程在飞。with_webview 的闭包由 tauri 派发到主线程执行——闭包内只做 COM
        // 调用、禁止 sleep（同一闭包内连续两次不同 rect 已满足重钉条件，v2.14 的 16ms
        // 间隔不再需要）。
        builder = builder.on_window_event({
            use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
            use std::sync::Arc;
            use std::time::Duration;
            let resize_gen = Arc::new(AtomicU64::new(0));
            let nudge_pending = Arc::new(AtomicBool::new(false));
            // Batch7-F23A：上次 nudge **闭包执行完毕**时的 (尺寸+全屏态) 打包值
            // （pack_nudge_state；0=从未）。最小化→恢复回到同状态时合成层没有错位
            // 理由（#4095 是 resize/maximize **过渡** bug），却会因 is_maximized()
            // 为 true 走 SetIsVisible 翻转 → 拆挂合成 visual 瞬间露白底（用户实测
            // 白闪）。同状态直接 skip 全部 COM 动作。两条取舍（审计 D 复核后留档）：
            // ① store 在 with_webview 闭包尾执行——"执行完"= 闭包跑完，单个 COM
            //   调用失败仍记录（COM 级失败不重试；派发失败才不记录）；
            // ② 拖拽一圈回到原尺寸的 settle 也会被 skip（终态==上次已修复态，
            //   wry 自身的 WM_SIZE 路径已实时跟踪中间态，残余风险接受）。
            // F11 全屏与 maximize 同 inner 尺寸的角例由打包值里的 fullscreen 位
            // 区分（状态变了照跑三板斧）。
            let last_nudged = Arc::new(AtomicU64::new(0));
            move |window, event| {
                let size = match event {
                    tauri::WindowEvent::Resized(s) => *s,
                    _ => return,
                };
                // 最小化：绝不动 webview bounds（见块头 ⚠），也不 bump gen
                if size.width == 0 && size.height == 0 {
                    return;
                }
                resize_gen.fetch_add(1, Ordering::SeqCst);
                // 已有一个去抖线程在飞 → 它会读到新的 gen 自行续等，不再 spawn
                if nudge_pending.swap(true, Ordering::SeqCst) {
                    return;
                }
                let window = window.clone();
                let resize_gen = resize_gen.clone();
                let nudge_pending = nudge_pending.clone();
                let last_nudged = last_nudged.clone();
                std::thread::spawn(move || {
                    let mut last = resize_gen.load(Ordering::SeqCst);
                    loop {
                        std::thread::sleep(Duration::from_millis(60));
                        let now = resize_gen.load(Ordering::SeqCst);
                        if now == last {
                            break;
                        }
                        last = now;
                    }
                    nudge_pending.store(false, Ordering::SeqCst);
                    // 二次守卫：去抖期间窗口可能又被最小化 / 尺寸归零
                    if window.is_minimized().unwrap_or(false) {
                        tracing::info!("nudge skip: window minimized during debounce");
                        return;
                    }
                    let target = match window.inner_size() {
                        Ok(t) if t.width > 0 && t.height > 0 => t,
                        _ => {
                            tracing::info!("nudge skip: zero/unknown inner_size");
                            return;
                        }
                    };
                    let maximized = window.is_maximized().unwrap_or(false);
                    let fullscreen = window.is_fullscreen().unwrap_or(false);
                    let flip = maximized || fullscreen;
                    // Batch7-F23A：同(尺寸+全屏态) skip（典型 = 最小化恢复）。
                    // 判定与打包是纯函数（单测见 nudge_skip_tests）。
                    let packed =
                        pack_nudge_state(target.width, target.height, fullscreen);
                    if nudge_should_skip(last_nudged.load(Ordering::SeqCst), packed) {
                        tracing::info!(
                            "nudge skip: size+state unchanged {}x{} fs={fullscreen} (restore-from-minimize path)",
                            target.width,
                            target.height
                        );
                        return;
                    }
                    let Some(webview) = window.webviews().into_iter().next() else {
                        tracing::warn!("nudge skip: no webview on window");
                        return;
                    };
                    tracing::info!(
                        "nudge settle: target={}x{} maximized={maximized} fullscreen={fullscreen} flip={flip}",
                        target.width,
                        target.height
                    );
                    let last_nudged_in = last_nudged.clone();
                    let res = webview.with_webview(move |pw| {
                        // RECT 必须来自 webview2-com 0.38 配对的 windows 0.61
                        // （windows-wv2 rename，见 Cargo.toml），0.56 的类型不互通
                        use windows_wv2::Win32::Foundation::RECT;
                        let controller = pw.controller();
                        let full = RECT {
                            left: 0,
                            top: 0,
                            right: target.width as i32,
                            bottom: target.height as i32,
                        };
                        let shrunk = RECT {
                            bottom: target.height.saturating_sub(1) as i32,
                            ..full
                        };
                        // 每个 COM 调用的失败单独 warn（不再静默）：理论上存在不对称失败
                        // ——如 SetIsVisible(false) 成功而 (true) 失败会让 webview 停在隐藏态，
                        // 无日志就无从取证。失败不中断后续调用（终态尽量推向可见+正确 bounds）。
                        unsafe {
                            if let Err(e) = controller.SetBounds(shrunk) {
                                tracing::warn!("nudge SetBounds(shrunk) failed: {e}");
                            }
                            if let Err(e) = controller.SetBounds(full) {
                                tracing::warn!("nudge SetBounds(full) failed: {e}");
                            }
                            if let Err(e) = controller.NotifyParentWindowPositionChanged() {
                                tracing::warn!("nudge NotifyParentWindowPositionChanged failed: {e}");
                            }
                            if flip {
                                if let Err(e) = controller.SetIsVisible(false) {
                                    tracing::warn!("nudge SetIsVisible(false) failed: {e}");
                                }
                                if let Err(e) = controller.SetIsVisible(true) {
                                    tracing::warn!("nudge SetIsVisible(true) failed: {e}");
                                }
                            }
                        }
                        // 闭包执行完毕才记录（with_webview 的 Ok 只代表"已派发到主
                        // 线程"——审计 D 修订：在这里 store 才是"执行完"的语义）
                        last_nudged_in.store(packed, Ordering::SeqCst);
                    });
                    if let Err(e) = res {
                        tracing::warn!("nudge with_webview failed: {e}");
                    }
                });
            }
        });
    }

    // ST1「关窗改隐藏」的另一半（`设计/01 §1.3` · `70 §1.3 F`）：设置窗关窗 = **隐藏**，永不自己销毁
    // ⇒ 它会把进程吊住（Tauri 是「最后一个窗口销毁才退出」）。主窗一销毁，就把它一起 destroy 掉，
    //    让「最后一个**看得见**的窗口关掉 ⇒ 进程退出」照旧成立。决策在纯函数里，这里只执行。
    // ⚠ 刻意**不** `app.exit(0)`：那会改变「主窗关了、viewer 窗还开着」时的行为，
    //   退出行为整体搬家归 `01 §3.3b`（第三波 B2），这里不预先改它的语义。
    builder = builder.on_window_event(|window, event| {
        if !matches!(event, tauri::WindowEvent::Destroyed) {
            return;
        }
        use tauri::Manager;
        let app = window.app_handle();
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

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
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
            // `src/bridge/tauri.sidecar.conf.json`，发版那一步用
            // `npx tauri build --config src/bridge/tauri.sidecar.conf.json` 注入。
            // ⚠ 它**刻意不进基础 `tauri.conf.json`** —— 进了会让 `cargo test` 也要求当前
            // target 的那份二进制存在。⇒ **「基础配置里没有」≠「没配」，别再把这两句写成一句。**
            //
            // ⇒ 降级那一支仍然在（裸 exe · 开发树走的就是它），只是**不再是常数**。
            // 它**不是**「接线没做」：接线在这里，是**这一份产物里没带**，两者的区别就在那个
            // tagged 返回值上。它**刻意不扫仓库 dev 产物** —— backend 一起来就无条件往
            // tmux server 装三条全局 hook 且没有开关，扫到 dev 产物就起它 = 去改用户真实
            // tmux 的状态（F05 摸底 §2.5）。
            {
                use backend::control::local_backend::Resolved;
                // P2z（定框 C10）：exe 旁边没有本机后端时，把**已内嵌**的那份释放到本机再起 ——
                // 「单 exe 也能起后端进程」那句话的落点。
                //
                // 两样宿主知识在这里给（backend 层不认识它们）：
                //   · 落点 `~/.cc-monitor/bin`：与远端自部署同一个目录，但**文件名带 build_id**
                //     ⇒ 与远端那份结构上不可能撞（理由见 `extract_embedded_to` 头注的 D1 段）。
                //   · 当前 arch：`sftp::backend_binary` 按它挑内嵌字节；缺内嵌（`cfg(embedded_backends)`
                //     未置）时给 None，函数会诚实降级、不伪造理由。
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
                        // 两条起法只有手动那条会让用户看见（`crate::backend::control::backend_control::backend_start`
                        // 回 `Err` ⇒ 前端 toast）。而这一条 —— 用户每天真正走的那条 ——
                        // 回修前是 `tracing::info!`，连 `warn` 都不是。
                        //
                        // 触发场景不是理论：口按家目录算死，`hello_verdict` 逐字比
                        // `BACKEND_BUILD_ID` ⇒ **升级 monitor 之后上一次脱离留下的那个
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
                                tracing::warn!("本机后端未启动: {reason}；找过 {looked_at:?}");
                                use tauri_plugin_notification::NotificationExt;
                                if let Err(e) = app
                                    .notification()
                                    .builder()
                                    .title("本机后端没起来")
                                    .body(&next_step)
                                    .show()
                                {
                                    // 通知发不出去也要留痕，别让「说出口」这件事静默失败。
                                    tracing::warn!("本机后端拒绝的通知发不出去（{e}）：{next_step}");
                                }
                            }
                            None => {
                                tracing::info!("本机后端未启动: {reason}；找过 {looked_at:?}")
                            }
                        }
                    }
                }
            }

            // 面 A 通道：绑回环、起路由器，外部前端（下一波接进文件窗口）经它说 call/subscribe。
            // 起不来只出声、不退回别的路（`D11`）；钥匙永不进日志（`chan::host` 头注）。
            if let Err(e) = tauri::async_runtime::block_on(chan::host::start()) {
                tracing::warn!("面 A 通道没起来：{e}");
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

            // F-MA：agent 数据目录 + 会话源布局都走活跃适配器（Claude Code = 第一个实例；
            // data_root 仍是三级回退 用户配置 → CLAUDE_CONFIG_DIR → ~/.claude）。子目录名不再硬编码。
            let agent = adapter::active();
            let claude_dir = agent.data_root().ok_or("agent data dir not found")?;
            tracing::info!("monitor using agent [{}] data dir: {}", agent.id(), claude_dir.display());
            let projects_dir = adapter::records_dir(&claude_dir);
            let sessions_dir = adapter::liveness_dir(&claude_dir);
            // v2.3.0 issue #11：任务追踪文件根（CC = tasks）
            let tasks_dir =
                adapter::tasks_dir(&claude_dir).unwrap_or_else(|| claude_dir.join("tasks"));

            // monitor 自己的数据目录：~/.claude/work
            let monitor_data_dir = paths::resolve_monitor_data_dir().ok_or("no data dir")?;
            tracing::info!("monitor_data_dir: {}", monitor_data_dir.display());

            // v2.0.0：把 AppHandle 注入给 ErrorEmitterLayer（之前一直是 None，
            // setup 期间的 ERROR 只写 log；从这里开始 ERROR 才会弹前端 toast）
            logging_state.install_error_emitter(app.handle().clone());

            // v1.7.1：把当前 exe 路径记到 auto-launch.json，让 cc function 能在用户启用
            // auto-launch 时主动启动 monitor（不硬编码安装路径）
            auto_launch::update_monitor_path_on_startup(&monitor_data_dir);

            // v1.7：BindRegistry 监听 ps-await/ → EnumWindows → 写 ps-registry/。
            // SidHwndCache 持久化 sid → 拉前所需信息（含复合指纹）。
            let bind_registry = bind::BindRegistry::spawn(monitor_data_dir.clone());
            let sid_hwnd_cache =
                bind::SidHwndCache::load(monitor_data_dir.join("sid-hwnd-cache.json"));

            // Feature ②（远端 Tab ↗ 拉前）：纯内存的 sid → hwnd 缓存。远端 session
            // 加入时扫本地窗口找 `ccm-rbind-<sid>` 标题（wrapper 在远端设的 OSC 标题，
            // 经 ssh 透传到本地 Windows Terminal）并绑定。bring_remote_terminal_to_front
            // IPC 取 State<Arc<RemoteHwndCache>>（INVARIANT § 8：必须 manage，见下方）。
            let remote_hwnd_cache = bind::RemoteHwndCache::new();

            // SessionMap = Claude Code 自己维护的 ~/.claude/sessions/<PID>.json
            let (session_map, session_changes) =
                session_map::SessionMap::load_with_changes(
                    sessions_dir,
                    load_show_bg_sessions(),
                );

            // Watcher: 只对活跃 session 的 jsonl emit
            let active_filter: watcher::ActiveFilter = {
                let map = session_map.clone();
                Arc::new(move |sid: &str| map.is_session_active(sid))
            };

            // v2.4 (修首次启动乱序)：watcher 直接调 on_line 回调（取代之前的 mpsc
            // 中间层）。回调内同步 parse + record() —— history buffer 在 watcher
            // 线程内同步落盘，初始全量扫完成 = history 完整 = frontend-ready 触发
            // replay 时 snapshot 一定完整。
            //
            // 旧设计：watcher tx → mpsc → tauri::async_runtime::spawn drain → record。
            // async drain 跟 frontend-ready 是竞态：drain 没追上时 snapshot 不完整，
            // 部分历史漏到 live emit 路径 → 跟 chunked replay 错位 → 首次启动乱序。
            // F5 因 backend 已稳定看不到 bug。详 watcher.rs::spawn_watcher 注释。
            let replay = Arc::new(event_replay::EventReplay::new());
            // v2.4.2 issue #2: watcher 改成一次 process_file 给一批 lines，
            // lib.rs 这里 parse 整批后一次 on_line_batch 给 EventReplay。
            // EventReplay 按 batch 大小分流（详 event_replay::on_line_batch 注释）。
            let on_batch: watcher::BatchHandler = {
                let replay = replay.clone();
                let handle = app.handle().clone();
                // 〔ST3〕本机 watcher 只读本机的 jsonl ⇒ 看不懂的东西记在本机名下（载荷上不带 origin，线上形状不变）。
                let local = crate::origin::Origin::local();
                Arc::new(move |lines: Vec<watcher::JsonlLine>| {
                    // 本地行载荷无 origin；远端行由 ssh_source 传那台的 origin。
                    let payloads = batch_to_payloads(lines, &local);
                    replay.on_line_batch(&handle, payloads);
                })
            };

            // 本地 jsonl-watcher：**始终** spawn（与 SSH-remote 引入前完全一致）。
            // 远端（如启用）是纯附加数据源（见下方 load_remote_configs 块），不影响这里。
            let watcher_handle = watcher::spawn_watcher(projects_dir, active_filter, on_batch);
            let force_rescan_tx = watcher_handle.force_rescan_tx;
            let initial_scan_done = watcher_handle.initial_scan_done;

            // 〔U4b · 第四波〕`session_facts` 的出口：两条后端流交来的会话事实在这里落地（见那个模块的头注）。
            //   装在两个 emitter 之前 —— 装之前交来的容器事实只记账，`frontend-ready` 对账会整份重发。
            {
                let handle = app.handle().clone();
                crate::session_facts::install_sink(move |fact| match fact {
                    crate::session_facts::Fact::Container { sid, container } => {
                        if let Err(e) = handle.emit(
                            bridge::events::SESSION_CONTAINER,
                            &bridge::SessionContainerPayload {
                                session_id: sid,
                                container: container.as_wire().to_string(),
                            },
                        ) {
                            tracing::warn!("emit session-container failed: {e}");
                        }
                    }
                    // 本机收割：这条可重连的会话，它的 tmux 会话没了 ⇒ 已结束（与本机 removed 臂的 Archive 同形）。
                    // 只对**仍在** idle 账本里的 sid 落地：收割算完到这里之间它可能已经复活（`added` 臂清过）。
                    crate::session_facts::Fact::LocalIdleGone { sid } => {
                        let origin = crate::backend::control::inbound_client::LOCAL_ORIGIN;
                        if !ssh_source::snapshot_idle_for_origin(origin).contains(&sid) {
                            return;
                        }
                        ssh_source::clear_idle(&sid);
                        if let Err(e) = handle.emit(
                            bridge::events::SESSION_ENDED,
                            &bridge::SessionEndedPayload {
                                session_id: sid.clone(),
                            },
                        ) {
                            tracing::warn!("emit session-ended（本机收割）failed: {e}");
                        } else {
                            tracing::info!("session ended（本机 tmux 也没了）: {sid}");
                        }
                    }
                });
            }

            // session 集合变化 emitter（本地）：
            //   - added：通知 jsonl-watcher 主动重扫该 session（修 Bug 2-A 竞态）
            //             + 调 SidHwndCache.record 把 sid → hwnd 绑定持久化
            //   - removed：透传 session-ended 给前端，Tab 灰显归档
            //              + 调 SidHwndCache.forget 清理过期 sid
            {
                let handle = app.handle().clone();
                let session_map_for_emitter = session_map.clone();
                let bind_for_emitter = bind_registry.clone();
                let cache_for_emitter = sid_hwnd_cache.clone();
                let spawned = std::thread::Builder::new()
                    .name("session-changes-emitter".into())
                    .spawn(move || {
                        while let Ok(change) = session_changes.recv() {
                            for sid in &change.added {
                                tracing::info!("session added: {sid}, triggering jsonl rescan");
                                if let Err(e) = force_rescan_tx.send(sid.clone()) {
                                    tracing::warn!("force_rescan send failed for {sid}: {e}");
                                }
                                // 尝试绑定 sid → hwnd（通过 claude_pid 的 parent PS）
                                if let Some(info) = session_map_for_emitter.lookup(sid) {
                                    let _ =
                                        cache_for_emitter.record(sid, info.pid, &bind_for_emitter);
                                }
                                // 〔U4b · G2〕本机也有可重连了 ⇒ 会话（重新）变活时清掉它的 idle 标记（远端那一臂同一条）。
                                ssh_source::clear_idle(sid);
                                // 会话（重新）变活 → 通知前端复活已归档的本地 Tab（resume：
                                // 崩溃→灰显→/resume 后免 F5 即回 live）。**仅在 PID 真探活通过
                                // 时发**：崩溃残留的旧 sessions/<PID>.json 被后续文件事件重扫也
                                // 会进 added（心跳已从 by_id 删过它），无 liveness 门会误复活刚
                                // 归档的死会话。is_session_active 读 by_id（此刻 = 本次重扫的
                                // next）并 re-probe 进程，门住该竞态。session-ended 的对称补全。
                                if session_map_for_emitter.is_session_active(sid) {
                                    // Batch7-F24：带 pidfile 元信息——前端无 Tab 时建
                                    // 骨架（中途出现的 bg 会话需要 kind 才有 ⚙/树状）。
                                    let info = session_map_for_emitter.lookup(sid);
                                    let payload = bridge::SessionStartedPayload {
                                        session_id: sid.clone(),
                                        cwd: info.as_ref().map(|i| i.cwd.clone()),
                                        kind: info.as_ref().and_then(|i| i.kind.clone()),
                                        name: info.as_ref().and_then(|i| i.name.clone()),
                                    };
                                    if let Err(e) =
                                        handle.emit(bridge::events::SESSION_STARTED, &payload)
                                    {
                                        tracing::warn!("emit session-started failed: {e}");
                                    } else {
                                        tracing::info!("session started (revive): {sid}");
                                    }
                                }
                            }
                            // 〔U4b · 第四波 · G2〕**本机也按容器在不在分「可重连 / 已结束」**，与远端同一条判定
                            //（`ssh_source::classify_removed`；`INVARIANTS §40`：本机 ＝ 不走 ssh 的远端）。
                            // 此前这里写着「本地路径没有 idle-tmux 灰点（`SESSION_IDLE` 是远端专有）」、无条件
                            // 发 `session-ended` ⇒ 本机 claude 退了而 tmux 会话还在时，tab 说「已结束」（`U4.md §0.1` G2）。
                            // 查的**只是本机那一格**原文（`find_local_tmux_origin_for_sid`，不跨 origin 猜）；
                            // `/branch` 那一形（`Superseded`）照旧恒归档 —— 本地 diff 早就判得出它（P3 刀 0），
                            // `session_map.rs` 那段「要等有人给那个 emitter 接上 `classify_removed` 才第一次生效」从此生效。
                            // 「可重连 → 已结束」的产出者是本机收割器（`local_backend::absorb_local_frame` 的两个 tmux 臂，
                            // 结论经 `session_facts` 回到下面 setup 装的那个出口）。
                            // 绑定照旧两种 cause 都忘（`SidHwndCache::apply_local_removal` 头注；本机 ↗ 只在 Windows 上有，
                            // 而 Windows 没有 tmux ⇒ 本机永远走不到 `Idle`，那条头注的行为这一拍不动）。
                            for removed in change.removed {
                                cache_for_emitter.apply_local_removal(&removed);
                                crate::session_facts::forget(&removed.sid);
                                let tmux_origin = ssh_source::find_local_tmux_origin_for_sid(&removed.sid);
                                let disposition =
                                    ssh_source::classify_removed(tmux_origin, removed.cause);
                                let sid = removed.sid;
                                match disposition {
                                    ssh_source::RemovedDisposition::Idle { origin } => {
                                        ssh_source::mark_idle(&origin, &sid);
                                        let payload = bridge::SessionIdlePayload {
                                            session_id: sid.clone(),
                                        };
                                        if let Err(e) =
                                            handle.emit(bridge::events::SESSION_IDLE, &payload)
                                        {
                                            tracing::warn!("emit session-idle failed: {e}");
                                        } else {
                                            tracing::info!("session idle-tmux（本机）: {sid}");
                                        }
                                    }
                                    ssh_source::RemovedDisposition::Archive => {
                                        ssh_source::clear_idle(&sid);
                                        let payload = bridge::SessionEndedPayload {
                                            session_id: sid.clone(),
                                        };
                                        if let Err(e) =
                                            handle.emit(bridge::events::SESSION_ENDED, &payload)
                                        {
                                            tracing::warn!("emit session-ended failed: {e}");
                                        } else {
                                            tracing::info!("session ended: {sid}");
                                        }
                                    }
                                }
                            }
                            // issue #23：红绿灯——status/waitingFor 变了才会出现在这里
                            // （session_map 重扫时逐会话比对），透传给前端改灯色。
                            for act in change.status_changed {
                                let payload = bridge::SessionActivityPayload {
                                    session_id: act.session_id,
                                    status: act.status,
                                    waiting_for: act.waiting_for,
                                };
                                if let Err(e) =
                                    handle.emit(bridge::events::SESSION_ACTIVITY, &payload)
                                {
                                    tracing::warn!("emit session-activity failed: {e}");
                                }
                            }
                        }
                    });
                if let Err(e) = spawned {
                    tracing::error!(
                        "failed to spawn session-changes-emitter thread: {e}; \
                         session 增减事件将丢失，Tab 不会自动归档 / 新会话可能丢首屏"
                    );
                }
            }

            // issue #20：远端当前活跃 sid 集 —— session_map 的远端对应物，专供
            // frontend-ready 重放后对账（远端 sid 不在 session_map，#19 的本地对账
            // 覆盖不到）。唯一写者是下面的 remote-session-emitter（backend 的
            // added/removed 与断连 flush 走同一 remote_tx 通道，集合恒等于"前端当前
            // 应视为 live 的远端 sid"）。无远端配置时恒空，对账自然 no-op。
            // 违反此约束见 src/doc/INVARIANTS.md § 24。
            let remote_active: Arc<parking_lot::Mutex<std::collections::HashSet<String>>> =
                Arc::new(parking_lot::Mutex::new(std::collections::HashSet::new()));

            // SSH-remote Phase 0 (issue #15)：远端是**纯附加**数据源。config.json 的
            // `remote.enabled = true` 且配置完整 → 在本地 watcher 之外**额外**起一条
            // ssh_source::run（aggregate：本地 + 远端 session 同时显示）。否则（默认 /
            // 无 remote 配置）此块不执行，本地路径与历史 bit-for-bit 一致。
            let remote_cfgs = load_remote_configs();
            if !remote_cfgs.is_empty() {
                tracing::info!(
                    "remote mode ENABLED (additive): {} SSH data source(s) (local jsonl-watcher still running)",
                    remote_cfgs.len()
                );

                // 远端独立 session 通道：ssh_source::run 持 sender；这里起一条**专用**的
                // 精简 emitter drain 它。
                //   - added（Feature ②）：远端 Tab 由 line 帧经 ensureTab 创建（不调
                //     force_rescan_tx，那是本地 jsonl 专用）。但要扫本地窗口找
                //     `ccm-rbind-<sid>` 标题绑定 hwnd，供 ↗ 拉前。wrapper 设标题经 ssh
                //     透传到本地 WT 有延迟（OSC 序列要等远端 shell 起来 + 透传），故
                //     +1500/3000/4500/6000ms 重试扫描，首次绑定成功即停。
                //   - removed：emit session-ended（远端 Tab 归档）+ forget 远端绑定。
                let (remote_tx, remote_rx) =
                    std::sync::mpsc::channel::<session_map::SessionChange>();
                {
                    let handle = app.handle().clone();
                    let remote_cache_for_emitter = remote_hwnd_cache.clone();
                    let remote_active_for_emitter = remote_active.clone();
                    let spawned = std::thread::Builder::new()
                        .name("remote-session-emitter".into())
                        .spawn(move || {
                            while let Ok(change) = remote_rx.recv() {
                                // issue #20：先维护远端活跃集（再做 emit/扫描等副作用）。
                                // 断连 flush 的 removed 也从这里清掉 → 断线期间集合为空，
                                // 与前端"全部已归档"的视图一致。
                                {
                                    let mut active = remote_active_for_emitter.lock();
                                    for sid in &change.added {
                                        active.insert(sid.clone());
                                    }
                                    for removed in &change.removed {
                                        active.remove(&removed.sid);
                                    }
                                }
                                // added 先处理：每个新 sid 起一条**独立** std::thread
                                // 做带 sleep 的重试扫描。
                                //
                                // 为何用 std::thread 而非 tauri::async_runtime::spawn：扫描
                                // 本体是同步 Win32（find_window_by_marker_substr，
                                // INVARIANT § 10 要求 Win32 同步调用不能压在 IPC/async
                                // 派发线程上），无任何 .await；用专用 std::thread + sleep
                                // 最简单且与 async runtime 是否就绪完全解耦（本块身处 std::thread
                                // 里，调 async_runtime::spawn 虽也可行但平添对全局 runtime 的
                                // 隐性依赖，无收益）。线程扫完即退，不长驻。
                                for sid in change.added {
                                    // audit-fixes F03.2：会话（重新）变活 → 清 idle 灰灯标记（resume/新会话）。
                                    ssh_source::clear_idle(&sid);
                                    // 〔U2〕带启动令牌的会话不预扫标题（理由见 `wants_title_prescan`）。
                                    // 令牌账本先于这条 `added` 记好（`ssh_source` 收 `SessionAdded` 时先 `note` 再发）。
                                    let token = bind::remote_rbind_tokens().token_of(&sid);
                                    if !wants_title_prescan(token.as_deref()) {
                                        continue;
                                    }
                                    let cache = remote_cache_for_emitter.clone();
                                    let spawn_res = std::thread::Builder::new()
                                        .name("remote-bind-scan".into())
                                        .spawn(move || {
                                            // 每 ~0.6s 扫一次、最多 ~9s，命中即停。比固定 4 次更稳健：
                                            // claude 启动时也会设标题（实测 ~once），wrapper 每 0.3s
                                            // 重刷整个 ~9s 窗口；多次扫描覆盖该窗口，大幅降低"恰好每次
                                            // 扫描都撞上 claude 标题而漏绑"的概率（EnumWindows 廉价，命中即停）。
                                            for _ in 0u32..15 {
                                                std::thread::sleep(
                                                    std::time::Duration::from_millis(600),
                                                );
                                                if cache.try_bind(&sid) {
                                                    tracing::info!(
                                                        "remote bind: sid={sid} → hwnd bound"
                                                    );
                                                    break;
                                                }
                                            }
                                        });
                                    if let Err(e) = spawn_res {
                                        tracing::warn!(
                                            "failed to spawn remote-bind-scan thread: {e}; 远端 Tab ↗ 拉前将不可用"
                                        );
                                    }
                                }
                                for removed in change.removed {
                                    let sid = removed.sid;
                                    // 〔U4b〕容器是那个进程的事实 ⇒ 它离开活跃集就忘掉（不跨进程沿用）。
                                    crate::session_facts::forget(&sid);
                                    // audit-fixes F03.2（灰灯三态分流）：backend-removed（claude 进程没了，权威）
                                    // 到达时，看该 sid 的 `@ccm_sid` 是否仍出现在某 origin 的 TmuxSessions 帧里：
                                    //   - Some(origin)=tmux 会话尚在（空 shell）→ **idle-tmux 灰灯**：mark_idle +
                                    //     emit SESSION_IDLE + **不 forget 绑定**（登录 shell 的 ssh 窗仍活、↗ 拉前有效）。
                                    //   - None=tmux 也没了 → **archived**（原逻辑）：clear_idle + forget + SESSION_ENDED。
                                    // 判据 command-agnostic（见 ssh_source::tmux_origin_for_sid）：帧的新鲜度**由 hook 决定**
                                    // （P5 删 ticker 后后端零定时器）——hook 覆盖到的近乎即时，覆盖不到的可能**永不刷新**。退出
                                    // 瞬间 command 列可能仍是 claude，故用 backend-removed 判"claude 死"、@ccm_sid
                                    // present 判"tmux 在"。**§24**：removed sid 已在上方从 remote_active 移出，idle 天然
                                    // 在集合外；idle 只写独立 REMOTE_IDLE（唯一写者=本 emitter），**不新增 remote_active 写点**。
                                    //
                                    // ★ S0：上面那段「查 @ccm_sid 还在不在」**只对 `Gone` 成立**。
                                    // `Superseded`（同 pidfile 原地换 sid，即 /branch）时旧 sid 的
                                    // tmux 格子确实还在、但已经改挂新 sid ⇒ 查快照必然误判成灰点，
                                    // 且那份快照在 P5 删掉 ticker 后没有任何事件路径会刷新它
                                    // ⇒ 永久灰点、按旧 sid 也 attach 不上（用户实测「杀不掉」）。
                                    // 故 cause 先于快照裁决，见 `classify_removed` 的文档注释。
                                    //
                                    // ★ P0（#60）：**这条 `info!` 只回答一个问题 —— 这条 removed 到没到这段。**
                                    // 病史：`control-parity` 前身 `issue-triage` 的 T7 把 #60 的现象 1
                                    // （带外杀掉后 tab 不变灰）全链级稳定复现 2/2，而 claude 死的那一刻
                                    // 日志里**四条归宿日志一条都没有**（`remote session idle-tmux` /
                                    // `remote session ended` / 两条 emit 失败的 warn）⇒ 只能推断
                                    // `classify_removed` 没跑到，但**分不清是帧没到、还是到了却在更上游被丢**。
                                    //
                                    // ⚠ **它不回答「帧为什么没到」** —— 只把搜索面从「整条链」砍成
                                    // 「上游 or 分类」两半。别把它当成 #60 的根因定位。
                                    //
                                    // ⚠ 两个入参**先绑定再打印再移交**，不是为了打日志多调一次
                                    // `find_tmux_origin_for_sid`（那会是行为改动：它读的是共享快照）。
                                    // `RemovalCause` 是 `Copy`，`tmux_origin` 打印时只借用。
                                    let tmux_origin = ssh_source::find_tmux_origin_for_sid(&sid);
                                    tracing::info!(
                                        "remote removed 到达: sid={sid} cause={:?} tmux_origin={:?}",
                                        removed.cause,
                                        tmux_origin
                                    );
                                    let disposition =
                                        ssh_source::classify_removed(tmux_origin, removed.cause);
                                    remote_cache_for_emitter
                                        .apply_remote_disposition(&sid, &disposition);
                                    match disposition {
                                        ssh_source::RemovedDisposition::Idle { origin } => {
                                            ssh_source::mark_idle(&origin, &sid);
                                            let payload = bridge::SessionIdlePayload {
                                                session_id: sid.clone(),
                                            };
                                            if let Err(e) =
                                                handle.emit(bridge::events::SESSION_IDLE, &payload)
                                            {
                                                tracing::warn!("emit remote session-idle failed: {e}");
                                            } else {
                                                tracing::info!("remote session idle-tmux: {sid}");
                                            }
                                        }
                                        ssh_source::RemovedDisposition::Archive => {
                                            ssh_source::clear_idle(&sid);
                                            let payload = bridge::SessionEndedPayload {
                                                session_id: sid.clone(),
                                            };
                                            if let Err(e) =
                                                handle.emit(bridge::events::SESSION_ENDED, &payload)
                                            {
                                                tracing::warn!("emit remote session-ended failed: {e}");
                                            } else {
                                                tracing::info!("remote session ended: {sid}");
                                            }
                                        }
                                    }
                                }
                                // Batch9-F27：远端红绿灯——backend session_status 帧/
                                // 宣告初始值经 status_changed 透传（与本地 emitter
                                // 同形状，前端 sid-keyed 零改动）。
                                for act in change.status_changed {
                                    let payload = bridge::SessionActivityPayload {
                                        session_id: act.session_id,
                                        status: act.status,
                                        waiting_for: act.waiting_for,
                                    };
                                    if let Err(e) =
                                        handle.emit(bridge::events::SESSION_ACTIVITY, &payload)
                                    {
                                        tracing::warn!("emit remote session-activity failed: {e}");
                                    }
                                }
                            }
                        });
                    if let Err(e) = spawned {
                        tracing::error!(
                            "failed to spawn remote-session-emitter thread: {e}; 远端 Tab 不会自动归档"
                        );
                    }
                }

                // 每台远端各起一条 ssh_source::run（多机 #30），与本地 watcher 走相同出口
                // （batch_to_payloads → on_line_batch）；session 变化共享 remote_tx → 上面那
                // 唯一的 remote-session-emitter（session 变化 host 无关，按 sid 维护）。
                // `connected` 是 connection-healthy signal（每台一份）：stream_loop 收到 backend
                // hello 时置 true，run() 的重连循环据此判定本次是否连上过（连上过→下次立即快速
                // 重连，否则指数退避）。远端**不**门控 frontend-ready（本地 watcher 的
                // initial_scan_done 才门控 replay；远端是实时流，无"初始扫完成"概念）。
                for cfg in remote_cfgs {
                    tracing::info!(
                        "  remote host [{}]: {}@{}:{}",
                        cfg.origin_label(),
                        cfg.user,
                        cfg.host,
                        cfg.port
                    );
                    // P2s（C8②）：起法包成**闭包**，把手交给 `backend_control` ——
                    // 那一层只按 origin 找把手，不认识 ssh（也不该认识）。
                    // 原来这里是直接 `spawn` 且**把 JoinHandle 丢掉** ⇒ 远端流起了就再也停不下来，
                    // 「每台机一个开关」在远端那侧根本无从谈起。
                    let origin = cfg.origin_label();
                    let replay_for_ssh = replay.clone();
                    let app_for_ssh = app.handle().clone();
                    let tx_for_ssh = remote_tx.clone();
                    let spawn_one = move || {
                        let cfg = cfg.clone();
                        let replay = replay_for_ssh.clone();
                        let app = app_for_ssh.clone();
                        let tx = tx_for_ssh.clone();
                        tauri::async_runtime::spawn(async move {
                            let label = cfg.origin_label();
                            // `connected` 每条流各一份：重起的那条不许继承上一条的健康状态，
                            // 否则「上次连上过 ⇒ 立即快速重连」这个判断会拿着旧账做决定。
                            let connected = Arc::new(std::sync::atomic::AtomicBool::new(true));
                            if let Err(e) =
                                ssh_source::run(cfg, replay, app, tx, connected).await
                            {
                                // S8/S9 会把"connection dropped"做成显眼的前端提示；先大声 log。
                                tracing::error!("ssh_source::run [{label}] exited: {e}");
                            }
                        })
                    };
                    let first = spawn_one();
                    backend::control::backend_control::register_remote(origin, Box::new(spawn_one), first);
                }
                // audit-fixes F03.2：tmux 存活对账**从 8s poller 改为收帧驱动**（甲-evented，零轮询）——
                // 收割器现落在 `ssh_source::stream_loop` 的 `TmuxSessions` 帧臂（backend **事件驱动**推帧即算），
                // 复用 `tmux_reconcile::reconcile_step`。故此处不再 spawn poller（`run_tmux_reconcile_poller` 已删）。
            }

            // 焦点同步功能已移除：Windows 11 默认 WT 是单进程多窗口架构，
            // GetForegroundWindow 永远返回 WT 主进程 PID，OS 无法区分 tab/window。
            // 旧 focus.rs / lookup_by_foreground_pid / focus-switch IPC 都已删。
            // Tab 切换走手动点击或 Ctrl+Tab 快捷键。

            // v2.3.0 issue #11：监听 task 文件变更，per-session 重读后 emit 给前端。
            // 不依赖 SessionMap，独立 watcher。tasks_dir 不存在时函数内部 no-op。
            tasks::spawn_task_watcher(tasks_dir.clone(), app.handle().clone());

            // issue #6：历史全文搜索索引。后台线程扫 projects/**/*.jsonl 建内存索引。
            // 延迟 1.5s 启动 —— 让首屏 replay 先跑完，不抢磁盘 / CPU；索引就绪前
            // search_history 返回 status="indexing"，前端显示"索引中"。
            let search_index = Arc::new(search::SearchIndex::new());
            {
                let idx = search_index.clone();
                let claude_dir_for_index = claude_dir.clone();
                let spawned = std::thread::Builder::new()
                    .name("search-index-build".into())
                    .spawn(move || {
                        idx.build_blocking(
                            &claude_dir_for_index,
                            std::time::Duration::from_millis(1500),
                        );
                    });
                if let Err(e) = spawned {
                    tracing::error!("failed to spawn search-index-build thread: {e}; 全文搜索不可用");
                }
            }

            // 前端 ready 事件 → 等 watcher 初始扫完成 → replay all。
            //
            // v2.4 修首次启动乱序：之前 listener 直接调 replay()，但 watcher 是
            // 异步全量扫，snapshot 时 history 不完整 → 部分历史漏到 live emit 路径
            // → 跟 chunked replay 错位。现在 listener 在 async task 里 spin-wait
            // `initial_scan_done`，扫完才 snapshot，保证 chunked replay 包含全部历史。
            //
            // 等待用 10ms 间隔 poll，整体 timeout 10s（防 watcher 死锁卡死整个 UI
            // 永远看不到内容）。timeout 到也会强行 replay，degraded but unblocked。
            {
                let replay = replay.clone();
                let handle = app.handle().clone();
                let initial_scan_done = initial_scan_done.clone();
                let session_map = session_map.clone();
                let remote_active = remote_active.clone();
                let t0_capture = t0;
                app.listen(bridge::events::FRONTEND_READY, move |event| {
                    // Batch5-F19：payload 携带用户上次所在 tab（localStorage 记忆），
                    // replay 按 session 分组、该 tab 的块先发。缺省/解析失败 → None
                    // （行为同 F19 前；viewer 等旧调用方不带 payload 也安全）。
                    // 契约定义在 bridge.rs（单一来源，G 验收纠偏）。
                    let priority_sid =
                        serde_json::from_str::<bridge::FrontendReadyPayload>(event.payload())
                            .ok()
                            .and_then(|p| p.priority_sid);
                    let replay = replay.clone();
                    let handle = handle.clone();
                    let initial_scan_done = initial_scan_done.clone();
                    let session_map = session_map.clone();
                    let remote_active = remote_active.clone();
                    let listen_recv_at = t0_capture.elapsed().as_millis();
                    tauri::async_runtime::spawn(async move {
                        tracing::info!(
                            "[perf] T+{}ms frontend-ready received, waiting for watcher initial scan",
                            listen_recv_at
                        );
                        let wait_started = std::time::Instant::now();
                        const WAIT_TIMEOUT: std::time::Duration =
                            std::time::Duration::from_secs(10);
                        while !initial_scan_done
                            .load(std::sync::atomic::Ordering::Acquire)
                        {
                            if wait_started.elapsed() > WAIT_TIMEOUT {
                                tracing::warn!(
                                    "watcher initial scan timed out after 10s; replay with partial history"
                                );
                                break;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        }
                        tracing::info!(
                            "[perf] T+{}ms watcher initial scan done (+{}ms wait), starting replay",
                            t0_capture.elapsed().as_millis(),
                            wait_started.elapsed().as_millis()
                        );
                        // Batch9-F28：replay 之前先重发全部已宣告远端会话（骨架+
                        // 初始灯）——remote-session-added 不进 replay buffer，F5 后
                        // 无行骨架/bg ⚙ 元数据/远端 lastActive 焦点全靠这次重发
                        // （Batch5 I-1 缺口）。宣告先于行的契约由这里的顺序保证。
                        ssh_source::reannounce_all(&handle);
                        // 〔U4b · G3〕容器账本整份重发（事件不进 replay buffer；同 `reannounce_all` 的理由）。
                        for (sid, c) in crate::session_facts::containers_snapshot() {
                            let _ = handle.emit(
                                bridge::events::SESSION_CONTAINER,
                                &bridge::SessionContainerPayload {
                                    session_id: sid,
                                    container: c.as_wire().to_string(),
                                },
                            );
                        }
                        replay
                            .replay_and_mark_ready(&handle, priority_sid.as_deref())
                            .await;

                        // issue #19：前端是纯事件增量模型——Tab 见行即建 live，只有一次性的
                        // session-ended 能归档。F5/HMR 重载后 replay 把 buffer 里**已结束**
                        // 会话的行也重放成 live Tab，而归档信号不在 buffer、不会重发 → 僵尸
                        // live Tab（还因 closeTab 门控 archived 而关不掉）。这里按当前活跃集
                        // 对账：对已不活跃的**本地** sid 补发 session-ended，复用前端
                        // archiveTab（幂等）。本段仅本地：session_map 只认本地，远端 sid
                        // 不在其中（远端对账见紧随其后的 issue #20 块）。
                        // 〔U4b · G2〕本机也有可重连了 ⇒ 本机的 stale 集同样排除 idle sid（远端那一段的同一条理由：
                        //   不排除就会补发 ended、F5 后可重连塌成已结束）。idle 账本本来就按 origin 存全部机器。
                        let idle_all: std::collections::HashSet<String> =
                            ssh_source::snapshot_idle_by_origin()
                                .into_values()
                                .flatten()
                                .collect();
                        let stale: Vec<String> = replay
                            .buffered_local_session_ids()
                            .into_iter()
                            .filter(|sid| !session_map.is_session_active(sid) && !idle_all.contains(sid))
                            .collect();
                        // issue #20：#19 的远端版。远端 sid 不在 session_map，活跃集由
                        // remote-session-emitter 维护（backend added/removed + 断连 flush
                        // 同一通道）。断连窗口期 F5 会把其实还活着的远端会话一并归档——
                        // 重连后后端重发 session-added + 重放行，前端 un-archive
                        // （tabs.ts ensureTab，仅远端）复活，自愈闭环。
                        //
                        // ⚠ 配套前提：前端把 session-ended 与行事件**同序**处理（events.ts
                        // 的 queue，#20 一并改）。否则这里补发的 ended 会抢在积压重放行
                        // 之前执行，归档随即被后续远端行 un-archive 翻回 live，补发等于无效。
                        // audit-fixes F03.2：idle-tmux sid 不在 remote_active（变 idle 时已移出），若不排除
                        // 会被当"死"补 SESSION_ENDED、F5 后灰灯塌成 archived。故排除 idle sid + 下面重发 SESSION_IDLE。
                        //（`idle_all` 〔U4b〕挪到了本机 stale 那一段之前，两段共用。）
                        let remote_stale: Vec<String> = {
                            let active = remote_active.lock();
                            replay
                                .buffered_remote_session_ids()
                                .into_iter()
                                .filter(|sid| !active.contains(sid) && !idle_all.contains(sid))
                                .collect()
                        };
                        // F03.2：F5 后把 idle sid 的灰灯盖回（行重放会把其 tab 建成 live，这次重发再变灰）。
                        for sid in &idle_all {
                            let _ = handle.emit(
                                bridge::events::SESSION_IDLE,
                                &bridge::SessionIdlePayload {
                                    session_id: sid.clone(),
                                },
                            );
                        }
                        for sid in stale.iter().chain(remote_stale.iter()) {
                            if let Err(e) = handle.emit(
                                bridge::events::SESSION_ENDED,
                                &bridge::SessionEndedPayload {
                                    session_id: sid.clone(),
                                },
                            ) {
                                tracing::warn!(
                                    "reconcile emit session-ended failed for {sid}: {e}"
                                );
                            }
                        }
                        if !stale.is_empty() || !remote_stale.is_empty() {
                            tracing::info!(
                                "replay 对账：补发 session-ended 归档已结束 Tab（本地 {} 个 + 远端 {} 个）",
                                stale.len(),
                                remote_stale.len()
                            );
                        }
                    });
                });
            }

            // 给 Tauri 命令暴露 state。
            //
            // **v1.7.4 修回归**：v1.6.7 撤 bring_terminal_to_front 时把
            // `app.manage(session_map.clone())` 也删了，但 history 命令
            // （list_history_projects / list_history_sessions_in_project）也接
            // `State<Arc<SessionMap>>`，导致历史浏览器打不开，报"state not managed
            // for field `map`"。这里补回去。
            app.manage(session_map.clone());
            app.manage(replay.clone());
            app.manage(bind_registry.clone());
            app.manage(sid_hwnd_cache.clone());
            // Feature ②：远端 sid → hwnd 缓存。bring_remote_terminal_to_front 取此 State。
            app.manage(remote_hwnd_cache.clone());
            // v2.0.0 (issue #4)：logging state 也要 manage，IPC handler 才能拿到
            app.manage(logging_state.clone());
            // issue #6：全文搜索索引 State（search_history / rebuild / status IPC 用）
            app.manage(search_index.clone());

            tracing::info!(
                "[perf] T+{}ms setup() completed (watchers spawned, state managed)",
                t0.elapsed().as_millis()
            );

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            backend_policy::backend_exit_policy,
            backend_policy::set_backend_exit_policy,
            backend::control::backend_control::backend_machines,
            backend::control::backend_control::backend_status,
            backend::control::backend_control::backend_start,
            backend::control::backend_control::backend_stop,
            config::load_config,
            config::save_config,
            // K-H2a：apikey 表那把 key。**读那条永远只回掩码**（`KS6`）；
            // 写那条是「界面」这个第二写者，它与人手编是同一份文件的两个写者（`KS10`）。
            read_apikey_credentials_status,
            write_apikey_credentials_key,
            // K-H2b `KH2B7`：界面问「这几个**本机**账号走不走 apikey 端点改写」。
            // 只答本机不是欠账 —— 中转是每台机器自己的进程，本机这台答不了远端那台。
            apikey_routing_for,
            relay_endpoint_for_launch,
            // 〔AL1 · 2026-09-24〕`设计/71`：别名只有一类（名字 ＋ 一组 ccm 参数），命令面两跳 ——
            // 渲染是纯的（预览 / 复制都只调它），写入是唯一的副作用；外加一个读回口。
            aliases_render,
            aliases_read,
            aliases_install,
            // 〔AL1d〕别名块（`cc` / `cct` · `__ccm_bind`）与清单同一族命令面（`AL1d.md §2.1`）。
            aliases_block_render,
            aliases_block_install,
            aliases_block_remove,
            // F87(#50+#51): MCP 管理——读跨 scope 展示 / 写只项目 .mcp.json（SS-14）
            // B03 批一：cc-bus 驾驶舱（只读，按需 SSH cat，无轮询）
            backend::control::cc_bus::read_cc_bus_state,
            backend::control::cc_bus::check_cc_bus_agent_online,
            backend::control::cc_bus::read_cc_bus_inbox,
            backend::control::cc_bus::cc_bus_send,
            backend::control::cc_bus::cc_bus_broadcast,
            backend::control::cc_bus::cc_bus_kill,
            backend::control::cc_bus::cc_bus_spawn,
            // B04：钩子只读诊断（本机 + 远端）。**没有任何写命令**——用户定调不改 settings.json
            config_surface::config_surface_report,
            drift_ledger::drift_ledger_report,
            backend::control::backend_launch::backend_send_into,
            backend::control::launch_wire::render_ccm_launch,
            backend::control::launch_wire::render_launch_payload,
            hooks_diag::diagnose_local_cc_bus_hooks,
            hooks_diag::diagnose_remote_cc_bus_hooks,
            mcp::read_mcp_servers,
            mcp::read_remote_mcp_servers,
            mcp::list_remote_mcp_origins,
            mcp::read_remote_project_mcp,
            // 〔步 12·C 收尾 09-20〕`write_remote_mcp_server` / `remove_remote_mcp_server`
            // **已退役**（不留别名）—— 并进了下面那两条吃 `origin` 的，它们是那两个函数
            // 今天唯一的调用点。理由住 `mcp.rs` 那两条命令的头注。
            mcp::list_mcp_project_dirs,
            mcp::write_project_mcp_server,
            mcp::remove_project_mcp_server,
            subagent::load_subagent,
            forget_session,
            // issue #10: 独立只读窗口（多窗口 / 双屏）
            open_session_in_new_window,
            replay_session_to_window,
            // F82a(#56+#47): 设置独立窗口
            open_settings_window,
            bring_terminal_to_front,
            // Feature ②: 远端 Tab ↗ 拉前对应本地终端窗口（ccm wrapper 设标题绑定）
            bring_remote_terminal_to_front,
            // issue #23: 红绿灯快照（启动/F5 初始收敛；增量走 session-activity 事件）
            list_session_activity,
            list_active_sessions,
            // v2.4 issue #2: 用户在终端输入时可选拉前 monitor 自身
            bring_monitor_to_front,
            // 🔴 `K-R135` / `R85`：用户级 PATH 那一格（现在状态 · 加 · 撤）。
            //    `R87` 裁定它住 Tauri 命令 —— 与别名块那几条同族（〔AL1d〕今天是 `aliases_block_*`，从前叫
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
            history::list_history_projects,
            history::stream_history_sessions_in_project,
            history::stream_read_session_jsonl,
            // 〔`设计/10` 骨架 · 子步 3〕`--read-session-from-offset` 在 monitor 侧的两个调用点。
            session_skeleton::read_session_index,
            session_skeleton::read_session_range,
            // 〔U3b〕接上骨架的会话，重放缓冲只留尾巴（`设计/10` 步 8）
            session_skeleton::replay_keep_tail_only,
            session_outline::list_user_inputs,
            session_find::find_in_session,
            remote_history::list_remote_history_projects,
            // F10：装 / 卸远端 rc 里的别名块（SFTP 写 profile，SS-H）。〔MC1〕从前叫「装/卸 ccm 助手」，
            // 推 `ccm` 入口那一半并进了下面的 `deploy_remote_backend`（`设计/71 §13.3`）。
            sftp::install_remote_alias_block,
            sftp::uninstall_remote_alias_block,
            // F08c：部署 / 卸载远端后端（SFTP 写 ~/.cc-monitor/bin，SS-G 部署写豁免）。
            // 〔MC1〕部署那一条同时放 `ccm` 入口 —— 「部署后端」只有一个动作。
            sftp::deploy_remote_backend,
            sftp::uninstall_remote_backend,
            acct_iso_deploy::deploy_remote_acct_iso,
            acct_iso_deploy::check_remote_acct_iso,
            acct_iso_deploy::remote_acct_iso_shellinit,
            history::delete_history_session,
            history::create_branch_session,
            history::update_history_metadata,
            history::list_last_accounts,
            history::resume_history_session,
            history::probe_session_record, // 〔U4b〕resume 之前问记录还在不在
            history::new_local_session,
            // 🔴 `K-R109`（09-13）：本机后端产「把终端接进那个会话」那一句（`ccm attach <名>`）。
            //    `R61` 裁定三〔用 09-13 逐字「归本机后端就好了啊」〕。注册这一行与
            //    `parity_ledger::LEDGER` 那一行、`src/ipc/commands.ts` 那个包装层
            //    **是同一拍的事**：拆开任意一处，`commands.vitest.ts` 的 `C04a`
            //    或 `parity_ledger` 的双向相等当场红（`K-R106` 实测过前一种）。
            history::render_local_attach,
            // A2：多账号只读查询（账号=一个 CLAUDE_CONFIG_DIR）。旧 backend
            // 台一律回 available:false，前端降级隐藏账号功能而不是弹错。
            accounts::list_remote_accounts,
            local_accounts::list_local_accounts,
            // 〔C4a · 第四波〕「某会话属于哪个账号」那两条（本机 E79 · 远端 A2）退役：
            //   本机与远端同一条路 —— 前端经通道 `chan_call` 说 `accounts-sessions`。
            // 〔`A3` 第二波〕`acct-iso.check` / `acct-iso.shellinit` 的本机对侧（问本机后端）。
            local_accounts::check_local_acct_iso,
            local_accounts::local_acct_iso_shellinit,
            accounts::check_account_trust,
            launch::launch_remote_terminal,
            // 〔F7c 收尾 09-24〕池子那十二条 Tauri 命令〔散文墓碑〕随老面板与窗口改走通道一起删了（`设计/60 §13b`）；
            //   〔第四波 S4〕最后一条（零流量复制）随门禁那一格退役一起删了 ⇒ 池子零条 Tauri 命令。
            // 🔴 `24e` 第二刀（`设计/60 §4 戊` / `§5` 第三段）：**原生文件管理窗口的入口。**
            //    它不是「又一条 sftp 命令」—— 它开的是那个 egui 窗口（同进程、次线程，
            //    进程形态见 `filewin/mod.rs` 头注）。先真的列一趟目录，列不出来就带原文报错，
            //    **不静默开一个空窗**（理由逐条住 `filewin/entry.rs` 头注）。
            //    ⚠ 界面上点得到它的地方是旧 SFTP 面板的表头 —— 那块面板按 `§6.6 C`
            //    要退役，而在这个窗口真能替代它之前删掉旧的等于把功能拿走 ⇒ 这一刀不删。
            filewin::entry::open_file_window,
            pubkey::push_public_key,
            backend::control::tmux::list_remote_tmux,
            backend::control::tmux::list_local_tmux,
            backend::control::tmux::capture_remote_pane,
            backend::control::tmux::kill_remote_tmux,
            backend::control::tmux::tmux_send_keys,
            ccm_probe::probe_ccm_cli,
            // 🔴 `K-R69` / `KR69D2`：本机 `ccm` 这一格（我们那一份 · PATH 上那一份 · 判词）。
            ccm_probe::local_ccm_entry_status,
            // Batch15-P1：code-picture 代码全景后端命令族（per-repo Engine 池,只读查询）
            // devbench F03：skill 接入面（列出 / 读 / 写那个「人手写的注入文件」）。
            // ⚠ 写走 `skill_host::resolve_editable` 的三道围栏 + `verified_write` 读回比对。
            skill_host::list_skills,
            skill_host::read_skill_file,
            skill_host::write_skill_file,
            plugins::list_plugin_marketplaces,
            cc_bus_deploy::deploy_local_cc_bus,
            cc_bus_deploy::cc_bus_install_state,
            panorama::panorama_index,
            panorama::panorama_reindex,
            panorama::panorama_status,
            panorama::panorama_overview,
            panorama::panorama_node,
            panorama::panorama_subgraph,
            panorama::panorama_callers,
            panorama::panorama_callees,
            panorama::panorama_impact,
            panorama::panorama_search,
            panorama::panorama_docs_for,
            panorama::panorama_touching,
            panorama::panorama_symbols_in_file,
            panorama::panorama_drift,
            panorama::panorama_list_annotations,
            panorama::panorama_diagram_kinds,
            panorama::panorama_diagram,
            panorama_call::panorama_call,
            panorama_call::panorama_edit,
            port_forward::start_forward,
            port_forward::stop_forward,
            port_forward::list_forwards,
            // 〔C4a · 第四波〕**主界面说 `call` 的那一跳**（`设计/05 §3.3`）：webview ⇒ 通道 ⇒ 注入的后端句柄。
            chan::webview::chan_call,
            // issue #6: 历史全文搜索（〔C4a〕只剩本机索引；远端那半前端经通道说 `history-search`）
            search::search_history,
            search::get_search_index_status,
            search::rebuild_search_index,
            // v2.3.0 issue #11: task 面板初次拉
            tasks::get_session_tasks,
            // v2.3.0 issue #3 (A 透明化): 设置面板「数据」区列出所有持久路径
            data_paths::get_data_paths,
            // issue #15 Tier 1: SSH 连接 UX —— ~/.ssh/config 导入 + 测试连接 + 指纹固化
            ssh_source::list_ssh_host_aliases,
            ssh_source::resolve_ssh_host,
            ssh_source::import_ssh_hosts,
            ssh_source::test_remote_connection,
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
                // 〔B2 · 条 66 · `设计/01 §3.3b ④`〕那个值住后端所在那台机器上 ⇒ 这里**在决定那一刻现问**
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
                // 🔴 它为什么留在臂里而不进缝：`backend/control/local_backend_tests.rs::the_exit_path_really_stops_the_local_backend`
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
                // 〔RL1 · V107〕本机中转住在上面那个后端进程里 ⇒ 这里**没有**第三个进程要收
                //   （`the_exit_arm_collects_no_relay` 的零命中守卫数着这件事）。
            }
        });
}

fn extract_cwd(rec: &messages::JsonlRecord) -> Option<String> {
    match rec {
        messages::JsonlRecord::User { cwd, .. } => cwd.clone(),
        _ => None,
    }
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
/// **0..N 个** [`ssh_source::RemoteConfig`]。**空 Vec = 本地模式**（与历史 bit-for-bit
/// 一致）：config.json 不存在 / 解析失败 / 无 `remote` 键 / `enabled != true` / 无任何
/// 合法 host → 空 Vec。
///
/// config.rs 是 schema-agnostic（只透传 serde_json::Value），所以这里直接读
/// `paths::resolve_config_path()` 的文件，自己取 `remote` 子对象。读法对齐
/// `paths.rs::read_user_override`（同一个 config.json，同样的 best-effort 容错）。
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
/// **向后兼容**：旧单对象形态 `"remote": { "enabled": true, "host": …, … }`（无 `hosts`
/// 键）归一成 1 元素列表（`label` 默认 = host）。每台缺必填字段(host/user/backendPath)
/// 则跳过 + warn；`label` 重复则后缀化 ` (#2)`（保证 by-label 选台 key 唯一）。
pub(crate) fn load_remote_configs() -> Vec<ssh_source::RemoteConfig> {
    let Some(cfg_path) = paths::resolve_config_path() else {
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

    // 全局 enabled 门控：未显式 true → 关闭（默认本地）。
    if remote.get("enabled").and_then(|v| v.as_bool()) != Some(true) {
        return Vec::new();
    }

    parse_remote_hosts(remote)
}

/// 把 `remote` 对象解析成 host 列表（抽出供单测直接喂 JSON 对象）。优先读 `hosts`
/// 数组；无 `hosts` 但有 `host`（旧单对象）→ 当 1 台。重复 label 后缀化去重。
fn parse_remote_hosts(
    remote: &serde_json::Map<String, serde_json::Value>,
) -> Vec<ssh_source::RemoteConfig> {
    let host_objs: Vec<&serde_json::Map<String, serde_json::Value>> =
        match remote.get("hosts").and_then(|v| v.as_array()) {
            Some(arr) => arr.iter().filter_map(|v| v.as_object()).collect(),
            None => vec![remote], // 向后兼容：旧单对象
        };

    let mut out: Vec<ssh_source::RemoteConfig> = Vec::new();
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
        out.push(cfg);
    }
    out
}

/// 解析单个 host JSON 对象 → RemoteConfig；缺必填字段(host/user/backendPath) → None+warn。
fn parse_host_obj(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> Option<ssh_source::RemoteConfig> {
    let str_field = |k: &str| {
        obj.get(k)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
    };

    let (host, user, backend_path) = match (
        str_field("host"),
        str_field("user"),
        str_field("backendPath"),
    ) {
        (Some(h), Some(u), Some(d)) => (h.to_string(), u.to_string(), d.to_string()),
        _ => {
            tracing::warn!("remote host 缺必填字段(host/user/backendPath)，跳过该台");
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
    //    （`src/remote-config.ts` 的 `LEGACY_NO_BACKEND_KEY`）认出来、指名告知一次，
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

    Some(ssh_source::RemoteConfig {
        label,
        host,
        port,
        user,
        key_path,
        backend_path,
        host_key_fingerprint,
        addresses,
        jump,
    })
}

/// 按 label 选台（`remote_history` 的历史查询据此选连哪台）。无匹配 → None。
pub(crate) fn load_remote_config_by_label(label: &str) -> Option<ssh_source::RemoteConfig> {
    load_remote_configs()
        .into_iter()
        .find(|c| c.origin_label() == label)
}

/// 把 watcher 读出的一批 `JsonlLine` parse 成可 emit 的 `JsonlLinePayload`。
///
/// v2.4.2 issue #2 抽出的最小 seam：watcher 回调和后续（SSH-remote）数据源都调
/// 这一个自由函数，保持 parse → is_displayable 过滤 → extract_cwd → 组 payload
/// 的行为唯一。过滤次序、解析错误 warn-then-continue、`seq` 透传都必须与历史一致。
///
/// `origin`：数据来源。载荷上的 `origin` 字段由它派生（与 `session_skeleton·rs::range_payloads` 同一口径）：
/// 本机 ⇒ 不带（前端 Tab 标题不加前缀，与历史一致）；远端 ⇒ 那台的名字（issue #15，前端 Tab 标题加
/// `[host]` 前缀以区分本地/远端）。透传到每条 payload，让前端按 sid 分流时知道该 Tab 是本地还是哪台远端主机。
/// 〔ST3〕它同时是记账的那台：看不懂的行记在 `origin` 名下（原先收 `Option<String>`，`None` = 本机）。
pub(crate) fn batch_to_payloads(
    lines: Vec<watcher::JsonlLine>,
    origin: &crate::origin::Origin,
) -> Vec<bridge::JsonlLinePayload> {
    let label = origin.host_name().map(str::to_string);
    let mut payloads = Vec::with_capacity(lines.len());
    for line in lines {
        match parser::parse_line(origin, &line.raw) {
            Ok(Some(record)) if record.is_displayable() => {
                let cwd = extract_cwd(&record);
                payloads.push(bridge::JsonlLinePayload {
                    session_id: line.session_id.clone(),
                    cwd,
                    path: line.path.to_string_lossy().into_owned(),
                    // P5.1：watcher 给每行单调编号；前端按 seq 排到 timeline
                    seq: line.seq,
                    origin: label.clone(),
                    message: record,
                });
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!("parse line failed in {}: {e}", line.path.display());
            }
        }
    }
    payloads
}

/// 前端关闭 archived Tab 时调用：从 event_replay 历史里抹掉这个 session，
/// 防止下次 F5 刷新它原地复活。
/// `K-H2a` `KS6`：读 apikey 表那把 key 的**状态**。**永远只回掩码，不回明文。**
///
/// ★ 这是本件里最要紧的一条：一旦回显，key 就从「只住在后端」变成
/// 「**每次打开那个界面都往前端传一遍**」⇒ 泄漏面从一次变成无数次，
/// 每一次都新增前端日志 / 崩溃报告 / 截图 / 录屏四个出口。
/// ⇒ 返回类型 [`creds_store::ApikeyCredentialsStatus`] **在类型上就装不下明文**，
/// 由 `the_status_type_cannot_carry_the_plaintext` 钉住。
///
/// 〔RM1a · 第四波〕**收 `origin`**：本机读 monitor 自己那一份（原样），远端问那台机器的后端
/// （`apikey-read`）—— 远端账号页显的从此是**那台机器上**那份文件的状态，不再是本机的。
/// 回的仍是同一个**装不下明文**的类型。
#[tauri::command]
async fn read_apikey_credentials_status(
    origin: origin::Origin,
) -> Result<creds_store::ApikeyCredentialsStatus, String> {
    apikey_remote::status_on(&origin).await
}

/// `K-H2b` `KH2B7`：界面问「**这几个本机账号在 apikey 表里有没有行、本机中转在不在跑**」。
///
/// # 为什么是一条**只答本机**的命令，而不是往账号列表里加两个字段
///
/// 账号列表那份结构（`accounts::RemoteAccount`）**同时**装着远端账号，
/// 而「走不走 apikey 端点改写」这件事**只对本机成立** —— 中转是**每台机器自己的一个进程**
/// （`relay/mod.rs` 自陈「独立进程」；注入的是那个 agent 进程自己的 `ANTHROPIC_BASE_URL`，
/// 而 `payload::relay_base_url` 拼的是**回环**地址，回环是**自指**的）
/// ⇒ 本机这一侧**在结构上答不了远端那台**。往那份结构里加字段，
/// 就是让远端那些行也带上两个这一侧答不出来的值。
/// 命令面的登记（`apikey.routing`，`NaturallyAsymmetric`）写着同一条理由。
///
/// # 两个字段各自的射程，别读宽
///
/// - `routed`：**这个 configDir 推出来的账号 id 在apikey 凭据表里有一行**。
///   推 id 的规则只有一份（`history::apikey_account_id_of_dir`），起会话那一侧调的是同一个，
///   由 `history::tests::the_ui_and_the_launch_side_derive_the_account_id_from_the_same_rule` 钉着。
///   ⚠ 它**不**答「那把 key 能不能用」（要到 claude 那边才知道），
///   也**不**答「这次拉起会不会真注入」（那还要过 `running` 那一格）。
/// - `running`：**我们起过本机中转而且没停过**，**不是**「那个口上真有人听」
///   （`local_backend_host::relay_running` 头注逐字写了那两个分家的窗口）。
///
/// ⚠ **本结构刻意不走 `ts-rs`**：`ApikeyCredentialsStatus` 的先例逐字记着理由 ——
/// 导出会在 `src/generated/` **新增一个文件**，而那个目录的清单由
/// `tests/generated-boundary-guard.vitest.ts` 逐项等号对拍，那个文件不在本件写区。
/// ⇒ TS 侧那份是**手写镜像**（`src/accounts.ts::ApikeyRoutingView`），两侧字段名手动同步。
/// **如实记：这一格今天没有判据对拍**（`ApikeyCredentialsStatus` 那条有，本条没有）。
#[derive(serde::Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct ApikeyRouting {
    /// 传进来的那些 configDir 里，apikey 表里**有对应行**的那几个（原样回，不是 id）。
    routed: Vec<String>,
    /// 本机中转在不在跑。射程见上。
    running: bool,
}

/// ⚠ **两个事实只从 [`history::inject_facts`] 取**（`D5 阻-1`）：这是那两个取值口的
/// **第二个**生产消费方（第一个是起会话那一侧的 `history::relay_prefix_for_launch`），
/// 两处走同一条缝、各有一条行为判据。直接在这里调那两个函数的写法只能靠「文本在不在」来钉，
/// 而那一形 `D5` 已经打穿了 —— 整段理由住 `history::InjectFactSources` 的头注。
///
/// # 〔RM1a · 第四波〕**收 `origin`**：两件事都问**那台机器**
///
/// 上面「只答本机」那段理由的前提是「本机这一侧在结构上答不了远端那台」—— 今天远端那台的后端
/// 自己答得了：表里有哪几行（账号层，`apikey-read`）· 那个口上有没有人在听（中转，`relay-status`）。
/// 两件事**各问各的**（`apikey_remote::rows_on` / `remote_relay::running_on`，两个模块互不引用），
/// 只在这里拼成一份给界面。本机那一臂两件事都照旧走 [`history::inject_facts`] 那条缝。
/// 〔RL1〕两台的 `running` 今天是**同一个判准**：「那个口上有人在听」（本机那一格也改成回环连一次）。
#[tauri::command]
async fn apikey_routing_for(
    origin: origin::Origin,
    config_dirs: Vec<String>,
) -> Result<ApikeyRouting, String> {
    let rows = apikey_remote::rows_on(&origin).await?;
    let running = remote_relay::running_on(&origin).await?;
    Ok(ApikeyRouting {
        routed: history::apikey_routed_subset(&config_dirs, &rows, history::launch_agent_id()),
        running,
    })
}

/// 〔RL1 · 第四波〕**这次拉起往 `ANTHROPIC_BASE_URL` 里写哪个中转地址**（`null` = 不注入）。
///
/// 前端拉起远端会话（与本机「就地 resume」那一格）之前问它一次，拿到地址就作为载荷里的一条
/// `export-relay-base-url` 交给 `render_launch_payload`。判断只在 `payload::relay_endpoint_for` 一处；
/// 远端那一臂**用到才起**那台的中转（`history::relay_endpoint_on` 头注）。
/// ⚠ 它接替了 RM1a 那条 `relay_ensure(origin)`（零调用方）：「让那台有一个中转」今天只在「要注入」时才发生，
/// 不再单独暴露给界面。
#[tauri::command]
async fn relay_endpoint_for_launch(
    origin: origin::Origin,
    account: Option<history::LaunchAccount>,
    sid: Option<String>,
) -> Result<Option<String>, String> {
    history::relay_endpoint_on(&origin, account.as_ref(), sid.as_deref()).await
}

/// `K-H2a` `KS10`：从界面配一把 key。
///
/// ⚠ **它和人手编是同一份文件的两个写者** —— 写的那一刻才去读盘，
/// 未知键一个不吃、字段顺序按名字排、原子替换、写完立刻收窄成只给本人。
/// 整段论证见 `creds_store::write_key`。
///
/// ⚠ **入参是明文，而它一进来就被包成 `SecretKey`**（在 `write_key` 里）。
/// 这一层的签名收 `String` 是没办法的事：IPC 边界上只有 JSON。
///
/// ⚠⚠ **订正措辞〔D1，08-27，PM 采纳审计改判〕**：先前这里写的是「那一段**不在本件的判据面里**」
/// ——**那个词说小了一格**。「射程外」意思是「本件裁定不管它」，而盘上的事实是
/// **没人量过**：`KS6` 保的是 key **回**前端那个方向，这里是 **去**后端那个方向，
/// 那是**另一条路上的另一个值**，本件一条判据都没打过它。
/// ⇒ 它的身份是 **`判不了`**，不是「射程外」。**这两个词不是一回事**：
/// 前者欠着一次测量，后者是已经裁过不做。件计划 `§0h` 已按这个身份登记。
///
/// # ⚠⚠ `K-H2c` `KH2C1`：**第二个入参是 `configDir`，不是账号名**
///
/// 界面手上的账号对象**两个字段都有**（`local_accounts.rs` 的 `RawAccount { name, configDir }`），
/// 而它们是 manifest 里**两个独立字段、可以漂开**。apikey 表按**账号 id** 索引，
/// 而那个 id 由 [`history::apikey_account_id_of_dir`] 从 `configDir` 推出来 ——
/// **全仓只有那一份规则**，起会话那一侧（`history::apikey_account_id`）调的是同一个函数。
///
/// ⇒ 这条命令**只收 `configDir`，由 Rust 推 id**。收 `name`、或让 TS 自己
/// `split('/').pop()`，都是在长出**第二份**规则，而那正是 `KH2C1` 红字禁的那件事
/// （前端那一侧由 `tests/settings/accounts-section.vitest.ts` 里那条机检钉着 ——
/// 标题以「KH2C1 机检：前端一个字都不推账号 id」打头的那个 `it`，
/// 它扫整份 `accounts-section.ts` 找「自己从路径取末段名」的四种写法）。
/// ⚠ 〔`K-R20` 订正 09-03〕原先点的是
/// `the_ui_never_derives_the_account_id_itself`〔散文墓碑〕，**那个名字全仓零定义**，
/// 而这句话是当现状在说。
///
/// # 〔RM1a · 第四波〕**收 `origin`**：key 落在会话跑的那台机器上
///
/// 本机那一臂只进 `creds_store`（原样）；远端那一臂由 monitor 推出账号 id、交**那台机器的后端**写
/// （帧面 `apikey-key-set`，后端账号域那一份是那台机器上唯一的写者）。分派住 `apikey_remote::write_key_on`。
/// 明文在本函数体里仍然**只被往下传一次**（`PLAINTEXT_HOPS` 那一行跟着改了住址）。
#[tauri::command]
async fn write_apikey_credentials_key(
    origin: origin::Origin,
    key: String,
    config_dir: String,
    // 〔第四波 ST2 · `设计/70 §4.4`〕加账号表单 apikey 那一支的 Base URL。缺席 = 用默认上游（不碰那一格）。
    // 〔RM1a〕它与 key 一起按 origin 走：本机进 `creds_store`，远端交那台机器的后端（`apikey-key-set` 的 `baseUrl`）。
    base_url: Option<String>,
) -> Result<(), String> {
    apikey_remote::write_key_on(&origin, &config_dir, key, base_url).await
}

/// 〔AL1 · 2026-09-24〕`设计/71 §12.6` 第①跳：**纯** —— 清单 → 代码。一个字节都不写。
/// 预览与「复制去手贴」都只调这一条；`dry_run` 那个布尔从此不需要了（「只生成不写」就是只调这一跳）。
/// 〔AL1c · 第四波 4B〕多一个 `shell`（`posix` / `powershell`）：同一份清单渲染成哪种 shell 的方言（`71 §4.4`）。
/// 必给，不留缺省 —— 缺了就是「替人猜一种 shell」。
#[tauri::command]
fn aliases_render(
    aliases: Vec<account_aliases::Alias>,
    shell: shell_dialect::Shell,
) -> account_aliases::AliasRender {
    account_aliases::render(&aliases, shell)
}

/// 〔AL1〕读回口：这台机器上那份别名文件今天有哪几条（`设计/70 §3.1` 「列出现在有哪些命令」）。
/// 〔AL1c〕按 `shell` 读那一种的文件（`aliases.sh` / `aliases.ps1`）与那一种的启动文件候选。
/// 〔AL1d · 第四波 4B〕启动文件候选各带**别名块**的现状（从前要另问终端集成那两条：列 `$PROFILE` · 扫一份），
/// 外加这台机器上完成了拉前握手的终端数（`BindRegistry`）。`rc_path` = 人另指的一份（过围栏后并进候选）。
/// 读若干份文件 ⇒ `spawn_blocking`（同步命令会占住主线程）。
#[tauri::command]
async fn aliases_read(
    shell: shell_dialect::Shell,
    rc_path: Option<String>,
    bind_state: tauri::State<'_, Arc<bind::BindRegistry>>,
) -> Result<account_aliases::AliasListing, String> {
    let bound = u32::try_from(bind_state.registration_count()).unwrap_or(u32::MAX);
    tokio::task::spawn_blocking(move || {
        let home = dirs::home_dir().ok_or_else(|| "找不到 home 目录".to_string())?;
        account_aliases::read_in(&home, shell, rc_path.as_deref(), bound)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// 〔AL1〕第②跳：**唯一的副作用**。收的是清单不是代码 —— 写进 shell 的文本只由后端渲染
/// （审计 S-1），而「写的就是预览的那一份」由两跳调同一个 `account_aliases::render` 保证。
/// 〔RW1 · 第四波 09-24〕落盘经**本机后端**的文件管理那一面（`user_files::BackendDoor`），
/// home 也问它 ⇒ 那边的测试拿替身门当后端，结构上碰不到真实家目录。
/// 〔AL1c〕`shell` 定写哪一种（别名文件 ＋ 它的写法）；`rc_path` 那份文件的方言由它自己的扩展名定。
#[tauri::command]
async fn aliases_install(
    aliases: Vec<account_aliases::Alias>,
    rc_path: Option<String>,
    shell: shell_dialect::Shell,
) -> Result<account_aliases::AliasInstallReport, String> {
    let door = user_files::BackendDoor::new(origin::Origin::local());
    account_aliases::install_in(&door, &aliases, rc_path.as_deref(), shell).await
}

/// 〔AL1d · 第四波 4B〕**别名块**的第①跳：纯 —— 块 → 代码（「装进一份空文件会写成什么」，BOM 除外）。
/// 两种方言都答（从前的预览只会 PowerShell 那一块）；与装那一跳调同一个 `profile_installer::plan_install`。
/// 收的是**目标文件**而不是 `shell`：方言由那份文件的扩展名定（与装那一跳同一个判法 `Shell::of_target`），
/// 前端不替后端判方言。只看扩展名、一个字节都不读 ⇒ 不过围栏。
/// `with_cc` 只对 PowerShell 有意义：要不要连 `function cc` 一起（不勾 = 只装 `__ccm_bind`，不抢用户自己的 `cc`）。
#[tauri::command]
fn aliases_block_render(rc_path: String, with_cc: bool) -> Result<String, String> {
    let shell = shell_dialect::Shell::of_target(std::path::Path::new(&rc_path));
    profile_installer::render_block(shell, with_cc)
}

/// 〔AL1d〕**别名块**的第②跳：装进人选的那份启动文件（方言按那份文件的扩展名定，`71 §4.4` 末段）。
/// 幂等：已有块就整块替换。落盘经本机后端的文件管理那一面（`user_files::BackendDoor`），本进程不写。
#[tauri::command]
async fn aliases_block_install(rc_path: String, with_cc: bool) -> Result<(), String> {
    let p = profile_installer::fence_profile_path(&rc_path)?;
    let door = user_files::BackendDoor::new(origin::Origin::local());
    profile_installer::install_to_profile(&door, &p, profile_installer::CC_FUNCTION_NAME, with_cc)
        .await
}

/// 〔AL1d〕**别名块**卸掉（整块删，块外一个字节不动；围栏损坏 ⇒ 中止）。经本机后端写。
#[tauri::command]
async fn aliases_block_remove(rc_path: String) -> Result<(), String> {
    let p = profile_installer::fence_profile_path(&rc_path)?;
    let door = user_files::BackendDoor::new(origin::Origin::local());
    profile_installer::uninstall_from_profile(&door, &p).await
}

#[tauri::command]
fn forget_session(
    session_id: String,
    replay: tauri::State<'_, Arc<event_replay::EventReplay>>,
) -> Result<(), String> {
    replay.forget(&session_id);
    Ok(())
}

/// issue #10：把某 session 在一个独立 WebviewWindow（`viewer-<sid>`）里打开，
/// 加载 `viewer.html?viewer=<sid>` —— 独立入口 `src/entry-viewer.ts`（三入口拆分，`设计/01 §1.2`）。
/// 窗口已存在则前置聚焦（不重复开）。双屏 / 并排查看用。
///
/// **必须 `async`**：Tauri 2 同步 `fn` 命令在**主线程**执行，而
/// `WebviewWindowBuilder::build()` 要把窗口创建派发到主线程并阻塞等待 —— 同步命令
/// 就是在主线程里等主线程 → 死锁（表现：新窗口白屏 + 整个 app 卡死连 X 都点不了）。
/// async 命令跑在 async runtime（非主线程）→ build() 派发给空闲主线程 → 正常建窗。
///
/// 拖拽撕离（tear-off）：`x` / `y` 为可选的**逻辑屏幕坐标**（CSS px），来自前端
/// mouseup 的 `e.screenX/screenY`。两者皆 `Some` 时新窗口在该落点打开（双屏拖出体验）；
/// 任一为 `None`（右键菜单 / Ctrl+Shift+N 老调用方）则维持默认居中行为，不破坏旧路径。
#[tauri::command]
async fn open_session_in_new_window(
    app: tauri::AppHandle,
    session_id: String,
    title: String,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<(), String> {
    use tauri::Manager;
    let label = format!("viewer-{session_id}");
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    let url = tauri::WebviewUrl::App(format!("viewer.html?viewer={session_id}").into());
    let mut builder = tauri::WebviewWindowBuilder::new(&app, &label, url)
        .title(if title.is_empty() {
            "cc-monitor"
        } else {
            &title
        })
        .inner_size(900.0, 720.0)
        // Batch7-F23B：与主窗口 backgroundColor 一致——合成间隙露底为主题深色
        // 而非 WebView2 默认白（tauri.conf.json 主窗口同款 #2b2a27）
        .background_color(tauri::window::Color(0x2b, 0x2a, 0x27, 0xff));
    // 落点定位：仅当 x/y 都给出时按逻辑坐标摆放（Tauri 2 builder 取 LogicalPosition）。
    if let (Some(x), Some(y)) = (x, y) {
        builder = builder.position(x, y);
    }
    builder
        .build()
        .map_err(|e| format!("create viewer window failed: {e}"))?;
    Ok(())
}

/// F82a（#56+#47）：把「设置」开进独立窗口（SS-3 终态：设置搬独立窗）。单例 `settings` 窗，
/// 已存在则前置聚焦。**必须 `async`**（同 `open_session_in_new_window`：同步命令建窗死锁，见其
/// doc + `viewer-window-investigation.md` 五坑之一）。设置窗加载 `settings.html`（独立入口 `src/entry-settings.ts`）→ `bootstrapSettings`
/// 精简挂载 SettingsPanel（windowMode）。设置项经既有 config 命令读写（窗口无关），无需 replay/事件流；
/// 保存时前端广播 `settings-applied`，主窗口 listen 后重读并应用主题/行为（跨窗同步）。
#[tauri::command]
async fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let label = SETTINGS_WINDOW_LABEL;
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    let url = tauri::WebviewUrl::App("settings.html".into());
    tauri::WebviewWindowBuilder::new(&app, label, url)
        .title("cc-monitor 设置")
        .inner_size(760.0, 820.0)
        // 与主窗口 backgroundColor 一致，合成间隙露底为主题深色而非 WebView2 默认白（同 viewer）
        .background_color(tauri::window::Color(0x2b, 0x2a, 0x27, 0xff))
        .build()
        .map_err(|e| format!("create settings window failed: {e}"))?;
    Ok(())
}

/// issue #10：独立 viewer 窗口加载后调用 —— 后端把该 sid 的历史定向 emit 给本窗口。
/// `window` 由 Tauri 注入 = 发起调用的窗口（即那个 viewer 窗口）。
#[tauri::command]
fn replay_session_to_window(
    session_id: String,
    window: tauri::WebviewWindow,
    replay: tauri::State<'_, Arc<event_replay::EventReplay>>,
) -> Result<(), String> {
    replay.replay_session_to_window(&window, &session_id);
    Ok(())
}

/// v2.4 (issue #2)：把 monitor 自己的主窗口拉到最前 + unminimize + 抢焦点。
///
/// 用途：用户在终端敲键时，前端 user-active 信号路径下，若用户开了「拉前
/// monitor 窗口」toggle 就 invoke 这个 IPC 让 monitor 主动浮上来。
///
/// **核心问题**：用户敲终端时前台是 PS/WT，**monitor 不是前台进程** →
/// `SetForegroundWindow` 直接调被 OS 拒绝（只闪任务栏图标）。这是 Windows
/// 对前台抢焦的设计限制（防恶意软件偷焦点）。
///
/// **解法 = AttachThreadInput hack**：临时把当前线程附加到前台线程的输入
/// 队列，OS 把它俩视作"同输入上下文" → 借用前台线程的拉前权限 →
/// SetForegroundWindow 通过 → 立刻 detach。广泛使用的可靠 hack
/// （Visual Studio / 各 IDE 都用），不被 OS 视为恶意。
///
/// v2.4.0 直接用 win.set_focus()（内部就是 SetForegroundWindow）必败，
/// v2.4.1 hotfix 改这版。
///
/// Tauri 内部用 windows crate 0.61（HWND.0 = *mut c_void），我们 0.56
/// （HWND.0 = isize）；用 `as isize` cast 跨版本兼容。
#[cfg(windows)]
#[tauri::command]
async fn bring_monitor_to_front(app: tauri::AppHandle) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        keybd_event, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VK_MENU,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic,
        SetForegroundWindow, SetWindowPos, ShowWindow, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE,
        SWP_NOSIZE, SW_RESTORE, SW_SHOW,
    };

    tracing::info!("bring_monitor_to_front: invoked");

    // HWND 必须在 webview 所属线程里取（Tauri 内部约束），随即 cast 成
    // isize 跨线程，INVARIANTS § 19 跨 windows crate 版本约定。
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let tauri_hwnd = win.hwnd().map_err(|e| format!("hwnd: {e}"))?;
    let hwnd_value = tauri_hwnd.0 as isize;

    // INVARIANTS § 10：Win32 同步调用必须 spawn_blocking，否则慢路径会让 Tauri
    // IPC 派发线程排队（v2.4 起 autoFollowUserActive 高频触发该 IPC）。
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        unsafe {
            let h = HWND(hwnd_value);
            tracing::info!("bring_monitor_to_front: monitor hwnd = {:#x}", hwnd_value);

            // === 三层 hack 突破 Win10/11 前台抢焦限制 ===
            // 详 ARCHITECTURE.md § 5「bring_monitor_to_front 三层 hack」。
            // attach/detach + Alt down/up 同闭包内必须配对，整段在同一 blocking
            // 线程内串行，安全。

            // 1. ShowWindow 先做：可能 minimize 状态
            if IsIconic(h).as_bool() {
                tracing::info!("bring_monitor_to_front: window iconic, SW_RESTORE");
                let _ = ShowWindow(h, SW_RESTORE);
            } else {
                let _ = ShowWindow(h, SW_SHOW);
            }

            // 2. 模拟 Alt 按键（down 阶段，up 在末尾）
            keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_EXTENDEDKEY, 0);

            // 3. AttachThreadInput
            let fg = GetForegroundWindow();
            let fg_thread = GetWindowThreadProcessId(fg, None);
            let cur_thread = GetCurrentThreadId();
            tracing::info!(
                "bring_monitor_to_front: fg_hwnd={:#x} fg_thread={} cur_thread={}",
                fg.0,
                fg_thread,
                cur_thread
            );
            let attached = fg_thread != 0
                && fg_thread != cur_thread
                && AttachThreadInput(fg_thread, cur_thread, true).as_bool();

            // 4. TOPMOST 强制 Z 序拉顶 + BringWindowToTop
            let _ = SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            let _ = SetWindowPos(h, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            let _ = BringWindowToTop(h);

            // 5. SetForegroundWindow 真正抢焦
            let ok = SetForegroundWindow(h).as_bool();
            tracing::info!(
                "bring_monitor_to_front: attached={} SetForegroundWindow={}",
                attached,
                ok
            );

            // 6. detach + Alt 释放
            if attached {
                let _ = AttachThreadInput(fg_thread, cur_thread, false);
            }
            keybd_event(
                VK_MENU.0 as u8,
                0,
                KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP,
                0,
            );

            if ok {
                Ok(())
            } else {
                // 拉前真失败也 Z 序已被推顶，视觉上窗口浮起来了
                // （只是焦点没抢到）。给前端 warn 但不视为 fatal。
                tracing::warn!("bring_monitor_to_front: SetForegroundWindow rejected (window Z-order raised but no focus)");
                Err("SetForegroundWindow rejected (window raised but not focused)".into())
            }
        }
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg(not(windows))]
#[tauri::command]
async fn bring_monitor_to_front(app: tauri::AppHandle) -> Result<(), String> {
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let _ = win.unminimize();
    let _ = win.show();
    win.set_focus().map_err(|e| format!("set_focus: {e}"))
}

/// issue #23：当前全部本地活跃会话的红绿灯快照。前端启动/F5 后调一次做初始收敛
/// （session-activity 是稀疏事件、不进 replay buffer，刷新会丢——同 get_session_tasks
/// 的「快照 + 事件增量」双路收敛模式）。纯内存读（RwLock clone），无需 spawn_blocking。
#[tauri::command]
fn list_session_activity(
    map: tauri::State<'_, Arc<session_map::SessionMap>>,
) -> Vec<bridge::SessionActivityPayload> {
    map.snapshot_activity()
        .into_iter()
        .map(|a| bridge::SessionActivityPayload {
            session_id: a.session_id,
            status: a.status,
            waiting_for: a.waiting_for,
        })
        .collect()
}

/// Batch5-F18：本地活跃会话清单（sid + cwd）——前端启动时（frontend-ready 之前）
/// 调一次，先建全部骨架 Tab。纯内存读（RwLock clone），无需 spawn_blocking。
#[tauri::command]
fn list_active_sessions(
    map: tauri::State<'_, Arc<session_map::SessionMap>>,
) -> Vec<bridge::ActiveSessionPayload> {
    map.snapshot_active()
        .into_iter()
        .map(|e| bridge::ActiveSessionPayload {
            session_id: e.session_id,
            cwd: e.cwd,
            kind: e.kind,
            name: e.name,
        })
        .collect()
}

/// v1.7：拉对应终端窗口。
///
/// 流程：sid → 查 SidHwndCache → 校验复合指纹（IsWindow + owner_pid + procStart）
/// → activate_window。
///
/// **必须 async + spawn_blocking** 隔离 Win32 sync 调用（v1.6.5 的教训）。
#[tauri::command]
async fn bring_terminal_to_front(
    session_id: String,
    cache: tauri::State<'_, Arc<bind::SidHwndCache>>,
) -> Result<(), String> {
    let cache = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        let binding = cache.lookup(&session_id).ok_or_else(|| {
            format!(
                "session {session_id} 未绑定窗口。用 cc 命令启动 claude 才能拉前；\
                 cc 集成的安装见设置面板。"
            )
        })?;
        bind::verify_binding(&binding)?;
        bind::activate(binding.hwnd)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Feature ②：拉对应**远端** Tab 的本地终端窗口。
///
/// 〔`设计/80 §8.7` 步 4 / 步 5，第二波 T4〕分派整条搬进 `bind::bring_remote_front`
/// （**唯一分派点**：先令牌 `sid → token → HWND`、后标题 `ccm-rbind-<sid>` 退路；
/// 失败说的话只由「这个 sid 有没有启动令牌」一个布尔决定）。本命令只剩「拿两份 State、
/// 挪到阻塞线程池」—— **必须 async + spawn_blocking** 隔离 Win32 sync 调用（INVARIANT § 10）。
#[tauri::command]
async fn bring_remote_terminal_to_front(
    session_id: String,
    cache: tauri::State<'_, Arc<bind::RemoteHwndCache>>,
    registry: tauri::State<'_, Arc<bind::BindRegistry>>,
) -> Result<(), String> {
    let cache = cache.inner().clone();
    let registry = registry.inner().clone();
    tokio::task::spawn_blocking(move || bind::bring_remote_front(&session_id, &registry, &cache))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

// === v1.7：PowerShell profile cc 集成 IPC ===
//
// 〔AL1d · 第四波 4B〕这里原来是「终端集成」那五条命令（`cc_integration_*`〔散文墓碑〕：状态 · 扫一份 · 预览 · 装 · 卸）
// 与它们的三个出参类型。它们办的是**别名块**（两种方言都办：`profile_installer::plan_install` 按扩展名分），
// 装进的是别名文件那一行同一批启动文件 ⇒ 并进 `aliases_*` 同一族命令面（`调研/第四波记录/AL1d.md §2.1`）：
// 状态 ＋ 扫一份 → `aliases_read`（候选各带块的现状）· 预览 → `aliases_block_render` · 装 / 卸 → `aliases_block_install` / `aliases_block_remove`。
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
fn cc_get_auto_launch() -> Result<auto_launch::AutoLaunchConfig, String> {
    let dir = auto_launch::data_dir().ok_or("no data dir")?;
    Ok(auto_launch::get_config(&dir))
}

/// UI toggle 改变时调：写 auto_launch_enabled。
#[tauri::command]
fn cc_set_auto_launch(enabled: bool) -> Result<(), String> {
    let dir = auto_launch::data_dir().ok_or("no data dir")?;
    auto_launch::set_enabled(&dir, enabled)
}

// ===== 🔴 `K-R135`（`R85` / `R87` / `R88`）：用户级 PATH 那一格 =====
//
// 用户 `R85` 逐字：「**应该让用户手动点击加，也能管理删除。就像是 log 数据管理一样。**」
// ⇒ 三样：**现在状态（现算不缓存）· 一个按钮加 · 一个按钮撤**，形状照
// `src/settings/diagnostics-section.ts`（用户点名的那个范式，它现打也全走 `commands.*`）。
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
async fn ccm_user_path_status() -> Result<profile_installer::UserPathStatus, String> {
    tokio::task::spawn_blocking(profile_installer::user_path_status)
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))
}

/// `KR135D1` ②：**一个按钮加**。跑的就是界面上显示给用户看的那段字节
/// （`render_user_path_setup_command`）—— 点按钮与自己复制去跑**逐字同一份**。
#[tauri::command]
async fn ccm_user_path_add() -> Result<(), String> {
    tokio::task::spawn_blocking(profile_installer::user_path_add)
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// `KR135D1` ③：**一个按钮撤**。**只摘自己那一格**（整格比，不碰用户 PATH 里别的东西）。
#[tauri::command]
async fn ccm_user_path_remove() -> Result<(), String> {
    tokio::task::spawn_blocking(profile_installer::user_path_remove)
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

// ===== v2.0.0 (issue #4): 诊断 / log IPC =====

/// 读当前 diagnostics 配置。设置面板打开时调一次。
#[tauri::command]
fn get_diagnostics_config(
    state: tauri::State<'_, Arc<logging::LoggingState>>,
) -> Result<logging::DiagnosticsConfig, String> {
    Ok(state.config())
}

/// 应用新 diagnostics 配置。日志级别 + error_toast 立即生效；
/// log_enabled / max_files 改了返回 `NeedsRestart` 让前端提示用户重启。
#[tauri::command]
fn set_diagnostics_config(
    cfg: logging::DiagnosticsConfig,
    state: tauri::State<'_, Arc<logging::LoggingState>>,
) -> Result<logging::RestartHint, String> {
    state.update_config(cfg)
}

/// 返回 log 目录 + 当前 log 文件 + 全部 .log 文件列表（path / size / mtime）。
/// 设置面板用来显示路径 + 文件大小，让用户一眼看到 log 状态。
#[tauri::command]
fn get_log_file_info(
    state: tauri::State<'_, Arc<logging::LoggingState>>,
) -> Result<logging::LogFileInfo, String> {
    Ok(state.log_file_info())
}

/// 用系统默认编辑器打开当前 log 文件（rolling::daily 写入的 mtime 最新那个）。
/// 失败常见原因：log_enabled=false 还没生成过 log 文件 → Err 让前端 alert 提示。
#[tauri::command]
async fn open_log_file(state: tauri::State<'_, Arc<logging::LoggingState>>) -> Result<(), String> {
    let path = state
        .current_log_file()
        .ok_or_else(|| "没有 log 文件（log 文件未启用或还没产生）".to_string())?;
    let path_str = path.to_string_lossy().into_owned();
    tokio::task::spawn_blocking(move || open_with_os(&path_str))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// 用资源管理器打开 log 目录。
#[tauri::command]
async fn open_log_dir(state: tauri::State<'_, Arc<logging::LoggingState>>) -> Result<(), String> {
    let dir = state.log_dir();
    // 目录可能还不存在（log_enabled=false 时不创建）
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("create log dir: {e}"))?;
    }
    let dir_str = dir.to_string_lossy().into_owned();
    tokio::task::spawn_blocking(move || open_with_os(&dir_str))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// 跨平台调系统默认 opener。Windows 用 `cmd /C start ""` 兜 path 中的空格。
/// 复用 tauri-plugin-opener 也行（前端就是走它），但这里在 Rust 端直接调更直接。
fn open_with_os(path_or_dir: &str) -> Result<(), String> {
    use crate::spawn_managed::{spawn_managed, ConsolePolicy, Lifetime, StderrSink};
    // 「用哪个程序打开」是平台差异，**留在这儿**；「怎么起它」三条策略走唯一出口。
    //
    // ★ 这一处走的是 `spawn_managed(bin, args, …)` 那个**五参数形态**（`00 §1.5.2`
    //   逐字写的那个签名），而不是它的内层 `spawn_managed_cmd` —— 因为这一跳**真的
    //   只有「一个二进制 ＋ 一串 argv」**：不设 env、不设 cwd、三根 stdio 一根都不碰。
    //   ⚠ 别把这读成「别处偷懒了」：别处要 env / cwd / stdin / stdout，那些不属于
    //   那三条策略，硬塞进这个签名只会长出第七、第八个参数。
    #[cfg(windows)]
    // `cmd /C start ""` 兜 path 里的空格；第一个空串是 `start` 的窗口标题位。
    let (bin, args) = (
        "cmd",
        vec![
            "/C".to_string(),
            "start".to_string(),
            String::new(),
            path_or_dir.to_string(),
        ],
    );
    #[cfg(target_os = "macos")]
    let (bin, args) = ("open", vec![path_or_dir.to_string()]);
    #[cfg(all(unix, not(target_os = "macos")))]
    let (bin, args) = ("xdg-open", vec![path_or_dir.to_string()]);
    // 三条策略（`00 §1.5.2`）：
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

#[cfg(test)]
#[path = "../../../tests/bridge/lib_nudge_skip_tests.rs"]
mod nudge_skip_tests;

#[cfg(test)]
#[path = "../../../tests/bridge/lib_env_scrub_tests.rs"]
mod env_scrub_tests;

#[cfg(test)]
#[path = "../../../tests/bridge/lib_batch_tests.rs"]
mod batch_tests;

#[cfg(test)]
#[path = "../../../tests/bridge/lib_mod_decl_hygiene_tests.rs"]
mod mod_decl_hygiene_tests;

#[cfg(test)]
#[path = "../../../tests/bridge/lib_remote_config_tests.rs"]
mod remote_config_tests;

#[cfg(test)]
#[path = "../../../tests/bridge/lib_window_lifecycle_tests.rs"]
mod window_lifecycle_tests;

#[cfg(test)]
#[path = "../../../tests/bridge/lib_remote_bind_prescan_tests.rs"]
mod remote_bind_prescan_tests;
