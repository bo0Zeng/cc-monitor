# 功能计划 — F10 剩余账号 UX

> 对应主计划功能清单中的 F10。本文件是该功能从规划到签收的全程记录。

## 0. Phase A 摸底结论（三个子项逐字来自已废弃前身工作区）

MASTERPLAN 的 F10 一句话描述"面板砍卡片 / 加号一键化 / 用量（plan 窗口 %）"逐字来自已废弃的
前身工作区 `.claude/planned-build/account-onboarding/MASTERPLAN.md` 的 F3/F4/F7（该工作区因
`MASTERPLAN-v2.md` 经四视角 full-audit 判定不可执行而被 unify-launch 接管，只有 F5/F1 真正交付，
F3/F4/F7 从未落地，原样打包移交成 F10）。摸底核实结论：

- **F3（面板砍卡片）**：原始目标是"卡片列表（头像/名/登录态/用量）+ 加号 + `<details>` 排障折叠
  （默认折叠）"，对照的"现状问题"是"卡片+维护区+向导混一坨、manifest/configDir 外露"。核实
  `src/settings/accounts-section.ts` 当前实际实现：`renderTable`/`accountRow`（401-438/517-630 行）
  是**表格行**（`.accounts-table`/`.accounts-row`），不是卡片；`renderMaintenance`（443-515 行）
  **已经**是 `<details>` 折叠（默认态按账号数动态给，U7 工作已完成）；`configDir` 长路径已降权成
  "复制路径"操作而非常驻显示。**结论：F3 的底层目标（紧凑主视图 + 折叠维护区 + 不常驻暴露
  configDir）已被更早的工作（表格布局 + U7 折叠）以不同的视觉形式达成，不是"待做"，是"已用
  另一种形式做完"——本功能不重做成字面意义的卡片（表格行在信息密度和可扫描性上不比卡片差，
  重做没有实质收益，纯粹是视觉风格切换）。**
- **F4（加号一键化）**：原始 DoD"加号 ≤2 步到『终端里 /login』；快照路径免重登；danger 二次
  确认保留"。核实当前 `renderMaintenance` 的 `addForm`（462-497 行）+ `launchStep`（138-164 行）：
  填账号名（可选填快照路径）→ 点"加账号…"→ danger 二次确认（`confirmExtra` 文案已明确写
  "随后在弹出的终端里用它 /login"）→ 终端弹出跑 `add --apply`。**结论：这个流程已经很接近
  DoD——按"离散动作数"算是 2 步（填表单、确认），且confirmExtra 已经把下一步该做什么说清楚了，
  不是"加完不知道去哪登录"的旧问题。判定为已满足，不需要重新设计流程**；本功能只做一次confirm
  文案的小幅复核（见 §4 步骤 1），不做流程重构。
- **F7（用量 plan 窗口%）**：核实这是唯一**真正未实现、需要新设计**的子项——不是 context window
  用量（`src/usage-hud.ts` 已有，语义完全不同），也不是本地 token 累计（`views/usage-view.ts` 已有，
  同样完全不同语义）,而是 Claude 订阅计划的 5h/周额度窗口剩余百分比+重置倒计时,是 Anthropic
  服务端权威数据,本地文件推不出来,必须真的起一个已登录的 claude 会话跑 `/usage` 斜杠命令、
  capture-pane 抓屏解析。这是 F10 的主体工作,详见 §1-§7。

**关键决策：不做真机 spike**。原计划要求 Phase B 先真机 spike 确认 `/usage` 实际输出格式再定
实现。本席判断**不应该在这个开发环境里启动一个真实已认证的 claude 子进程去测试**——那会消耗
用户的真实 API/订阅额度、且与当前正在跑的会话可能产生不可控交互,这类操作的风险等级和这个
仓库其余"真机测试"(全部用隔离 tmux socket + 假 launcher/mock 数据,如 `e2e/ccm-pretrust-
acceptance.sh` 用"假 launcher 打印固定文本"而非真 claude,理由是"真 claude 会清屏/重绘,把
断言搞乱")不是一回事。**改为**：开一个 Plan agent 设计一版"无法真机验证前提下"的防御性方案
——分层设计,让"解析器猜错"这个最大不确定性的爆炸半径被限制在一个独立纯函数文件里,失败态
诚实降级为"读不到 + 说明可能原因",不是伪装成功或崩溃。§7 给出用户上线前必须在真机上做的
验证清单。

## 1. 目标与验收标准（DoD）

- **目标**：给每个隔离账号显示 Claude 订阅计划的用量窗口百分比（"plan 窗口%"），走
  `/usage` + capture-pane 解析这条路径；不新增轮询；解析失败诚实降级。
- **验收标准**（可验证、可勾选）：
  - [x] 新 Rust 模块 `src-tauri/src/account_usage.rs`：IPC `account_usage(origin, account_name,
        launch_payload) -> AccountUsageProbeResult{captured, raw, error}`；命令构造拆成可单测的
        纯函数（同 `build_capture_pane_cmd`/`build_kill_session_cmd` 既有惯例）
  - [x] 探针会话：固定命名 `ccm-usage-<slug>`（**实现期修正**：不做 `-2`/`-3`/`-4` 撞名候选
        重试——探针名字前缀专属本功能，任何撞到这个前缀的会话必然是自己的残留，直接无条件
        `kill-session`（存在才杀，不存在忽略）后同名重建即可，不需要枚举候选名；`e2e/
        usage-probe-acceptance.sh` 场景 2 已验证撞名残留会被正确清场重建）；自毁看门狗
        （`setsid`+`disown`+`sleep 30`+`kill-session`，独立于 SSH 通道存活）；画面稳定轮询
        （连续两次 capture-pane 内容一致）代替固定 sleep 猜测
  - [x] `list_remote_tmux` 按**名字前缀**（`ccm-usage-`）过滤掉探针会话——**实现期修正**：
        Plan agent 原方案建议给 `TMUX_LS_FMT` 加一列 `@ccm_usage_probe` tmux user-option 打标，
        但 `TMUX_LS_FMT` 是机器化锁死的"双写点"（红线 I8，`tmux.rs` 的
        `tmux_ls_fmt_double_write_point_stays_in_sync` 测试断言 monitor 侧此常量与
        `remote-daemon-proto/src/watcher.rs` 逐字节一致——daemon 也用它做自己的 idle-tmux
        对账轮询）——改这个格式串需要同步改两个独立 crate 的源码，代价和风险都不成比例。改用
        名字前缀过滤（探针会话名本来就完全由本功能控制，`ccm-usage-` 前缀足够独特，不需要额外
        的 tmux user-option），零风险、不碰任何双写点
  - [x] 新文件 `src/account-usage-parse.ts`：纯函数 `parseUsageCapture(raw)` 判别式返回
        `{status:"ok",buckets}|{status:"unrecognized",reason,raw?}|{status:"not-logged-in",raw?}|
        {status:"cli-missing",raw?}`，绝不 throw；正则模式明确标注"基于训练知识猜测，非真机
        验证"
  - [x] 新文件 `src/account-usage.ts`：`buildUsageProbePayload`/`fetchAccountUsage`（invoke 包装 +
        去抖缓存，非 TTL 轮询）
  - [x] `src/settings/accounts-section.ts` 的 `accountRow`：新增用量单元格（懒加载"查看用量"
        按钮，点击才探测；成功/未识别/未登录/CLI 缺失/探测失败五种状态均有明确短句）
  - [x] `src/account-chip.ts`：折叠态 + 下拉菜单为**当前账号**懒加载用量摘要（菜单展开时才
        探测，不是 app 启动/`refresh()` 时）；"刷新用量"与既有"刷新"（账号列表）分开
  - [x] daemon `--usage`"补充同显"降级/不做——已核实 `projects/` 目录按 cc-acct-iso 设计跨账号
        共享（symlink 同 inode），daemon `--usage` 聚合的是主机级总量，不能按账号拆分，硬塞进
        账号行会误导颗粒度（详见 §5）
  - [x] 真机 e2e：用隔离 tmux socket + 假"claude"stand-in 脚本（不是真 claude，同
        `ccm-pretrust-acceptance.sh` 的既有纪律），验证探针的**编排机制**本身（建会话/打标/
        自毁看门狗/candidate 命名冲突避让/capture/清理），不验证解析器对真实 `/usage` 格式的
        正确性（那部分见 §7 真机验证清单，标注为用户后续待办）
  - [x] F3/F4：不做代码改动，本文件 §0 的核实结论即签收依据
  - [x] tsc 0 / npm test 全绿 / cargo test 全绿（新增 Rust 单测） / 既有 e2e 套件不变 + 新增
        `test:usage-probe`（真机 e2e，隔离 socket + 假 stand-in）
  - [x] `remote-launch.test.ts`/两个 e2e driver/`src/tabs.ts` 全程 `git status` 核对零 diff
        （本功能不碰 resume/restart 路径）
- **明确不做什么**（防范围蔓延）：
  - 不做真机 spike（见 §0 决策，理由：消耗真实 API 额度+风险等级不同于既有隔离测试）
  - 不做"批量扫描孤儿会话 + 启发式判定 + 确认清理"这类通用机制——这个仓库**已经做过又主动
    砍掉**这个模式（`audit-fixes/features/05-cleanup-orphans.md` 落地，`auto-e2e/features/
    remove-orphan-cleanup.md` 2026-07-26 因"UX 审计 footgun：把别窗口/实例正跑的活会话误列
    孤儿劝杀"而删除），探针会话的生命周期管理走自包含确定性机制（tag+看门狗+隐藏），不重蹈
    覆辙
  - 不做 daemon `--usage` 与账号级用量的"同显融合"（颗粒度语义不匹配，见 §5，除非用户明确
    要求先解决 `projects/` 按账号分区这个更大的架构问题——不在本轮范围）
  - 不改 F3/F4 对应的现有代码（§0 已核实达标，重做没有实质收益）
  - 不复用 F03 的 `LaunchPlan`/`ccm` CLI 起探针会话——探针是纯只读诊断、寿命以秒计，混进
    正牌会话身份体系（`@ccm_sid`/F11 预信任等）反而增加被 tab 误识别的风险，保持独立更审慎

## 2. 与主计划的对接

- **触及的共享面**：`src-tauri/src/tmux.rs`（`list_remote_tmux` 加隐藏过滤，不改其余函数）、
  `src/remote-launch.ts`（新增 `buildUsageProbePayload`，复用既有 `buildEnvPrefix`/
  `CLAUDE_NESTED_ENV_VARS`/`AGENT_PROFILE`，不改现有导出签名）。**不触及** `src/launch-plan.ts`/
  `src/launch-dimensions.ts`/两个渲染器/`src/tabs.ts`/`e2e/*-cmd-driver.ts`。
- **遵循的最终形态设计**：探针是全新、独立的子系统，不是"改造现有启动路径去多做一件事"——
  刻意不复用 `LaunchPlan` IR（见 §1"不做什么"），避免这次改动牵连 unify-launch 核心账本的
  任何一行。
- **新引入、需登记进账本的共享面**：`src-tauri/src/account_usage.rs`（新）、
  `src/account-usage-parse.ts`（新）、`src/account-usage.ts`（新）——F10 独有，暂不登记进
  MASTERPLAN §3 主账本（不是 unify-launch 核心路径的一部分，是账号设置面板的独立诊断功能）。
- **本功能的边界**：不改 `src-tauri/src/ssh_source.rs`（只读它现成的 `connect_and_exec_cmd`/
  `shell_quote`）；不改 `remote_history.rs`/`usage.rs`（daemon `--usage` 链路，只读不改，
  §5 已判定不融合）。

## 3. 接口 / 契约设计

```rust
// src-tauri/src/account_usage.rs（新）

#[derive(serde::Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsageProbeResult {
    pub captured: bool,
    pub raw: Option<String>,
    pub error: Option<String>,
}

/// 命令构造纯函数（可单测，同 build_capture_pane_cmd 既有惯例）——
/// 清场（无条件 kill 同名残留）+ 建会话 + 看门狗 + send-keys launch_payload + 稳定轮询 +
/// `/usage` + 稳定轮询 + capture-pane + kill-session，全部编译成一条远端 shell 脚本。
/// `watchdog_timeout_secs` 独立传入（生产恒传 30s 常量）——真机 e2e 验证看门狗本身需要
/// 短得多的值才能在合理时间内跑完，不代表生产可配置。
fn build_usage_probe_cmd(account_slug: &str, launch_payload: &str, watchdog_timeout_secs: u32) -> String;

#[tauri::command]
pub async fn account_usage(
    origin: String,
    account_name: String,
    launch_payload: String,
) -> Result<AccountUsageProbeResult, String>;
```

```ts
// src/account-usage-parse.ts（新，零依赖纯函数）
export interface AccountUsageBucket {
  label: string;
  usedPercent: number;
  resetIn?: string;
}
export type AccountUsageParseResult =
  | { status: "ok"; buckets: AccountUsageBucket[] }
  | { status: "unrecognized"; reason: string; raw?: string }
  | { status: "not-logged-in"; raw?: string }
  | { status: "cli-missing"; raw?: string };
export function parseUsageCapture(raw: string): AccountUsageParseResult;

// src/account-usage.ts（新）
export type AccountUsageOutcome =
  | { status: "probe-failed"; error: string }
  | AccountUsageParseResult;
export async function fetchAccountUsage(
  origin: string,
  accountName: string,
  configDir: string,
  opts?: { force?: boolean },
): Promise<AccountUsageOutcome>;
```

## 4. 实现步骤（严格顺序执行，逐条勾选）

- [x] 步骤 1：F3/F4 复核确认（§0 已有结论）——重读 `accounts-section.ts` 的 `renderTable`/
      `accountRow`/`renderMaintenance`/`addForm`，确认无遗漏的真实缺口；`confirmExtra` 文案
      做一次可读性复核（不改逻辑）。验证：走查代码，无需改动即视为通过。
- [x] 步骤 2：`src-tauri/src/tmux.rs` 新增 `is_usage_probe_session`（纯函数，判定 tmux 会话名
      是否匹配 `ccm-usage-` 前缀——**不**碰 `TMUX_LS_FMT`/daemon 双写点，见 §1 实现期修正）；
      `list_remote_tmux` 在 `parse_tmux_ls` 之后过滤掉这类会话。验证：新增 cargo 单测，构造
      含 `ccm-usage-*` 与不含的 `tmux ls` 输出，断言过滤前后行为。
- [x] 步骤 3：新建 `src-tauri/src/account_usage.rs`——`build_usage_probe_cmd` 纯函数 + `account_usage`
      IPC；`lib.rs` 接线（`mod account_usage;` + handler 注册）。验证：cargo 单测锁定
      `build_usage_probe_cmd` 产出的脚本含正确的清场重建/看门狗/清理逻辑（字符串断言，
      同 `build_kill_session_cmd` 的测试风格）。
- [x] 步骤 4：新建 `src/account-usage-parse.ts` + `src/account-usage-parse.vitest.ts`——按 §3
      判别式实现，正则模式含"训练知识猜测、非真机验证"的显式注释；夹具覆盖多种措辞变体（防
      正则过拟合单一猜测）+ 未登录态 + CLI 缺失态 + 空白输入 + 完全认不出的格式。验证：
      `npx vitest run` 绿，含一条"删掉某个 LABEL_PATTERNS 条目/改坏 PERCENT_RE 对应测试必须
      变红"的变异验证。
- [x] 步骤 5：新建 `src/remote-launch.ts::buildUsageProbePayload` + `src/account-usage.ts`
      （`fetchAccountUsage` + 去抖缓存）。验证：vitest 覆盖 invoke 参数正确性 + 缓存去抖行为
      （非 force 时不重复 invoke，force 时忽略缓存）。
- [x] 步骤 6：`src/settings/accounts-section.ts::accountRow` 接入用量单元格（懒加载按钮 + 五种
      状态渲染）。验证：jsdom 测试模拟点击 → mock `account_usage` 返回各种结果 → 断言渲染
      文案正确。
- [x] 步骤 7：`src/account-chip.ts` 接入当前账号用量摘要（菜单展开懒加载）+"刷新用量"动作。
      验证：vitest 覆盖菜单展开触发探测、"刷新用量"与"刷新"账号列表互不干扰。
- [x] 步骤 8：真机 e2e `e2e/usage-probe-acceptance.sh`（隔离 `-L` tmux socket + 假"claude"
      stand-in 脚本 FAKECLAUDE，收到 `/usage` 后打印固定文本）——3 个场景：正常路径（建/送键/
      抓屏/清理，拿到 FAKECLAUDE 的固定回应且不残留）、撞名残留（探针名字前有个不相关旧会话，
      须清场重建而非卡住/误判）、自毁看门狗（故意用 `timeout` 在流程中途打断，验证会话仍在
      看门狗超时窗口内被独立清理，不需要人工介入）。**不**验证解析器对真实 CC `/usage` 格式的
      正确性（那是 §7 用户真机验证清单的范围）。
- [x] 步骤 9：全量门禁——`tsc`/`npm test`/`cargo test`/全部既有 e2e 套件 + 新增
      `test:usage-probe`（`set -o pipefail` + 重定向到文件 + grep 核实）。
- [x] 步骤 10：`git status --short` 核对 `e2e/resume-cmd-driver.ts`/`e2e/restart-cmd-driver.ts`/
      `src/remote-launch.test.ts`/`src/tabs.ts` 零 diff。

## 5. daemon `--usage` 现状调查结论（不融合的理由，供 Phase D/E 复核）

已核实存在：`remote-daemon-proto/src/usage_query.rs`（daemon `--usage` 子命令）+
`src-tauri/src/remote_history.rs::aggregate_remote_usage_all` + `src-tauri/src/usage.rs`
（本地同口径）+ `src/views/usage-view.ts`（独立"用量"面板消费者）。**语义硬伤**：daemon
`--usage` 扫 `<claude_dir>/projects/**/*.jsonl`，而按 cc-acct-iso 既定设计（`account-isolation/
MASTERPLAN.md`），`projects/` 是跨账号共享目录（symlink 回同一份，同 inode）——daemon `--usage`
聚合出的 token 数**是主机级总量，不是、也不可能是某个特定账号专属**。硬塞进账号行会让用户
误以为"这是该账号的累计"，是在撒谎。**判定**：不融合（不在账号行/chip 里重复展示这份数据，
维持它现在在 `views/usage-view.ts` 的位置），本功能只做 plan 窗口% 这一件事。

## 6. 测试策略

- **单元**：`account-usage-parse.vitest.ts`（解析器，含变异验证）、`account-usage.vitest.ts`
  （invoke 包装 + 缓存）、`accounts-section.vitest.ts`/`account-chip.vitest.ts` 补用量渲染
  用例、Rust 侧 `account_usage.rs`/`tmux.rs` 新增单测（命令构造 + 隐藏过滤）。
- **集成 / E2E**：`e2e/usage-probe-acceptance.sh`（新，隔离 tmux socket + 假 stand-in，验证
  编排机制不验证解析正确性）。
- **属性 / 快照**：无。
- **本功能覆盖率 / 门禁要求**：同仓库既有标准。
- **修 bug 时**：先写复现的失败测试再修。

## 7. 上线前用户必须在真机上做的验证清单（★ 关键，Phase F 会再提醒一次）

1. **格式核实**：手动 `CLAUDE_CONFIG_DIR=<某账号目录> claude`，等启动完成，敲 `/usage` +
   回车，记录：窗口数量（1/2/3 个）、每个窗口标签原文、百分比呈现形式（已用%还是剩余%——
   方向反了不报错但会误导）、重置倒计时原文措辞、是否混有 ANSI 转义、alt-screen 重绘还是
   scrollback 追加。
2. **冷启动+网络耗时核实**：掐表测量"敲 claude 到稳定 REPL"、"敲 /usage 到用量数据渲染完成"
   各要多久（正常网络 + 模拟慢网络），核对本方案"稳定轮询、最多 7s"这个上限是否够用。
3. **未登录态验证**：对未登录账号跑一遍，记录实际显示内容，核对 `not-logged-in` 判据。
4. **CLI 缺失态验证**：对 PATH 里没有 `claude` 的账号 shell 环境跑一遍，记录错误原文
   （注意 bash/zsh 等不同 shell 措辞可能不同）。
5. **窗口尺寸验证**：窄（80 列）/宽（200 列）tmux 会话各跑一遍，确认百分比数字是否被
   换行/裁切截断。
6. **孤儿防护验证**：故意在"发送 /usage 之后、抓屏之前"打断（如断网几秒），确认探针会话
   在看门狗超时窗口内自行消失，不需人工介入。
7. **共享库验证**：同一台机两个账号下分别跑 `<daemon> --usage`，核对两次输出是否完全相同
   （验证 §5"不可能按账号区分"这一结论，不是单纯推断）。
8. 验证完成后回来重写 `src/account-usage-parse.ts` 的正则模式与 `AccountUsageBucket` 字段
   含义，把验证结果记进本文件 §6，供以后 CC 版本升级导致格式再次漂移时溯源。

## 8. 代码审计结果（Phase D）

双 agent 审（后端架构 + UX）+ 本席在 2026-07-28 四视角复核中的追加核实。

- **正确性**：
  - **阻塞（已修）**：chip 用量显示存在**跨账号陈旧写入竞态**——用户在探测未返回时切号，
    旧账号的 `await` 回来后仍写进 chip，显示成新账号的用量。已加 in-flight 账号比对守卫。
  - `disown` 是 bash-ism（`setsid` 之后本就多余），POSIX `sh` 下会让整条看门狗命令报错；
    内层 `bash -c` 同理。已改 `setsid sh -c`，并加断言 `!cmd.contains("disown")` /
    `!cmd.contains("bash -c")` 防回归。
  - `build_usage_probe_cmd` 自己拼 `-t` 目标、**绕过了 `tmux::exact_target`**——等于在 F01/F04
    刚统一的目标语法上开了个新的旁路。已改走真 `exact_target`（连带它变 `pub(crate)`、
    `build_usage_probe_cmd` 变 fallible），并加测试对 `["z","","collision"]` 三个 slug
    逐一对拍 `exact_target` 的输出。
  - 顺带订正一处本席自己的错误判断：曾以为空 slug 会被 Gate1 拒（写了个失败的测试），
    实测 `build_usage_probe_cmd("")` 产出会话名 `"ccm-usage-"` **非空**、Gate1 放行。
    改走 `exact_target` 的真实收益是 `=name:` 引号规则的**单一事实来源**，不是空值防护——
    测试已按事实改名为 `probe_cmd_target_tracks_exact_target_and_prefix_keeps_gate1_unreachable`。
- **计划符合度**：步骤 1-10 全部落地。F3/F4 按 §0 复核结论只做确认性小改（未重做）；
  F7 是唯一新建部分，与 §3 契约一致。无计划外夹带。
- **架构符合度**：未新增轮询（懒加载 + `force` 显式重查，无 `setInterval`）；未碰 daemon；
  探针会话经 `is_usage_probe_session` 从可见会话列表过滤，不污染 tab 视图。
  `buildUsageProbePayload` 复用既有 `buildEnvPrefix`/`posixQuote`/`AGENT_PROFILE`，未另起一套。
- **代码质量 / 伪测试专项**：本功能查出并修掉 **3 条伪测试**，是本轮伪测试问题的主要来源：
  1. shell-quote 测试原来只断言 `cmd.contains("send-keys")`——改成三个对抗性 payload 经
     `sh -c "printf '%s' <quoted>"` 真做往返比对。
  2. 百分比正则的"变异验证"测试原来验的是**测试文件里本地构造的** `/^100%$/`，与生产
     `PERCENT_RE` 无关。改成三条真调 `parseUsageCapture` 的用例（个位数 %、边界 0%/100%、
     `%` 前多空格）。
  3. tmux 会话过滤测试原来断言的是被测函数之外的东西。抽出
     `parse_visible_tmux_sessions` 后重新指向它，**并做了变异验证**（删掉生产侧
     filter 确认转红）。
  - **R01 收口补的最后一条**：`launchPayload`——本功能里唯一真正被送到远端 shell 执行的
    字符串——此前**从未被断言过内容**（原测试只用 `objectContaining` 查了 origin/accountName）。
    已补逐字节断言 + 5 条 fail-closed 边界（引号/`;`/`$()`/相对路径/路径穿越）。
    **两次变异验证**：去掉账号隔离前缀 → 6 条转红；去掉嵌套 env 清理 → 1 条转红。
    第一条变异尤其值得记：它模拟的是"探针探到错账号的用量、而界面看起来完全正常"这类
    静默错误——正是 R11 那一族病症的形状。
- **处置**：全部阻塞与重要项已修；修复后重跑门禁与 7 套真机 e2e 全绿（见 §9）。

## 9. 工程审计结果（Phase E）

- **主计划是否仍自洽**：是。本功能不新增维度、不改 `LAUNCH_DIMENSIONS`、不碰双渲染器，
  是 IR 之外的独立读路径（探针自己拼一次性载荷，不经 `LaunchPlan`）。这一点**有意为之**：
  用量探针不是"起一个会话给用户用"，而是"起一个会话读一行字然后自毁"，
  塞进 `LaunchAction` 会污染那个模型的语义（同 F09 判定 `restart` 不进 `LaunchAction` 的同型理由）。
- **是否引入拖累后续功能的耦合/技术债**：
  - 一处**已知且已记录**的诚实债：`/usage` 输出格式基于训练知识猜测、**未经真机验证**。
    缓解是三层的——解析失败诚实降级（不猜）、`status:"ok"` 也带
    `OK_USAGE_UNVERIFIED_CAVEAT` 提示、所有带 `raw` 的状态都能"复制诊断文本"。
    真机验证清单在 §7，留给用户上线前跑。这条不阻塞后续功能。
  - `exact_target` 因本功能升为 `pub(crate)`。可接受：它本就是全仓 tmux 目标语法的唯一
    事实来源，扩大可见性正是为了**杜绝**旁路，与账本对 `tmux.rs` 的最终形态一致。
- **是否有应现在就做的统一重构**：**有，且已在本轮直接兑现**——伪测试问题在 F10 里暴露得
  最集中（3 条），说明它不是 F10 的局部问题而是**跨功能的验证信号问题**。据此把它提升为
  R02（伪测试扫荡）作为 R 段独立一项，而不是在 F10 里修完就算完。同理，`cargo fmt` 红 29 处、
  分支从未跑过 CI、7 套真机 e2e 不在 CI —— 提升为 R00。
  这是账本铁律 6「预见的重叠现在就优雅重构」在**验证基础设施**上的一次应用。
- **工程健康度**：`tsc` 0 · `npm test` 697（46 文件）· `cargo test --all` 390 ·
  daemon 125 + `cargo fmt --check` 干净 · vendor `code-picture-core` 25 ·
  CI 四道真门禁按**原样命令**复跑全过（`npm audit --omit=dev --audit-level=high` /
  `shellcheck --severity=error e2e/*.sh` / `python3 -m py_compile e2e/*.py` /
  `npm run coverage` 地板棘轮）· `vite build` 出产物 ·
  **7 套真机 e2e 共 126 条断言全绿**（26/14/39/12/15/13/7），
  且跑完真实 `~/.claude.json` / `~/.codex/config.toml` 的 md5 未变（沙盒未污染）。
- **反馈到主计划的改动**（→ Phase F）：新增 R15（`/usage` 格式未经真机验证的已知债）；
  账本 `src/accounts.ts` 行补记用量探针是**独立读路径**、不经 IR 的理由。

## 10. 签收（Sign-off）
- [x] 通过代码审计（无阻塞项）
- [x] 通过工程审计
- [x] 主计划已据此更新（含变更记录）
