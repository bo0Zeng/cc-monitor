//! **测试层分级**：五层（单元 / 源码扫描 / 集成 / e2e / 真机），
//! 每层一张登记表、每层一条会红的反空真自检。
//!
//! # 它回答什么，不回答什么
//!
//! 层是**机制轴**：跑它要什么环境、谁来跑它、它会以什么方式**静默变空**。
//! 它**不**回答「这条判据服务哪条业务要求」—— 那是价值轴（已实测两轴统计上不相关，
//! 价值轴的住址归各判据头注）。所以这里一个字都不判「哪一层更有用」。
//!
//! # 五层，各自最典型的「静默变空」与本文件给它的那一条自检
//!
//! | 层 | 静默变空的方式 | 自检（相等 / 零命中带正控） |
//! |---|---|---|
//! | 单元 | 文件在盘上、没有 `#[path]` 把它挂进编译单元 ⇒ 零条被编译、`cargo test` 照绿；`.test.ts` 不在 `npm test` 链上 | [`unit_tier_every_file_is_reached_by_its_runner`] |
//! | 源码扫描 | 路径断了 ⇒ 扫空集 ⇒ 恒绿 | [`scan_tier_every_root_anchored_path_literal_resolves`] |
//! | 集成 | 缺环境时裸 `return` ⇒ 没跑也算过 | [`integration_tier_never_skips_silently`] |
//! | e2e | 写了套件没人跑 | [`e2e_tier_is_what_package_json_runs_and_no_helper_is_orphaned`] |
//! | 真机 | `#[ignore]` 挡在门禁外，改个名触发者就点不到它（`cargo test` 跑零条 exit 0） | [`real_machine_tier_every_ignored_test_is_registered_and_its_trigger_still_reaches_it`] |
//!
//! 分区本身（五张表 ＋ 支撑表两两不相交、并集 == 盘上全集、每份的层 == 判别器）是
//! [`the_tiers_partition_the_test_files_on_disk`]；判别器与几个抽取器的自检在文件末尾。
//!
//! # 分层规则（逐份；机械判别 ＋ 例外表）
//!
//! 先剥注释、再把字符串字面量的**内容**抹成空格（针表里的 `"Command::new("` 是数据不是调用），再认信号：
//! 没有测试 ⇒ 支撑 · 有「碰 OS」信号 ⇒ 集成 · 否则有「读仓内文本」信号 ⇒ 源码扫描 · 否则 ⇒ 单元。
//! 一份文件里几种都有 ⇒ 取环境要求最高的那个（单元 < 扫描 < 集成）。`#[ignore]` 那几条**另进真机层逐条登记**，
//! 不改文件的层。判别器判错的 ⇒ 进 [`OVERRIDES`] 并写理由，而每一行都必须**真的**与判别器不一致（死行 ⇒ 红）。
//!
//! # 已经有人守的，不重做（本文件只在它们没盖到的地方长东西）
//!
//! - 「e2e 要么进门禁要么登记理由」：`e2e_gate_registry_tests.rs::every_e2e_suite_is_either_gated_or_registered_as_exempt`
//!   —— 本文件只按名字钉住它在（它被删 / 改名，e2e 层那条自检红）。
//! - 「壳那两棵树里的 `#[ignore]` 有触发者」：`shared_crate_registry_tests.rs::every_ignored_test_still_has_someone_who_triggers_it`。
//!   它**只扫壳那一侧**；后端那几条子进程入口今天不在任何触发表里 —— 真机层把两棵树与 TS 一起圈进来。
//!
//! # 买不到什么（逐条）
//!
//! - **层是按文本信号判的，不是按真跑时碰了什么判的。** 测试调一个 helper、helper 里才起进程 ⇒ 判成单元。
//!   判别器的全部信号表在下面，改信号表就是改层的定义。
//! - 单元层的「可达」只核到「有一处 `#[path]` 挂它、挂它的那份文件在 `src/` 或是另一份测试文件」，
//!   **不**核挂它的那份 `src/` 文件自己在不在模块树里（那一格是编译器的活，死文件编译器也不报）。
//! - 扫描层只核**以仓内根为基**的路径字面量（`repo_root().join("…")` 及其 `let` 绑定）；
//!   `format!` 拼出来的路径、TS 侧的路径一个都不看。
//! - 集成层的「静默跳过」只认「读环境变量 ⇒ 三行内裸 `return`」那一形，只看 Rust，只看测试函数体本身
//!   （helper 里跳过的看不见）。
//! - 真机层的触发者核的是「过滤串是那条测试全名的子串」（`cargo test` 的过滤语义），
//!   **不**核那个脚本今天有没有人跑、跑了是不是绿。
//! - `tests/evidence/` 里的 `.py` / `.md` / `.rs` 量具与读数不在任何一层的人群里：
//!   它们不是哪个跑者按后缀会捡起来的东西。那棵树里真编译的那条 `[[bench]]` 已搬去 `tests/benches/`。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

// ───────────────────────────── 登记表 ─────────────────────────────

/// 单元层：不读仓内文本、不碰 OS 的测试文件。
const UNIT: &[&str] = &[
    "tests/frontend/ui/remote-launch.test.ts", // SCAN → UNIT：读 e2e 替身源码那一格随换号重启下沉删了，剩下的格都是纯函数
    // SCAN → UNIT：读 `main.rs` 源码的那一条（`--capture-pane` 够不够得到）随那条子命令删了，余下都是纯函数。
    "tests/backend/main_stream_flag_tests.rs",
    // 记录解释搬进后端：Claude 轮次判词（原 `observe/turn_detect_tests.rs`）· Codex 记录映射（原 monitor `codex_record_tests.rs`）·
    //   按路径读正文出成品那个核（新）。
    "tests/backend/agents/claudecode/turn_tests.rs",
    "tests/backend/agents/claudecode/quota_tests.rs", // 回包头 → 额度快照：读法表的金样（纯函数）
    "tests/backend/accounts/quota/decide_tests.rs",   // 换号的唯一判定（纯函数）
    "tests/backend/accounts/quota/show_tests.rs",     // 额度显示态（纯函数）
    "tests/common/creds-core/token_tests.rs",         // 登录令牌的读 · 写回 · 续期请求体（纯函数）
    "tests/backend/observe/record_page_tests.rs",
    "tests/frontend/ui/startup-active.vitest.ts", // F19 启动时记住的那一格（按事件判的真值表）
    "tests/backend/dial_machine_tests.rs",        // 机器配置 → 拨号请求的规则
    "tests/backend/dial_probe_tests.rs",          // 测试连接三步的结局（链路替身）
    "tests/backend/dial_terminal_processes_tests.rs", // 认终端进程：系统那一趟换成合成 JSON
    "tests/backend/files/query_tests.rs", // Everything 式搜索词：手写语料上的解析与匹配（纯函数）
    // `tests/frontend/ui/remote-probe.vitest.ts` 挪进 SCAN：它多读一份 Rust 源码对拍进度流名（`event_replay.rs::PROBE_PROGRESS_KIND`）。
    "tests/frontend/ui/session-writes.vitest.ts", // 删会话 · 分叉经通道直说那台后端：解码器读金样 ＋ 替身数请求
    "tests/frontend/ui/pubkey-push.vitest.ts",    // 公钥推送走通道：解码器读金样 ＋ 替身数请求
    "tests/backend/assets/hub_tests.rs", // 两台之间那几件的枢纽（替身的这台 ＋ 替身的远端 capture，纯内存）
    "tests/backend/agents/claudecode/cards_tests.rs", // 原 `tests/common/agent-tools-core/lib_tests.rs`：工具词表收进后端适配层（纯函数）
    "tests/backend/observe/facts_query_tests.rs", // SCAN → UNIT：读生成物那条异源对拍随两份收成一份退役，余下全是行为判据
    "tests/backend/observe/relay_marks_tests.rs", // 中转看见的请求标记：带过即算 · 没看见过说不出 · 上界丢最早的
    "tests/frontend/ui/config-lost-update.vitest.ts", // J1 两 realm 11 写者同拍写 · J5 写者路径集合
    "tests/frontend/ui/config-persist-failure.vitest.ts", // J8 落盘失败恰好一条 toast
    "tests/frontend/ui/events-stream-closed.vitest.ts", // 会话流 closed 格 ⇒ 恰好一条 toast
    "tests/frontend/ui/account-color.vitest.ts",
    "tests/frontend/ui/account-commands.vitest.ts",
    "tests/frontend/ui/account-restart.vitest.ts",
    "tests/frontend/ui/accounts.vitest.ts",
    "tests/frontend/ui/settings/accounts-mcp-block.vitest.ts", // 账号页各账号共用的 MCP 那一块：只画名字与各版、只交意图（纯替身）
    "tests/frontend/ui/backend-policy.vitest.ts",
    "tests/backend/accounts/upstream_select/table_tests.rs",
    "tests/backend/accounts/manage/aliases_tests.rs",
    "tests/backend/accounts/manage/exec_tests.rs",
    "tests/backend/accounts/manage/layout_tests.rs",
    "tests/backend/accounts/manage/model_tests.rs",
    "tests/backend/accounts/manage/verify_tests.rs",
    "tests/backend/accounts/manage/json_key_tests.rs", // 账号之间同步 MCP：只换一个键、别的字节不动（扫描一张排版各异的原文表）
    "tests/backend/accounts/manage/mcp_share_tests.rs", // 账号之间同步 MCP：三方对照（纯）
    "tests/backend/agents/claudecode/records_tests.rs",
    "tests/backend/agents/claudecode/resume_tests.rs",
    "tests/backend/agents/codex/parse_tests.rs",
    "tests/backend/agents/codex/resume_tests.rs",
    "tests/backend/agents/codex/token_tests.rs", // 原 `tests/common/codex-token-core/lib_tests.rs`（crate 搬进后端成模块）
    "tests/backend/dial_pool_tests.rs",
    "tests/backend/observe/history_query_tail_tests.rs",
    "tests/backend/observe/session_ledger_tests.rs", // 会话账本（可重连 / 已结束的裁决）真值表
    "tests/backend/observe/user_inputs_tests.rs",
    "tests/backend/platform/shell_tests.rs",
    "tests/backend/platform/tcp_rtt_tests.rs", // Windows 臂 `SIO_TCP_INFO` 的布局与控制码对 SDK
    "tests/backend/plugin/probe_tests.rs",
    // 集成 → 单元：真起进程的那几条（可打断的等法）随代码全景删了，剩下的只喂纯函数。
    "tests/backend/plugin/invoke_tests.rs",
    "tests/comms/outward/door_tests.rs",
    "tests/comms/outward/http1_tests.rs",
    "tests/backend/relay/member_tests.rs",
    "tests/comms/outward/route_tests.rs", // 跨半边抠 monitor 源码那几条退役 ⇒ 只剩纯解析 ＋ 成品→决策表（SCAN → UNIT）
    "tests/comms/outward/tee_tests.rs",
    "tests/comms/outward/upstream_tests.rs",
    "tests/backend/stream/tap_tests.rs",       // hub
    "tests/backend/stream/run_route_tests.rs", // 流归位那一跳的丢弃账
    "tests/frontend/ui/live-card.vitest.ts", // 活卡：状态机 · 真 TabManager 三向相等（台架夹具那一条随折法搬进后端）
    "tests/frontend/ui/runs.vitest.ts", // 子运行：主 tab 零子运行行、agent 面板分组与五态、主活卡只有主运行那段、状态标到那张卡上
    "tests/backend/writer_task_tests.rs", // 写者优先序（tap 最低）
    "tests/frontend/ui/branch-button.vitest.ts",
    "tests/frontend/ui/branch-fold-batching.vitest.ts",
    "tests/frontend/ui/branching.test.ts",
    // `tests/frontend/shell/accounts_tests.rs` 删了（随 `accounts.rs` 整份出列）。
    // `tests/frontend/shell/adapter/claude_code_tests.rs` 删了（monitor 那份适配表退役，对拍与生成器随家进了后端 `tests/backend/agents_tests.rs`）。
    // `tests/frontend/shell/adapter/codex_tests.rs` 删了（monitor 那份适配表退役，对拍与生成器随家进了后端 `tests/backend/agents_tests.rs`）。
    "tests/backend/control/launch_render/launch_cli_parity_tests.rs",
    "tests/frontend/shell/chan/transfer_stream_tests.rs",
    "tests/backend/agents/claudecode/branch_tests.rs", // 原 `tests/common/branch-core/lib_tests.rs`：分叉变换收进后端适配层
    "tests/backend/control/gate_rules_tests.rs", // 原 `tests/common/gate-core/lib_tests.rs`：gate-core 收成后端模块
    "tests/common/relay-route-core/lib_tests.rs", // 中转门牌共享 crate
    // `search-core` 删了：它的纯函数判据随家拆成两份（通用口径 · Claude 记录文本），同层。
    "tests/backend/agents/claudecode/text_tests.rs",
    "tests/frontend/filewin/corpus_tests.rs",
    "tests/frontend/shell/filewin/cross_half_tests.rs", // 窗口独立成包之后跨两半的那几条（异源一侧在 monitor）
    "tests/frontend/filewin/create_tests.rs",
    "tests/frontend/filewin/editor_tests.rs",
    "tests/frontend/filewin/preview_tests.rs",
    "tests/frontend/filewin/kind_tests.rs", // 文件种类：手写的名字 → 种类表 · 图标与词各不相同 · 按类型排
    "tests/frontend/filewin/chrome_tests.rs", // 窗口的框：历史 · 地址栏 · 导航键 · 状态栏 · 隐藏文件 · 表头（合成事件）
    "tests/frontend/filewin/picker_tests.rs", // 原生选文件框（假选择框注入）
    "tests/frontend/filewin/rows_tests.rs",
    "tests/frontend/filewin/scale_tests.rs",
    "tests/frontend/filewin/shell_keys_tests.rs",
    "tests/frontend/filewin/size_tests.rs", // 算大小（窗口那一侧）
    "tests/frontend/filewin/extract_tests.rs", // 解压到这里（窗口那一侧，合成后端）
    "tests/frontend/filewin/workspace_tests.rs",
    "tests/frontend/filewin/writeops_tests.rs",
    "tests/frontend/shell/lib_batch_tests.rs",
    "tests/frontend/shell/lib_nudge_skip_tests.rs",
    "tests/frontend/shell/lib_remote_config_tests.rs",
    // `port_forward_tests.rs` 随转发账进本机常驻后端删了（判据搬去 `tests/backend/dial_forwards_tests.rs`）。
    "tests/frontend/shell/session_book_tests.rs", // 会话成品缓存的真值表（替掉容器账与本机活会话表那两份判据，两本账随裁决搬进后端）
    // `session_map_f13_tests.rs` 与 `session_map_linux_liveness.rs` 随 monitor 自己那份本机判活删了。
    "tests/frontend/shell/sftp_pool_tests.rs",
    "tests/comms/inward/ssh_link_tests.rs",
    "tests/comms/inward/backend_route_tests.rs",
    "tests/frontend/shell/stream_source/batcher_tests.rs",
    "tests/frontend/shell/stream_source/seam_tests.rs",
    "tests/frontend/shell/stream_source/snapshot_tail_tests.rs",
    "tests/frontend/shell/stream_source/snapshot_tests.rs",
    // 足迹申报表的两份判据随表搬进后端（原 `tests/frontend/shell/tool_registry_{environment,not_managed}_tests.rs`）。
    "tests/backend/footprint/registry_environment_tests.rs",
    "tests/backend/footprint/registry_not_managed_tests.rs",
    "tests/frontend/shell/footprint_client_tests.rs", // monitor 自己进程那几条事实（纯函数；两拍之后它不再 stat）
    "tests/frontend/ui/cards/api-error.test.ts",
    "tests/frontend/ui/cards/bash-collapse.vitest.ts",
    "tests/frontend/ui/cards/diff.test.ts",
    "tests/frontend/ui/cards/file-input.vitest.ts",
    "tests/frontend/ui/cards/interactive.vitest.ts",
    "tests/frontend/ui/cards/json-prefix.vitest.ts", // firstLineOf / jsonPrefix 与原式对拍（纯函数）
    "tests/frontend/ui/config-fields.vitest.ts",
    "tests/frontend/ui/e2e-probe.vitest.ts",
    "tests/frontend/ui/kit/toast.vitest.ts",
    "tests/frontend/ui/kit/components.vitest.ts", // 通用组件各态（DOM 形状与行为）
    "tests/frontend/ui/kit/interrupts.vitest.ts",
    "tests/frontend/ui/kit/menu.vitest.ts",
    "tests/frontend/ui/events-burst.vitest.ts",
    "tests/frontend/ui/events-yield.vitest.ts",
    "tests/frontend/ui/file-window.vitest.ts",
    "tests/frontend/ui/fork-ask.vitest.ts",
    "tests/frontend/ui/fork-start.vitest.ts",
    "tests/frontend/ui/format.test.ts",
    "tests/frontend/ui/height-estimate.vitest.ts",
    "tests/frontend/ui/invariants-frontend-guard.vitest.ts",
    "tests/frontend/ui/keybindings/actions.vitest.ts",
    "tests/frontend/ui/launch-arrival.vitest.ts", // 起会话的真成功正信号（纯函数 ＋ 假定时器，不碰真窗口）
    "tests/frontend/ui/launch-menu.vitest.ts",
    "tests/frontend/ui/launcher-diagnostics.vitest.ts",
    "tests/frontend/ui/live-window.vitest.ts",
    "tests/frontend/ui/reconcile-shell.vitest.ts",
    "tests/frontend/ui/record-timeline.vitest.ts",
    "tests/frontend/ui/remote-health.test.ts",
    "tests/frontend/ui/remote-launch-run.vitest.ts",
    "tests/frontend/ui/render-window.vitest.ts",
    "tests/frontend/ui/resume-presets.vitest.ts",
    "tests/frontend/ui/route-parity.vitest.ts",
    "tests/frontend/ui/session-accounts-poll.vitest.ts",
    "tests/frontend/ui/session-status.vitest.ts",
    "tests/frontend/ui/settings/account-new-form.vitest.ts",
    "tests/frontend/ui/settings/cc-bus-section.vitest.ts",
    "tests/frontend/ui/settings/claude-dir-check.vitest.ts", // Claude 数据目录存之前问本机后端在不在（假通道）
    "tests/frontend/ui/settings/config-surface-section.vitest.ts",
    "tests/frontend/ui/settings/context-limits-section.vitest.ts", // `contextLimits` 的入口（假 IPC）
    "tests/frontend/ui/settings/diagnostics-section.vitest.ts",
    "tests/frontend/ui/settings/drift-ledger-section.vitest.ts",
    "tests/frontend/ui/settings/host-os.vitest.ts",
    "tests/frontend/ui/kit/tooltip.vitest.ts",
    "tests/frontend/ui/settings/machine-aliases.vitest.ts",
    "tests/frontend/ui/settings/machine-context.vitest.ts",
    "tests/frontend/ui/settings/machine-list-backend-cells.vitest.ts",
    "tests/frontend/ui/settings/machine-status.vitest.ts",
    "tests/frontend/ui/settings/machine-sync.vitest.ts",
    "tests/frontend/ui/settings/panel-block-isolation.vitest.ts",
    "tests/frontend/ui/settings/panel-deferred-io.vitest.ts",
    "tests/frontend/ui/settings/panel-groups.vitest.ts",
    "tests/frontend/ui/settings/panel-machine-page-visibility.vitest.ts",
    "tests/frontend/ui/settings/panel-per-machine-deferred-io.vitest.ts",
    "tests/frontend/ui/settings/panel-window-lifecycle.vitest.ts",
    "tests/frontend/ui/settings/pending-and-block-errors.vitest.ts",
    "tests/frontend/ui/settings/readiness.vitest.ts",
    "tests/frontend/ui/settings/remote-section-smoke.vitest.ts",
    "tests/frontend/ui/settings/restart-notice.vitest.ts",
    "tests/frontend/ui/settings/router.vitest.ts",
    "tests/frontend/ui/settings/settings-skeleton.vitest.ts",
    "tests/frontend/ui/settings/settings-unique-names.vitest.ts",
    "tests/frontend/ui/settings/ui-copy-discipline.vitest.ts",
    "tests/frontend/ui/settings/unknown-keys-notice.vitest.ts",
    "tests/frontend/ui/skeleton-ledger.vitest.ts",
    "tests/frontend/ui/skeleton-view.vitest.ts",
    "tests/frontend/ui/stream-viewport-resize.vitest.ts",
    "tests/frontend/ui/tab-bar-perf.vitest.ts",
    "tests/frontend/ui/tab-bar-state.vitest.ts",
    "tests/frontend/ui/tab-collections.vitest.ts",
    "tests/frontend/ui/tasks-panel-origin.vitest.ts",
    "tests/test-support/component-sources.vitest.ts",
    "tests/frontend/ui/usage-hud.vitest.ts",
    "tests/frontend/ui/main-window-fixes.vitest.ts", // 主窗口交互逻辑：组字 · 弹层栈挡单键 · 确认框焦点 · 子 agent 时间线 · agent 面板原地更新
    "tests/frontend/ui/main-window-behavior.vitest.ts", // 主窗口：状态栏两块浮层 · 右键菜单躲边与关闭 · 说不清单列 · 多机选单 · 工具组与命令卡（jsdom，假后端）
    "tests/frontend/ui/views/cc-bus-view.vitest.ts",
    "tests/frontend/ui/views/context-limit.test.ts",
    "tests/frontend/ui/views/counted.vitest.ts",
    "tests/frontend/ui/views/grid-monitor.vitest.ts",
    "tests/frontend/ui/views/history-actions.test.ts",
    "tests/frontend/ui/views/history-actions.vitest.ts",
    "tests/frontend/ui/views/history-cache.test.ts",
    // 两份等本机索引的前端判据（`history-close-stops-retry` · `history-index-wait`）随那条 1 s 重跑链删了。
    "tests/frontend/ui/views/history-counted.vitest.ts",
    "tests/frontend/ui/views/history-state-chip.vitest.ts", // 历史状态词只住 sessionState.*
    "tests/frontend/ui/views/history-filter-collapse.vitest.ts",
    "tests/frontend/ui/views/history-prefs.test.ts",
    "tests/frontend/ui/views/history-search-resume.vitest.ts",
    "tests/frontend/ui/views/history-search-truncation.vitest.ts",
    "tests/frontend/ui/views/history-search.vitest.ts",
    "tests/frontend/ui/views/history-source-cache.vitest.ts",
    "tests/frontend/ui/views/pane-preview.vitest.ts",
    "tests/frontend/ui/views/session-find.vitest.ts",
    "tests/frontend/ui/views/session-viewer-scroll.vitest.ts",
    "tests/frontend/ui/views/session-viewer-skeleton.vitest.ts",
    "tests/frontend/ui/views/user-input-panel.vitest.ts",
    "tests/common/upstream-url-core/lib_tests.rs", // 新共享 crate `upstream-url-core` 的判定（纯函数）
    "tests/common/deploy-contract/lib_tests.rs", // 部署契约（戳格式那几格；判定那几格随判定进了后端 `deploy_plan_tests`）
    "tests/frontend/ui/cards/long-reply.vitest.ts", // 超长回复切片 ＋「显示全部」分片渲染（jsdom，假定时器）
    // 基数 → 增量 +1：起会话那几问的帧命令应答（`control/launch_render/mod.rs::answer_*`）。
    "tests/backend/control/launch_render/answers_tests.rs",
    // 基数 → 增量 +1：`$PROFILE` 备份那一格问本机后端（假通道）。
    "tests/frontend/ui/settings/profile-backups.vitest.ts",
    // 从 SCAN 挪来：本机起会话那一行（`control/launch_render/local.rs::plan` 纯函数，喂确定的事实）。
    "tests/backend/control/launch_render/local_tests.rs",
    "tests/frontend/ui/tab-batch-menu.vitest.ts", // 批量菜单：后端那两件与宿主都是替身
    "tests/frontend/ui/tab-batch-run.vitest.ts",  // 批量停 / 起交给那几台：`chan_call` 那一跳是替身
    "tests/frontend/ui/terminal-front-pending.vitest.ts", // ↗ 在飞不重发 ＋「进行中」的显隐时刻（假定时器）
    "tests/backend/dial_known_hosts_tests.rs", // cc-monitor 那份 known_hosts 的合并（纯函数）
];

/// 源码扫描层：读仓内文本（`include_str!` · `repo_root()` 一族 · `readFileSync` 一族）、不碰 OS。
const SCAN: &[&str] = &[
    "tests/backend/control/kill_tests.rs", // INTEGRATION → SCAN：总期限那条挪进 `stream/caps_tests.rs`（走真分派），剩下的读跨语言金样
    "tests/backend/control/fork_launch_tests.rs", // 分叉之后起的推断（纯函数）＋ 读跨语言金样那一条
    "tests/frontend/ui/ext-reads.vitest.ts", // 扩展页几问走通道：严格收 ＋ 问对那台（假通道 ＋ 金样）
    "tests/frontend/ui/settings/ext-section.vitest.ts", // 扩展页：表 · 抽屉 · 确认卡（假通道）＋ 界面零判定扫描
    "tests/frontend/ui/settings/relay-optin-section.vitest.ts", // 「终端」栏直接敲的也走中转：严格收（金样）＋ 照态画（假通道）＋ 读口源码只问 relay-optin
    // UNIT → SCAN：起前清洗那份名单读生成物（`include_str!` 画像表），判据对金样读。
    "tests/frontend/shell/lib_env_scrub_tests.rs",
    // 原 `search-core` 纯函数判据的通用那一半 ＋ 「口径只有一个家」那两道（读后端三份生产源码）⇒ 扫描层。
    "tests/backend/observe/search_rules_tests.rs",
    // 记录解释搬进后端：漂移账（原 monitor `drift_ledger_tests.rs` 的记录两面）· 抢救与记账（原 `parser_tests.rs`）·
    //   记录的线上形状（原 `messages_tests.rs`）· 标题记录一个不漏被接住（原 `history_title_coverage.rs`）。
    "tests/backend/agents/claudecode/drift_tests.rs",
    "tests/backend/agents/claudecode/parse_tests.rs",
    "tests/backend/agents/claudecode/schema_tests.rs",
    "tests/backend/agents/claudecode/schema_title_coverage.rs",
    "tests/frontend/ui/remote-probe.vitest.ts", // 测试连接的读口（请求体 · 进度流 · 严格收）＋ 与 Rust `event_replay.rs::PROBE_PROGRESS_KIND` 对拍流名
    "tests/frontend/shell/remote_resident_tests.rs", // UNIT → SCAN：多一条跨半边期限对拍（`include_str!` 读后端 `control/resident.rs`）
    "tests/frontend/ui/tasks-decode.vitest.ts",      // 读跨语言金样 tasks-list.golden.json
    "tests/frontend/ui/record-file-notice.vitest.ts", // D-d：活会话 jsonl 不见了 / 被截短 / 被改写 ⇒ tab 顶一行提示
    "tests/frontend/ui/config-patch-fake.vitest.ts",  // 假盘对跨语言金样 config-patch.golden.json
    "tests/frontend/ui/tab-bar-width.vitest.ts", // J7 tab 栏宽度走存储接入层、零裸 localStorage
    "tests/frontend/ui/kit/single-home.vitest.ts", // 对话框 · 菜单 · 悬停提示 · toast 只在 kit 里建（扫生产 TS）
    "tests/frontend/ui/design-tokens.vitest.ts", // 对比度实算对规范表 · 令牌之外不写字面量（读 CSS）
    "tests/frontend/ui/kit/dialog.vitest.ts", // D1 生产 TS 零原生 confirm/prompt（AST 扫）· D1b 对话框必 await ＋ 调用方清单
    "tests/frontend/ui/bg-flat.vitest.ts", // tab 栏通用代码零 bg 分叉（扫 `src/frontend/ui/tabs.ts` ＋ `src/tab-*.ts`）· CSS 零 `.tab-bg`
    "tests/frontend/ui/account-availability-guard.vitest.ts",
    "tests/frontend/ui/account-base-semantics.vitest.ts",
    "tests/frontend/ui/account-chip.vitest.ts",
    "tests/frontend/ui/accounts-decode.vitest.ts", // 读跨语言金样（`tests/__fixtures__/accounts.golden.json`）
    "tests/frontend/ui/apikey-reads.vitest.ts", // 读跨语言金样（`tests/__fixtures__/apikey.golden.json`）
    "tests/frontend/ui/ssh-config-reads.vitest.ts", // 读跨语言金样（`tests/__fixtures__/ssh-config.golden.json`）
    "tests/frontend/ui/terminal-reads.vitest.ts", // 读跨语言金样（`tests/__fixtures__/terminals.golden.json`）
    "tests/frontend/ui/port-forward-reads.vitest.ts", // 读跨语言金样（`tests/__fixtures__/forward-list.golden.json`）
    "tests/frontend/ui/session-stream-credit.vitest.ts", // 读跨语言金样（`tests/__fixtures__/session-stream-credit.golden.json`）
    "tests/frontend/ui/history-reads.vitest.ts", // 读跨语言金样（`tests/__fixtures__/history-products.golden.json`）
    "tests/frontend/ui/agent-profile-parity.vitest.ts",
    "tests/frontend/ui/app-grid-claims.vitest.ts",
    "tests/backend/agent_boundary_guard.rs",
    "tests/backend/agent_locality_guard.rs",
    "tests/backend/alloc_probe_tests.rs",
    "tests/backend/build_id_guard.rs",
    "tests/backend/capability_ledger_guard.rs",
    "tests/backend/cc_bus_boundary_guard.rs",
    "tests/backend/common/session_snapshot_tests.rs",
    "tests/backend/common/tmux_utf8_tests.rs",
    "tests/backend/control/cc_bus_tests.rs",
    "tests/backend/control/ccm/argv_tests.rs",
    "tests/backend/control/cli_control_tests.rs",
    "tests/backend/control/gate_tests.rs",
    "tests/backend/control/resolve_query_tests.rs", // UNIT → SCAN：跨仓承诺那一族读冻结金样与 IPC-PROTOCOL
    "tests/backend/dial_sftp_tests.rs",
    "tests/backend/dial_forwards_tests.rs", // 转发账判据 ＋ 写跨语言金样（`include_str!` 读 `forward-list.golden.json`）
    "tests/backend/dial_ssh_config_tests.rs", // 规则判据 ＋ 写跨语言金样（`include_str!` 读 `ssh-config.golden.json`）
    "tests/common/creds-core/store_tests.rs", // UNIT → SCAN：多一条「两侧读家目录同一个函数」（读两棵生产树的源码现扫调用点）
    "tests/backend/dial_terminal_tests.rs", // UNIT → SCAN：开终端那一行的渲染规则 ＋ 起会话夹具的全部请求都过守卫（`include_str!` 读 `launch_render/fixtures/cli-golden.json`）
    "tests/backend/dial_tests.rs",
    "tests/backend/files/module_boundary_guard.rs",
    "tests/backend/guard_support_tests.rs",
    "tests/backend/stream/inbound_structure_guards.rs",
    // 集成 → SCAN：唯一那条真进程判据（`cancel` 真打断在飞的全景请求）随代码全景删了。
    "tests/backend/stream/inbound_tests.rs",
    "tests/backend/stream/listen_tests.rs",
    "tests/backend/main_argv_table_guard.rs",
    "tests/backend/main_window_raise_guard.rs",
    "tests/backend/no_timer_guard.rs",
    // 会话事实的口径与续传（纯字节）＋ 一条异源对拍读生成物 `src/frontend/ui/generated/agent-profile-table.ts` ⇒ 判别器判扫描层。
    "tests/backend/platform/cfgless_guard.rs",
    "tests/backend/platform/fallback_guard.rs",
    "tests/backend/platform/pidwatch/linux_tests.rs",
    "tests/backend/platform/pidwatch_death_event_leg_tests.rs",
    "tests/backend/platform/pidwatch_fallback_shape_tests.rs",
    "tests/backend/platform/pidwatch_windows_shape_tests.rs",
    "tests/backend/platform/win_proc_contract_tests.rs",
    "tests/backend/plugin_layer_guard.rs",
    "tests/backend/protocol_doc_guard.rs",
    "tests/backend/ratchet_guard.rs",
    "tests/backend/readonly_guard.rs",
    "tests/backend/relay/upstream_selection_guard.rs",
    "tests/backend/relay/bind_guard.rs",
    "tests/backend/relay/creds_guard.rs",
    "tests/comms/outward/framer_tests.rs",
    "tests/backend/relay/table_guard.rs",
    "tests/backend/single_stream_guard.rs",
    "tests/backend/target_parity_guard.rs",
    "tests/frontend/ui/base-flag-contract-guard.vitest.ts",
    // `tests/frontend/shell/agent_dispatch_registry_tests.rs` 删了（monitor 那份适配表退役，对拍与生成器随家进了后端 `tests/backend/agents_tests.rs`）。
    "tests/frontend/shell/arch_doc_shape_guard_tests.rs",
    "tests/frontend/shell/asset_sync_tests.rs", //
    "tests/frontend/shell/atomic_replace_registry_tests.rs",
    // `tests/frontend/shell/agent_profile_parity_tests.rs` 删了（monitor 那份适配表退役，对拍与生成器随家进了后端 `tests/backend/agents_tests.rs`）。
    "tests/frontend/shell/backend_control_tests.rs",
    "tests/frontend/shell/backend_kill_tests.rs", // 挂载点从 `backend_kill.rs` 换成 `backend/control/mod.rs`（发送端删了，判据留着）；同拍 `backend_launch_tests.rs` / `backend_send_keys_tests.rs` 随发送端删掉、摘了
    "tests/frontend/shell/backend_route_senders_tests.rs",
    "tests/backend/control/launch_render/ccm_invocation_tests.rs",
    "tests/frontend/shell/frame_query_tests.rs",
    "tests/frontend/shell/inbound_client_tests.rs",
    "tests/frontend/shell/backend_policy_tests.rs",
    "tests/frontend/shell/backend_client_guard_tests.rs", // 原 `backend_tests.rs`：`backend/` 目录没了，两道判据改看逐个点名的那一组
    "tests/frontend/shell/platform/platform_home_guard.rs", // 壳里平台形态只许住 `platform/`（与 host-core）＋ 待收名单
    "tests/frontend/shell/contract_crate_guard_tests.rs",   // 读 manifest 与生产源码
    "tests/frontend/shell/bus_identity_registry_tests.rs",
    "tests/frontend/shell/byte_cap_registry_tests.rs",
    "tests/frontend/shell/ccm_cli_contract_tests.rs",
    "tests/frontend/shell/chan/webview_tests.rs",
    "tests/frontend/ui/alias-reads.vitest.ts", // 别名八问走通道：解码器读跨语言金样 aliases.golden.json ＋ 问对那台
    "tests/frontend/ui/settings/machine-aliases-no-ccm-grammar.vitest.ts", // 别名页零处自己解析 ccm 参数（读后端 argv.rs 的旗标表 ＋ 扫界面源码）
    "tests/backend/assets/aliases/form_tests.rs", // 别名表单两向：往返逐字相等（造的语料）＋ 规范写法交 ccm 解析器 ＋ 读金样 aliases.golden.json
    "tests/backend/platform/shell/dialect_tests.rs", // 随方言搬进 `platform/shell/` // 方言进了那台后端（读法 ＋ `$PROFILE` 一个家的全树普查）
    "tests/backend/assets/aliases/fence_tests.rs",   // 围栏块配对 ＋ 形状账（源码扫描）
    "tests/frontend/shell/cc_bus_deploy_tests.rs", // 装 / 三态进了后端，只剩装前 `ccm` 预检（纯函数 ＋ 读源码）⇒ INTEGRATION → SCAN
    "tests/frontend/shell/command_home_registry_tests.rs", // Tauri 命令两张封闭表（扫源码）
    "tests/backend/footprint/rows_tests.rs", // 原 `tests/frontend/shell/config_surface_tests.rs` 随判定搬进后端
    "tests/frontend/shell/copy_table_tests.rs",
    "tests/frontend/shell/creds_store_tests.rs", // 读侧那几条（读真文件）搬去后端 ⇒ 只剩源码扫描（INTEGRATION → SCAN）
    "tests/common/acct-core/lib_tests.rs",
    "tests/common/copy-core/lib_tests.rs",
    "tests/common/creds-core/lib_tests.rs",
    "tests/common/guard-core/lib_tests.rs",
    "tests/common/guard-core/test_tiers_tests.rs",
    "tests/frontend/shell/cross_half_edge_registry_tests.rs",
    "tests/frontend/shell/doc_claim_registry_daemon_wording_registry.rs",
    "tests/frontend/shell/doc_claim_registry_frozen_daemon_census.rs",
    "tests/frontend/shell/doc_copy_registry_tests.rs",
    "tests/frontend/shell/drift_ledger_tests.rs",
    // 单元层 → 扫描层：多了一条「分档那一套在两棵生产段零命中」（`repo_root()` 扫 `src/`）。
    "tests/frontend/shell/event_replay_tests.rs",
    "tests/frontend/shell/exec_site_registry_tests.rs",
    "tests/frontend/shell/frame_tally_tests.rs", // 丢帧账：账本行为 ＋ 三条读帧循环的接线（剥过的生产文本）
    "tests/frontend/shell/swallow_registry_tests.rs", // 业务路径零裸吞：人群从四棵生产源码树派生 == 登记表
    // `tests/frontend/shell/fenced_block_tests.rs` 随别名那一族进了那台后端（`tests/backend/assets/aliases/`）。
    "tests/frontend/filewin/bigfile_tests.rs",
    "tests/frontend/filewin/guard_support_tests.rs", // 窗口包的判据住址反空真
    "tests/frontend/shell/filewin/boundary_tests.rs",
    "tests/frontend/filewin/copy_tests.rs",
    "tests/frontend/filewin/download_tests.rs",
    "tests/frontend/shell/filewin/entry_tests.rs", // INTEGRATION → SCAN：落盘那条（书签旧键搬家）退役删了
    "tests/frontend/filewin/find_tests.rs",
    "tests/frontend/filewin/grep_tests.rs", // 按内容搜：真通道上的合成后端 ＋ 跨半边金样
    "tests/frontend/filewin/lossy_pull_tests.rs", // 有损名下载：合成对端 ＋ 读后端源码钉暂存区常量相等
    "tests/frontend/filewin/cross_copy_tests.rs", // 复制到另一台：合成对端（按 origin 记）＋ 读 app 源码钉 <local> 相等
    "tests/frontend/filewin/fonts_tests.rs",
    "tests/frontend/filewin/select_tests.rs",
    "tests/frontend/filewin/transfer_tests.rs",
    "tests/frontend/shell/fixture_guard_tests.rs",
    // `tests/frontend/shell/footprint_remote_tests.rs` 删了：两趟问法随实现进后端 face（`tests/backend/footprint/face_tests.rs`）。
    "tests/frontend/ui/settings/footprint-reads.vitest.ts", // 足迹成品的跨语言金样（TS 那一侧读同一份）
    "tests/frontend/shell/frame_cadence_guard_tests.rs",
    "tests/frontend/shell/gate_singleton_guard_tests.rs",
    "tests/frontend/shell/guard_support_tests.rs",
    "tests/frontend/shell/launcher_identity_registry_tests.rs",
    "tests/frontend/shell/lib_invariant_population_tests.rs", // §47 / §49 人群判据（读仓内源码）
    "tests/frontend/shell/lib_mod_decl_hygiene_tests.rs",
    "tests/frontend/shell/lib_window_lifecycle_tests.rs",
    "tests/frontend/shell/link_mux_tests.rs",
    "tests/frontend/shell/local_origin_registry_tests.rs",
    "tests/frontend/shell/local_read_surface_registry_tests.rs",
    "tests/frontend/shell/lockfile_conflict_guard_tests.rs",
    "tests/frontend/shell/needle_anchor_registry_tests.rs",
    "tests/comms/inward/origin_tests.rs",
    "tests/frontend/shell/parity_ledger_tests.rs",
    "tests/frontend/shell/paths_tests.rs",
    "tests/frontend/shell/plugin_class_registry_tests.rs",
    "tests/frontend/shell/polling_registry_tests.rs",
    "tests/frontend/shell/profile_installer_handshake_doc_guard.rs",
    "tests/frontend/shell/quote_singleton_guard_tests.rs",
    // `tests/frontend/shell/remote_history_kr83_tests.rs` 删了（`K-R83` 那三条随 join 搬进后端 `history_join_tests.rs`）。
    "tests/frontend/shell/remote_write_registry_tests.rs",
    "tests/frontend/shell/rust_timer_registry_tests.rs",
    "tests/frontend/shell/search_kou_jing_guard.rs",
    "tests/frontend/shell/session_name_registry_tests.rs",
    "tests/frontend/shell/sftp_family_registry_tests.rs",
    "tests/frontend/shell/shell_lint_registry_tests.rs",
    "tests/frontend/shell/snapshot_resume_tests.rs",
    "tests/frontend/shell/spawn_managed_exit_sites.rs",
    "tests/frontend/shell/spawn_managed_tests.rs",
    "tests/frontend/ui/events-tap-machines.vitest.ts", // 读 `src/frontend/ui/main.ts` 的 tap 订阅清单
    "tests/frontend/shell/stream_source/capped_line_tests.rs",
    "tests/frontend/shell/stream_source/coldstart_perf_guard.rs",
    "tests/frontend/shell/stream_source/coldstart_preflight_guard.rs",
    "tests/frontend/shell/stream_source/dial_move_judge.rs",
    "tests/frontend/shell/stream_source/golden_tests.rs", // 读跨语言金样（`tests/__fixtures__/session-stream.golden.jsonl`）
    "tests/frontend/shell/stream_source/frame_dispatch_shape.rs",
    "tests/frontend/shell/stream_source/parse_frame_tests.rs",
    "tests/frontend/shell/stream_source/stream_flag_gate_tests.rs",
    "tests/frontend/shell/stream_source/tier1_tests.rs",
    "tests/frontend/shell/stream_source/write_half_guard.rs",
    "tests/frontend/shell/sync_command_registry_tests.rs", // 同步 IPC 命令的调用闭包零 `block_on` / 零同步连后端（`INVARIANTS §10`）
    "tests/frontend/shell/tmux_backend_gate_guard_tests.rs",
    "tests/backend/footprint/registry_tests.rs", // 原 `tests/frontend/shell/tool_registry_tests.rs` 随申报表搬进后端
    "tests/frontend/shell/write_site_registry_spawn_sites.rs",
    "tests/frontend/shell/write_site_registry_tests.rs",
    "tests/frontend/ui/config-unknown-keys.vitest.ts",
    "tests/copy/copy-rules.vitest.ts",
    "tests/copy/copy-table.vitest.ts",
    "tests/copy/copy-terms.vitest.ts",
    "tests/frontend/ui/css-conventions.vitest.ts",
    "tests/frontend/ui/css-ledger.vitest.ts",
    "tests/frontend/ui/css-modules.vitest.ts",
    "tests/frontend/ui/entry-graphs.vitest.ts",
    "tests/frontend/ui/events-batch-schedule.vitest.ts",
    "tests/frontend/ui/first-run-hint.vitest.ts",
    "tests/frontend/ui/fork-flow.vitest.ts",
    "tests/frontend/ui/generated-boundary-guard.vitest.ts",
    "tests/frontend/ui/gray-light-wiring.vitest.ts",
    "tests/frontend/ui/identifier-rules-parity.vitest.ts", // 读共用金样（仓内文本）⇒ 扫描层
    "tests/frontend/ui/import-cycle-guard.vitest.ts",
    "tests/comms/inward/chan.vitest.ts",
    "tests/frontend/ui/ipc/commands.vitest.ts",
    // 判定只有一个家（读 `*-core` 与 TS 生产段全集 ⇒ 扫描层）。
    "tests/frontend/ui/judgment-single-home.vitest.ts",
    "tests/frontend/ui/launch-cli-wire.vitest.ts",
    "tests/frontend/ui/launch-no-shell-in-ts.vitest.ts", // 条 1（接替 session-backend-gate）
    // 铸名 / 本机 resume 编排各只有一个家（读生产段全集 ⇒ 扫描层）。
    "tests/frontend/ui/launch-orchestration-single-home.vitest.ts",
    // UNIT → SCAN：Codex 记录「谁说的」读金样（`include_str!` 读 `tests/__fixtures__/codex-speaker.golden.jsonl`）。
    "tests/backend/agents/codex/record_tests.rs",
    "tests/backend/agents/sse_anthropic_tests.rs", // Anthropic 流的折法 ＋ 台架那一轮（读 `tests/__fixtures__/tap-bench.json`，随折法从界面那一侧搬来）
    // `tests/frontend/ui/liveness-process-names-parity.vitest.ts` 删：判活进程名的前端那一份随判定进了后端，
    //   后端两处（tmux 那一格 · cmdline 判活）今天读同一张 `agents/claudecode/cards.rs::PROCESS_NAMES`，对拍无对象。
    // `onLine` 调用人群 ＋ 事实字段写者（读 `src/frontend/ui/tabs.ts` 与 `src/**/*.ts` 的 AST）。
    "tests/frontend/ui/online-bypass-ledger.vitest.ts",
    // 「是不是本机」只在 `src/frontend/ui/ipc/origin.ts` 判（读生产段全集 ⇒ 扫描层）。
    "tests/frontend/ui/origin-single-home.vitest.ts",
    // overlay 路由的语义 ＋ `main.ts` 零处自判开没开（源码扫描）。
    "tests/frontend/ui/overlay-router.vitest.ts",
    "tests/frontend/ui/paste-block-guard.vitest.ts",
    "tests/frontend/ui/restart-in-backend-guard.vitest.ts",
    "tests/frontend/ui/cc-bus-hooks-reads.vitest.ts", // cc-bus 钩子状态：严格收（金样）＋ 四态不误说 ＋ 读口源码只问 hooks-diag（读源码 ⇒ 扫描层）
    "tests/frontend/ui/paste-block.vitest.ts",
    "tests/frontend/ui/remote-config.vitest.ts",
    "tests/frontend/ui/replay-tail-keep.vitest.ts",
    "tests/frontend/ui/render.vitest.ts", // 由 UNIT 挪来：D3 那一格读 `src/frontend/ui/render.ts` 源码（顶层零 `let`）
    "tests/frontend/ui/scale1-render-cost.vitest.ts",
    "tests/frontend/ui/scale2-height-truth.vitest.ts",
    "tests/frontend/ui/scale3-one-screen-gate.vitest.ts",
    "tests/frontend/ui/scale4-frame-ledger.vitest.ts",
    "tests/frontend/ui/scale5-replay-queue-depth.vitest.ts",
    "tests/frontend/ui/scale6-memory-ledger.vitest.ts",
    "tests/frontend/ui/scanning-guard-registry.vitest.ts",
    "tests/frontend/ui/session-reads.vitest.ts", // 读跨语言金样（`tests/__fixtures__/session-reads.golden.json`）
    "tests/frontend/ui/tmux-control.vitest.ts", // 读跨语言金样（`tests/__fixtures__/tmux-control.golden.json`）
    "tests/frontend/ui/resync.vitest.ts", // 读跨语言金样（`tests/__fixtures__/resync.golden.json`）
    "tests/frontend/ui/cc-bus-control.vitest.ts", // 读跨语言金样（`tests/__fixtures__/cc-bus-control.golden.json`）
    "tests/frontend/ui/settings/accounts-section.vitest.ts",
    "tests/frontend/ui/settings/backend-section.vitest.ts",
    "tests/frontend/ui/settings/base-wording-guard.vitest.ts",
    "tests/frontend/ui/settings/data-section.vitest.ts",
    "tests/frontend/ui/settings/facet-producer-guard.vitest.ts",
    "tests/frontend/ui/settings/machine-aliases-naming.vitest.ts", // 读后端 `plan.rs` 原文对拍撞名退让
    "tests/frontend/ui/settings/open-settings.vitest.ts",
    "tests/frontend/ui/settings/remote-section.vitest.ts",
    "tests/frontend/ui/settings/settings-source-markdown.vitest.ts",
    // `tests/shell-quote-deceptive-parity.vitest.ts` 删了：它拍的是 TS 那份 `isValidConfigDir` 对 Rust 欺骗字符集，
    //   TS 那份按删了（拒绝集只在 Rust，`payload_tests.rs` 逐码位钉）。
    "tests/frontend/ui/tab-session-state.vitest.ts",
    "tests/frontend/ui/tabs-copy-terms.vitest.ts",
    "tests/frontend/ui/tabs-split-graph.vitest.ts",
    "tests/frontend/ui/tabs.vitest.ts",
    "tests/frontend/ui/terminal-front-command.vitest.ts",
    "tests/frontend/ui/terminal-open.vitest.ts", // 开终端三步 ＋「开窗只有一个家」（扫生产段）
    "tests/test-support/strip-comments.vitest.ts",
    "tests/frontend/ui/topbar-icons.vitest.ts",
    "tests/frontend/ui/topbar-list-parity.vitest.ts",
    "tests/frontend/ui/turn-notify.vitest.ts",
    "tests/frontend/ui/views/command-bar.vitest.ts",
    "tests/frontend/ui/views/history-fanout.vitest.ts",
    "tests/frontend/ui/views/live-user-inputs.vitest.ts",
    "tests/frontend/ui/views/session-viewer-user-inputs.vitest.ts",
    "tests/frontend/ui/upstream-url-parity.vitest.ts", // 读共用金样（仓内文本）⇒ 扫描层
    "tests/frontend/ui/events-tap.vitest.ts", // session-tap 走 subscribe；另读后端 `event_replay.rs` 的流名钉两侧同名（仓内文本）⇒ 扫描层
    "tests/frontend/ui/cc-bus-read.vitest.ts", // 驾驶舱读面读跨语言金样（`tests/__fixtures__/cc-bus-read.golden.json`）
    "tests/frontend/shell/cc_bus_tests.rs", // INTEGRATION → SCAN：起进程的那几条（本机 shell 读 · 超时不留孤儿）随驾驶舱 shell 读退役
    // 从 INTEGRATION 挪来（候选那一条不再建临时目录）。
    // `tests/frontend/shell/shell_dialect_tests.rs` 随别名那一族进了那台后端（`tests/backend/assets/aliases/`）。
    // `tests/backend/control/launch_render/local_tests.rs` 挪去 UNIT：重写成喂确定事实驱动纯函数 `plan`，不再读源码文本。
    // 基数 → 增量 +1：方言专属语法字面量只住 `platform/shell/`（扫后端生产树的字符串字面量）。
    "tests/backend/platform/shell_home_guard.rs",
    "tests/frontend/filewin/theme_tests.rs", // 样子：名单两边对上 · 映射落到 egui · 窗口零色值（读源码）
];

/// 集成层：碰 OS（起进程 · 套接字 · 真文件系统写 · 临时目录）。
const INTEGRATION: &[&str] = &[
    "tests/backend/observe/interrupts_query_tests.rs", // 会打断什么：夹具家目录 ＋ 进程级活簿表
    "tests/backend/runs_guard.rs", // 子运行：扫描 ＋ 两套形状跑同一批运行判据（临时目录里造夹具）
    "tests/backend/agents/claudecode/runs_tests.rs", // Claude Code 的子运行形状（子运行记录住址那一条碰临时目录）
    // SCAN → INTEGRATION：部署计划的编排（替身对面）＋ 读金样与后端历史表之外，`place-verdict` 帧面那一条真读写临时目录里的落点文件。
    "tests/backend/control/deploy_plan_tests.rs",
    // SCAN → INTEGRATION：「内嵌的几份后端字节要彼此同一版」那一条真跑本包的构建脚本（临时目录里铺假字节）。
    "tests/frontend/shell/byte_table_tests.rs",
    // 扫描层 → 集成层：读 gate.sh ＋ package.json 两份外部件（套件名单的唯一住址是门禁的 `run_e2e` 行）。
    "tests/frontend/shell/e2e_gate_registry_tests.rs",
    "tests/backend/observe/cc_bus_hooks_tests.rs", // 钩子诊断进后端：造一台假机器（临时目录里的 settings ＋ 程序）读回成品
    "tests/backend/control/tmux_hook_tests.rs", // 读真 `/proc` 的那一条进来之后判别器判集成（SCAN → INTEGRATION）
    "tests/backend/platform/lock_tests.rs",     // 目录锁：真目录、真线程
    "tests/backend/platform/child_tests.rs",    // 起子进程原语：真子进程超时杀组 · 真 `env` 看环境
    "tests/backend/common/own_dir_tests.rs",    // O1–O3 自家目录一律 0700、建目录调用点两向登记
    "tests/frontend/shell/config_tests.rs",     // J2/J3 12 线程 × 20 轮并发补丁写 · 补丁语义
    "tests/backend/control/overwrite_atomic_tests.rs", // W1 ulimit -f 下子进程写到一半被 SIGXFSZ 杀，目标仍是旧整份
    "tests/backend/stream/drain_tests.rs", // D2 真子进程 ＋ 真 SIGTERM：在飞阻塞命令做完才退
    "tests/common/shell-quote-core/lib_tests.rs", // 单元层 → 集成层：字节形 quote 由真 bash 读回来对拍
    "tests/frontend/filewin/chunk_upload_tests.rs", // 上传块形：临时目录里一份本机文件 ＋ 合成对端
    "tests/backend/accounts/upstream_select/creds_tests.rs",
    "tests/backend/accounts/upstream_select/endpoint_tests.rs", // 上游选择出的两份成品（金样那条读夹具文件）
    "tests/backend/accounts/upstream_select/file_face_tests.rs",
    "tests/backend/faces/accounts_face_tests.rs", // 账号库那几条命令在临时家目录上真建目录 · 链接 · 复制 · 回滚
    // 账号之间同步 MCP：临时家目录上两个号真走建库 · 加号 · 同步 · 删 · 冲突 ＋ 文件事件触发
    "tests/backend/accounts/manage/mcp_share_exec_tests.rs",
    "tests/backend/agents/claudecode/assets_tests.rs", //
    "tests/backend/agents/claudecode/paths_tests.rs", // 设置文件压不压过进程环境里的上游地址（临时目录夹具）
    "tests/frontend/shell/ccm_probe_tests.rs", // SCAN → INTEGRATION：多了「先读字节认身份」那条（临时文件夹具）
    "tests/backend/agents/fake_tests.rs",
    "tests/backend/agents_tests.rs",
    "tests/backend/assets/asset_catalog_tests.rs", //
    "tests/backend/assets/asset_sync_tests.rs",    //
    "tests/backend/agents/codex/history_tests.rs", // Codex 历史清单那一面（临时目录上的会话树）
    "tests/backend/history/history_annotations_tests.rs", // 注解读写（夹具拷进临时目录真写真读）
    "tests/backend/history/history_join_tests.rs", // 历史跨机 join（临时目录上的记录树 ＋ 替身对面）
    "tests/backend/stream/remote_ask_tests.rs",    // 问远端那一跳（真 sh 读回引号 ＋ 替身对面）
    "tests/backend/assets/skill_install_tests.rs", //
    "tests/backend/common/fs_tests.rs",
    "tests/backend/control/capture_pane_tests.rs",
    "tests/backend/control/ccm/claude_flags_tests.rs", // 读 claude --help 快照 ＋ PATH 上有就跑真 `claude --help`
    "tests/backend/control/ccm/plan_tests.rs",
    "tests/backend/control/exit_policy_tests.rs",
    "tests/backend/control/launch_account_tests.rs",
    "tests/backend/control/files_commit_tests.rs",
    "tests/backend/control/files_extract_tests.rs",
    "tests/backend/control/files_upload_chunks_tests.rs",
    "tests/backend/control/files_write_tests.rs",
    "tests/backend/control/files_toctou_tests.rs", // TOCTOU 闭合的竞争判据：临时目录上真改名 / 真复制 / 真链接
    "tests/backend/control/fork_write_tests.rs",
    "tests/backend/control/identity_tag_tests.rs",
    "tests/backend/stream/caps_tests.rs", // 总期限归发起方：隔离子进程里拿假 tmux 真走一趟分派，带 / 不带 / 带坏了的期限各一条
    "tests/backend/control/session_batch_tests.rs", // UNIT → INTEGRATION：整批总期限那条真起一个卡住的子进程
    "tests/backend/control/session_restart_tests.rs", // 等压缩那几条盯真记录文件（真 inotify）
    "tests/backend/control/launch_tests.rs", // 由扫描层挪来：S4 那条判据真起一个假 tmux 子进程（`ran` 收 stderr）
    "tests/backend/control/terminals_tests.rs", // 终端管理 L1：隔离 socket 上起真 tmux，真列 · 真抓 · 真送
    "tests/backend/control/resident_tests.rs", // 临时目录上真铸钥匙、读钥匙文件 ·真 sh 子进程 ＋ 真信号：graceful / killed / not_running
    "tests/backend/control/transfer_tests.rs",
    "tests/backend/dial_compress_tests.rs",
    "tests/backend/dial_link_tests.rs",
    "tests/backend/faces/feature_face_tests.rs",
    "tests/backend/files/browse_watch_tests.rs",
    "tests/backend/files/capability_guard.rs",
    "tests/backend/files/index_tests.rs",
    "tests/backend/files/raw_tests.rs",
    "tests/backend/files/scale_f2.rs",
    "tests/backend/files/size_tests.rs", // 算目录大小（临时目录真走一棵树）
    "tests/backend/files/grep_tests.rs", // 按内容搜：真铺一棵临时树、注入设备号与小上界
    "tests/backend/footprint/face_tests.rs", // `footprint-report` 那一面（真 stat 临时目录；原 `tests/backend/footprint_tests.rs`）
    "tests/backend/layering_guard.rs",
    "tests/backend/main_fourth_face_tests.rs",
    "tests/backend/assets/mcp_sync_tests.rs",        //
    "tests/backend/assets/mcp_edit_tests.rs", // MCP 写进了那台后端（临时目录 ＋ 跨语言金样 mcp-edit.golden.json）
    "tests/backend/assets/mcp_sync_flow_tests.rs", // MCP 装到一台的内层三问（临时目录；密钥值不出来源机）
    "tests/backend/assets/ext_tests.rs", // 扩展页：表的判定 ＋ 两台（本机 ＋ 假远端）装 / 卸端到端 ＋ 金样 ext-flow.golden.json
    "tests/backend/assets/skill_flow_tests.rs", // skill 装卸进了被写那台后端（临时目录 ＋ 金样 skill-flow.golden.json）
    "tests/backend/assets/cc_bus_install_tests.rs", // cc-bus 装进了本机后端（临时目录真装 · 真改名备份 · 可执行位）
    "tests/backend/assets/aliases/aliases_tests.rs", // 别名进了那台后端（临时 home 上真走本进程 files-* ＋ 真 bash ＋ 金样 aliases.golden.json）
    "tests/backend/assets/aliases/block_tests.rs",   // 别名块进了那台后端（临时目录真装真卸）
    "tests/backend/observe/accounts_query_tests.rs",
    "tests/backend/observe/fence_tests.rs", // 读路径围栏一个家（扫 observe 全树）＋ 放行 / 拒绝行为（临时目录 · symlink）
    "tests/backend/observe/history_query_f07_tests.rs",
    "tests/backend/observe/history_query_index_tests.rs",
    "tests/backend/observe/history_query_kr83_tests.rs",
    "tests/backend/observe/history_query_tests.rs",
    "tests/backend/observe/history_query_user_inputs_tests.rs",
    "tests/backend/observe/search_query_find_tests.rs",
    "tests/backend/observe/search_query_tests.rs",
    "tests/backend/observe/tasks_query_tests.rs",
    "tests/backend/observe/tmux_observe_tests.rs",
    "tests/backend/observe/watcher_tests.rs",
    "tests/backend/observe/session_terminals_tests.rs",
    "tests/backend/observe/one_wait_tests.rs",
    "tests/backend/plugin/discover_tests.rs",
    "tests/backend/plugin_walk_fixture.rs",
    "tests/backend/faces/read_face_tests.rs",
    // 〔MG1 合 RK1〕中转口的门（403 / 421）：铺真钥匙文件、起真监听 ⇒ 判别器判集成层。
    "tests/backend/relay/key_tests.rs",
    "tests/backend/relay/host_tests.rs",
    "tests/comms/outward/server_tests.rs",
    // 中转的 `observe` / `retry` 口走到账号域那一头：真中转 ＋ 生产段的上游选择 ＋ 按鉴权头作答的假上游。
    "tests/backend/relay/observe_retry_tests.rs",
    "tests/backend/relay/server_tests.rs",
    // 订阅号令牌：对一个回环上的假令牌端点续期、加锁、整份原子写回（真起监听 ＋ 临时目录）。
    "tests/backend/accounts/oauth/oauth_tests.rs",
    // 额度账：按号记账 · 落盘 · 读帧（临时目录里真写 `quota.json`）。
    "tests/backend/accounts/quota/ledger_tests.rs",
    // 换号那一族的帧命令：默认轮换 · 一批会话的那一份 · 改会话轮换 · 不重启换（临时家目录、假凭据）。
    "tests/backend/faces/rotation_face_tests.rs",
    // 轮换配置：整份收 · 钉号与记录 · 落盘与重读（临时目录里真写 `rotation.json`）。
    "tests/backend/accounts/quota/rotation_tests.rs",
    // 换号时请求体里的账号身份：读哪个号的 `.claude.json`（临时目录里真写）· 只换那几个字节。
    "tests/backend/agents/claudecode/accounts_tests.rs",
    "tests/backend/relay/wire_golden.rs",
    // L2 真起子进程（re-exec 本测试二进制，fd 2 真被换走）
    // 〔MG1 合 SU1〕skill 装记录：临时家目录里真写 / 读 / 摘 `~/.cc-monitor/skill-installs.json` ⇒ 判别器判集成层。
    "tests/backend/assets/skill_ledger_tests.rs",
    "tests/backend/stderr_log_tests.rs",
    // 历史里恢复 ⇒ 起的是那个会话的那一家：临时家目录 ＋ 假启动器，re-exec 本测试二进制让 `ccm` exec 掉它。
    "tests/backend/control/launch_render/history_resume_e2e_tests.rs",
    // 经 ccm 用某个号起一次 ⇒ 记到那条会话名下：临时家目录 ＋ 假启动器，re-exec 本测试二进制让 `ccm` exec 掉它。
    "tests/backend/control/ccm/launch_note_e2e_tests.rs",
    "tests/backend/main_claim_tests.rs", // 先抢口再接日志：占一个真回环口
    "tests/backend/stream/wire_tests.rs",
    // `tests/frontend/shell/account_aliases_tests.rs` 随别名那一族进了那台后端（`tests/backend/assets/aliases/`）。
    // `tests/frontend/shell/adapter_tests.rs` 删了（monitor 那份适配表退役，对拍与生成器随家进了后端 `tests/backend/agents_tests.rs`）。
    "tests/frontend/shell/auto_launch_tests.rs",
    "tests/frontend/shell/local_backend_tests.rs",
    // 标识符放行判定的生成物（写 `src/frontend/ui/generated/judgment-rules.ts`）＋ 共用金样 ⇒ 写真文件 ⇒ 集成层。
    "tests/frontend/shell/payload_judgment_rules.rs",
    // `tests/frontend/shell/backend_layering.rs` 删：它判 monitor 侧 `backend/` 里 `observe/` ↔ `control/` 两条线的方向，目录没了。
    "tests/frontend/shell/bind_tests.rs",
    "tests/frontend/shell/capability_registry_tests.rs",
    "tests/frontend/shell/chan/chan_tests.rs",
    "tests/frontend/shell/comm_boundary_registry_tests.rs",
    "tests/common/creds-core/perm_tests.rs",
    "tests/common/host-core/lib_tests.rs", // 原子写（真写临时文件）＋ 窗口几何，原在 `utils_tests` / `lib_window_lifecycle_tests`
    // 旧版 `~/.local/bin/ccm` 那一份：替身门在临时目录上真读真删。
    "tests/frontend/shell/ccm_legacy_tests.rs",
    "tests/frontend/shell/data_paths_tests.rs",
    "tests/frontend/shell/dial_home_registry_tests.rs",
    "tests/frontend/shell/dial_host_tests.rs",
    "tests/frontend/shell/doc_claim_registry_tests.rs",
    "tests/common/guard-core/no_outside_refs_tests.rs", // 起 `git ls-files` 取人群，同上一行
    "tests/frontend/filewin/bookmarks_tests.rs",
    "tests/frontend/filewin/proc_tests.rs", // 窗口进程那一侧（拨回 · 第一屏 · 就绪那一行）随躯体搬进窗口包
    "tests/frontend/shell/filewin/proc_tests.rs",
    "tests/frontend/filewin/shell_tests.rs",
    "tests/frontend/filewin/source_tests.rs",
    "tests/frontend/filewin/upload_tests.rs",
    "tests/frontend/filewin/xvfb_rig.rs",
    "tests/frontend/shell/history_tests.rs",
    "tests/frontend/shell/launch_tests.rs",
    // `tests/frontend/shell/local_accounts_tests.rs` 挪进 `UNIT`：驱动本机 manifest 参照实现的那几条（临时目录真写真读）随实现删了，
    //   剩下的只喂纯函数、判别器判它是单元层。
    "tests/frontend/shell/local_backend_host_tests.rs",
    "tests/frontend/shell/local_lines_tests.rs",
    "tests/frontend/shell/logging_tests.rs",
    "tests/frontend/shell/profile_installer_tests.rs",
    "tests/frontend/shell/platform/console_text_tests.rs", // 控制台字节按 OEM 代码页解（Windows 臂 ＋ PowerShell 读数起进程）
    "tests/backend/assets/pubkey_tests.rs", // 原 `tests/frontend/shell/pubkey_tests.rs` 随实现搬来（真 `sh` 上跑那一串 · 临时目录当家）
    "tests/frontend/shell/scanning_guard_registry_tests.rs",
    // `search_tests.rs` 随 monitor 内存索引删了；`session_map_linux_liveness.rs` 随本机判活删了；`session_map_tests.rs` 挪进 SCAN（判据里多了读源码的那两条）。
    "tests/frontend/shell/sftp_tests.rs",
    "tests/frontend/shell/shared_crate_registry_tests.rs",
    // `shell_dialect_tests.rs` 挪进 SCAN：候选那一条不再建临时目录（方言只给路径与列法、不读盘）。
    // `skill_host_tests.rs` · `claude_data_fence_tests.rs` 随 skill 接入面进后端删了（判据搬去 tests/backend/agents/claudecode/skill_host_tests.rs）。
    "tests/frontend/shell/structural_scan_tests.rs",
    "tests/frontend/shell/user_files_tests.rs",
    "tests/frontend/shell/utils_tests.rs",
    // `tests/frontend/shell/watcher_tests.rs` 随 monitor 自己那套 jsonl watcher 一起删了（本机会话内容改走本机后端的 `line` 帧）。
    "tests/frontend/ui/copy-verdicts-ledger.vitest.ts",
    "tests/copy/backend-copy-pending.vitest.ts",
    // 〔MG1 合 CP2b〕CP2b 待办表判据：起 python3 子进程跑 `CP2b-copy-pending.py --json` ⇒ 判别器判集成层（CP2b 分支上漏登记）。
    "tests/copy/copy-pending.vitest.ts",
    "tests/frontend/ui/eslint-baseline.vitest.ts",
    "tests/naming/account-vs-relay-naming.vitest.ts",
    "tests/frontend/ui/node-suite-registry-guard.vitest.ts",
    "tests/frontend/ui/offline-cargo-cache.vitest.ts",
    "tests/backend/observe/search_query_golden_tests.rs", // J1 合成语料写临时目录、对冻结金样
    "tests/backend/observe/search_query_reading.rs",      // 秤（真机层那一条）：起 python3 丢页缓存
    "tests/backend/observe/search_query_index_tests.rs",  // J2 / J3 临时目录上一串变更
    "tests/backend/control/ccm_tests.rs", // SCAN → INTEGRATION：多了一条隔离 socket 真 tmux 判据（`BUS_ID_RECIPE` 读会话名走 UTF-8 客户端）
    "tests/backend/agents/claudecode/mcp_tests.rs", // Claude 的 MCP 布局读法（临时目录夹具 ＋ 跨语言金样 mcp-read.golden.json）
    "tests/backend/observe/watcher_lines_tests.rs", // watcher D 块（jsonl 增量读）：临时目录假行
    "tests/frontend/filewin/props_tests.rs", // 属性：应答解法 ＋ 菜单那一下真去问合成后端（临时目录）
];

/// 支撑：测试树里**没有一条测试**的那几份（夹具 / helper）。不是一层，是让分区闭合的补集。
/// ⚠ 一份真测试文件掉光了测试属性，判别器会把它判进这里 ⇒ 与登记不一致 ⇒ 红。
const SUPPORT: &[&str] = &[
    "tests/backend/observe/runs_testing.rs", // 往运行簿里直接记一个在跑的子运行
    "tests/backend/files/index_testing.rs",
    "tests/backend/control/identity_tag_door.rs", // `identity_tag` 起 tmux 那个口的测试构建那一份（假 tmux 注入，§48.3）
    "tests/backend/platform/child_tmux_fence.rs", // 子进程起前那一道的测试构建那一份（裸名 tmux 落到本进程的空 socket 目录）
    "tests/backend/sftp_rig.rs",
    "tests/frontend/shell/backend_kill_creation_detect.rs",
    "tests/frontend/filewin/copy_testing.rs",
    "tests/frontend/filewin/find_testing.rs",
    "tests/frontend/filewin/rows_testing.rs",
    "tests/frontend/filewin/theme_testing.rs", // 缺省主题：照 tokens.css 解
    "tests/frontend/filewin/chrome_testing.rs", // 一个目录视图连同它的框跑一帧（同 `Workspace::chrome_ui` 那一套面板）
    "tests/frontend/shell/shared_crate_registry_ci_yaml.rs",
    "tests/frontend/shell/write_site_registry_writers.rs",
];

/// e2e 层：`package.json` 的脚本真的去跑的那批 `tests/e2e/` 下的 shell。
const E2E: &[&str] = &[
    "tests/e2e/backend-cc-bus.sh",
    "tests/e2e/backend-fork-session.sh",
    "tests/e2e/backend-gate2-acceptance.sh",
    "tests/e2e/backend-sessions-rewatch.sh",
    "tests/e2e/backend-tmux-late-server.sh",
    "tests/e2e/cc-bus-queue-drain.sh",
    "tests/e2e/cc-spawn-uplift.sh",
    "tests/e2e/ccm-cli.test.sh",
    "tests/e2e/ccm-contract-parity.sh",
    "tests/e2e/ccm-print-parity.sh",
    "tests/e2e/exec-bit-guard.sh",
    "tests/e2e/f40-suite.sh",
    "tests/e2e/graylight-backend-frames.sh",
    "tests/e2e/graylight-suite.sh",
    "tests/e2e/inbound-backend-frames.sh",
    "tests/e2e/local-backend-supervise.sh",
    "tests/e2e/p3t-local-tmux.sh", // 接回执行链（`package.json` ＋ 门禁 `run_e2e`）
    "tests/e2e/restart-suite.sh",
    "tests/e2e/resume-backend-frames.sh",
    "tests/e2e/resume-suite.sh",
    "tests/e2e/tmux-target-acceptance.sh",
];

/// e2e 的辅助件：`tests/e2e/` 下其余的 shell（桩 · 台架 · 判法）。每一份都得有人引用它。
const E2E_SUPPORT: &[&str] = &[
    "tests/e2e/assert-pass-floor.sh",
    "tests/e2e/backend-wrapper.sh",
    "tests/e2e/ccm-shim.sh", // 把编出来的后端二进制以 `ccm` 之名放上 PATH ＋ 沙箱 HOME（真跑那一行 `ccm …` 的几套 source 它）
    "tests/e2e/fake-backend.sh",
    "tests/e2e/fake-claude",
    "tests/e2e/gen-idle-tmux.sh",
    "tests/e2e/launch-render-emit.sh", // 生产 Rust 渲染器给 e2e 的出口（`launch-render-driver.ts` 调它）
    "tests/e2e/local-backend-container/build-image.sh",
    "tests/e2e/local-backend-container/guard-run-netns.sh",
    "tests/e2e/local-backend-container/rig.sh",
    "tests/e2e/reap-orphan-backends.sh",
    "tests/e2e/sandbox-env.sh", // 各套最前面 source：无条件清掉继承来的 CCM_* / CLAUDE* / ANTHROPIC_* / TMUX* / CC_BUS_*
    "tests/e2e/tier2-rig.sh",
    "tests/e2e/tmux-shim.sh",
    "tests/e2e/weak-net/assert-floor.sh",
    "tests/e2e/weak-net/build-image.sh",
    "tests/e2e/weak-net/guard-run-netns.sh",
    "tests/e2e/weak-net/rig.sh",
];

/// `tests/e2e/` 下**没有任何一份脚本 / 配置 / 测试引用**的 shell（剥注释后按文件名找）——
/// **报备、不处置**（删还是接回执行链，待定）：
/// `(文件, 为什么还留着 / 现状)`。每一行必须真的仍是孤儿（有人引用了 ⇒ 死行 ⇒ 红）。
///
/// TQ1 落地时报备的两份都裁了，表清空：`p3t-local-tmux.sh` 现打 10 过 / 0 败 ⇒ 接回执行链（进了 `E2E`）；
/// `tier2-rig.sh` 是手动真机台架 ⇒ 留，挪进 [`MANUAL_RIGS`]（有了裁定就不再是「报备」）。再长出孤儿 ⇒ 照红。
const UNREFERENCED: &[(&str, &str)] = &[];

/// **手动、真机**的台架：刻意不进任何执行链（要真图形会话 / 真 dev 实例，门禁与 CI 都起不来），由人照 README 跑。
/// `(文件, 为什么不进执行链 · 谁来跑)`。与 [`UNREFERENCED`] 同一道闸：每一行必须真的仍是没人引用的辅助件
/// （有脚本 / 配置 / 测试引用它了 ⇒ 它不再是「只有人手跑」⇒ 死行 ⇒ 红）。
const MANUAL_RIGS: &[(&str, &str)] = &[(
    "tests/e2e/tier2-rig.sh",
    "tier-2 台架搭建器（`setup` / `dev` / `run` / `teardown`：沙箱 ＋ Xvfb ＋ dev 实例 ＋ 回环 ssh）；\
     它起的是**跑着的 dev app**，无头门禁与 CI 里没有那个 app（同 `e2e_gate_registry` 里 `graylight` / `f40` 两条豁免的理由）。\
     真机测试资源未拍 ⇒ 留作手动工具，跑法在 `tests/e2e/README.md`「全链套件怎么跑」",
)];

/// 集成层自检现打逮到的「缺环境就 `return`」，**报备、不处置**（改测试体不在本件写区）：
/// `(文件, 测试函数, 现状)`。每一行必须真的仍是那一形（修好了 ⇒ 死行 ⇒ 红）。
// 〔TQ1 合并时修〕落地时报备的两条（`accounts_query_tests.rs` 里 `CLAUDE_CONFIG_DIR` 有值就 `return` 的那两条）
//   已改成另起一个清掉该变量的子进程当「裸起会话」，不再读宿主环境 ⇒ 表清空。再长出这一形 ⇒ 集成层自检照红。
const SILENT_SKIPS_REPORTED: &[(&str, &str, &str)] = &[];

/// 真机层的一条的触发者。
#[derive(Debug, Clone, Copy)]
enum Trigger {
    /// `by` 那份文件里（剥注释之后）出现 `needle`，而 `needle` 是这条测试**全名**的子串
    /// （`cargo test <过滤串>` 的语义）。
    Filter {
        by: &'static str,
        needle: &'static str,
    },
    /// 没有自动触发者，靠人按理由跑。
    Manual(&'static str),
}

/// 真机层：逐条登记（`(文件, 测试名 / TS 那一处的条件, 触发者)`）。
/// 人群 = 两棵 Rust 测试树里每一条 `#[ignore]` ＋ TS 测试里每一处 `skipIf` / `runIf` / `.skip(`。
const REAL_MACHINE: &[(&str, &str, Trigger)] = &[
    (
        "tests/backend/control/tmux_hook_tests.rs",
        "hx2_real_tmux_reading_on_a_private_socket",
        Trigger::Manual("读数不是判据：真 tmux 私有 socket（`-L`）上装一趟 hook 看段内格位；跑法住它自己的头注"),
    ),
    (
        "tests/backend/control/overwrite_atomic_tests.rs",
        "w1_child",
        Trigger::Filter { by: "tests/backend/control/overwrite_atomic_tests.rs", needle: "control::files_write::overwrite_atomic_tests::w1_child" },
    ),
    (
        "tests/backend/control/files_write_tests.rs",
        "own_home_child_entry_point",
        Trigger::Filter { by: "tests/backend/control/files_write_tests.rs", needle: "control::files_write::tests::own_home_child_entry_point" },
    ),
    (
        "tests/backend/stream/caps_tests.rs",
        "caps_child",
        Trigger::Filter { by: "tests/backend/stream/caps_tests.rs", needle: "stream::inbound::caps_tests::caps_child" },
    ),
    (
        "tests/backend/stream/drain_tests.rs",
        "d2_child_harness",
        Trigger::Filter { by: "tests/backend/stream/drain_tests.rs", needle: "stream::inbound::drain_tests::d2_child_harness" },
    ),
    (
        "tests/backend/plugin_walk_fixture.rs",
        "a_plugin_started_here_never_sees_the_listen_port_or_token_inner",
        Trigger::Filter { by: "tests/backend/plugin_walk_fixture.rs", needle: "plugin_walk_fixture::tests::a_plugin_started_here_never_sees_the_listen_port_or_token_inner" },
    ),
    (
        "tests/backend/relay/host_tests.rs",
        "hosted_relay_child_entry_point",
        Trigger::Filter { by: "tests/backend/relay/host_tests.rs", needle: "relay::listen::host_tests::hosted_relay_child_entry_point" },
    ),
    (
        "tests/backend/stream/tap_tests.rs",
        "tap_capacity_reading_with_a_dozen_concurrent_streams",
        Trigger::Manual("读数不是判据：十几路并发时 tap 通道的峰值占用与丢件（写者是模型）；跑法住它自己的头注"),
    ),
    (
        "tests/backend/control/launch_render/history_resume_e2e_tests.rs",
        "history_resume_child_entry_point",
        Trigger::Filter { by: "tests/backend/control/launch_render/history_resume_e2e_tests.rs", needle: "control::launch_render::history_resume_e2e::history_resume_child_entry_point" },
    ),
    (
        "tests/backend/control/ccm/launch_note_e2e_tests.rs",
        "launch_note_child_entry_point",
        Trigger::Filter { by: "tests/backend/control/ccm/launch_note_e2e_tests.rs", needle: "control::ccm::launch_note_e2e::launch_note_child_entry_point" },
    ),
    (
        "tests/backend/stderr_log_tests.rs",
        "stderr_log_child_entry_point",
        Trigger::Filter { by: "tests/backend/stderr_log_tests.rs", needle: "stderr_log::tests::stderr_log_child_entry_point" },
    ),
    (
        "tests/backend/relay/server_tests.rs",
        "relay_child_process_entry_point",
        Trigger::Filter { by: "tests/backend/relay/server_tests.rs", needle: "relay::server_tests::relay_child_process_entry_point" },
    ),
    (
        "tests/frontend/shell/local_backend_tests.rs",
        "e2e_a_binary_that_always_dies_is_given_up_on_within_the_cap",
        Trigger::Filter { by: "tests/e2e/local-backend-supervise.sh", needle: "local_backend" },
    ),
    (
        "tests/frontend/shell/local_backend_tests.rs",
        "e2e_a_missing_local_backend_degrades_honestly_against_the_real_filesystem",
        Trigger::Filter { by: "tests/e2e/local-backend-supervise.sh", needle: "local_backend" },
    ),
    (
        "tests/frontend/shell/local_backend_tests.rs",
        "e2e_the_supervisor_restarts_a_real_backend_after_it_is_killed",
        Trigger::Filter { by: "tests/e2e/local-backend-supervise.sh", needle: "local_backend" },
    ),
    (
        "tests/frontend/shell/local_backend_tests.rs",
        "the_local_backend_host_really_registers_an_inbound_client",
        Trigger::Filter { by: "tests/e2e/local-backend-supervise.sh", needle: "local_backend" },
    ),
    // `local_backend_tests.rs` 那条真 tmux 实测（本机 tmux 帧进 monitor 账本）随那本账删了（真机层 28 → 27）。
    (
        "tests/frontend/shell/local_lines_tests.rs",
        "a_real_backend_feeds_local_lines_through_the_production_read_loop",
        Trigger::Filter { by: "tests/evidence/CF1-local-lines.py", needle: "a_real_backend_feeds_local_lines_through_the_production_read_loop" },
    ),
    (
        "tests/frontend/shell/dial_host_tests.rs",
        "loopback_roundtrip_through_the_resident_backend",
        Trigger::Filter { by: "tests/evidence/SR1a-link-loopback.py", needle: "loopback_roundtrip_through_the_resident_backend" },
    ),
    (
        "tests/frontend/filewin/bigfile_tests.rs",
        "the_readings_behind_the_two_thresholds",
        Trigger::Manual("读数不是判据：大文件模式两个门槛的来源，只在 release 档上有意义；跑法住它自己的头注"),
    ),
    (
        "tests/frontend/filewin/workspace_tests.rs",
        "screenshot_for_the_shots_tool",
        Trigger::Filter { by: "tests/shots/filewin.mjs", needle: "workspace::tests::screenshot_for_the_shots_tool" },
    ),
    (
        "tests/frontend/filewin/rows_tests.rs",
        "xvfb_worker_real_pointer_events_on_a_row",
        Trigger::Filter { by: "tests/frontend/filewin/rows_tests.rs", needle: "rows::tests::xvfb_worker_real_pointer_events_on_a_row" },
    ),
    (
        "tests/frontend/filewin/shell_keys_tests.rs",
        "xvfb_worker_real_keys_on_the_window",
        Trigger::Filter { by: "tests/frontend/filewin/shell_keys_tests.rs", needle: "shell::keys_tests::xvfb_worker_real_keys_on_the_window" },
    ),
    (
        "tests/frontend/filewin/shell_tests.rs",
        "xvfb_worker_opens_a_real_window",
        Trigger::Filter { by: "tests/frontend/filewin/shell_tests.rs", needle: "shell::tests::xvfb_worker_opens_a_real_window" },
    ),
    (
        "tests/frontend/filewin/shell_tests.rs",
        "xvfb_worker_opens_with_no_x_server_at_all",
        Trigger::Filter { by: "tests/frontend/filewin/shell_tests.rs", needle: "shell::tests::xvfb_worker_opens_with_no_x_server_at_all" },
    ),
    (
        // 生产命令 `launch-render-cli` 给 e2e 的数据出口（`resume-suite` · `resume-backend-frames` ·
        //   `tmux-target-acceptance` 几套经这个驱动取「app 真正会交给终端的那一行 `ccm …`」）。
        "tests/backend/control/launch_render/launch_cli_parity_tests.rs",
        "emit_launch_render_for_e2e",
        Trigger::Filter { by: "tests/e2e/launch-render-emit.sh", needle: "emit_launch_render_for_e2e" },
    ),
    (
        "tests/backend/control/launch_render/local_tests.rs",
        "emit_local_launch_command_for_e2e",
        Trigger::Filter { by: "tests/e2e/p3t-local-tmux.sh", needle: "emit_local_launch_command_for_e2e" },
    ),
    (
        "tests/frontend/shell/local_backend_host_tests.rs",
        "e2e_a_detached_backend_that_dies_leaves_no_zombie",
        Trigger::Filter { by: "tests/e2e/local-backend-supervise.sh", needle: "local_backend_host" },
    ),
    (
        "tests/frontend/shell/local_backend_host_tests.rs",
        "e2e_a_second_host_adopts_the_running_backend_instead_of_starting_a_second_one",
        Trigger::Filter { by: "tests/e2e/local-backend-supervise.sh", needle: "local_backend_host" },
    ),
    (
        "tests/frontend/shell/local_backend_host_tests.rs",
        "e2e_the_door_follows_the_home_not_the_claude_dir",
        Trigger::Filter { by: "tests/e2e/local-backend-supervise.sh", needle: "local_backend_host" },
    ),
    (
        "tests/frontend/shell/local_backend_host_tests.rs",
        "e2e_children_of_the_resident_backend_carry_none_of_its_own_env",
        Trigger::Filter { by: "tests/e2e/local-backend-supervise.sh", needle: "local_backend_host" },
    ),
    (
        "tests/frontend/shell/local_backend_host_tests.rs",
        "the_local_backend_host_can_be_stopped_and_started_again",
        Trigger::Filter { by: "tests/e2e/local-backend-supervise.sh", needle: "local_backend_host" },
    ),
    // 住址随记录解析搬进后端（原 `tests/frontend/shell/parser_tests.rs`）。
    (
        "tests/backend/agents/claudecode/parse_tests.rs",
        "f63_real_data_ledger",
        Trigger::Manual("要本机真实历史数据（只读），改 F63 解析时人工重算的台账；跑法住它自己的头注"),
    ),
    (
        "tests/frontend/ui/render.vitest.ts",
        "!process.env.W2_COST",
        Trigger::Manual("读数不是判据：`W2_COST=1` 时才量慢路与快路的墙钟（墙钟不当判据）"),
    ),
    (
        "tests/frontend/shell/sftp_tests.rs",
        "sr1b_loopback_deploy_and_transfer_through_the_resident_backend",
        Trigger::Filter { by: "tests/evidence/SR1b-sftp-loopback.py", needle: "sr1b_loopback_deploy_and_transfer_through_the_resident_backend" },
    ),
    (
        "tests/backend/dial_compress_tests.rs",
        "zr_real_sshd_negotiates_zlib_and_moves_fewer_bytes_when_forced",
        Trigger::Filter { by: "tests/evidence/NT1-net-loopback.py", needle: "zr_real_sshd_negotiates_zlib_and_moves_fewer_bytes_when_forced" },
    ),
    // 一次性容器 sshd 上强制压、放不可压的大块（那份驱动一趟跑 ZR 与 ZR2 两条）。
    (
        "tests/backend/dial_compress_tests.rs",
        "zr_real_sshd_takes_incompressible_puts_when_forced",
        Trigger::Filter { by: "tests/evidence/WF2-zlib-container.py", needle: "zr_real_sshd" },
    ),
    (
        "tests/backend/observe/search_query_reading.rs",
        "sx1_real_history_search_reading",
        Trigger::Manual("读数不是判据：真规模本机历史只量冷首趟 / 热态耗时与条数；跑法住它自己的头注"),
    ),
    (
        "tests/frontend/shell/profile_installer_tests.rs",
        "wf1_real_powershell_add_then_remove_restores_the_user_path",
        Trigger::Manual("读数不是判据：要一个 PowerShell（`CCM_PWSH`）跑生成的加 / 撤两段（注册表换替身）；跑法住它自己的头注"),
    ),
    (
        "tests/frontend/shell/ccm_probe_tests.rs",
        "wf1_the_windows_probe_script_reports_card_and_where_ccm_resolves",
        Trigger::Manual("读数不是判据：要一个 PowerShell（`CCM_PWSH`）跑 Windows 那一形的探测串三种情形；跑法住它自己的头注"),
    ),
    (
        "tests/frontend/shell/profile_installer_tests.rs",
        "p2_the_path_probe_reads_back_a_non_ascii_home_under_an_oem_console",
        Trigger::Manual("读数不是判据：要一个 PowerShell（`CCM_PWSH`）在 936 控制台编码替身下跑 PATH 探针、读回汉字目录；跑法住它自己的头注"),
    ),
    (
        "tests/frontend/shell/platform/console_text_tests.rs",
        "p2_powershell_under_a_936_console_writes_the_sample_bytes_to_stderr",
        Trigger::Manual("读数不是判据：要一个 PowerShell（`CCM_PWSH`）核 936 替身字节就是它往 stderr 写的那一段；跑法住它自己的头注"),
    ),
    (
        "tests/backend/assets/aliases/block_tests.rs",
        "claim_the_rendered_block_registers_in_the_background_without_a_word",
        Trigger::Manual("读数不是判据：要一个 PowerShell（`CCM_PWSH`）解析渲染出来的接入块、跑后台登记的五种情形（标题与 monitor 换替身）；跑法住它自己的头注"),
    ),
];

/// 判别器判错的那几份：`(文件, 应归的层, 理由)`。
const OVERRIDES: &[(&str, Tier, &str)] = &[];

// ───────────────────────────── 人群 ─────────────────────────────

/// 本文件（仓根相对）。引用语料走 `scan_tree_excluding` 的明写名单摘掉它
/// （`file!()` 那条路在 `#[path]` 挂载下不生效 —— 纪律 4）。
const SELF: &str = "tests/common/guard-core/test_tiers_tests.rs";

/// 仓根。`guard-core` 零依赖、够不着 monitor 的 `guard_support` ⇒ 这里就地算一次。
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("guard-core 的上四级 = 仓根")
}

fn rel_of(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// R：两棵 Rust 测试树的全部 `.rs`（仓根相对路径 → 原文）。
fn rust_test_files(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for sub in [
        "tests/frontend/shell",
        "tests/frontend/filewin", // 文件窗口独立成包
        "tests/backend",
        "tests/common",
        "tests/comms",
    ] {
        for (p, disk_text) in crate::scan_tree_excluding(&root.join(sub), &["rs"], &[]) {
            out.insert(rel_of(root, &p), disk_text);
        }
    }
    out
}

/// T：`tests/` 下的 `.vitest.ts` 与 `.test.ts`（按后缀认 —— 跑者认的也是后缀：vitest 的 include
/// 是 `tests/**/*.vitest.ts`，连 `tests/evidence/` 里的也会跑，所以这里**不**排除任何一棵）。
fn ts_test_files(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (p, disk_text) in crate::scan_tree_excluding(&root.join("tests"), &["ts"], &[]) {
        let rel = rel_of(root, &p);
        if is_ts_test_name(&rel) {
            out.insert(rel, disk_text);
        }
    }
    out
}

fn under(rel: &str, prefix: &str) -> bool {
    rel.len() > prefix.len() && rel.starts_with(prefix)
}

fn is_ts_test_name(rel: &str) -> bool {
    const VITEST: &str = ".vitest.ts";
    const TSX: &str = ".test.ts";
    rel.ends_with(VITEST) || rel.ends_with(TSX)
}

// ───────────────────────────── 词法：剥注释 ＋ 抹字符串 ─────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Lang {
    Rust,
    Ts,
}

fn lang_of(rel: &str) -> Lang {
    if rel.ends_with(".rs") {
        Lang::Rust
    } else {
        Lang::Ts
    }
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// 剥注释、把字符串字面量的**内容**抹成空格（引号留着），**逐字符等长**、换行原样保留
/// ⇒ 输出与原文逐行、逐字符位置一一对应（抽取器靠这一点回原文取字面量）。
///
/// 认得：`//` · `/* */`（Rust 可嵌套）· `"…"`（带转义）· Rust 的 `r#"…"#` / `br"…"` 与字符字面量
/// （`'a` 这种生命周期**不是**字符字面量）· TS 的 `'…'` 与 `` `…` ``（模板里的 `${}` 一并抹掉）。
/// 不认：TS 的正则字面量（当代码留着）。
fn code_only(text: &str, lang: Lang) -> String {
    let b: Vec<char> = text.chars().collect();
    let n = b.len();
    let mut out = String::with_capacity(text.len());
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    let mut i = 0usize;
    while i < n {
        let c = b[i];
        let next = if i + 1 < n { b[i + 1] } else { '\0' };
        if c == '/' && next == '/' {
            while i < n && b[i] != '\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if c == '/' && next == '*' {
            let mut depth = 0usize;
            while i < n {
                if b[i] == '/' && i + 1 < n && b[i + 1] == '*' && (lang == Lang::Rust || depth == 0)
                {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                    continue;
                }
                if b[i] == '*' && i + 1 < n && b[i + 1] == '/' {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                out.push(blank(b[i]));
                i += 1;
            }
            continue;
        }
        // Rust 原始串：`r"…"` / `r#"…"#` / `br#"…"#`。`r#ident`（原始标识符）不是串。
        if lang == Lang::Rust && c == 'r' {
            let prev = if i > 0 { b[i - 1] } else { ' ' };
            let prev2 = if i > 1 { b[i - 2] } else { ' ' };
            let starts = !is_ident(prev) || (prev == 'b' && !is_ident(prev2));
            if starts {
                let mut j = i + 1;
                while j < n && b[j] == '#' {
                    j += 1;
                }
                let hashes = j - (i + 1);
                if j < n && b[j] == '"' {
                    for k in i..=j {
                        out.push(b[k]);
                    }
                    let mut k = j + 1;
                    loop {
                        if k >= n {
                            break;
                        }
                        if b[k] == '"' && (k + 1..k + 1 + hashes).all(|h| h < n && b[h] == '#') {
                            break;
                        }
                        out.push(blank(b[k]));
                        k += 1;
                    }
                    for h in k..(k + 1 + hashes).min(n) {
                        out.push(b[h]);
                    }
                    i = (k + 1 + hashes).min(n);
                    continue;
                }
            }
        }
        // TS 的正则字面量：`/` 前面（跳过空白）是运算符 / 开括号 / 行首 / `return` ⇒ 它开一条正则，
        // 一直吃到同一行里不在 `[…]` 里、没被转义的那个 `/`。不认的话，正则里的一个引号或反引号
        // 会被当成串的开头，把后面整份文件吃掉（`/`|⇒/` 这一形现打就有）。
        if lang == Lang::Ts && c == '/' && ts_regex_can_start(&out) {
            let mut k = i + 1;
            let mut in_class = false;
            let mut closed = None;
            while k < n && b[k] != '\n' {
                match b[k] {
                    '\\' => {
                        k += 2;
                        continue;
                    }
                    '[' => in_class = true,
                    ']' => in_class = false,
                    '/' if !in_class => {
                        closed = Some(k);
                        break;
                    }
                    _ => {}
                }
                k += 1;
            }
            if let Some(end) = closed {
                out.push('/');
                for _ in i + 1..end {
                    out.push(' ');
                }
                out.push('/');
                i = end + 1;
                continue;
            }
        }
        let quote = match (lang, c) {
            (_, '"') => Some('"'),
            (Lang::Ts, '\'') => Some('\''),
            (Lang::Ts, '`') => Some('`'),
            _ => None,
        };
        if let Some(q) = quote {
            out.push(c);
            let mut k = i + 1;
            while k < n && b[k] != q {
                if b[k] == '\\' && k + 1 < n {
                    out.push(' ');
                    out.push(blank(b[k + 1]));
                    k += 2;
                    continue;
                }
                out.push(blank(b[k]));
                k += 1;
            }
            if k < n {
                out.push(q);
            }
            i = k + 1;
            continue;
        }
        if lang == Lang::Rust && c == '\'' {
            // 字符字面量：`'\n'` / `'x'`；否则是生命周期，当代码。
            let close = if next == '\\' {
                (i + 2..n.min(i + 12)).find(|&k| b[k] == '\'')
            } else if i + 2 < n && b[i + 2] == '\'' {
                Some(i + 2)
            } else {
                None
            };
            if let Some(end) = close {
                out.push('\'');
                for k in i + 1..end {
                    out.push(blank(b[k]));
                }
                out.push('\'');
                i = end + 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// TS：此处的 `/` 能不能开一条正则（看已输出部分最后一个非空白字符 / 最后一个词）。
fn ts_regex_can_start(out: &str) -> bool {
    let t = out.trim_end_matches([' ', '\t']);
    match t.chars().next_back() {
        None | Some('\n') => true,
        Some(c) if "(,=:[!&|?{};+-*%<>~^".contains(c) => true,
        Some(c) if is_ident(c) => {
            let mut w: Vec<char> = t.chars().rev().take_while(|c| is_ident(*c)).collect();
            w.reverse();
            let word: String = w.into_iter().collect();
            matches!(word.as_str(), "return" | "typeof" | "case" | "do" | "else")
        }
        _ => false,
    }
}

/// 原文第 `line` 行、从第 `col` 个字符（一个 `"` / `'`）起的那个字面量的内容。
fn literal_at(raw_line: &str, col: usize) -> Option<String> {
    let chars: Vec<char> = raw_line.chars().collect();
    let q = *chars.get(col)?;
    let mut out = String::new();
    let mut k = col + 1;
    while k < chars.len() && chars[k] != q {
        if chars[k] == '\\' && k + 1 < chars.len() {
            out.push(chars[k + 1]);
            k += 2;
            continue;
        }
        out.push(chars[k]);
        k += 1;
    }
    (k < chars.len()).then_some(out)
}

// ───────────────────────────── 判别器 ─────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
enum Tier {
    Support,
    Unit,
    Scan,
    Integration,
}

/// Rust：「碰 OS」的信号。针运行时拼（本文件自己不许成为一处真调用的样子）。
fn rust_os_needles() -> Vec<String> {
    [
        ("Command", "::new("),
        ("process", "::Command"),
        ("TcpList", "ener"),
        ("TcpStr", "eam"),
        ("UnixList", "ener"),
        ("UnixStr", "eam"),
        ("UdpSoc", "ket"),
        ("tokio", "::net"),
        ("fs::wr", "ite("),
        ("File::cr", "eate("),
        ("OpenOp", "tions"),
        ("create_dir", "_all("),
        ("create_", "dir("),
        ("remove_", "file("),
        ("remove_dir", "_all("),
        ("set_perm", "issions("),
        ("temp", "dir("),
        ("temp_", "dir("),
        ("Temp", "Dir"),
    ]
    .iter()
    .map(|(a, b)| format!("{a}{b}"))
    .collect()
}

/// Rust：「读仓内文本」的信号。
fn rust_repo_text_needles() -> Vec<String> {
    [
        ("include", "_str!"),
        ("include", "_bytes!"),
        ("repo_", "root("),
        ("src_", "root("),
        ("tests_", "root("),
        ("crate_", "root("),
        ("crate_src_", "root("),
        ("backend_src_", "root("),
        ("repo_src_", "root("),
        ("CARGO_MANIFEST", "_DIR"),
        ("scan_", "tree!"),
        ("scan_tree", "_excluding("),
        ("scan_tree_excluding", "_self("),
        ("files_by_", "extension("),
        ("shell_", "scripts("),
        ("read_to_", "string("),
        ("read_", "dir("),
        ("fs::", "read("),
        // 壳 `guard_support` 读会话流来源那个目录的几个口（目录拆开之后，读那份源码的判据经它们读）。
        ("stream_source_", "files("),
        ("stream_source_", "file("),
        ("stream_source_", "production("),
        ("stream_source_", "raw("),
    ]
    .iter()
    .map(|(a, b)| format!("{a}{b}"))
    .collect()
}

fn ts_os_needles() -> Vec<String> {
    [
        ("spawn", "Sync("),
        ("execFile", "Sync("),
        ("exec", "Sync("),
        ("spa", "wn("),
        ("exec", "File("),
        ("writeFile", "Sync("),
        ("mkdtemp", "Sync("),
        ("mkdir", "Sync("),
        ("rm", "Sync("),
        ("create", "Server("),
    ]
    .iter()
    .map(|(a, b)| format!("{a}{b}"))
    .collect()
}

fn ts_repo_text_needles() -> Vec<String> {
    [
        ("readFile", "Sync("),
        ("readdir", "Sync("),
        ("exists", "Sync("),
        ("stat", "Sync("),
        ("import.meta", ".glob("),
        ("srcDir", "Of("),
        ("read", "File("),
    ]
    .iter()
    .map(|(a, b)| format!("{a}{b}"))
    .collect()
}

/// TS 的 import 说明符（`from "x"` / `import "x"` / `import("x")` / `require("x")`），从原文取。
fn ts_specifiers(raw: &str, code: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (raw_line, code_line) in raw.lines().zip(code.lines()) {
        let chars: Vec<char> = code_line.chars().collect();
        for k in 0..chars.len() {
            let q = chars[k];
            if q != '"' && q != '\'' {
                continue;
            }
            let head: String = chars[..k].iter().collect();
            let head = head.trim_end();
            let opens = head.ends_with("from")
                || head.ends_with("import")
                || head.ends_with("import(")
                || head.ends_with("require(");
            if !opens {
                continue;
            }
            if let Some(lit) = literal_at(raw_line, k) {
                out.push(lit);
            }
        }
    }
    out
}

fn has_any_word(code: &str, needles: &[String]) -> bool {
    needles.iter().any(|n| crate::contains_word(code, n))
}

fn rust_has_test(code: &str) -> bool {
    let attr = concat!("#[te", "st]");
    let tokio = concat!("#[tokio::", "test");
    code.lines().any(|l| {
        let t = l.trim();
        t == attr || t.starts_with(tokio)
    })
}

fn ts_has_test(code: &str) -> bool {
    const HEADS: &[&str] = &["it(", "test(", "it.", "test.", "describe("];
    code.lines().any(|l| {
        let t = l.trim_start();
        HEADS.iter().any(|h| t.starts_with(h))
    })
}

/// 一份测试文件按信号判出来的层。
fn detect(rel: &str, raw: &str) -> Tier {
    let lang = lang_of(rel);
    let code = code_only(raw, lang);
    match lang {
        Lang::Rust => {
            if !rust_has_test(&code) {
                Tier::Support
            } else if has_any_word(&code, &rust_os_needles()) {
                Tier::Integration
            } else if has_any_word(&code, &rust_repo_text_needles()) {
                Tier::Scan
            } else {
                Tier::Unit
            }
        }
        Lang::Ts => {
            let specs = ts_specifiers(raw, &code);
            let spec_is = |pred: &dyn Fn(&str) -> bool| specs.iter().any(|s| pred(s));
            let os_spec = spec_is(&|s| {
                matches!(
                    s,
                    "child_process"
                        | "node:child_process"
                        | "node:net"
                        | "node:http"
                        | "node:https"
                )
            });
            let text_spec = spec_is(&|s| {
                s.ends_with("?raw")
                    || s.ends_with("test-support/repo-root.ts")
                    || s.ends_with("test-support/production-sources.ts")
                    || s.ends_with("test-support/component-sources.ts")
            });
            if !ts_has_test(&code) {
                Tier::Support
            } else if os_spec || has_any_word(&code, &ts_os_needles()) {
                Tier::Integration
            } else if text_spec || has_any_word(&code, &ts_repo_text_needles()) {
                Tier::Scan
            } else {
                Tier::Unit
            }
        }
    }
}

fn registered_tier(rel: &str) -> Option<Tier> {
    let mut hits = Vec::new();
    for (tier, table) in [
        (Tier::Unit, UNIT),
        (Tier::Scan, SCAN),
        (Tier::Integration, INTEGRATION),
        (Tier::Support, SUPPORT),
    ] {
        if table.contains(&rel) {
            hits.push(tier);
        }
    }
    (hits.len() == 1).then(|| hits[0])
}

fn tier_table(t: Tier) -> &'static [&'static str] {
    match t {
        Tier::Unit => UNIT,
        Tier::Scan => SCAN,
        Tier::Integration => INTEGRATION,
        Tier::Support => SUPPORT,
    }
}

fn table_name(t: Tier) -> &'static str {
    match t {
        Tier::Unit => "UNIT",
        Tier::Scan => "SCAN",
        Tier::Integration => "INTEGRATION",
        Tier::Support => "SUPPORT",
    }
}

/// 盘上每一份的层（判别器 ＋ 例外表）。
fn tiers_on_disk(root: &Path) -> BTreeMap<String, Tier> {
    let mut out = BTreeMap::new();
    for (rel, disk_text) in rust_test_files(root).into_iter().chain(ts_test_files(root)) {
        let t = detect(&rel, &disk_text);
        let t = OVERRIDES
            .iter()
            .find(|(f, _, _)| *f == rel)
            .map_or(t, |(_, o, _)| *o);
        out.insert(rel, t);
    }
    out
}

// ───────────────────────────── 挂载（`#[path]`） ─────────────────────────────

/// 一处 `#[path = "…"] mod <名>;`：被挂的文件（仓根相对）← (声明它的文件, 模块名)。
fn mounts(root: &Path) -> BTreeMap<String, Vec<(String, String)>> {
    let mut out: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for sub in ["src", "tests"] {
        for (p, disk_text) in crate::scan_tree_excluding(&root.join(sub), &["rs"], &[]) {
            let decl = rel_of(root, &p);
            let dir = p.parent().expect("文件有父目录").to_path_buf();
            for (target, name) in path_mounts(&disk_text) {
                let abs = normalize(&dir.join(&target));
                out.entry(rel_of(root, &abs))
                    .or_default()
                    .push((decl.clone(), name));
            }
        }
    }
    out
}

/// 一份 Rust 源码里的 `#[path = "…"]` 与紧随其后的 `mod <名>`（跳过中间的属性行）。
fn path_mounts(raw: &str) -> Vec<(String, String)> {
    let code = code_only(raw, Lang::Rust);
    let raw_lines: Vec<&str> = raw.lines().collect();
    let code_lines: Vec<&str> = code.lines().collect();
    let head = concat!("#[pa", "th");
    let mut out = Vec::new();
    for (i, cl) in code_lines.iter().enumerate() {
        let t = cl.trim_start();
        if !t.starts_with(head) {
            continue;
        }
        let Some(q) = cl.chars().position(|c| c == '"') else {
            continue;
        };
        let Some(target) = literal_at(raw_lines[i], q) else {
            continue;
        };
        let name = code_lines[i + 1..]
            .iter()
            .take(4)
            .map(|l| l.trim())
            .find(|l| !l.starts_with("#["))
            .and_then(mod_decl_name);
        if let Some(name) = name {
            out.push((target, name));
        }
    }
    out
}

/// `mod x;` / `pub(crate) mod x;` / `pub mod x {` ⇒ `x`。
fn mod_decl_name(line: &str) -> Option<String> {
    let t = crate::strip_visibility(line.trim());
    let rest = t.strip_prefix("mod ")?;
    let name: String = rest.chars().take_while(|c| is_ident(*c)).collect();
    (!name.is_empty()).then_some(name)
}

/// 词法规范化（`a/b/../c` ⇒ `a/c`），不碰文件系统。
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// 一份文件的模块路径（crate 内，`::` 连接；crate 根为空串）。
///
/// `src/` 下按目录推（`lib.rs` / `main.rs` / `mod.rs` 折叠）；`tests/` 下的文件 = 挂它的那份的模块路径 ＋ 模块名
/// （挂了不止一处 ⇒ `None`，调用方另报）。
fn module_path(
    rel: &str,
    mounts: &BTreeMap<String, Vec<(String, String)>>,
    depth: usize,
) -> Option<String> {
    if depth > 8 {
        return None;
    }
    // 文件窗口独立成包：它那棵 `src/` 也按目录推模块路径。
    const ROOTS: &[&str] = &[
        "src/frontend/shell/src/",
        "src/frontend/filewin/src/",
        "src/backend/",
    ];
    for r in ROOTS {
        if under(rel, r) {
            let inner = rel[r.len()..].trim_end_matches(".rs");
            let mut segs: Vec<&str> = inner.split('/').collect();
            if matches!(segs.last(), Some(&"lib") | Some(&"main") | Some(&"mod")) {
                segs.pop();
            }
            return Some(segs.join("::"));
        }
    }
    // 住在「被 `#[path]` 挂进来的 `mod.rs`」那一层目录里、由它隐式声明的子模块（`x/y.rs` ← `x/mod.rs` 的 `mod y;`，
    //   而那份 `mod.rs` 由别处挂进来）。
    if !mounts.contains_key(rel) {
        let (dir, file) = rel.rsplit_once('/')?;
        let mod_rs = format!("{dir}/mod.rs");
        if file != "mod.rs" && mounts.contains_key(&mod_rs) {
            let parent = module_path(&mod_rs, mounts, depth + 1)?;
            let stem = file.trim_end_matches(".rs");
            return Some(if parent.is_empty() {
                stem.to_string()
            } else {
                format!("{parent}::{stem}")
            });
        }
    }
    let decls = mounts.get(rel)?;
    if decls.len() != 1 {
        return None;
    }
    let (decl, name) = &decls[0];
    let parent = module_path(decl, mounts, depth + 1)?;
    Some(if parent.is_empty() {
        name.clone()
    } else {
        format!("{parent}::{name}")
    })
}

// ───────────────────────────── 分区 ─────────────────────────────

fn set_diff(a: &BTreeSet<String>, b: &BTreeSet<String>) -> Vec<String> {
    a.difference(b).cloned().collect()
}

/// P：分区。五张文件表 ＋ 支撑两两不相交；单元∪扫描∪集成∪支撑 == R∪T（两向）；每份的层 == 判别器。
#[test]
fn the_tiers_partition_the_test_files_on_disk() {
    let root = repo();
    let disk = tiers_on_disk(&root);
    let mut problems: Vec<String> = Vec::new();

    // ① 两两不相交（同一份登记进两张表）。
    let mut seen: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for t in [Tier::Unit, Tier::Scan, Tier::Integration, Tier::Support] {
        for f in tier_table(t) {
            seen.entry(f).or_default().push(table_name(t));
        }
    }
    for f in E2E.iter().chain(E2E_SUPPORT) {
        seen.entry(f).or_default().push("E2E*");
    }
    for (f, ts) in &seen {
        if ts.len() > 1 {
            problems.push(format!("  `{f}` 同时登记在 {ts:?}"));
        }
    }

    // ② 两向相等：登记 == 盘上。
    let registered: BTreeSet<String> = [UNIT, SCAN, INTEGRATION, SUPPORT]
        .iter()
        .flat_map(|t| t.iter().map(|s| s.to_string()))
        .collect();
    let on_disk: BTreeSet<String> = disk.keys().cloned().collect();
    for f in set_diff(&on_disk, &registered) {
        problems.push(format!(
            "  盘上有、没登记：`{f}` —— 判别器判它是 {}，照这个进 `{}`：\n        \"{f}\",",
            table_name(disk[&f]),
            table_name(disk[&f])
        ));
    }
    for f in set_diff(&registered, &on_disk) {
        problems.push(format!(
            "  登记了、盘上没有：`{f}` —— 搬走了 / 删了 / 改名了，删掉这一行"
        ));
    }

    // ③ 每份的层 == 判别器（带例外表）。
    for (f, t) in &disk {
        if let Some(r) = registered_tier(f) {
            if r != *t {
                problems.push(format!(
                    "  `{f}` 登记在 {}，判别器判它是 {} —— 要么挪表，要么它真判错了、进 `OVERRIDES` 写理由",
                    table_name(r),
                    table_name(*t)
                ));
            }
        }
    }
    // ④ 例外表无死行：每一行必须真的与判别器不一致。
    let rust = rust_test_files(&root);
    let ts = ts_test_files(&root);
    for (f, o, why) in OVERRIDES {
        let Some(raw) = rust.get(*f).or_else(|| ts.get(*f)) else {
            problems.push(format!(
                "  `OVERRIDES` 里的 `{f}` 盘上没有（理由原是：{why}）"
            ));
            continue;
        };
        if detect(f, raw) == *o {
            problems.push(format!(
                "  `OVERRIDES` 里的 `{f}` 判别器现在自己就判成 {} 了 —— 死行，删掉（理由原是：{why}）",
                table_name(*o)
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "测试层分区对不上（{} 处）：\n{}\n\n\
         本表的人群是 `tests/frontend/shell/**/*.rs` ∪ `tests/frontend/filewin/**/*.rs` ∪ `tests/backend/**/*.rs` ∪ `tests/common/**/*.rs` ∪ `tests/comms/**/*.rs` ∪ `tests/**/*.{{vitest,test}}.ts`。\n\
         ⇒ 新加一份测试文件就要在这里登记它的层；那是这张表存在的理由（人群从登记表来，\
         盘上条数 == 登记条数，不是「扫到几个算几个」）。",
        problems.len(),
        problems.join("\n")
    );
}

// ───────────────────────────── T1 单元层：可达 ─────────────────────────────

/// 一张表里每一份都被它的跑者够得着：Rust 恰好一处 `#[path]` 挂它；`.vitest.ts` 落在 vitest 的 include 里；
/// `.test.ts` 被 `npm test` 链上某个 `test:*` 脚本点名。返回违例。
fn reach_violations(root: &Path, files: &[&str]) -> Vec<String> {
    let m = mounts(root);
    let pkg = std::fs::read_to_string(root.join("package.json")).expect("读 package.json");
    let chain = npm_test_chain(&pkg);
    let vitest_cfg =
        std::fs::read_to_string(root.join("vitest.config.ts")).expect("读 vitest.config.ts");
    let include_pinned =
        crate::pin_line(&vitest_cfg, r#"include: ["tests/**/*.vitest.ts"],"#).is_ok();
    let vitest_in_chain = chain.iter().any(|(_, v)| v == "vitest run");
    let mut bad = Vec::new();
    for f in files {
        match lang_of(f) {
            Lang::Rust => {
                let decls = m.get(*f).map(Vec::len).unwrap_or(0);
                if decls != 1 {
                    bad.push(format!(
                        "  `{f}` 被 {decls} 处 `#[path]` 挂载（要恰好 1：0 = 零条被编译、`cargo test` 照绿；2 = 编两遍）"
                    ));
                } else {
                    let (decl, _) = &m[*f][0];
                    let decl_ok = under(decl, "src/") || m.contains_key(decl.as_str());
                    if !decl_ok {
                        bad.push(format!(
                            "  `{f}` 挂在 `{decl}` 上，而那份自己没被挂进任何编译单元"
                        ));
                    }
                }
            }
            Lang::Ts => {
                if f.ends_with(".vitest.ts") {
                    if !(include_pinned && vitest_in_chain) {
                        bad.push(format!(
                            "  `{f}`：vitest 的 include 不再是 `tests/**/*.vitest.ts` 一整行，或 `npm test` 链上没有 `vitest run` \
                             ⇒ 这一份不再被 `npm test` 跑到"
                        ));
                    }
                } else if !chain
                    .iter()
                    .any(|(_, v)| tsx_target(v).as_deref() == Some(*f))
                {
                    bad.push(format!(
                        "  `{f}` 不在 `npm test` 那条 `&&` 链上的任何一个 `tsx` 脚本里 ⇒ 没人跑它"
                    ));
                }
            }
        }
    }
    bad
}

/// `package.json` 的 `"scripts"` 里 `"名": "值"` 那种行（本仓的 `package.json` 一行一条，手写的行解析认的就是这一形）。
fn npm_scripts(manifest: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in manifest.lines() {
        let t = line.trim().trim_end_matches(',');
        let Some(body) = t.strip_prefix('"') else {
            continue;
        };
        let Some((name, rest)) = body.split_once("\": \"") else {
            continue;
        };
        let Some(val) = rest.strip_suffix('"') else {
            continue;
        };
        out.push((name.to_string(), val.replace("\\\"", "\"")));
    }
    out
}

/// `npm test` 那条链真的会跑的脚本：`"test"` 的值按 `&&` 切、每段 `npm run <名>` ⇒ 那个脚本。
fn npm_test_chain(manifest: &str) -> Vec<(String, String)> {
    let scripts = npm_scripts(manifest);
    let Some((_, test)) = scripts.iter().find(|(n, _)| n == "test") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for step in test.split("&&") {
        let Some(name) = step.trim().strip_prefix("npm run ") else {
            continue;
        };
        if let Some(hit) = scripts.iter().find(|(n, _)| n == name.trim()) {
            out.push(hit.clone());
        }
    }
    out
}

fn tsx_target(val: &str) -> Option<String> {
    let rest = val.trim().strip_prefix("tsx ")?;
    Some(rest.trim().to_string())
}

/// T1：单元层每一份都被它的跑者够得着。
#[test]
fn unit_tier_every_file_is_reached_by_its_runner() {
    let root = repo();
    let bad = reach_violations(&root, UNIT);
    assert!(
        !UNIT.is_empty() && bad.is_empty(),
        "单元层（{} 份）有够不着的：\n{}\n\n⇒ 一份测试文件在盘上、跑者够不着它 ＝ 零条被执行、门禁照绿。",
        UNIT.len(),
        bad.join("\n")
    );
}

// ───────────────────────────── T2 扫描层：语料根在盘 ─────────────────────────────

/// 一个「根」函数名 ⇒ 它在那一侧指向的仓根相对目录。按文件所在的树分两侧（两个 crate 的 `guard_support` 语义不同）。
fn root_fn_base(rel: &str, fname: &str) -> Option<&'static str> {
    // 后端 crate 挂载的那棵（`tests/comms/` 由通信层两个 crate 自己挂载，它们没有 `guard_support`）。
    let backend = under(rel, "tests/backend/");
    // 文件窗口独立成包：它的判据由窗口包挂载 ⇒ `guard_support` 是窗口包那一份（三个根）。
    if under(rel, "tests/frontend/filewin/") {
        return match fname {
            "repo_root" => Some("."),
            "crate_src_root" => Some("src/frontend/filewin/src"),
            "crate_root" => Some("src/frontend/filewin"),
            _ => None,
        };
    }
    match (backend, fname) {
        (_, "repo_root") => Some("."),
        (false, "tests_root") => Some("tests"),
        (false, "crate_src_root") => Some("src/frontend/shell/src"),
        (false, "crate_root") => Some("src/frontend/shell"),
        (false, "backend_src_root") => Some("src/backend"),
        (false, "repo_src_root") => Some("src"),
        (true, "tests_root") => Some("tests/backend"),
        (true, "src_root") => Some("src/backend"),
        // 后端那一侧的三个根：中转 crate · 它的单测 · 后端里中转的宿主。
        (true, "relay_root") => Some("src/comms/outward"),
        (true, "comms_tests_root") => Some("tests/comms/outward"),
        (true, "relay_host_root") => Some("src/backend/relay"),
        _ => None,
    }
}

/// 一份 Rust 测试文件里「以仓内根为基」的路径字面量：`(行号, 根函数, 字面量)`。
///
/// 两种形：`repo_root().join("…")` 直连；`let r = repo_root(); … r.join("…")` 绑定。
/// 本文件里**自己定义了**同名根函数的（`fn src_root(`），那个名字的语义不是共享的 ⇒ 跳过；
/// 一个变量名在本文件里还被绑到别的东西上 ⇒ 歧义 ⇒ 跳过（宁可少核，不许误红）。
fn root_anchored_literals(rel: &str, raw: &str) -> Vec<(usize, String, String)> {
    let code = code_only(raw, Lang::Rust);
    let raw_lines: Vec<&str> = raw.lines().collect();
    let code_lines: Vec<&str> = code.lines().collect();
    const FNS: &[&str] = &[
        "repo_root",
        "tests_root",
        "src_root",
        "crate_root",
        "crate_src_root",
        "backend_src_root",
        "repo_src_root",
        "relay_root",
        "comms_tests_root",
        "relay_host_root",
    ];
    // 本文件自己定义的同名根函数：函数体（定义行起 5 行）里转调的正是 `guard_support::<同名>()`
    // ⇒ 语义与共享那个相同，照认（本仓 25 份文件是这一形）；否则 ⇒ 语义是本地的 ⇒ 不认。
    let local_fns: BTreeSet<&str> = FNS
        .iter()
        .copied()
        .filter(|f| {
            let def = format!("fn {f}(");
            let Some(at) = code_lines
                .iter()
                .position(|l| crate::contains_word(l, &def))
            else {
                return false;
            };
            let body = code_lines[at..(at + 5).min(code_lines.len())].join("\n");
            !crate::contains_word(&body, &format!("guard_support::{f}()"))
        })
        .collect();
    // 绑定：`let [mut] <v> [: T] = [&][path::]<fn>()`。
    let mut binds: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for cl in &code_lines {
        let t = cl.trim_start();
        let Some(rest) = t.strip_prefix("let ") else {
            continue;
        };
        let rest = rest.strip_prefix("mut ").unwrap_or(rest);
        let Some((lhs, rhs)) = rest.split_once('=') else {
            continue;
        };
        let v = lhs.split(':').next().unwrap_or("").trim().to_string();
        if v.is_empty() || !v.chars().all(is_ident) {
            continue;
        }
        let r = rhs.trim().trim_start_matches('&');
        let callee = r
            .split("()")
            .next()
            .and_then(|p| p.rsplit("::").next())
            .unwrap_or("")
            .to_string();
        let is_root_call = FNS.contains(&callee.as_str()) && r.contains(&format!("{callee}()"));
        binds
            .entry(v)
            .or_default()
            .insert(if is_root_call { callee } else { String::new() });
    }
    let good: BTreeMap<String, String> = binds
        .into_iter()
        .filter(|(_, fs)| fs.len() == 1 && !fs.contains(""))
        .map(|(v, fs)| (v, fs.into_iter().next().expect("恰好一个")))
        .filter(|(_, f)| !local_fns.contains(f.as_str()))
        .collect();
    let mut out = Vec::new();
    let mark = ".join(\"";
    for (i, cl) in code_lines.iter().enumerate() {
        let chars: Vec<char> = cl.chars().collect();
        let line: String = chars.iter().collect();
        let mut from = 0usize;
        while let Some(rel_at) = line[from..].find(mark) {
            let at_byte = from + rel_at;
            from = at_byte + mark.len();
            let before = &line[..at_byte];
            // 接收者：`<fn>()` 或 `<v>`。
            let (recv, is_call) = match before.strip_suffix("()") {
                Some(b) => (b, true),
                None => (before, false),
            };
            let name: String = recv
                .chars()
                .rev()
                .take_while(|c| is_ident(*c))
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let func = if is_call {
                (FNS.contains(&name.as_str()) && !local_fns.contains(name.as_str()))
                    .then(|| name.clone())
            } else {
                good.get(&name).cloned()
            };
            let Some(func) = func else {
                continue;
            };
            if root_fn_base(rel, &func).is_none() {
                continue;
            }
            let q_col = line[..at_byte + mark.len() - 1].chars().count();
            if let Some(lit) = literal_at(raw_lines[i], q_col) {
                out.push((i + 1, func, lit));
            }
        }
    }
    out
}

/// **按设计必须不在**的路径字面量：判据用它钉「这个文件删了、长回来就红」（`!root.join(..).is_file()`），
/// 下面那条「字面量必须解析得到」对它们反着核 —— 在表里的必须**不存在**，存在了就红；表里的行在盘上用不到也红（两向）。
/// 加一行 = 显式声明「这一处是反向量法」，不是给路径断了的扫描开后门。
const ABSENT_BY_DESIGN: &[(&str, &str)] = &[
    // LR2：TS 座 `session-backend.ts` 按删了；`doc_claim_registry_tests.rs` 两处量「它必须不在」。
    (
        "tests/frontend/shell/doc_claim_registry_tests.rs",
        "src/session-backend.ts",
    ),
];

/// T2：扫描层每一处以仓内根为基的路径字面量都指得到盘上东西（`ABSENT_BY_DESIGN` 里的反着核）。
#[test]
fn scan_tier_every_root_anchored_path_literal_resolves() {
    let root = repo();
    let rust = rust_test_files(&root);
    let mut bad = reach_violations(&root, SCAN);
    let mut checked = 0usize;
    let mut absent_seen: std::collections::BTreeSet<(&str, String)> =
        std::collections::BTreeSet::new();
    // 人群：三层里**每一份** Rust 测试文件（一份既扫源码又起进程的文件按层归集成，但它的扫描照样会扫空集）。
    for f in UNIT
        .iter()
        .chain(SCAN)
        .chain(INTEGRATION)
        .filter(|f| lang_of(f) == Lang::Rust)
    {
        let Some(raw) = rust.get(*f) else { continue };
        for (line, func, lit) in root_anchored_literals(f, raw) {
            let base = root_fn_base(f, &func).expect("上面筛过");
            checked += 1;
            if ABSENT_BY_DESIGN
                .iter()
                .any(|(af, al)| af == f && *al == lit)
            {
                absent_seen.insert((*f, lit.clone()));
                if root.join(base).join(&lit).exists() {
                    bad.push(format!(
                        "  `{f}` 第 {line} 行 `{lit}` 登记在 `ABSENT_BY_DESIGN`（按设计必须不在），可它在盘上 —— 删掉的东西长回来了"
                    ));
                }
                continue;
            }
            if !root.join(base).join(&lit).exists() {
                bad.push(format!(
                    "  `{f}` 第 {line} 行 `{func}().join(\"{lit}\")` ⇒ `{base}/{lit}` 盘上不存在 —— \
                     这条扫描此刻在扫空集"
                ));
            }
        }
    }
    for (af, al) in ABSENT_BY_DESIGN {
        if !absent_seen.contains(&(*af, (*al).to_string())) {
            bad.push(format!("  `ABSENT_BY_DESIGN` 里 `{af}` → `{al}` 这一行在盘上没有对应的字面量 —— 表在腐烂，删掉这一行"));
        }
    }
    // 抽取器自检：一处都没抽到 ⇒ 抽取器在真文件上瞎了（合成夹具那条管不到真文件的写法漂移）⇒ 下面的零命中不携带信息。
    assert!(
        checked > 0,
        "扫描层的路径抽取器在 {} 份真文件上一处都没抽到 —— 它瞎了，本条此刻在空转",
        UNIT.len() + SCAN.len() + INTEGRATION.len()
    );
    assert!(
        !SCAN.is_empty() && bad.is_empty(),
        "扫描层（{} 份；路径字面量在单元 / 扫描 / 集成三层的 Rust 文件里都核，本趟核了 {checked} 处）：\n{}\n\n\
         ⇒ 路径断了的扫描型判据不报错，它扫空集然后绿。",
        SCAN.len(),
        bad.join("\n")
    );
    println!(
        "TQ1 · 扫描层 {} 份，核了 {checked} 处以仓内根为基的路径字面量",
        SCAN.len()
    );
}

// ───────────────────────────── T3 集成层：零静默跳过 ─────────────────────────────

/// 一份 Rust 测试文件里的每一个测试函数：`(函数名, 是否 #[ignore], 函数体（code_only）)`。
fn rust_test_fns(code: &str) -> Vec<(String, bool, String)> {
    let lines: Vec<&str> = code.lines().collect();
    let attr = concat!("#[te", "st]");
    let tokio = concat!("#[tokio::", "test");
    let ignore = concat!("#[ign", "ore");
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let t = lines[i].trim();
        if !(t == attr || t.starts_with(tokio)) {
            i += 1;
            continue;
        }
        // 往上看紧挨着的属性行，往下找 `fn`。
        let mut ignored = false;
        let mut k = i;
        while k > 0 && lines[k - 1].trim().starts_with("#[") {
            k -= 1;
            ignored |= lines[k].trim().starts_with(ignore);
        }
        let mut j = i + 1;
        while j < lines.len() && !crate::contains_word(lines[j], "fn") {
            ignored |= lines[j].trim().starts_with(ignore);
            j += 1;
        }
        if j >= lines.len() {
            break;
        }
        let name = fn_name(lines[j]).unwrap_or_default();
        // 函数体：从 `fn` 那行起配平花括号（code_only 之后串与注释里的括号都没了）。
        let mut depth = 0i32;
        let mut opened = false;
        let mut body = String::new();
        let mut e = j;
        while e < lines.len() {
            for c in lines[e].chars() {
                if c == '{' {
                    depth += 1;
                    opened = true;
                } else if c == '}' {
                    depth -= 1;
                }
            }
            body.push_str(lines[e]);
            body.push('\n');
            if opened && depth <= 0 {
                break;
            }
            e += 1;
        }
        out.push((name, ignored, body));
        i = e + 1;
    }
    out
}

fn fn_name(line: &str) -> Option<String> {
    let at = line.find("fn ")?;
    let name: String = line[at + 3..]
        .chars()
        .take_while(|c| is_ident(*c))
        .collect();
    (!name.is_empty()).then_some(name)
}

/// 函数体里「读环境变量 ⇒ 三行内一个 `return`」那一形出现的行号（相对函数体）。
fn silent_skips(body: &str) -> Vec<usize> {
    let lines: Vec<&str> = body.lines().collect();
    let env_needles = [concat!("env::", "var("), concat!("env::", "var_os(")];
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if !env_needles.iter().any(|n| crate::contains_word(l, n)) {
            continue;
        }
        let window = lines[i..(i + 4).min(lines.len())].join("\n");
        if crate::contains_word(&window, "return") {
            out.push(i + 1);
        }
    }
    out
}

/// T3：集成层（以及同一个跑者跑的另两层）非 `#[ignore]` 的测试函数里，没有「缺环境就 `return`」那一形。
#[test]
fn integration_tier_never_skips_silently() {
    let root = repo();
    let rust = rust_test_files(&root);
    let mut bad = reach_violations(&root, INTEGRATION);
    let mut fns = 0usize;
    let mut reported_seen: BTreeSet<(String, String)> = BTreeSet::new();
    for f in UNIT
        .iter()
        .chain(SCAN)
        .chain(INTEGRATION)
        .filter(|f| lang_of(f) == Lang::Rust)
    {
        let Some(raw) = rust.get(*f) else { continue };
        for (name, ignored, body) in rust_test_fns(&code_only(raw, Lang::Rust)) {
            if ignored {
                continue;
            }
            fns += 1;
            let reported = SILENT_SKIPS_REPORTED
                .iter()
                .any(|(rf, rn, _)| rf == f && *rn == name);
            if reported && !silent_skips(&body).is_empty() {
                reported_seen.insert((f.to_string(), name.clone()));
                continue;
            }
            for at in silent_skips(&body) {
                bad.push(format!(
                    "  `{f}::{name}` 函数体第 {at} 行：读环境变量之后三行内有 `return` —— \
                     缺环境时这条测试没跑也算过。要么缺了就 panic（带原因），要么标 `#[ignore]` 进真机层登记触发者"
                ));
            }
        }
    }
    for (rf, rn, _) in SILENT_SKIPS_REPORTED {
        if !reported_seen.contains(&(rf.to_string(), rn.to_string())) {
            bad.push(format!(
                "  `SILENT_SKIPS_REPORTED` 里的 `{rf}::{rn}` 已经不是那一形了 —— 死行，删掉"
            ));
        }
    }
    assert!(
        !INTEGRATION.is_empty() && fns > 0 && bad.is_empty(),
        "集成层（{} 份；本趟看了 {fns} 个非 ignore 的 Rust 测试函数）：\n{}",
        INTEGRATION.len(),
        bad.join("\n")
    );
    println!(
        "TQ1 · 集成层 {} 份；静默跳过扫了 {fns} 个测试函数",
        INTEGRATION.len()
    );
}

// ───────────────────────────── T4 e2e 层 ─────────────────────────────

/// `package.json` 任一脚本的值里点名的 `tests/e2e/…` 路径。
fn e2e_named_by_package_json(manifest: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (_, v) in npm_scripts(manifest) {
        for tok in v.split_whitespace() {
            if under(tok, "tests/e2e/") {
                out.insert(tok.to_string());
            }
        }
    }
    out
}

/// T4：e2e 层 == `package.json` 真的去跑的那批（两向）；其余 shell 是辅助件，每一份都有人引用；
/// 「进门禁或登记理由」那一半由 `e2e_gate_registry` 守，这里按名字钉住它在。
#[test]
fn e2e_tier_is_what_package_json_runs_and_no_helper_is_orphaned() {
    let root = repo();
    let pkg = std::fs::read_to_string(root.join("package.json")).expect("读 package.json");
    let shells: BTreeSet<String> = crate::shell_scripts(&root.join("tests/e2e"))
        .into_iter()
        .map(|r| format!("tests/e2e/{r}"))
        .collect();
    let named = e2e_named_by_package_json(&pkg);
    let suites: BTreeSet<String> = shells.intersection(&named).cloned().collect();
    let helpers: BTreeSet<String> = shells.difference(&named).cloned().collect();
    let reg_suites: BTreeSet<String> = E2E.iter().map(|s| s.to_string()).collect();
    let reg_helpers: BTreeSet<String> = E2E_SUPPORT.iter().map(|s| s.to_string()).collect();
    let mut bad = Vec::new();
    for f in set_diff(&suites, &reg_suites) {
        bad.push(format!(
            "  `package.json` 在跑、没登记进 `E2E`：\n        \"{f}\","
        ));
    }
    for f in set_diff(&reg_suites, &suites) {
        bad.push(format!(
            "  登记在 `E2E`、而 `package.json` 不跑它（或盘上没有）：`{f}`"
        ));
    }
    for f in set_diff(&helpers, &reg_helpers) {
        bad.push(format!(
            "  `tests/e2e/` 下的辅助件没登记进 `E2E_SUPPORT`：\n        \"{f}\","
        ));
    }
    for f in set_diff(&reg_helpers, &helpers) {
        bad.push(format!(
            "  登记在 `E2E_SUPPORT`、盘上已不是辅助件（或没了）：`{f}`"
        ));
    }
    // 辅助件不许是孤儿：它的文件名得在别的某份（剥注释之后的）脚本 / 配置 / 测试里出现。
    let corpus = reference_corpus(&root);
    for h in &helpers {
        let base = h.rsplit('/').next().expect("有文件名");
        let users = corpus
            .iter()
            .filter(|(p, _)| p != h)
            .filter(|(_, text)| crate::contains_word(text, base))
            .count();
        let reported = UNREFERENCED.iter().chain(MANUAL_RIGS).any(|(f, _)| f == h);
        match (users == 0, reported) {
            (true, false) => bad.push(format!(
                "  辅助件 `{h}` 没有任何一份脚本 / 配置 / 测试引用它 —— 孤儿：接回执行链或删掉（或进 `UNREFERENCED` 报备）"
            )),
            (false, true) => bad.push(format!(
                "  `UNREFERENCED` / `MANUAL_RIGS` 里的 `{h}` 现在有 {users} 份在引用它了 —— 死行，删掉"
            )),
            _ => {}
        }
    }
    for (f, why) in UNREFERENCED.iter().chain(MANUAL_RIGS) {
        if !helpers.contains(*f) {
            bad.push(format!(
                "  `UNREFERENCED` / `MANUAL_RIGS` 里的 `{f}` 已不是 `tests/e2e/` 下的辅助件 —— 死行，删掉"
            ));
        }
        if why.trim().chars().count() < 20 {
            bad.push(format!(
                "  `{f}` 那一行的理由太短 —— 写清为什么不进执行链、谁来跑"
            ));
        }
    }
    // 两张表不许同时登记同一份（「报备待裁」与「已裁：手动」是两个互斥的状态）。
    for (f, _) in MANUAL_RIGS {
        if UNREFERENCED.iter().any(|(u, _)| u == f) {
            bad.push(format!(
                "  `{f}` 同时在 `UNREFERENCED` 与 `MANUAL_RIGS` 里 —— 只许一个状态"
            ));
        }
    }
    // 另一半的守门人还在（恰好一处定义）。
    let gate_reg =
        std::fs::read_to_string(root.join("tests/frontend/shell/e2e_gate_registry_tests.rs"))
            .expect("读 e2e_gate_registry_tests.rs");
    if let Err(e) = crate::find_pinned(
        &code_only(&gate_reg, Lang::Rust),
        "fn every_e2e_suite_is_either_gated_or_registered_as_exempt(",
    ) {
        bad.push(format!(
            "  「套件要么进门禁要么登记理由」那条判据找不着了（本层的可达那一半靠它）：{e}"
        ));
    }
    assert!(
        !suites.is_empty() && bad.is_empty(),
        "e2e 层（套件 {} 份 · 辅助件 {} 份）：\n{}",
        suites.len(),
        helpers.len(),
        bad.join("\n")
    );
}

/// 引用语料：`tests/` 下全部 shell ＋ `.rs` ＋ `.ts`/`.mts`，`.github/workflows/*.yml`，`package.json`
/// —— 各自剥掉注释（只在注释里提到的不算引用）。
fn reference_corpus(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for r in crate::shell_scripts(&root.join("tests")) {
        let rel = format!("tests/{r}");
        let disk_text = std::fs::read_to_string(root.join(&rel)).unwrap_or_default();
        out.push((rel, crate::strip_hash_comment_lines(&disk_text)));
    }
    // 摘掉本文件自己：登记表里逐字写着每一份辅助件的路径，不摘的话「有人引用」恒真。
    for (p, disk_text) in
        crate::scan_tree_excluding(&root.join("tests"), &["rs", "ts", "mts"], &[SELF])
    {
        let rel = rel_of(root, &p);
        let lang = lang_of(&rel);
        out.push((
            rel,
            crate::strip_comment_lines(&code_only_keep_strings(&disk_text, lang)),
        ));
    }
    for (p, disk_text) in crate::scan_tree_excluding(&root.join(".github/workflows"), &["yml"], &[])
    {
        out.push((
            rel_of(root, &p),
            crate::strip_hash_comment_lines(&disk_text),
        ));
    }
    let pkg = std::fs::read_to_string(root.join("package.json")).unwrap_or_default();
    out.push(("package.json".into(), pkg));
    out
}

/// 只剥注释、**留着**字符串（引用常常就在串里：`bash "$HERE/tmux-shim.sh"`、`include_str!("…")`）。
fn code_only_keep_strings(text: &str, lang: Lang) -> String {
    let code = code_only(text, lang);
    // 注释处 code_only 给的是空格、串内容也是空格 —— 用原文回填「串内容」那部分：
    // 逐字符比，原文与 code_only 不同且 code_only 是空格的位置，若落在一对引号之间就回填原文。
    let raw: Vec<char> = text.chars().collect();
    let cod: Vec<char> = code.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut in_str: Option<char> = None;
    for (k, &c) in cod.iter().enumerate() {
        let r = raw.get(k).copied().unwrap_or(' ');
        match in_str {
            Some(q) => {
                if c == q && r == q {
                    in_str = None;
                    out.push(c);
                } else {
                    out.push(r);
                }
            }
            None => {
                if (c == '"' || (lang == Lang::Ts && (c == '\'' || c == '`'))) && r == c {
                    in_str = Some(c);
                }
                out.push(c);
            }
        }
    }
    out
}

// ───────────────────────────── T5 真机层 ─────────────────────────────

/// 盘上的真机层人群：`(文件, 键)`。Rust 的键是测试函数名；TS 的键是 `skipIf(…)` 里那段条件（原样去空白）。
fn real_machine_on_disk(root: &Path) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for (rel, disk_text) in rust_test_files(root) {
        for (name, ignored, _) in rust_test_fns(&code_only(&disk_text, Lang::Rust)) {
            if ignored {
                out.insert((rel.clone(), name));
            }
        }
    }
    for (rel, disk_text) in ts_test_files(root) {
        for key in ts_skip_keys(&code_only(&disk_text, Lang::Ts)) {
            out.insert((rel.clone(), key));
        }
    }
    out
}

/// TS 里 `.skipIf(` / `.runIf(` / `.skip(` 的键：括号里那段条件（去掉空白）；`.skip(` 没有条件 ⇒ `skip`。
fn ts_skip_keys(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    for head in [".skipIf(", ".runIf("] {
        let mut from = 0usize;
        while let Some(at) = code[from..].find(head) {
            let start = from + at + head.len();
            let mut depth = 1i32;
            let mut end = start;
            for (k, c) in code[start..].char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                if depth == 0 {
                    end = start + k;
                    break;
                }
            }
            out.push(code[start..end].split_whitespace().collect::<String>());
            from = end.max(start);
        }
    }
    for head in ["it.skip(", "test.skip(", "describe.skip("] {
        let n = code
            .lines()
            .filter(|l| l.trim_start().starts_with(head))
            .count();
        for k in 0..n {
            out.push(format!("{head}#{k}"));
        }
    }
    out
}

/// T5：真机层 == 盘上 `#[ignore]` / `skipIf` 全集（两向，两棵 Rust 树 ＋ TS）；每条的过滤串现打仍是它全名的子串。
#[test]
fn real_machine_tier_every_ignored_test_is_registered_and_its_trigger_still_reaches_it() {
    let root = repo();
    let on_disk = real_machine_on_disk(&root);
    let registered: BTreeSet<(String, String)> = REAL_MACHINE
        .iter()
        .map(|(f, k, _)| (f.to_string(), k.to_string()))
        .collect();
    let mut bad = Vec::new();
    for (f, k) in on_disk.difference(&registered) {
        bad.push(format!(
            "  盘上有、没登记：`{f}` 的 `{k}` —— 进 `REAL_MACHINE` 并写清谁来跑它：\n        \
             (\"{f}\", \"{k}\", Trigger::Manual(\"…\")),"
        ));
    }
    for (f, k) in registered.difference(&on_disk) {
        bad.push(format!(
            "  登记了、盘上已不是 ignore / skip：`{f}` 的 `{k}` —— 删掉这一行"
        ));
    }
    let m = mounts(&root);
    for (f, k, trig) in REAL_MACHINE {
        match trig {
            Trigger::Manual(why) => {
                if why.trim().is_empty() {
                    bad.push(format!("  `{f}::{k}` 登记成手动却没写理由"));
                }
            }
            Trigger::Filter { by, needle } => {
                let Ok(disk_text) = std::fs::read_to_string(root.join(by)) else {
                    bad.push(format!("  `{f}::{k}` 的触发者 `{by}` 盘上没有"));
                    continue;
                };
                let text = if by.ends_with(".rs") || by.ends_with(".ts") {
                    crate::strip_comment_lines(&code_only_keep_strings(&disk_text, lang_of(by)))
                } else {
                    crate::strip_hash_comment_lines(&disk_text)
                };
                if !crate::contains_word(&text, needle) {
                    bad.push(format!(
                        "  `{f}::{k}` 的触发者 `{by}` 里（剥注释后）已经没有 `{needle}` —— 触发链断了"
                    ));
                    continue;
                }
                if lang_of(f) == Lang::Rust {
                    let Some(mp) = module_path(f, &m, 0) else {
                        bad.push(format!("  `{f}` 算不出模块路径（挂载不是恰好一处）"));
                        continue;
                    };
                    let inline = inline_mods_of(&code_only(
                        &std::fs::read_to_string(root.join(f)).unwrap_or_default(),
                        Lang::Rust,
                    ));
                    let full = full_test_name(&mp, inline.get(*k).map(String::as_str), k);
                    // 带 `::` 的是全名（触发者用 `--exact` 起它）⇒ 必须逐字相等；否则是子串过滤。
                    let reaches = if needle.contains("::") {
                        full == *needle
                    } else {
                        full.contains(needle)
                    };
                    if !reaches {
                        bad.push(format!(
                            "  `{f}::{k}` 的全名现打是 `{full}`，而触发者 `{by}` 用的过滤串 `{needle}` 不是它的子串 \
                             ⇒ `cargo test <过滤串>` 跑零条、exit 0 —— 那条测试再没执行过"
                        ));
                    }
                }
            }
        }
    }
    assert!(
        !on_disk.is_empty() && bad.is_empty(),
        "真机层（盘上 {} 条）：\n{}",
        on_disk.len(),
        bad.join("\n")
    );
}

fn full_test_name(module: &str, inline: Option<&str>, name: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if !module.is_empty() {
        parts.push(module);
    }
    if let Some(i) = inline.filter(|s| !s.is_empty()) {
        parts.push(i);
    }
    parts.push(name);
    parts.join("::")
}

/// 一份测试文件里，每个测试函数外面套着的**行内** `mod x { … }` 链（`a::b`）。
fn inline_mods_of(code: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut stack: Vec<(String, i32)> = Vec::new();
    let mut depth = 0i32;
    for line in code.lines() {
        let t = line.trim();
        if let Some(name) = mod_decl_name(t) {
            if t.ends_with('{') {
                stack.push((name, depth));
            }
        }
        if let Some(n) = fn_name(t) {
            if crate::contains_word(t, "fn") {
                let chain: Vec<&str> = stack.iter().map(|(s, _)| s.as_str()).collect();
                out.entry(n).or_insert_with(|| chain.join("::"));
            }
        }
        for c in line.chars() {
            if c == '{' {
                depth += 1;
            } else if c == '}' {
                depth -= 1;
                while stack.last().is_some_and(|(_, d)| *d >= depth) {
                    stack.pop();
                }
            }
        }
    }
    out
}

// ───────────────────────────── 判别器 / 抽取器自检（合成夹具） ─────────────────────────────

/// S1：每一层的签名都认得出；串里 / 注释里的针不算。
#[test]
fn the_detector_sees_each_tier_and_ignores_strings_and_comments() {
    let t = concat!("#[te", "st]");
    let rs = |body: &str| format!("{t}\nfn a() {{\n{body}\n}}\n");
    assert_eq!(detect("x.rs", "fn helper() {}\n"), Tier::Support);
    assert_eq!(detect("x.rs", &rs("assert_eq!(1 + 1, 2);")), Tier::Unit);
    assert_eq!(
        detect(
            "x.rs",
            &rs(&format!("let s = {}!(\"../x.rs\");", "include_str"))
        ),
        Tier::Scan
    );
    assert_eq!(
        detect(
            "x.rs",
            &rs(&format!(
                "let o = std::process::{}::new(\"ls\").output();",
                "Command"
            ))
        ),
        Tier::Integration
    );
    // 串里的针是数据，不是调用；注释里的更不是。
    let needle_in_string = rs(&format!("let n = \"{}::new(\";", "Command"));
    assert_eq!(detect("x.rs", &needle_in_string), Tier::Unit);
    let needle_in_raw = rs(&format!("let n = r#\"{}::new(\"#;", "Command"));
    assert_eq!(detect("x.rs", &needle_in_raw), Tier::Unit);
    let needle_in_comment = rs(&format!(
        "// {}::new(\n/* {}_str!( */",
        "Command", "include"
    ));
    assert_eq!(detect("x.rs", &needle_in_comment), Tier::Unit);
    // 生命周期不是字符字面量（否则后面整段被当成串吃掉）。
    let lifetime = rs(&format!(
        "fn f<'a>(x: &'a str) -> &'a str {{ x }}\nlet s = {}!(\"x\");",
        "include_str"
    ));
    assert_eq!(detect("x.rs", &lifetime), Tier::Scan);
    // TS。
    assert_eq!(
        detect(
            "a.vitest.ts",
            "import { x } from \"../src/x.ts\";\nit(\"a\", () => {});\n"
        ),
        Tier::Unit
    );
    assert_eq!(
        detect(
            "a.vitest.ts",
            "import s from \"../src/x.ts?raw\";\nit(\"a\", () => {});\n"
        ),
        Tier::Scan
    );
    assert_eq!(
        detect(
            "a.vitest.ts",
            "import { execFileSync } from \"node:child_process\";\nit(\"a\", () => {});\n"
        ),
        Tier::Integration
    );
    assert_eq!(
        detect(
            "a.vitest.ts",
            "const s = \"readFileSync(\";\nit(\"a\", () => {});\n"
        ),
        Tier::Unit
    );
    assert_eq!(
        detect("a.vitest.ts", "export const x = 1;\n"),
        Tier::Support
    );
    // 正则字面量里的反引号 / 引号不是串的开头（不认的话后面整份被吃掉，`it(` 看不见 ⇒ 误判支撑）。
    let regex_with_tick =
        "const R = /`|\"/;\nconst d = a / b / c;\nit(\"x\", () => { readFileSync(\"p\"); });\n";
    assert_eq!(detect("a.vitest.ts", regex_with_tick), Tier::Scan);
}

/// S2：挂载抽取器认得出「`#[path]` ＋ 中间隔着属性 ＋ 带可见性的 `mod`」，注释里的不算。
#[test]
fn the_mount_extractor_sees_path_attributes_and_skips_comments() {
    let p = concat!("#[pa", "th");
    let src = format!(
        "#[cfg(test)]\n{p} = \"../../tests/a_tests.rs\"]\nmod tests;\n\
         {p} = \"../b.rs\"]\n#[allow(dead_code)]\npub(crate) mod bee;\n\
         // {p} = \"../ghost.rs\"]\n// mod ghost;\n"
    );
    assert_eq!(
        path_mounts(&src),
        vec![
            ("../../tests/a_tests.rs".to_string(), "tests".to_string()),
            ("../b.rs".to_string(), "bee".to_string()),
        ]
    );
}

/// S3：扫描层的路径抽取器认得直连与绑定两形；本文件自己定义的同名根函数、歧义变量都不认。
#[test]
fn the_root_anchored_literal_extractor_sees_both_shapes_and_nothing_ambiguous() {
    let src = "fn a() {\n    let r = repo_root();\n    let x = r.join(\"src/frontend/shell\");\n    \
               let y = repo_root().join(\"tests\");\n    let t = tmp();\n    let z = t.join(\"no\");\n}\n\
               fn b() {\n    let r2 = std::env::temp_dir();\n    let w = r2.join(\"no\");\n}\n";
    let got: Vec<String> = root_anchored_literals("tests/frontend/shell/x.rs", src)
        .into_iter()
        .map(|(_, f, l)| format!("{f}:{l}"))
        .collect();
    assert_eq!(
        got,
        vec![
            "repo_root:src/frontend/shell".to_string(),
            "repo_root:tests".to_string()
        ]
    );
    // 同一个名字绑到两种东西上 ⇒ 歧义 ⇒ 不认。
    let amb = "fn a() { let r = repo_root(); let _ = r.join(\"x\"); }\nfn b() { let r = tmp(); let _ = r.join(\"y\"); }\n";
    assert!(root_anchored_literals("tests/frontend/shell/x.rs", amb).is_empty());
    // 本文件自己定义了 `src_root` ⇒ 它的语义不是共享的那个 ⇒ 不认。
    let local =
        "fn src_root() -> PathBuf { todo!() }\nfn a() { let _ = src_root().join(\"x\"); }\n";
    assert!(root_anchored_literals("tests/backend/x.rs", local).is_empty());
    // 本地同名、但只是转调共享那个 ⇒ 语义相同 ⇒ 照认。
    let delegate = "fn repo_root() -> PathBuf {\n    crate::guard_support::repo_root()\n}\nfn a() { let _ = repo_root().join(\"x\"); }\n";
    assert_eq!(
        root_anchored_literals("tests/frontend/shell/x.rs", delegate).len(),
        1
    );
}

/// S4：静默跳过的认法 —— 正控（缺环境 ⇒ return）认得出；读了环境变量但缺了就 panic 的不认。
#[test]
fn the_silent_skip_detector_can_tell_a_skip_from_a_panic() {
    let v = concat!("env::", "var(");
    let skip = format!(
        "fn t() {{\n    let Ok(h) = std::{v}\"HOME\") else {{\n        return;\n    }};\n}}\n"
    );
    assert_eq!(silent_skips(&code_only(&skip, Lang::Rust)), vec![2]);
    let panic = format!("fn t() {{\n    let h = std::{v}\"HOME\").expect(\"要 HOME\");\n}}\n");
    assert!(silent_skips(&code_only(&panic, Lang::Rust)).is_empty());
}

/// S5：测试函数切法认得出上方 / 下方的 `#[ignore]`、`tokio` 测试，以及行内 `mod` 链。
#[test]
fn the_test_fn_splitter_sees_ignores_on_both_sides_and_inline_mods() {
    let t = concat!("#[te", "st]");
    let ig = concat!("#[ign", "ore");
    let src = format!(
        "{t}\nfn plain() {{}}\n\n{ig} = \"x\"]\n{t}\nfn above() {{}}\n\n{t}\n{ig}]\nfn below() {{}}\n\n\
         mod inner {{\n    #[tokio::test]\n    async fn deep() {{ let x = {{ 1 }}; }}\n}}\n"
    );
    let code = code_only(&src, Lang::Rust);
    let got: Vec<(String, bool)> = rust_test_fns(&code)
        .into_iter()
        .map(|(n, i, _)| (n, i))
        .collect();
    assert_eq!(
        got,
        vec![
            ("plain".to_string(), false),
            ("above".to_string(), true),
            ("below".to_string(), true),
            ("deep".to_string(), false),
        ]
    );
    assert_eq!(
        inline_mods_of(&code).get("deep").map(String::as_str),
        Some("inner")
    );
    assert_eq!(
        inline_mods_of(&code).get("plain").map(String::as_str),
        Some("")
    );
}

// ───────────────────────────── `benches/`：量具的家 ─────────────────────────────

/// bench 源码的唯一住址（仓根相对）。
const BENCHES: &str = "tests/benches/";

/// 一份 `Cargo.toml` 里每个 `[[bench]]` 块：`(path 字面量, test = true 没有)`。
fn bench_blocks(manifest: &str) -> Vec<(Option<String>, bool)> {
    let mut out: Vec<(Option<String>, bool)> = Vec::new();
    let mut open = false;
    for row in crate::strip_hash_comment_lines(manifest).lines() {
        let t = row.trim();
        if t.starts_with('[') {
            open = t == "[[bench]]";
            if open {
                out.push((None, false));
            }
            continue;
        }
        if !open {
            continue;
        }
        let Some((k, v)) = t.split_once('=') else {
            continue;
        };
        let last = out.last_mut().expect("块已开");
        match k.trim() {
            "path" => last.0 = Some(v.trim().trim_matches('"').to_string()),
            "test" => last.1 = v.trim() == "true",
            _ => {}
        }
    }
    out
}

/// B：`[[bench]]` 的源码集合 == `tests/benches/*.rs` 盘上集合（两向）；每条都标 `test = true`
/// （⇒ `cargo test` 走它的冒烟档 ⇒ 门禁那一格证明它跑得起来）；墙钟不进任何判据。
#[test]
fn every_bench_lives_in_the_benches_home_and_runs_under_cargo_test() {
    let root = repo();
    let mut declared: BTreeSet<String> = BTreeSet::new();
    let mut bad = Vec::new();
    let manifests: Vec<String> = crate::files_by_extension(&root.join("src"), "toml")
        .into_iter()
        .filter(|r| r.ends_with("Cargo.toml") && !r.split('/').any(|seg| seg == "vendor"))
        .collect();
    for m in &manifests {
        let dir = root.join("src").join(m);
        let dir = dir.parent().expect("清单有父目录").to_path_buf();
        let manifest_text =
            std::fs::read_to_string(root.join("src").join(m)).expect("读 Cargo.toml");
        for (path, test) in bench_blocks(&manifest_text) {
            let Some(path) = path else {
                bad.push(format!("  `src/{m}` 有一个 `[[bench]]` 没写 `path` —— 它的源码落在清单旁的 `benches/` 里，那不是本仓的家"));
                continue;
            };
            let rel = rel_of(&root, &normalize(&dir.join(&path)));
            if !test {
                bad.push(format!("  `src/{m}` 的 `[[bench]]`（`{rel}`）没标 `test = true` ⇒ `cargo test` 不跑它的冒烟档，坏了没人知道"));
            }
            declared.insert(rel);
        }
    }
    let on_disk: BTreeSet<String> = crate::files_by_extension(&root.join(BENCHES), "rs")
        .into_iter()
        .map(|r| format!("{BENCHES}{r}"))
        .collect();
    for f in set_diff(&declared, &on_disk) {
        bad.push(format!(
            "  `[[bench]]` 指向 `{f}`，它不在 `{BENCHES}` 下（或盘上没有）"
        ));
    }
    for f in set_diff(&on_disk, &declared) {
        bad.push(format!(
            "  `{f}` 在 `{BENCHES}` 下，却没有任何 `[[bench]]` 指它 —— 它不编译，是死文件"
        ));
    }
    assert!(
        !manifests.is_empty() && !on_disk.is_empty() && bad.is_empty(),
        "`benches/`（清单 {} 份 · 盘上 {} 份 · 声明 {} 条）：\n{}",
        manifests.len(),
        on_disk.len(),
        declared.len(),
        bad.join("\n")
    );
}
