# 功能计划 — F05 AccountResolver（账号名线通进 LaunchPlan IR）

> 对应主计划 §1 的 F05。本文件是该功能从规划到签收的全程记录。
> **动手前先读 MASTERPLAN §0 核心思想**——推论④「向下兼容 = 少传参数」与 R11 的教训
> （`ccm` 在 `--account`/`--base` 都不传时会静默落 manifest 默认账号）是本功能的直接约束：
> 账号维度一旦能在 CLI 里说话，就**必须每次都显式说话**，不能有"沉默"的第三态。

## 0. 本计划的来源（Phase B 方法论说明）

未开 Plan agent fanout——F05 的目标形态已由 MASTERPLAN §3 账本给定（判别联合 + `isSelectable`
过滤 + 保留 `useBase`），设计空间主要是"如何在不破坏 8 个既有消费方的前提下把账号名线通进
IR"，属于摸清现状后可直接规划的范围（而非 F03/F04 那种需要比较架构方案的开放设计）。

摸清现状做法：先通读 `src/accounts.ts` 全文（440 行，`withAccount` + `resolveFollowAccount` +
`accountConfigDir` + `alignableCurrentAccount` + `currentWorkingAccount`/`effectiveDefault` +
`isSelectable` + `detectAccountMismatch` + `accountColorsActive` 等 8 个关键导出），再开一个
Explore agent精确映射这 8 个导出在全代码库的每个调用点（区分 UI-only vs launch-path），确认
改动范围：

- **`withAccount`**：6 个调用点，**全部**是 launch-path（`resumeTab`/`resumeTabTmuxInner` 两分支/
  `history.ts` 两处/`remote-section.ts` 一处），各自的 `run(configDir)` 回调最终调
  `remote-launch-run.ts` 的某个 executor。
- **`accountConfigDir`**：production 里恰好一个外部调用者——`account-restart.ts`（**独立**
  `fetchAccounts`+`accountConfigDir`调用，不经过 `withAccount`，两者是**并列**两条路径，不是
  一个包一个）。
- **`alignableCurrentAccount`/`currentWorkingAccount`/`isSelectable`/`detectAccountMismatch`/
  `accountColorsActive`/`resolveFollowAccount`**：全部 UI-only（徽章/菜单构建/mismatch 判定/
  chip 展示），或只在 `withAccount` 内部私有调用——**这些不需要改**，只是筛选/展示逻辑，不构造
  启动命令。
- **Rust 侧核实**：`accounts.rs` 只有 3 个只读命令（`list_remote_accounts`/
  `list_remote_session_accounts`/`check_account_trust`），`history.rs`（本地路径）零账号维度
  （MASTERPLAN 自己的账本原话："两套独立逻辑,零账号维度"）——账号解析今天**只存在于 TS/远端侧**，
  F05 不需要顾及并行的 Rust 侧概念（本地路径的账号维度是 F06 的范围）。

**综合过程中发现的真实缺口**（不是任一份既有文档报的，是设计 F05 时核对 F03 的
`ACCOUNT_DIMENSION` 实现才挖出来的，同 F03 综合时挖出 R11 的方式）：见 §2 第 3 条。

## 1. 目标与验收标准（DoD）

- **目标**：把账号**名字**（不只是 `configDir` 路径）线通进 `LaunchContext`/`LaunchPlan`，让
  `ACCOUNT_DIMENSION.cliFlags` 从恒 `null`（F03 遗留的移交点）变成真的吐 `--account <名>` 或
  `--base`；同时把 `accounts.ts` 里"5 个谓词散落"的账号解析升格为一个判别联合返回的纯函数
  `resolveAccount`，供 `withAccount` 内部使用（`account-restart.ts` 的独立解析路径不强行合并，
  见 §2 第 2 条理由）。

- **验收标准**：
  - [ ] `src/accounts.ts`：新增 `AccountResolution = {kind:"account";name;configDir} |
        {kind:"base"} | {kind:"unavailable";requestedName?}` + 纯函数 `resolveAccount(state, opts)`
        （从 `withAccount` 现有内联逻辑抽出，行为逐字节不变）；`withAccount` 改用它，`run` 回调
        签名从 `(configDir?) => Promise<void>` 扩为 `(configDir?, accountName?) => Promise<void>`
  - [ ] `src/launch-plan.ts`：`LaunchAccount` 从 `{kind:"account";configDir}|{kind:"none"}` 改成
        `{kind:"account";name;configDir}|{kind:"base"}`（语义诚实化：这一层永远是"account 或
        base"二选一，不存在"未决定"的第三态——上游 `resolveAccount`/`accountConfigDir` 已经替
        调用方做过这个决定）
  - [ ] `src/launch-dimensions.ts`：`ACCOUNT_DIMENSION.applies` 从"只在 `kind==="account"` 时触发"
        改成**恒 `true`**（两种 kind 都要在 CLI 语境下显式表态）；`cliFlags` 从恒 `null` 改成
        `kind==="account"` 时吐 `["--account", name]`、`kind==="base"` 时吐 `["--base"]`——**永不
        再返回 `null`**（F05 的核心交付：F03 的移交点在此接上）；`apply()` 内部逻辑不变（仍只在
        `kind==="account"` 时才 `export CLAUDE_CONFIG_DIR`，`base` 时无 env op，与今天字节相同）
  - [ ] `src/launch-render-cli.ts`：`CLI_REQUIRED_CAPS` 加入 `"account"`（账号维度从"可能生效可能
        不生效"变成"每次调用都生效"，探测门槛应同步收紧）
  - [ ] `src/launch-requests.ts`：`accountOf(configDir?, name?)` 扩参；**4 个**（不是 5 个——
        `planAttach` 不涉及账号注入，本就没有 `configDir` 参数，attach 从不需要账号维度）
        `planXxx` 函数各加一个**末尾追加**的可选 `accountName?: string` 参数（不打乱既有位置
        参数顺序）
  - [ ] `src/remote-launch-run.ts`：**5 个**（不是 6 个——`runRemoteAttach` 同上无账号参数）
        executor 各加一个**末尾追加**的可选 `accountName?: string` 参数，传给对应 `planXxx`；
        6 个 `withAccount` 调用点（`tabs.ts` ×3/`history.ts` ×2/`remote-section.ts` ×1）的 `run`
        回调改用新的双参签名，把 `accountName` 转传给 executor
  - [ ] `src/account-restart.ts`：`runRemoteResumeTmux` 调用点补传 `accountName`（已知——
        `restartWithAccount` 本来就收到显式 `accountName` 参数，纯粹是把已有数据多传一层，
        **不改**其 `accountConfigDir` 解析逻辑，见 §2 第 2 条）
  - [ ] 门禁：`tsc`/`npm test`/`cargo test`/`test:tmux-target`/`test:ccm-cli`/`test:ccm-acceptance`/
        `test:ccm-print-parity`/`test:tmux-guarded` 全绿；`e2e/resume-cmd-driver.ts`/
        `restart-cmd-driver.ts` 位置参数签名逐字不变（新参数全部是末尾可选，旧调用零改动仍类型检查通过）

- **明确不做什么**：
  - **不强行统一 `withAccount` 与 `account-restart.ts` 的解析路径**——两者失败语义故意不同
    （`withAccount` 不可选时**退化成基座**；`account-restart.ts` 不可选时**中止整个重启**），
    是 F04 之前就已经过审计确认的既有设计（account-restart.ts 头注明确写了这条理由）。合并
    会让一份代码要同时表达两种互斥的失败反应，冒不必要的回归风险换不到实际收益。`account-restart.ts`
    在本轮只做"多传一个已有的 accountName 参数"这一件事。
  - **不做本地路径（history.rs）的账号维度**——那是 F06 的范围，Rust 侧目前"零账号维度"是
    既定现状，不在本轮改变。
  - **不碰 UI 层**（徽章/菜单/chip/设置面板）——本轮只动账号名字如何进入 IR，不动它如何被
    展示；`isSelectable`/`currentWorkingAccount`/`detectAccountMismatch` 等 UI-only 谓词全部
    不改。
  - **不改 `useBase` 的既有语义/存储**——`{kind:"base"}` 与今天 `--base`/"不传账号"的行为完全
    对齐，只是让这个既有语义在 CLI 渲染时也能被诚实表达，不是新增一个概念。

## 2. 与主计划的对接 + 关键决策（附理由）

**触及的共享面**：`src/accounts.ts`、`src/launch-plan.ts`、`src/launch-dimensions.ts`、
`src/launch-requests.ts`、`src/remote-launch-run.ts`、`src/launch-render-cli.ts`、
`src/tabs.ts`（6 处 `withAccount` 调用点之三）、`src/views/history.ts`（之二）、
`src/settings/remote-section.ts`（之一）、`src/account-restart.ts`（1 处补传参数）。

**三处关键决策**：

1. **`resolveAccount` 只重构 `withAccount` 内部,不强推给 `account-restart.ts`**（理由见 §1
   "明确不做什么"第一条）——`AccountResolution` 判别联合本身是干净、可复用的类型，`account-restart.ts`
   未来若要接入也很容易（它已经知道 accountName、只需调 `resolveAccount({explicit: name})` 换掉
   自己的 `accountConfigDir` 直调），但本轮不做，降低单次改动的回归面。

2. **`account-restart.ts` 与 `withAccount` 是并列而非包含关系**——Explore 摸底已确认两者各自
   独立调用 `fetchAccounts`/`accountConfigDir`，不是一个调另一个。这意味着"给 executor 传
   accountName"这件事要在**两条独立路径**上各做一次（`withAccount` 内部 + `account-restart.ts`
   直调处），不能只改一处就自动覆盖两者——已在 §1 验收标准里分别列出。

3. **发现并修复一个真实的、F03 遗留的潜在 R11 同型 bug**（不是任一份既有审计报的，是设计 F05
   时核对 F03 的 `ACCOUNT_DIMENSION.applies` 才挖出来的）：F03 的 `applies` 写成
   `ctx.account.kind === "account"`——即**只有真的选中了具名账号时，这个维度才在 CLI 渲染器
   里"发声"**；`kind==="none"`（今天绝大多数用户的常态：单账号或未装账号库）时，`applies`
   返回 `false`，`canRenderCli` 的"任一维度 `cliFlags` 返回 `null` 就降级"检查**根本不会跑到
   这个维度**（因为 `applies` 已经是 `false`，循环直接跳过）——于是一个解析结果是"基座"的
   plan，只要满足其余 CLI 渲染条件（ccm 已装、容器形态是 create-or-attach…），就会被
   `renderCli` 渲染成一条**既不带 `--account` 也不带 `--base` 的 `ccm resume …` 命令**。这正是
   R11 的病灶复现：远端 shell 若恰好没有继承 `CLAUDE_CONFIG_DIR`（裸新 SSH 会话的常态），
   `ccm` 会走它自己"两者都没传→查 manifest 默认账号"的回退逻辑，把一个"用户/系统解析成基座"
   的意图，静默换成"manifest 里标 `isDefault` 的那个账号"——对多账号用户而言，这可能是**错的
   账号**，且和 R11 一样"看起来生效了（确实起来了），只是号不对"。**这不是假设性风险**——
   F03 上线以来，任何一个"未选账号"的 resume/新建（这是最常见路径）一旦远端 ccm 已装且能力
   齐全，就会经过这条路径。修法：`applies` 恒 `true`，`cliFlags` 对 `kind==="base"` 也吐
   `["--base"]`——让账号维度像 F03 设计之初就该有的那样，**在 CLI 语境下永远显式表态，不存在
   沉默的第三态**，与推论④「向下兼容 = 少传参数」的精神并不冲突：「少传」在 IR 层已经通过
   "不构造 `--account` 分支"实现了，但一旦要走 CLI，就必须把"不传account"翻译成"显式 `--base`"，
   而不是"两者都不翻译"。

**两版历史文档都提前预见、且本功能需要遵循的既有约束**：
- `e2e/resume-cmd-driver.ts`/`restart-cmd-driver.ts` 的位置参数签名不受影响——本功能新增的所有
  参数都是**末尾追加的可选参数**，旧的固定位置调用不需要改一个字符仍类型检查通过。
- `remote-launch.test.ts` 的 15 个符号 import 不受影响——本功能不碰 `remote-launch.ts`。

## 3. 接口 / 契约设计

### 3.1 `src/accounts.ts`：`AccountResolution` + `resolveAccount`

```ts
export type AccountResolution =
  | { kind: "account"; name: string; configDir: string }
  | { kind: "base" }
  | { kind: "unavailable"; requestedName?: string };

/**
 * 纯函数——从 withAccount 现有内联逻辑抽出（显式选号 / 跟随解析两分支），行为逐字节不变。
 * `opts.explicit` 非空 → 显式选号语义（不可选→unavailable，调用方自行决定退化还是中止）；
 * 否则若 `opts.follow` 存在 → 跟随解析（lastAccount → 当前账号 → base）；都不满足 → base。
 */
export function resolveAccount(
  state: AccountsState,
  opts: { explicit?: string | null; follow?: { lastAccount?: string | null } },
): AccountResolution { ... }
```

### 3.2 `withAccount` 的 `run` 回调扩参

```ts
export async function withAccount(
  origin: string,
  accountName: string | null,
  run: (configDir?: string, accountName?: string) => Promise<void>, // 新增第二参数
  opts: { sessionId?: string; onUnselectable?: (name: string) => void;
          follow?: { lastAccount?: string | null } } = {},
): Promise<void> { ... }
```
内部改用 `resolveAccount` 求解，`kind==="account"` 时 `run(configDir, name)`；否则 `run(undefined,
undefined)`——与今天 `run(undefined)` 等价（新增参数缺省 `undefined`，调用方不必判空）。

### 3.3 `LaunchAccount`（`launch-plan.ts`）

```ts
export type LaunchAccount =
  | { kind: "account"; name?: string; configDir: string } // name **可选**——见下方实现期修正
  | { kind: "base" };
```

**实现期修正**（不是本节最初的设计，落实时发现并改正）：本节最初把 `name` 设计成必需字段
（`{kind:"account"; name: string; configDir: string}`），`accountOf(configDir, name)` 也写成
"两者都非空才算 `account`"。这直接撞上 `remote-launch.test.ts`——它直调 `buildResumeDirectCmd`
等老式 builder（只传 `configDir`，不传名字），必须继续正确触发 `export CLAUDE_CONFIG_DIR`（F03
定的"零编辑"硬约束）。改法：`name` 变可选，`accountOf` 触发条件改回"`configDir` 单独非空即算
`account`"（同 F03 原行为）；`ACCOUNT_DIMENSION.cliFlags` 对"账号存在但名字未知"这一具体情形
（老式直调路径的产物）诚实返回 `null`（强制走兜底），而不是把整个账号状态错误降级成 `base`
（那会连兜底渲染器的 env 注入也漏掉，是真回归，不是诚实降级）。见 `doc/INVARIANTS.md` §35。

### 3.4 `ACCOUNT_DIMENSION`（`launch-dimensions.ts`）

```ts
export const ACCOUNT_DIMENSION: LaunchDimension = {
  id: "account",
  order: 20,
  applies: () => true, // 恒真——账号维度永远要在 CLI 语境下显式表态（§2 第3条）
  apply: (plan, ctx) => {
    if (ctx.account.kind !== "account") return; // base：无 env op，字节不变
    if (!isValidConfigDir(ctx.account.configDir)) throw new Error(...);
    plan.env.push({ kind: "export-config-dir", value: ctx.account.configDir });
  },
  // 三分支（实现期修正后的真实形态，见 §3.3）：account 有名字 → 吐 flag；base → 吐 --base；
  // account 无名字（老式直调路径产物）→ null，诚实放弃、强制走兜底。
  cliFlags: (ctx) => {
    if (ctx.account.kind !== "account") return ["--base"];
    return ctx.account.name ? ["--account", ctx.account.name] : null;
  },
};
```

## 4. 实现步骤（严格顺序执行）

- [x] **步骤 1**：`accounts.ts` 加 `AccountResolution`/`resolveAccount`（纯函数，先写单测覆盖
      显式选号命中/不可选、跟随解析三级下沉、都不满足→base 四种情形，逐字节对拍 `withAccount`
      今天的实际决策路径）；`withAccount` 改用它，`run` 回调扩参。
      — 验证：`src/accounts.vitest.ts` 既有断言零改动全绿（证明重构行为保持）；新增
      `resolveAccount` 单测（6 条）。
- [x] **步骤 2**：`launch-plan.ts` 改 `LaunchAccount`（`name` 实现期改为可选，见 §3.3 实现期
      修正）；`launch-dimensions.ts` 改 `ACCOUNT_DIMENSION`（`applies`恒真 + `cliFlags` 三分支：
      account 有名字/base/account 无名字→null）；`launch-requests.ts` 的 `accountOf` 扩参 + 4 个
      `planXxx` 加末尾 `accountName?` 参数。
      — 验证：`launch-dimensions.test.ts`/`launch-render-cli.test.ts` 里所有 `{kind:"none"}`
      改 `{kind:"base"}`；补新用例：`kind==="account"` → `cliFlags` 吐 `--account <名>`；
      `kind==="base"` → `cliFlags` 吐 `--base`（锁死 §2 第3条修复）。**实现期回归**：`name` 最初
      设计成必需字段导致 `remote-launch.test.ts` 3 个测试炸——已改可选并修复，见 §3.3。
- [x] **步骤 3**：`launch-render-cli.ts` 的 `CLI_REQUIRED_CAPS` 加 `"account"`；
      `remote-launch-run.ts` 5 个 executor 加末尾 `accountName?` 参数并传给 `planXxx`。
      — 验证：`launch-render-cli.test.ts` 补一条"未装 account 能力的探测结果 → 即便 kind 是
      base 也强制降级"的测试（防止 `CLI_REQUIRED_CAPS` 漏加）。
- [x] **步骤 4**：`tabs.ts`（3 处）/`history.ts`（2 处）/`remote-section.ts`（1 处）的
      `withAccount` 调用点改用双参 `run` 回调，把 `accountName` 转传进 executor 调用；
      `account-restart.ts` 补传 `accountName`。
      — 验证：`tabs.vitest.ts`/`account-restart.vitest.ts` 既有断言零改动全绿；**Phase D 审计
      发现并补齐**：新增 4 条集成层断言（`tabs.ts` ×2 覆盖显式选号+跟随解析两种模式、
      `history-search-resume.vitest.ts` ×1、`history-actions.vitest.ts` ×1）证明账号真正可选
      时 `accountName` 确实转传到了 `runRemoteResume`/`runRemoteResumeTmux`/`runNewSessionRemote`
      （此前所有断言的 accountName 恒为 `undefined`，接线本身从未被验证过）。`remote-section.ts`
      的对应集成测试因该文件对"开新 Claude"对话框缺乏既有测试脚手架，本轮未补（同一接线模式
      已在另 4 处验证过，标记为有意识的范围收尾，非遗漏）。
- [x] **步骤 5**：`e2e/ccm-print-parity-emit.mts` 的 `{kind:"none"}` 改 `{kind:"base"}`，补一个
      "base 场景"验证 `--base` 真的出现在 `ccm --print` 的真实解析里（跨语言对拍，同 F03 惯例，
      不手搓）。
      — 验证：`npm run test:ccm-print-parity` 10/10 全绿。
- [x] **步骤 6**：`doc/INVARIANTS.md` §33 第1条更新（不再说"F05 移交点"，已落地）+ 新增 §35
      "维度的 applies 绝不能条件性跳过 cliFlags 的 null 安全网"（§2 第3条发现的教训，Phase D
      审计发现计划承诺的这条不变量最初漏写、只留在源码注释里，已补上）；MASTERPLAN §1/§6 更新
      （F05→完成，新增 R13）；两个独立 agent（后端架构 + UX）双审已跑完，均无阻塞项，结论见
      下方 §6。
- [x] **步骤 7**：全量门禁（`tsc`0/`npm test`625/`cargo test`377/`test:tmux-target`26/
      `test:ccm-cli`36/`test:ccm-acceptance`15/`test:ccm-print-parity`10/`test:tmux-guarded`14/
      `resume-suite`17/`restart-suite`24），结果重定向落盘后 Read 核实；`git status` 核对
      `e2e/resume-cmd-driver.ts`/`restart-cmd-driver.ts`/`remote-launch.ts` 零 diff。

## 5. 测试策略

- **黄金串对拍**：`resolveAccount` 抽出前后，`withAccount` 对既有 `accounts.vitest.ts` 全部
  测试用例逐字节同结果（同 F03/F04 的"重实现零回归"纪律）。
- **`--print` 平价预言机扩展**：新增 base 场景验证 `--base` 真的出现在 `ccm` 的真实解析里
  （闭环验证 §2 第3条的修复，不只是 TS 侧单测自证）。
- **回归**：`tabs.vitest.ts`/`history` 相关 vitest/`account-restart.vitest.ts` 既有断言零改动
  全绿（本功能对现有调用点只做"多传一个参数"，不改变已验证行为）；e2e driver 位置参数签名
  不变。

## 6. 代码审计结果（Phase D）

两个独立 agent 并行审（prompt 自包含，各带 MASTERPLAN §0 核心思想全文），均**无阻塞项**。

**后端架构 + 正确性 agent**：`resolveAccount`/`withAccount` 核心决策逻辑逐分支手工重演与老代码
字节对齐；本地重跑 `tsc`/`accounts.vitest.ts`+`tabs.vitest.ts`+`account-restart.vitest.ts`（265
项）/`launch-dimensions.test.ts`/`launch-render-cli.test.ts`/`remote-launch.test.ts`（零 diff）/
`ccm-print-parity.sh`（10/10，含新增 `--base` 断言）全部通过。`account-restart.ts` 的并列路径
边界确认未被侵蚀。发现 2 条重要项，均已修复：
1. **`doc/INVARIANTS.md` §33 第1条已过期**（仍写"F05 移交点"，F05 已落地）+ **计划步骤6承诺的
   新不变量实际没写进 INVARIANTS.md，只留在源码注释里**——已补 §33 更新 + 新增 §35（见步骤6）。
2. **集成层测试覆盖缺口**：6 个 `withAccount` 调用点对应的执行器断言此前全部只覆盖
   accountName 恒为 `undefined` 的场景，`(cd, an) => runRemoteXxx(..., cd, an)` 这行接线本身
   从未被验证过（哪怕写反成 `(cd) => runRemoteXxx(..., cd)` 也会全绿）——已补 4 条集成测试
   （见步骤4）。
「建议」级两条（计划文档"5个planXxx/6个executor"计数应为4/5——已订正；`priorPin` 在
`resolveAccount`/`withAccount` 两处独立计算同一值，判定为低风险的可接受重复，不做改动）。

R11 同型 bug 修复完整性评估（agent 原话精神）：修法完整且已端到端验证——`ACCOUNT_DIMENSION.applies`
恒真后，`canRenderCli` 的 null 检查对 base 场景真正跑了起来，`renderCli`/`--print` 平价预言机
都证实最终命令串确实带 `--base`（真 `ccm --print` 解析通过，非仅 TS 自证）。审计另外三个维度
（identity/env-reset/nested-env-reset）确认它们的 `cliFlags` 无论 `applies` 真假都从不返回
`null`，结构上不可能重蹈这个坑——当前代码库不存在第二个沉默点，但这个"安全"完全依赖每个维度
作者自己写对，没有类型层/lint 层强制，本该承担"防未来重蹈"职责的 INVARIANTS.md 新条目是文档侧
唯一没堵上的洞（已在步骤6补齐）。

**UX agent**：Job A（零回归+测试断言正确性）、Job B（恒发 --account/--base 的现实影响）、
Job C（F09 前瞻兼容性）均无阻塞。Job A 确认 6 个调用点接线字节未变、测试断言逐条对得上各自
语义。Job C 确认 `resolveAccount` 的纯函数接口给 F09 留了干净的缝。发现 1 条重要项：
**`shared/ccm` 的 `--base` 不是无害透传，是无条件 `unset CLAUDE_CONFIG_DIR`**——F05 让每次
"未选账号"的 CLI 调用都携带这个 flag，对"自己在 shell profile 手动管理 CLAUDE_CONFIG_DIR"的
边缘配置用户是一次此前不存在的静默覆盖。**已处理**：登记为 MASTERPLAN 风险表 R13（已接受，
非阻塞）——不发 `--base` 会让 R11 同型 bug 对多数用户复发，代价换收益成立；
`forceLegacyLaunchRenderer` 手动逃生口可退避，不做代码改动。

## 7. 工程审计结果（Phase E）

主线程对账（读 MASTERPLAN §3/§6 + 本功能计划）：F05 落地后主计划仍自洽——§3 账本尚无专属
`src/accounts.ts` 行需要更新的措辞变化（该行"最终形态"描述本就是判别联合，F05 是落地不是改
终态）。R11 的教训在 F05 里被系统性复查了一遍（不只是"F05 本身别踩"，还主动去检查了 F03 遗留
的 `ACCOUNT_DIMENSION` 是否已经踩了同型坑——确实踩了，已修，见 §2 第3条），这正是"账本预见的
重叠现在就优雅重构"这条铁律的体现，不是留给以后打补丁。唯一转发给后续功能的开放项是 R13（已
接受的边缘代价，不是技术债）和 R12（container/agent 轴，F04 就已转发 F09，F05 未新增）。

无新增技术债留给后续功能背负——Phase D 审计发现的 3 条重要项（2 后端 + 1 UX）全部就地修复。
`remote-section.ts` 缺集成测试是唯一有意识留下的收尾缺口，已在步骤4显式记录理由（同一接线
模式已在另 4 处验证），不是模糊的"以后再说"。

## 8. 签收（Sign-off）

- [x] 通过代码审计（无阻塞项；2 条重要项已修复：INVARIANTS 文档缺口、集成层测试覆盖缺口）
- [x] 通过双 agent 架构/UX 审（含对 §2 第3条修复完整性的复核结论：修法完整，代码库无第二个
      沉默点；1 条重要项已处理：R13 风险登记）
- [x] 通过工程审计（主计划仍自洽；R11 教训系统性复查而非局部打补丁；唯一转发项已显式记录）
- [x] 主计划已据此更新（§1 状态、§6 新增 R13、§7 变更记录见下）
