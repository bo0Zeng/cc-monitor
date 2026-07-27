# 功能计划 — F01 tmux 目标精确匹配

> 对应主计划 §1 的 F01。本文件是该功能从规划到签收的全程记录。

## 1. 目标与验收标准（DoD）

- **目标**：把所有 `tmux … -t <target>` 从**前缀/glob 匹配**改成**精确匹配**（`=name:`），
  修掉今天正在生产环境里杀错会话、把按键投进无关 claude 的 bug。

- **背景（真机实测，tmux 3.6，隔离 `-L` socket）**：
  裸 `-t <名>` 依次按「精确名 → **名字开头** → **glob**」解析。只有 `sib-2` 存在时：
  - `kill-session -t sib` → **杀掉 `sib-2`，rc=0**（当成功回报）
  - `send-keys -t sib 'HELLO' Enter` → 投进 `sib-2`
  - `capture-pane -p -t sib` → 抓的是 `sib-2`
  - `kill-session -t 'si*'` → glob 命中并杀掉

  **本仓必然踩**：`pickFreshTmuxName` 刻意造 `cc-<sid8>-2/-3`；终端 `cct` 造 `<dir>_cc-2/-3`。
  **失败链**：`restartWithAccount` 第④步向上一次快照的 `live.name` 发 `Escape`+`/exit`。
  若该会话已自然结束 → 前缀命中 `cc-<sid8>-2` → 把 `/exit` 敲进**另一个还活着的 claude**；
  ④c 的 kill 再把它销毁，输出为空 → 判定成功 → ⑤ 新建 + `recordLastAccount`。
  **净结果：无关会话被静默销毁 + pin 写错，而 UI 报告「已重启」。**

- **修法（真机实测定型，见 MASTERPLAN §5.3）**：统一用 `=name:`。
  `=name`（无尾冒号）**只对 target-session 有效**；`send-keys`/`capture-pane`/`set-option`/`show-options`
  收的是 pane 目标，`=name` 会 **rc=1 完全失效**。`=name:` 是唯一通用且精确的形式。

- **验收标准**：
  - [x] Rust 3 处命令构造（`capture-pane` / `kill-session` / `send-keys`）目标为 `'=<name>:'`
  - [x] TS `session-backend.ts` 全部 8 处 `-t` 目标为 `=<name>:`（引号内），`new-session -s` **不变**
  - [x] glob 第二道防线（D 审计后拆分：`isValidNewTmuxName` **仅创建路径**禁 glob；attach 走宽松谓词，见 §6 I2）
  - [x] e2e shim `restart-shims/core.mjs` 的 tmux 调用同步用 `=name:`（保持与生产同构）
  - [x] **真机行为验收表全绿**（26/26，已升为常设门禁 `npm run test:tmux-target`）（本功能的核心门禁，见 §5）：以**生产命令串**为输入，在隔离 socket 上
        验证「命中真名 / 不误伤兄弟名 / 不被 glob 命中 / 缺失时 rc≠0」
  - [x] 回归测试钉死：Rust 单测 + TS 黄金串
  - [x] 门禁：`npm test` 全绿（tsx 黄金串 + vitest）、`cargo test` 全绿、`tsc` 0

- **明确不做什么**：
  - 不动 `is_ccm_tmux_name` 的身份语义（三道门是 F04 的事）
  - 不动 `new-session -s <名>`（收的是名字不是目标）
  - 不动 `tmux ls -F` / `TMUX_LS_FMT`（daemon 双写点，红线）
  - 不动 `shared/ccm-wrapper.sh:24` 的 `set-option @ccm_sid`（无 `-t`，从会话**内部**设，本就精确）
  - 不引入 LaunchPlan IR（F03）

## 2. 与主计划的对接

- **触及的共享面**（对照 §3 账本）：
  - `src/session-backend.ts` —— 账本最终形态：「所有 `-t` 走 `exactTarget()` 产出 `=name:`」。本功能**落地这一条**。
  - `src-tauri/src/tmux.rs` —— 账本最终形态含三条：`=name:` / 三道门 / 单条原子命令。
    本功能**只做第一条**，另两条留 F04。不在此处提前动身份判定（那会与 F04 抢同一段代码并制造补丁叠补丁）。
  - `src/remote-launch.ts` —— 本功能只碰 `isValidTmuxName` 一个纯函数，不碰 builder 结构（留给 F03）。
- **遵循的最终形态**：`exactTarget()` / `exact_target()` 做成**单一 helper**，将来 F03 的渲染器直接复用；
  绝不在 7 个调用点各自拼 `=`。
- **新引入的共享面**：无。
- **边界**：不越界到 F03/F04 的结构改动。

## 3. 接口 / 契约设计

```ts
// src/session-backend.ts —— target 可能是裸名或已 posixQuote，`=` 与 `:` 必须落在引号内
function exactTarget(target: string): string {
  return target.startsWith("'") && target.endsWith("'") && target.length >= 2
    ? `'=${target.slice(1, -1)}:'`   // 'cc-x'      → '=cc-x:'
    : `=${target}:`;                  // cc-s1       → =cc-s1:
}
// 含转义的情形：'a'\''b'  → '=a'\''b:'  → shell 解析为 =a'b:
```

```rust
// src-tauri/src/tmux.rs
fn exact_target(target: &str) -> String {
    ssh_source::shell_quote(&format!("={target}:"))
}
```

**为什么是 `=name:` 而不是 `=name`**（写进 doc 注释，防将来"简化"掉）：
`=` 前缀只在 target-**session** 解析路径上被识别；`send-keys`/`capture-pane` 收 target-**pane**，
`set-option`/`show-options` 走 pane 解析后上溯——这些路径上 `=name` 直接 `can't find pane`，rc=1。
尾冒号把字符串强制成 `session:`（当前 window、活动 pane）形态，于是 `=` 落在会话名段上被正确识别。
实测矩阵见 MASTERPLAN §5.3。

## 4. 实现步骤（严格顺序执行）

- [x] **步骤 1**：`session-backend.ts` 加 `exactTarget()` + 完整 doc 注释（记录实测矩阵与"为什么带冒号"）。
      7 处 `-t` 全改；`new-session -s` 保持裸 `target`。
      — 验证：`tsc` 0；肉眼核 diff 里 `-s` 未被误改。
- [x] **步骤 2**：`tmux.rs` 加 `exact_target()` + doc 注释；3 处命令构造改用它。
      — 验证：`cargo check` 0。
- [x] **步骤 3**：`remote-launch.ts` 的 `isValidTmuxName` 正则加 `*` `?`，补 doc 说明「第二道防线」。
      — 验证：`tsc` 0。
- [x] **步骤 4**：**先写失败回归测试再看**（bug 修复纪律）：
      Rust 加 `tmux_targets_use_exact_match`（断言含 `-t '=cc-abc12345:'`、**不含**裸 `-t 'cc-abc12345'`、
      且 `exact_target` 对 `cc-x` / `proj_cc-2` / 含引号名的输出）；
      TS 加 `exactTarget` 单测（裸名 / 已引号 / 含转义引号三态）+ `isValidTmuxName` 拒 glob。
      — 验证：先跑一次确认**新测试红**（证明它们真的在测东西），再进步骤 5。
- [x] **步骤 5**：黄金串 re-baseline —— `src/remote-launch.test.ts` / `src/session-backend.test.ts`。
      **机械替换但必须逐条核**：只改 `-t` 后的目标，`-s` 一律不动。
      — 验证：`npm test` 全绿；`git diff` 里 `-s` 零改动。
- [x] **步骤 6**：e2e shim `e2e/restart-shims/core.mjs` 的 tmux 参数同步 `=name:`。
      — 验证：e2e 套件仍绿（若本机跑不动则记录并说明，不算通过）。
- [x] **步骤 7**：**真机行为验收**（本功能的核心门禁）——写 `scratchpad` 脚本：
      从**真 builder** 取生产命令串 → 抽出每个 `tmux <verb> … -t <target>` → 在隔离 `-L` socket 上
      对「真名 + 兄弟名 `-2` + glob 名」三种布局逐一执行 → 落盘结果 → Read 核实。
      — 验证：§5 的表格全绿。
- [x] **步骤 8**：三门禁 `npm test` / `cargo test` / `tsc`，结果重定向到文件后 Read 核实（`pipefail`）。

## 5. 测试策略

- **单元**：`exactTarget` / `exact_target` 三态；`isValidTmuxName` 拒 glob。
- **黄金串**：`remote-launch.test.ts` / `session-backend.test.ts` re-baseline，钉死 `-t` 形态与 `-s` 不变。
- **回归**：Rust `tmux_targets_use_exact_match` —— 显式断言**不含**裸目标，防将来被"简化"回去。
- **真机行为验收表**（新增硬门禁，MASTERPLAN §5.2）：

  | 布局 | 动词 | 目标 | 期望 |
  |---|---|---|---|
  | 只有 `cc-p1-2` | `kill-session` | `=cc-p1:` | rc≠0，`cc-p1-2` **存活** |
  | 只有 `cc-p1-2` | `send-keys` | `=cc-p1:` | rc≠0，探针文件**无内容** |
  | 只有 `cc-p1-2` | `capture-pane` | `=cc-p1:` | rc≠0 |
  | 只有 `cc-p1-2` | `set-option` | `=cc-p1:` | rc≠0 |
  | `cc-p1` + `cc-p1-2` 并存 | `send-keys` | `=cc-p1:` | rc=0，**只有 `cc-p1`** 收到 |
  | `cc-p1` + `cc-p1-2` 并存 | `kill-session` | `=cc-p1:` | rc=0，`cc-p1-2` **存活** |
  | `cc-p1` + `cc-p1-2` 并存 | `set-option`+`show-options` | `=cc-p1:` | 读回自己写的值 |
  | 存在 `alpha` | `kill-session` | `=a*a:` | rc≠0，`alpha` **存活**（glob 不生效） |

  这张表以**生产命令串**为输入，不是手搓的等价命令——上一轮翻车正是因为只验了手搓形状。

- **本功能覆盖率 / 门禁**：`npm test` + `cargo test` + `tsc` 全绿，且上表全绿。
- **修 bug 纪律**：步骤 4 先确认新测试红，再改实现让它绿。

## 6. 代码审计结果（Phase D）

两个并行 agent（正确性/行为等价 · 计划符合度/架构符合度），均**独立复跑**了真机验收与三门禁。

**阻塞：无。** 两方都确认：`-t` 位点无遗漏（Rust 3 + TS 8）、`-s` 零误改、`exactTarget` 三态引号处理
经手工推演 + shell/tmux 实证正确（含 `'ab'\'''` / `''\''abc'` 这类刁钻形态）、黄金串 re-baseline 无机械
替换误伤、`shared/ccm-wrapper.sh` 的无 `-t` `set-option` 正确地未被动、daemon 零改、老会话（`cc-*` 无
`@ccm_sid`）仍可 kill/send-keys。

**重要（已全部修复并复验）**：

| # | 发现 | 处置 |
|---|---|---|
| I1 | Rust 3 个 `-t` 位点只钉死 1 个：`capture_remote_pane`/`kill_remote_tmux` 的命令串内联在 `async fn` 的 `format!` 里，改回裸目标 `cargo test` 依旧全绿——而 `kill_remote_tmux` 正是失败链的破坏性一端 | 抽 `build_capture_pane_cmd` / `build_kill_session_cmd` 纯函数，`tmux_targets_use_exact_match` 改为三点全钉（含显式「不含裸目标」断言）。F04 的原子命令本来就要动这两处，提前抽零浪费 |
| I2 | `isValidTmuxName` 加禁 glob 是 **attach 路径上的行为回归**：它同时把守 `buildAttachCmd`，而那里的输入是用户自己已存在的会话名；tmux 允许 `st*ar`，且 `=名:` 已彻底关闭 glob 这一级（实测 `-t '=st*ar:'` rc=0 且精确）→ 禁它挡不住任何东西，只把可用变 throw | 拆成两个谓词：`isValidTmuxName`（attach，不禁 glob）+ `isValidNewTmuxName`（**仅创建**，禁 glob）。`buildLauncherCmd` 改用后者 |
| I3 | glob 禁令**零测试**：删掉正则里的 `*?`，`npm test` 全绿 | 补 3 条用例（谓词三态 + `buildLauncherCmd` throw + `buildAttachCmd` 放行） |
| I4 | `e2e/resume-suite.sh:101` 与 `e2e/resume-daemon-frames.sh:115` 的断言 `grep -q "send-keys -t $S1 "` 结构性必红 | re-baseline 为 `=$S1: ` |
| I5 | e2e 的 shell **探针本身**仍用裸 `-t`：`gen-idle-tmux.sh:23` 的 `set-option -t "$SESSION"` 会把 `@ccm_sid` 写到**错的会话**上直接污染 fixture；`has-session -t "$1"` 只剩兄弟时返 0 → 存活断言假阳 | 7 个 e2e 脚本共 19 处探针全部改 `=名:`。理由：F01 的整个论点就是「前缀匹配会说谎」，探针不能例外 |
| I6 | `=名:` 现编码在**三处**（TS 座 / Rust / e2e shim），无任何对齐门禁。仓库对同类跨语言双写有立条先例（INVARIANTS I8 `TMUX_LS_FMT`） | 立 **INVARIANTS §31a**；`session-backend.test.ts` 加漂移守卫（读 `core.mjs` 断言同构 + 断言不得留裸目标）。shim 是 IPC 边界 mock、结构上无法 import Rust，**去重不可能，只能守卫** |
| I7 | 真机验收表有 3 个洞：场景 A 缺 `set-option` 行；`attach` 在无 tty 下必失败、判不出精确性（伪证据）；`launcher`（posixQuote 名路径）生成了却从未执行 | 全补：加 `set-option` rc≠0 + 兄弟未被写入；加场景 E 用 `script(1)` 造 pty 验 attach **正例**；加 `launcher` 生产串执行。表从 19 项扩到 **26 项** |

**建议（记档，不在 F01 处置）**：
- 空 target 的 `=:` 会落到「当前会话」——**不是本次回归**（改前 `-t ''` 同样如此），且 kill/send-keys 有
  `is_ccm_tmux_name` 门（要求 `cc-` 前缀，空串进不来）；唯一无门的 `capture_remote_pane` 是只读路径。
  → 归入 **F04 的 `is_safe_tmux_target`（闸门1 恒强制）**，已记进 MASTERPLAN §3 账本。
- `exactTarget` 用字符串形状嗅探区分「已引号/裸名」，今天全分支安全，但 F03 让 IR 渲染器接管后
  多一条「传裸名且首尾恰为 `'`」的路径就会静默打错目标 → **F03 改判别式入参**，已记进账本。
- 账本原措辞「所有 `-t` 走 `exactTarget()`……由 IR 渲染器调用」若字面执行（导出给渲染器直呼）会破
  INVARIANTS §31①「前端绝不硬编码后端命令」→ 已改措辞为「渲染器经 `SessionBackend` 接口取命令」。
  代价是步骤 4 的 TS helper 直接单测写不了（helper 不导出），三态实际由黄金串完整覆盖（裸名 / 引号名 /
  含转义引号各一条），这是合理取舍。
- 用户可见变化：目标会话已自然结束但存在兄弟 `-2` 时，换号重启从「静默成功（实为杀错）」变成明确的
  「重启已中止」。是修复的正确表现，值得写进发版说明。

**red-first 证据**（步骤 4 要求）：把 `exact_target` 临时改回 `shell_quote(target)` 后跑 `cargo test --lib tmux::`
→ `send_keys_cmd_construction` 与 `tmux_targets_use_exact_match` **双双 FAILED**（9 passed / 2 failed），
恢复后 11/11 绿。另在真 tmux 上直接对照：裸 `send-keys -t cc-p1` rc=0 且 marker 落进 `cc-p1-2`；
`-t '=cc-p1:'` rc=1 零投递。

## 7. 工程审计结果（Phase E）

- **主计划是否仍自洽**：是。F01 只落地账本三条最终形态里的第一条（`=名:`），未触碰 F04 的身份三道门与
  原子命令、未引入 LaunchPlan IR。`git diff` 核实 `is_ccm_tmux_name` / `TMUX_LS_FMT` / `tmux ls -F` /
  `new-session -s` / `shared/ccm-wrapper.sh` / `remote-daemon-proto/` 全部零改。
- **是否引入拖累后续的债**：无。目标语法从散落字面量收敛为「每条执行通道一个所有者」（shell 渲染面 =
  `session-backend.ts`，IPC 面 = `tmux.rs`），11 个调用点零手拼；F04 可直接复用 `exact_target`，且
  `show-options -v` 未设置时 rc=1 的 fail-closed 前提已实测就位。
- **账本预见的重叠 → 现在就做的统一重构**（铁律 6）：真机验收 harness 原本只在 scratchpad。MASTERPLAN
  §5.2 已把它定成**每个碰 tmux/shell 的功能都要过的常设门禁**，F02/F04 马上要用 → **收进仓库**：
  `e2e/tmux-target-emit.mts`（从真 builder 取生产串）+ `e2e/tmux-target-acceptance.sh` + `npm run test:tmux-target`。
  不这么做，F02/F04 各自再搓一遍就是账本要防的那种补丁叠补丁。
- **工程健康度**：三门禁全绿（tsc 0 / npm test 41 文件 598 测试 + 13 个 tsx 套件 / cargo 369）；
  e2e `restart-suite` 24 ok、`resume-suite` 17 ok、`resume-daemon-frames` 7 ok，全 rc=0；
  `npm run test:tmux-target` 26/26。文档-代码无漂移（新立 INVARIANTS §31a 与实现同步）。
- **反馈到主计划**：账本 3 条措辞修正 + §5.2 增列常设门禁 + F03/F04 各接一条来自审计的约束（见 MASTERPLAN 变更记录 03）。

## 8. 签收（Sign-off）

- [x] 通过代码审计（无阻塞项；7 项重要发现全部修复并复验）
- [x] 通过工程审计
- [x] 主计划已据此更新（含变更记录 03）
