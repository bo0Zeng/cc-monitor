# 功能计划 — F07 每账号默认模型（维度注册表的架构验收：第一个真实新维度）

> 对应主计划 §1 的 F07。本文件是该功能从规划到签收的全程记录。
> **动手前先读 MASTERPLAN §0 核心思想 + §4 第4条**：「F07 刻意排在此当**架构验收**——加『每
> 账号模型』若做不到零改 builder/CLI 主体，说明 F03/F02 没做到位，回炉」。本功能的成败标准
> 不是"模型选择好不好用"，是"加一个全新维度，`buildLaunchPlan`/两个渲染器的**主体结构**要
> 零改"——`launch-dimensions.ts` 头注自己写了这句承诺（"加一个新维度（如 F07 的 `model`）=
> 往 `LAUNCH_DIMENSIONS` 数组追加一条注册 + `LaunchContext` 加一个可选字段，零改
> `buildLaunchPlan`、零改两个渲染器主体结构"）——本功能就是来兑现这句话的。

## 0. 本计划的来源（Phase B 方法论说明）

代码库里**没有任何"model"相关的既有实现**（已核实：`accounts.rs`/`accounts_manifest.rs`/
`accounts_query.rs`/`shared/ccm` 均零 model 字面量），"每账号默认模型"具体该长什么样是一个
从零开始的设计问题——但开了一个 Explore fork 之后发现：**这个设计问题在更早的 `account-onboarding`
工作区已经有过一轮规划**（`account-onboarding/MASTERPLAN-v2.md:75,18`），语义与落点已经定了
大半，本功能不是从空白画布起步，是把一个搁置的既定方向接上 unify-launch 的 IR 架构——因此
**未开 Plan agent fanout**，直接依据既有先例 + 现状约束规划。

**Explore fork 的关键证据**：

- **语义先例**：`account-onboarding/MASTERPLAN-v2.md:75`——"账号→模型映射存 cc-monitor，起
  会话时经 `LaunchSpec.extraEnv` 注入 `ANTHROPIC_MODEL`"。即：这是 Claude Code 自己认的
  `ANTHROPIC_MODEL` 环境变量机制（不是 cc-monitor 发明的新概念），映射关系**存在 cc-monitor
  自己这边**（不是远端 manifest/daemon 的字段）。
- **`shared/ccm` 核实**：零 `--model`/`ANTHROPIC_MODEL` 处理——`ccm` 今天完全不知道"模型"这个
  概念,`--account`/`--base` 是仅有的账号相关 flag。
- **本机已有的"本地单值配置"先例**：`src/accounts.ts` 的 `getDefaultName`/`setDefaultName`
  （`accounts.ts:286-315`）把"这台 cc-monitor 起新会话默认用哪个账号"存在**本机 `config.json`
  的 `accounts.defaultName`**——一个纯本地、不经 daemon/manifest 的账号相关偏好存储先例。
  "每账号模型"结构上是同一类东西（本地偏好，按账号名索引），可以照抄这个模式而不是另起一套。
- **`EnvOp` 是窄类型**（`launch-plan.ts:53-55`，头注：F03 故意拒绝通用 `{op:"export";key;value}`
  形态，防任何维度绕开校验塞任意变量名进命令——呼应账号隔离审计 D7 的教训）。任何新维度要注入
  环境变量，必须加一个**具名**的新 `EnvOp` 变体，不能走通用 export。
- **`renderEnvOps`**（`launch-render-fallback.ts:16-24`）是一个小的、自包含的 `switch`——加一个
  新分支是成比例的、可控的增量，不是"改渲染器主体结构"。

## 1. 目标与验收标准（DoD）

- **目标**：允许用户给每个账号配置一个默认 Claude 模型（`ANTHROPIC_MODEL` 环境变量值），起
  会话时若该账号配了模型偏好，就在命令里带上；实现方式必须是**往 `LAUNCH_DIMENSIONS` 追加一条
  新维度注册**，不改 `buildLaunchPlan`/两个渲染器的既有分支结构——这是本功能存在的理由。

- **验收标准**：
  - [x] `src/accounts.ts`：新增本机 `config.json` 字段 `accounts.modelByAccount:
        Record<accountName,string>`（结构上是 `defaultName` 的复数版：按账号名索引而非单值）；
        新增 `getModelForAccount(name): Promise<string|undefined>`/`setModelForAccount(name,
        model: string|null): Promise<void>`（`null`=清除该账号的偏好，同 `setDefaultName` 的
        清除语义）
  - [x] `withAccount` 的 `run` 回调再扩一个**末尾**参数：`(configDir?, accountName?,
        modelOverride?) => Promise<void>`——`resolution.kind==="account"` 时额外
        `await getModelForAccount(resolution.name)` 传入；`account-restart.ts` 的独立解析路径
        也补同一次查询（同 F05 对 `accountName` 的两条路径分别补传的模式，理由同 F05 §2 第2条：
        两条路径是并列而非包含关系）
  - [x] `src/launch-plan.ts`：`EnvOp` 新增变体 `{kind:"export-model";value:string}`；
        `LaunchContext` 新增可选字段 `modelOverride?: string`
  - [x] `src/shell-quote.ts`：新增 `isValidModelName(name): boolean`（白名单
        `/^[A-Za-z0-9._-]{1,128}$/`——覆盖"claude-opus-4-5-20260101"这类完整 ID 与"opus"这类
        简写别名，拒一切 shell 元字符）
  - [x] `src/launch-dimensions.ts`：新增 `MODEL_DIMENSION`（`order: 25`，卡在 `account`(20) 与
        `nested-env-reset`(30) 之间——语义上"模型是账号的一个细化"，导出顺序上"账号目录先、
        模型偏好次、嵌套清理最后"）：
        - `applies: (ctx) => !!ctx.modelOverride`（**不是**恒真——§2 第1条附推理，说明这与
          F05 修的 R11 同型坑不是同一类问题，不能照搬"恒真"）
        - `apply`：校验 `isValidModelName`（非法即 throw，拒绝拼入命令），push
          `{kind:"export-model", value: ctx.modelOverride}`
        - `cliFlags: () => null`——`ccm` 今天没有 `--model`，任何配了模型偏好的会话诚实强制走
          兜底渲染器（**不是**遗漏，是`shared/ccm` 缺这个能力的诚实反映；`ccm` 补 `--model` 是
          F08 的范围，见 §2 第2条）
        - `assertDimensionOrderInvariants` 加一条：`idx("account") < idx("model") <
          idx("nested-env-reset")`
  - [x] `src/launch-render-fallback.ts`：`renderEnvOps` 加一个分支：
        `op.kind === "export-model" ? \`export ANTHROPIC_MODEL=${posixQuote(op.value)}; \` : ...`
  - [x] `src/launch-requests.ts`：4 个（`planResumeDirect`/`planResumeTmux`/
        `planResumeIntoExistingTmux`/`planLauncher`——同 F05 排除 `planAttach` 的理由：attach
        不启动任何东西,不需要模型维度）各加一个**末尾**可选参数 `modelOverride?: string`，塞进
        `ctx.modelOverride`
  - [x] `src/remote-launch-run.ts`：对应 5 个 executor 各加末尾可选参数 `modelOverride?:
        string`，转传给 `planXxx`；`tabs.ts`/`history.ts`/`remote-section.ts` 的 6 个
        `withAccount` 调用点改用三参 `run` 回调并转传 `modelOverride`；`account-restart.ts` 补传
  - [x] `src/settings/accounts-section.ts`：`accountRow()` 新增一个可编辑的"默认模型"字段
        （自由文本输入——模型 ID 会随时间变化，不硬编码枚举；空 = 跟随该账号自身默认，不下发
        override）,读写 `getModelForAccount`/`setModelForAccount`
  - [x] `doc/INVARIANTS.md`：记一条"新维度落地样例"——`MODEL_DIMENSION` 是 F07 的架构验收产物，
        往后任何新维度都应照它的形状（`applies` 条件式而非恒真、`cliFlags` 诚实 `null`、
        `order` 卡在既有维度之间并更新不变量断言）；同时记 §2 第1条的"`applies` 条件式 vs 恒真"
        判断依据，防未来某个新维度作者把 F05 的"恒真"教训错误地当成万能模板照抄
  - [x] 门禁：`tsc`/`npm test`/`cargo test`（预期零受影响，本功能不碰 Rust）/`test:ccm-cli`/
        `test:ccm-acceptance`/`test:ccm-print-parity`/`test:tmux-target`/`test:tmux-guarded`/
        `resume-suite`/`restart-suite` 全绿；`remote-launch.test.ts`/两个 e2e driver 零 diff
        （本功能不碰这三者——`remote-launch.ts` 的 7 个老式 builder 从未有模型概念，不需要跟着改）

- **明确不做什么**：
  - **不教 `ccm` 认识 `--model`**——`cliFlags` 恒 `null` 是刻意的诚实降级，不是缺口；给 `ccm`
    加 `--model` flag（连带需要真机验收脚本）留给 F08（终端集成打磨），本功能只做"IR + 兜底
    渲染器能表达"这一半。
  - **不把模型偏好存进远端账号 manifest / 经 daemon 分发**——Explore fork 已论证：manifest 是
    跨机器共享的账号真相源，牵一发要动 daemon schema + `cc-acct-iso` vendor 脚本，与"架构验收"
    这个目标不成比例；本机 `config.json` 是 cc-monitor 自己的、不需要跨机器同步的偏好存储，
    `defaultName` 已经是这个模式的先例，不重新发明。
  - **不给本地路径（F06 的 Windows PowerShell）加模型维度**——`MODEL_DIMENSION` 只在
    `LaunchContext.modelOverride` 被设置时触发；本地路径的 `LaunchContext` 构造点（F06）恒不设
    这个字段（本地会话没有"账号"概念，自然也没有"账号的模型偏好"这个下游概念），不是本功能
    需要处理的分支。
  - **不做模型名称的枚举/校验"这是不是一个真实存在的 Claude 模型"**——`isValidModelName` 只做
    shell 注入安全校验（charset 白名单），不做语义校验（远端 `claude` 自己会在模型名不存在时
    报错，这是它的职责不是 cc-monitor 的）。

## 2. 与主计划的对接 + 关键决策（附理由）

**触及的共享面**：`src/accounts.ts`、`src/launch-plan.ts`、`src/launch-dimensions.ts`、
`src/shell-quote.ts`、`src/launch-render-fallback.ts`、`src/launch-requests.ts`、
`src/remote-launch-run.ts`、`src/tabs.ts`/`src/views/history.ts`/`src/settings/remote-section.ts`
（`withAccount` 调用点）、`src/account-restart.ts`、`src/settings/accounts-section.ts`（UI）。
**不触及** `buildLaunchPlan`（`launch-plan.ts` 里那个函数本身）、`renderCli`/`canRenderCli` 的
分支结构、`renderFallback` 的分支结构——这正是 DoD 要验收的"零改主体"。

**两处关键决策**：

1. **`MODEL_DIMENSION.applies` 是条件式（`!!ctx.modelOverride`），不是像 F05 修完的
   `ACCOUNT_DIMENSION` 那样恒真**——这两者看似同构（都是"账号相关的维度"）但**问题结构不同**，
   不能机械照搬 F05 的教训：
   - F05 的坑是"最常见场景（未选账号=base）静默地不表态，导致远端 `ccm` 落到 manifest 默认
     账号——一个和用户期望不同的身份"。危险在于"沉默 = 意外身份切换"。
   - 模型偏好没有这个坑：`applies` 为 `false`（没配模型偏好）时，远端 `claude`
     直接用它自己已经配置好的默认模型——这**正是**用户没配置 override 时应该发生的事，不是
     "意外切换成了别的模型"，是"没有 override，用你原本就在用的"。这里的"沉默"和"不表态"
     是同一件事，不存在"沉默≠不表态"的裂缝。
   - 结论：`applies` 该不该恒真，取决于"这个维度的默认态（不触发）是否等价于用户的期望"，不是
     "这个维度是不是账号相关"。**这条判断依据本身写进 §1 验收标准的 INVARIANTS.md 新增条目**，
     防未来重蹈"看见"账号"两个字就照抄恒真"的机械式误用。

2. **`cliFlags` 恒 `null`（配了模型偏好时）是本功能故意接受的、可见的降级，不是缺口**——效果
   等价于 F03 刚落地、F05 还没做时的 `ACCOUNT_DIMENSION`：任何触发这个维度的会话，CLI 渲染器
   路径整体降级到兜底渲染器。**这不重蹈 F05 修的那个坑**，因为坑的本质是"`applies` 恒假导致
   null 检查根本跑不到"（结构性检测不到）；这里 `applies` 会在配了偏好时正确变真，`canRenderCli`
   的 null 检查**确实会跑到**并正确返回 `false`——是"检测到了、诚实报告降级"，不是"检测不到、
   悄悄放过"。两者外观相似（"配了模型的会话不走 CLI 渲染器"）但机制完全不同（一个是设计使然
   的显式降级，另一个曾是隐藏的检测盲区）。

## 3. 接口 / 契约设计

### 3.1 `src/accounts.ts`：模型偏好存取（照抄 `defaultName` 模式）

```ts
const modelCfgKey = "modelByAccount";

export async function getModelForAccount(name: string): Promise<string | undefined> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  const a = cfg[CFG_KEY];
  if (a && typeof a === "object") {
    const map = (a as Record<string, unknown>)[modelCfgKey];
    if (map && typeof map === "object") {
      const v = (map as Record<string, unknown>)[name];
      if (typeof v === "string" && v) return v;
    }
  }
  return undefined;
}

export async function setModelForAccount(name: string, model: string | null): Promise<void> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  const prev = (cfg[CFG_KEY] && typeof cfg[CFG_KEY] === "object" ? cfg[CFG_KEY] : {}) as Record<string, unknown>;
  const prevMap = (prev[modelCfgKey] && typeof prev[modelCfgKey] === "object" ? prev[modelCfgKey] : {}) as Record<string, string>;
  const nextMap = { ...prevMap };
  if (model) nextMap[name] = model; else delete nextMap[name];
  cfg[CFG_KEY] = { ...prev, [modelCfgKey]: nextMap };
  await saveConfig(cfg);
}
```

### 3.2 `withAccount` 三参 `run`

```ts
run: (configDir?: string, accountName?: string, modelOverride?: string) => Promise<void>
```
`resolution.kind === "account"` 时：`const modelOverride = await getModelForAccount(resolution.name);`
与已有的 `fetchAccounts` 调用同批 await，不额外增加一次串行等待（可 `Promise.all`，视实现时
是否有额外依赖顺序决定，不强求）。

### 3.3 `MODEL_DIMENSION`（`launch-dimensions.ts`）

```ts
export const MODEL_DIMENSION: LaunchDimension = {
  id: "model",
  order: 25, // account(20) < model(25) < nested-env-reset(30)
  applies: (ctx) => !!ctx.modelOverride, // 条件式而非恒真，理由见 §2 第1条
  apply: (plan, ctx) => {
    if (!ctx.modelOverride) return;
    if (!isValidModelName(ctx.modelOverride)) {
      throw new Error(`非法模型名（拒绝拼入命令）: ${JSON.stringify(ctx.modelOverride)}`);
    }
    plan.env.push({ kind: "export-model", value: ctx.modelOverride });
  },
  cliFlags: () => null, // ccm 无 --model；诚实强制走兜底，见 §2 第2条
};
```
`LAUNCH_DIMENSIONS` 数组追加这一条（`.sort` 已按 `order` 排，无需手动插入位置）；
`assertDimensionOrderInvariants` 加 `idx("account") < idx("model")` + `idx("model") <
idx("nested-env-reset")` 两条断言。

## 4. 实现步骤（严格顺序执行）

- [x] **步骤 1**：`shell-quote.ts` 加 `isValidModelName`；`launch-plan.ts` 加 `EnvOp` 新变体 +
      `LaunchContext.modelOverride`。
      — 验证：纯类型/纯函数改动，`tsc` 通过即可，无需专门测试（`isValidModelName` 的行为在
      步骤2 随 `MODEL_DIMENSION` 的测试一起覆盖）。
- [x] **步骤 2**：`launch-dimensions.ts` 加 `MODEL_DIMENSION` + 两条新顺序不变量断言；
      `launch-render-fallback.ts` 的 `renderEnvOps` 加 `export-model` 分支。
      — 验证：`launch-dimensions.test.ts` 新增：`applies` 在有/无 `modelOverride` 两态下正确
      （反证"不是恒真"）；`apply` 对合法模型名推入正确 `EnvOp`、对非法模型名 throw；`cliFlags`
      恒 `null`；故意错序验证新增的两条不变量断言真的会 throw（同现有"错序验证"测试模式）。
      新增一条"buildLaunchPlan + renderFallback 整体golden串"测试，直接断言渲染出的字符串里
      精确含 `export ANTHROPIC_MODEL='opus'; `子串（子串位置在 `export CLAUDE_CONFIG_DIR`
      之后、启动命令之前——锁 order=25 的实际效果，不只是锁孤立的 `apply()` 输出）。
- [x] **步骤 3**：`accounts.ts` 加 `getModelForAccount`/`setModelForAccount`；`withAccount` 扩
      三参回调（内部 await 新查询）。
      — 验证：新增读写往返测试（设置→读回一致、清除→读回 `undefined`、多账号互不影响）；
      `withAccount` 既有断言从两参改三参（同 F05 步骤1 的模式：先跑一遍确认零回归，再加新维度
      的断言）。
- [x] **步骤 4**：`launch-requests.ts` 4 个 `planXxx` 加 `modelOverride?` 末尾参数；
      `remote-launch-run.ts` 对应 5 个 executor 加末尾参数并转传。
      — 验证：`launch-render-cli.test.ts`/`launch-dimensions.test.ts` 已有断言零改动全绿（新
      参数全部末尾可选，旧调用不受影响）。
- [x] **步骤 5**：`tabs.ts`/`history.ts`/`remote-section.ts` 的 6 个 `withAccount` 调用点改用
      三参回调并转传 `modelOverride`；`account-restart.ts` 补查询+补传。
      — 验证：对应 vitest 文件既有断言零改动全绿；新增至少 1 条集成测试证明"配了模型偏好的
      账号 resume → executor 真的收到 `modelOverride`"（同 F05 Phase D 补的接线验证模式，不
      等审计发现了才补）。
- [x] **步骤 6**：`accounts-section.ts` 的 `accountRow()` 加模型输入框，读写新函数。
      — 验证：手动检查（或若该文件已有 jsdom 测试脚手架，补一条最小交互测试）；至少确认
      `npm run test:dom` 全绿（不因新增 DOM 元素破坏既有断言）。
- [x] **步骤 7**：`doc/INVARIANTS.md` 补记新维度样例 + §2 第1条判断依据；双 agent 审（后端架构 +
      UX，prompt 自包含带 MASTERPLAN §0 全文，UX agent 额外核对§2 两条决策的说服力）；
      MASTERPLAN §1/§3/§7 更新；全量门禁；commit。

## 5. 测试策略

- **架构验收本身就是测试标的**：`git diff` 核对 `buildLaunchPlan`/`renderCli`/`renderFallback`
  三者的既有分支结构（`switch`/`if` 数量与形状）在本功能落地前后一致——这是 DoD 的核心断言，
  应在 Phase D 审计里明确让后端架构 agent 逐行确认，不能只靠"没报错"默认过关。
- **顺序不变量测试**：仿照 F03 定的"故意错序验证真 throw"模式，新增两条覆盖 `model` 维度的
  顺序断言测试。
- **回归**：`remote-launch.test.ts`（7 个老式 builder，本功能不碰）、两个 e2e driver、
  `resume-suite`/`restart-suite`、`test:ccm-*` 全部零改动全绿——本功能只加末尾可选参数，不改
  任何既有必需参数的顺序/语义。
- **不新增真机 e2e 脚本**——本功能的风险面是"字符串生成对不对"，不是"tmux/ccm 行为对不对"，
  已有的 golden 串测试（步骤2）已经是恰当粒度的验证，硬造一个新 e2e 套件是不成比例的投入。

## 6. 代码审计结果（Phase D）

两个独立 agent 并行审（prompt 自包含，各带 MASTERPLAN §0 核心思想全文），均**无阻塞项**（UX
agent 报了 1 条真实阻塞，见下）。

**后端架构 + 正确性 agent**：逐项验证了"架构验收"这句核心承诺——`git diff` 确认
`buildLaunchPlan`/`renderCli`/`canRenderCli`/`renderFallback` 的既有分支结构全程零改动，唯一
改动是 `renderEnvOps` 的 switch 加一个成比例的 `"export-model"` 分支；`canRenderCli` 循环逻辑
手工推演正确（未配模型偏好时 `applies` 假、循环压根不问这个维度，完全不影响未配置会话的 CLI
资格；配了偏好时 `applies` 真且 `cliFlags` 为 `null`，安全网确实触发）；顺序不变量新增的两条
断言逐条按错序数组手工推演确认真 throw；`withAccount`/`account-restart.ts` 两条并列路径都正确
独立补了查询并线通；`shared/ccm`/`accounts_query.rs` 核实零 model 字面量（确认无隐藏 fallback，
§2 第1条的判断依据成立）。发现 4 条重要项：
1. `canRenderCli` 的模型降级此前零自动化测试覆盖（`launch-render-cli.test.ts` 全程未改，只
   靠 `launch-dimensions.test.ts` 孤立测过 `cliFlags()` 返回值）——已补 2 条端到端断言
   （配了/未配 `modelOverride` 各一条，见步骤2 更新）。
2. `doc/INVARIANTS.md` 未按 DoD 补记新维度落地样例——已补 §37（判断依据 + 与 §35 的区分）。
3. 计划台账（本文件 checkbox、MASTERPLAN/STATUS）当时未同步实际实现状态——本次 Phase F 一并
   补齐（同 F05/F06 的既有节奏：审计先于文档收尾）。
4. `modelOverride` 在 `tabs.ts`/`history.ts`/`account-restart.ts` 集成层的真值转传此前从未被
   验证过（`accounts.vitest.ts` 只测过 `withAccount` 模块边界内的行为，所有涉及 executor 的
   既有断言恒 `modelOverride=undefined`）——同 F05 Phase D 曾堵过的同一类接线覆盖缺口，已在
   `tabs.vitest.ts` 补 1 条真实模型字符串（"opus"）的集成测试。

**UX agent**：Job 1（新输入框的可发现性/合法值提示）、Job 2（保存反馈）、Job 3（隐藏 CLI 降级
副作用）、Job 4（async 逐行渲染的响应性）、Job 5（"同一模型三处投影"这条论题在本功能上是否
真的成立）。发现 **1 条阻塞 + 3 条重要**：
- **阻塞**：模型输入框保存路径（`accounts-section.ts`）完全不做合法性校验——`isValidModelName`
  全仓库唯一调用点是 `MODEL_DIMENSION.apply()`（起会话时），意味着非法值（如带空格的展示名
  "Claude Opus 4.5"）会先静默落盘，直到该账号**下一次任何会话拉起**（resume/新建/tmux resume，
  全部共用同一次 `buildLaunchPlan`）才在 `apply()` 里统一 `throw`，用户只看到一堆"无法构造
  resume 命令"toast、设置面板的输入框也不会标出"当前值非法"，很难联想到根因是设置里那个不起眼
  的小输入框。**已修**：`isValidModelName` 校验移到 `accounts.ts::setModelForAccount` 本身
  （写入点，fail-closed），`accounts-section.ts` 的 `saveModel` 相应改成 try/catch + 失败时不
  落盘、保留用户已输入文本以便就地修正；补 2 条 vitest（非法名 throw 不落盘 / 清除不受校验约束）。
- **重要**：保存动作是该文件里唯一没有任何反馈的操作（同文件 `selectDefault`/复制路径/登录
  终端都有 toast），且失败会真正无声消失（设置窗口没有主窗那个全局 `unhandledrejection` 兜底，
  因为 `#status-bar` 嵌在被 `.settings-window-mode` CSS 隐藏的 `#app` 里）——已按 `selectDefault`
  既有模式补齐 try/catch + 成功/失败 toast，加 `lastSaved` 去重（值未变不弹噪音 toast）。
- **重要**：`cliFlags` 恒 `null` 这个隐藏 CLI 降级切换，处置力度未达到 F05 R13 的先例（R13 既
  登记进风险表又有逃生口；F07 初版只在计划文档里自我辩护，未登记、应用内也无提示）——已登记
  **R14**（MASTERPLAN §6，已接受非阻塞）+ 输入框 tooltip 补一句"仅对本 app 发起的会话生效，
  终端里手敲 ccm 暂不识别（见 F08）"。
- **重要**：手动敲 `ccm` 完全不识别这个偏好，字面违反 corollary②"单一渲染目标"，且没有任何
  UI 提示这道边界——处理同上（tooltip 补充说明 + R14 登记，判定为 F08 关闭前的可接受过渡态）。
- 建议三条（未处理，判定为可选优化非阻塞）：加 `aria-label`；加非强制 `<datalist>` 候选模型名
  （不引入维护枚举的负担）；N 个账号 N 次独立 `load_config` IPC、无批量/缓存（`fetchAccounts`
  有 30s TTL 缓存作对照，`getModelForAccount` 没有）——账号数通常很少，影响可忽略，留作已知的
  可优化点，不在本轮处理。

## 7. 工程审计结果（Phase E）

主线程对账（读 MASTERPLAN §0/§0.1/§3/§6 + 本功能计划）：F07 落地后主计划仍自洽——**架构验收
通过**，`buildLaunchPlan`/两个渲染器主体结构 diff 为零的断言经后端审计逐行核实成立，不是自我
声明。§3 账本的 `launch-plan.ts`+`launch-dimensions.ts`/`accounts.ts` 两行已更新为落地后的
真实状态。

**账本预见的重叠，现在优雅处理而非留给以后打补丁**：F05 修 R11 同型坑时留下的"`applies` 恒真"
经验，若被后续维度机械照抄会造成新的自伤（几乎所有会话被拖进兜底渲染器）——本功能没有重蹈，
且把"为什么不能照抄"这条判断依据系统性地写进了 INVARIANTS §37，供 F08/F09 及以后任何新维度
的作者查阅，这正是"预见的重叠现在就优雅重构"这条铁律在**文档层面**的体现（不是代码重叠，是
认知模式重叠——同一个错误教训被误用的重叠）。

新增的 R14 是本功能对"隐藏机制切换"这类风险的诚实登记，与 R13 同构、处置力度对齐（tooltip
提示 + 风险表存档），不留给后续功能背负；②留给 F08 的分工在 R14 与 §1"明确不做什么"里双重
记录，F08 规划时应显式对照关闭。审计发现的 4 条重要项（后端）+ 1 阻塞 3 重要（UX）全部就地
修复，无新增技术债转发给后续功能。

## 8. 签收（Sign-off）

- [x] 通过代码审计
- [x] 通过双 agent 架构/UX 审（含"零改渲染器主体结构"这条 DoD 的逐行确认）
- [x] 通过工程审计
- [x] 主计划已据此更新
