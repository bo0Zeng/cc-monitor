# 功能计划 — R03 位置参数长列车 → options bag（成功标准② 达成）

## 0. 为什么做这个（问题陈述）

MASTERPLAN §0.1 **成功标准②**：

> 加一个新启动维度 = 注册一个 dimension + CLI 加一个 flag + UI 加一个修饰项，**零改** builder / renderer / 调用点。

F07（每账号默认模型）作为"第一个真实新维度"做架构验收时，**渲染器主体确实零改**（`MODEL_DIMENSION`
新增，`buildLaunchPlan`/`renderCli`/`canRenderCli`/`renderFallback` 的既有分支结构 diff 为零）。
但它同时暴露了另一半：`modelOverride` 这个值要**从 UI 一路手动透传到渲染器**，
于是 4 个 `planXxx` + 5 个 `runXxx` 的签名尾部各加了一个参数。F07 commit message 自己
承认了 9 处签名改动。

**今天的形态**（尾部三元组在 9 个函数里逐字重复）：

```ts
planResumeTmux(sid, cwd, launcher, name?, configDir?, accountName?, modelOverride?)
runRemoteResumeTmux(origin, sid, cwd, launcher, name?, configDir?, accountName?, modelOverride?)  // 8 个位置参数
```

这三个字段是**同一族东西**（都是"正交修饰"：account 维度 + model 维度），
却被摊成三个平级位置参数。后果：
1. **成功标准② 实际未达成**——加第 4 个维度要同时改 9 处签名。
2. 8 个位置参数的调用点可读性差。**（审计订正：初稿称"`undefined, undefined, "opus"`
   这种占位实参已经出现"——核实 HEAD 上生产代码里一个都没有，只在测试代码里有。
   这条理由只对测试成立，写进"问题陈述"时是给方案抬价，已改。）**
3. 相邻同类型可选参数（三个都是 `string | undefined`）**传错顺序 tsc 抓不到**——
   `configDir` 与 `accountName` 互换编译照过，行为却是"账号选择静默失效"（R11 那一族的形状）。

## 1. 目标与验收标准（DoD）

- [ ] 新增 `LaunchModifiers`（`configDir?` / `accountName?` / `modelOverride?`），
      9 个函数的尾部三元组统一替换为 `mods: LaunchModifiers = {}`。
- [ ] **成功标准② 可验收**：加第 4 个维度只需 `LaunchModifiers` 加一个可选字段 +
      注册 dimension + 提供值的那一个源头调用点，**其余 8 个函数签名与全部透传调用点零改**。
      验收方式：文档里写清这条路径，并用一次真实的"假想第 4 维度"演练（只在计划里推演，不落代码）。
- [ ] `src/remote-launch.ts` 的 **7 个导出 builder 签名逐字不变**——
      `e2e/resume-cmd-driver.ts` 直接 import 其中 5 个（`buildResumeIntoExistingTmuxCmd` /
      `buildResumeTmuxCmd` / `buildResumeDirectCmd` / `pickFreshTmuxName` / `buildEnvPrefix`）。
- [ ] **两个 e2e driver 零改动**（`git status` 核对 diff 为空）。
- [ ] 行为逐字节不变：7 套真机 e2e 131 条断言全绿（尤其 `ccm-print-parity` 12 条外部预言机）。
- [ ] tsc 0 / npm test 全绿 / cargo 不受影响（本功能纯 TS）。

**不做什么**（防范围蔓延）：
- **不**把 `name`（tmux 会话名）收进 bag。
  **（Phase D 审计订正：初稿援引 `doc/INVARIANTS.md` §38 为理由，那是用错了。**
  §38 回答的是"一条新轴该进 `LAUNCH_DIMENSIONS` 注册表，还是该做 `LaunchPlan`/`LaunchContext`
  的硬编码一等字段"；而 `LaunchModifiers` **既不是注册表也不是 IR**，它是函数入参形状。
  把 `name` 收进 bag 并不会让 container 进注册表——`planResumeTmux` 照样从 `mods.name` 构
  `container:{kind:"tmux",…}` 这个硬编码一等字段。§38 **不禁止**这件事。**
  真正的两条理由：① `planResumeIntoExistingTmux(sid, **name**, launcher, mods)` 与
  `planLauncher(cwd, **tmuxName**, …)` 里 `name` 是**必填**，塞进全可选的 bag 会把
  "必须给名字"这条约束从**编译期降级成运行期**；② `remote-launch.ts` 7 个导出 builder 的
  位置参数签名被 e2e driver 锁死（`buildResumeTmuxCmd(sid, cwd, launcher, name?, configDir?)`），
  `name` 保持在位才能让适配器保持一行直传。
- **不**改 `remote-launch.ts` 的 7 个 builder 导出签名（e2e 依赖面）。
- **不**合并 6 个 executor 成单一 `runLaunch`——那是账本里另一条独立的事，
  且 F03 已论证代价与收益不匹配；本功能只治"参数形状"，不治"函数个数"。
- **不**动 `shared/ccm` / 渲染器 / 维度注册表。

## 2. 与主计划的对接

触及共享面账本两行：
- `src/remote-launch.ts`：账本写"**保持位置参数签名**（e2e driver 直接 import）"——
  本功能**遵守**，只改其内部对 `planXxx` 的调用形态。
- `src/remote-launch-run.ts`：账本最终形态写的是"6 个 executor → 单一 `runLaunch(plan)`"。
  本功能**不实现那一条**（见 §1 不做什么），但把它的前置障碍之一（位置参数长列车）清掉。
  账本需据此更新：把"位置参数长列车"从阻塞因素里划掉，留"函数个数"为唯一剩余分歧。

## 3. 接口设计

```ts
// src/launch-plan.ts（IR 叶子模块，planXxx 与 runXxx 都从这里取型）
export interface LaunchModifiers {
  configDir?: string;
  accountName?: string;
  modelOverride?: string;
}
```

放 `launch-plan.ts` 而非 `launch-requests.ts`：后者 import 前者，前者是叶子；
两个消费方（`launch-requests.ts` 的 planXxx、`remote-launch-run.ts` 的 runXxx）都能无环取型。

**与 `LaunchContext` 的关系**（不是重复）：`LaunchModifiers` 是**解析前**的原始形态
（调用方手上有的东西：一个目录、一个名字、一个模型串）；`LaunchContext.account`
是**解析后**的判别联合（`{kind:"account"|"base"}`）。`planXxx` 正是这个转换的发生地。

## 3.1 实现期发现：**列车的车头是 `withAccount`，不是 `planXxx`**（计划扩围，铁律 4）

改到调用点时发现，6 个生产调用点几乎全是这个形状：

```ts
void withAccount(origin, accName, (cd, an, mo) => runRemoteLauncher(origin, cwd, name, command, cd, an, mo));
```

`accounts.ts::withAccount` 的 `run` 回调签名正是 **同一个三元组**：
`(configDir?: string, accountName?: string, modelOverride?: string) => Promise<void>`。
账本 `src/accounts.ts` 那一行也记着它被扩过两次（F05 加 `accountName`、F07 再加 `modelOverride`）。

**只 bag 掉 `planXxx`/`runXxx` 是不够的**：调用点仍然要把三元组逐个接住再逐个转发，
于是加第 4 个维度**依旧要改全部 6 个调用点**——成功标准② 仍然不达成，只是把病灶推后一层。

→ **扩围**：`withAccount` 的 `run` 回调改为收 `LaunchModifiers`。调用点变成

```ts
void withAccount(origin, accName, (mods) => runRemoteLauncher(origin, cwd, name, command, mods));
```

加第 4 个维度时：`withAccount` 内部把它塞进 `mods`、`LaunchModifiers` 加一个字段，
**上面这行一个字符都不用改**。

→ 审计实测口径：拿"加第 4 个维度"当尺子，`remote-launch-run.ts`(14 行)/`tabs.ts`(10)/
`views/history.ts`(4)/`settings/remote-section.ts`(4) 共 **32 行纯透传编辑归零**。
**但这只达成成功标准② 的"零改调用点"那一半**——"零改 builder"没达成：
`launch-requests.ts` 的 4 个 `planXxx` 仍要各改 2 行（解构 + ctx），`LaunchContext` 也要加字段。
别把它写成"只需三处"。

代价：`withAccount` **6** 个调用点（审计核实是 6 不是 7） + `account-restart.ts` 的并列解析路径（账本已记它**有意不合并**，
失败语义不同：`withAccount` 退化基座 / `account-restart.ts` 中止）。合计仍在原估量级内。

## 4. 实现步骤

- [ ] 步骤 1：`launch-plan.ts` 加 `LaunchModifiers` 类型（含头注说明它存在的理由 = 成功标准②）。
- [ ] 步骤 2：4 个 `planXxx` 签名改为收 `mods: LaunchModifiers = {}`，函数体内解构。
- [ ] 步骤 3：5 个 `runXxx` 签名同改；`runNewSessionRemote` 对 `runRemoteLauncher` 的内部转发同步。
- [ ] 步骤 4：`src/remote-launch.ts` 7 个 builder 内部改为传 bag，**导出签名不动**。
- [ ] 步骤 5：5 个生产调用点文件改为传 bag：`account-restart.ts` / `settings/remote-section.ts` /
      `tabs.ts` / `views/history.ts`。靠 tsc 逐个揪出，不靠记忆。
      （审计订正：初稿还列了 `views/session-viewer.ts`，但那里对 `runRemoteResume` 只有一句
      注释引用（`:355`）、**没有调用**，清单已删掉它，免得下一个人去找一个不存在的改动。）
- [ ] 步骤 6：测试断言改形状（`tabs.vitest.ts` / `remote-launch-run.vitest.ts` /
      `account-restart.vitest.ts` / 4 个 history 相关）。**这些断言正是接线的验证手段**，
      逐条改、不批量正则替换——改错了会变成"断言了错的形状仍然绿"。
- [ ] 步骤 7：门禁全量 + 两个 e2e driver 与 `shared/ccm` 的 `git status` 零 diff 核对。
- [ ] 步骤 8：Phase D 独立对抗性 agent（论证"不该这么做"）+ 实现视角审计。

## 5. 测试策略

- 既有断言改形状后必须**仍然真的在验接线**：抽 1-2 条做变异检查
  （把生产侧 `mods.accountName` 改成不传，断言必须转红）——防"改形状时把断言改松了"。
- `ccm-print-parity` 12 条是外部预言机，本功能不该动它一个字符。
- 新增 1 条类型层保护：相邻同类型可选参数消失后，"传错顺序"这个类别的 bug 应当**编译期不可表达**。
  用一条 `@ts-expect-error` 测试钉死（传旧的位置参数形态必须编译失败）。

## 6. 代码审计结果（Phase D）

开一个**独立对抗性 agent**（任务是论证"不该这么做"，明确要求不为对抗而对抗）。
它跑了 60 次工具调用、做了 5 个变异实验 + 11 条 vitest 判等语义探针，**无阻塞项**，
但揪出的东西比我自己查得深，且**推翻了我两处声称**。全部已修。

### 重要-1（真实的断言强度损失，已修）
`toHaveBeenCalledWith` 对**位置参数**严格比 arity，对**对象**却忽略"值为 undefined 的键"。
审计用 11 条探针实测确认了这个判等语义差异，然后做变异 M1：删掉 `account-restart.ts`
bag 里的 `modelOverride,` → `account-restart.vitest.ts` **12/12 仍全绿**
（改造前同一变异会因 arity 7≠8 转红）。根因：该文件 `:15` 把 `getModelForAccount` 恒 mock 成
`undefined`，全文件唯一那条断言只能钉 `modelOverride: undefined`。
**这是本次改造唯一真实的断言退化。** 缓解事实（审计也实测了）：`tsc` 的 `noUnusedLocals`
会抓到它（`TS6133: 'modelOverride' is declared but its value is never read`），所以门禁整体没漏，
漏的是测试这一道。→ 已补一条 `getModelForAccount → "opus"` 的用例把它钉成非 undefined；
复跑 M1 变异确认现在**转红**（1 failed / 12 passed）。

### 重要-2（F07 遗留缺口，R03 是最该补它的功能，已修）
`planXxx` 里"解包 bag → 填 ctx"这一层**全仓零覆盖**。审计变异 M5：让 `planResumeTmux`
不消费 `mods.modelOverride`（等价于"tmux resume 路径上每账号默认模型静默失效"）→
`tsc` 无输出、`npm test` **699 全绿**、`ccm-print-parity` **12 全绿**，**三道门全瞎**。
根因：`launch-dimensions.test.ts`/`launch-render-cli.test.ts` 都手搓 ctx、从不经 `planXxx`；
`tabs.vitest.ts` 把 `remote-launch-run` 整个 mock 掉；`remote-launch.ts` 的 builder 只传 `{configDir}`。
这缺口改造前就有（只是那时 `noUnusedParameters` 会顺手抓到），但我新加的测试只断言了
`ctx.account`、把手边最该钉的漏了——计划 §5 那条"抽 1-2 条做变异检查"事实上没做到位。
→ 已补断言"三个修饰字段都真的落进 ctx"；复跑 M5 确认**转红**（tsc 仍静默，证明这条测试
是该变异的唯一门）。

### 重要-3（我的过度声称，已改）
`launch-plan.ts` 原头注写"加第 4 个维度只需 本接口加一个字段 + 注册 dimension + 一处源头"
——**不成立**。审计拿 F07（真发生过的"加第 4 个维度"，`git show 9531ef3 --stat` 13 个生产文件）
当基线逐文件计数：`launch-requests.ts` 的 4 个 `planXxx` 仍要各改 2 行（解构 + ctx 字面量），
`LaunchContext` 要同步加字段，需要新 `EnvOp` 种类时 `launch-render-fallback.ts` 还要加 switch 分支。
→ 已改成诚实口径：**只达成"零改调用点"那一半**（32 行归零），"零改 builder"未达成，
并记下真要闭合它得让 `LaunchContext` 持有**纯透传子集**、且**绝不能**把
`configDir`/`accountName` 也搬进去（那会让未解析的原始字段与已解析的 `account` 判别联合并存，
未来某个维度读了原始字段就绕过 `accountOf`，正是 R11 那一族病）。

### 重要-4（引用用错，已换理由）
§1 原以 `doc/INVARIANTS.md` §38 为"不把 `name` 收进 bag"的理由。§38 回答的是
"新轴进注册表还是做 IR 一等字段"，而 `LaunchModifiers` 既不是注册表也不是 IR、是**入参形状**
——§38 不禁止这件事，属拿不相干的决策背书。→ 已换成两条真理由：
① `name` 在 `planResumeIntoExistingTmux`/`planLauncher` 里是**必填**，塞进全可选 bag 会把约束
从编译期降级成运行期；② builder 位置参数签名被 e2e driver 锁死，`name` 在位才能一行直传。
（审计另核实 `runRemoteResumeTmux(…, name?, mods?)` 这个"可选位置参数在 bag 之前"的形状
是**类型安全**的：两个方向都被 tsc 挡（weak-type 检测），且今天没有任何调用点需要写 `undefined`
占位。属可接受的形状瑕疵，非缺陷。）

### 建议项（已全部落实）
- `withAccount` 头注 + 7 条测试标题仍在用旧的 `run(configDir, accountName)` 写法描述回调
  ——审计一句话点破："这个仓正在因为 `INVENTORY.md` 文档腐烂而重写它，别同一天在这里新造一处。"
  已全改。
- "7 个调用点"实为 **6** 个（`remote-section.ts` 1 + `views/history.ts` 2 + `tabs.ts` 3）。已订正。
- 步骤 5 列了 `views/session-viewer.ts`，但那里对 `runRemoteResume` 只有一句注释引用（`:355`）、
  **没有调用**。已从清单删掉。
- §0 理由 2 称"`undefined, undefined, "opus"` 这种占位实参已经出现"——HEAD 上生产代码里
  **一个都没有**，只在测试里有。已标注订正（理由 3「相邻同类型传错顺序 tsc 抓不到」才是真正值钱那条）。

### 审计独立确认为真的部分（含比我更强的证据）
- **行为逐字节不变**：它没读我的 diff，而是 `git archive` 导出 HEAD 快照，写两份 emit 脚本跑同一
  修饰矩阵（`configDir × accountName × modelOverride × launcher` = 2×2×2×2）× 5 个 `plan*` + `planAttach`
  = **81 条组合**，各自 `JSON.stringify({ctx, plan})` 后 `diff` → **零字节差异**。
- 敏感面零 diff 逐个核对：两个 e2e driver / `shared/ccm` / 三个渲染器 / `session-backend.ts` /
  `remote-launch.test.ts`（448 行逐字节字符串断言）全 0 行。
- 两个 e2e driver 不只"没改"，而是**仍编译得过**（临时 tsconfig 跑 tsc exit 0）；并核实
  `resume-cmd-driver.ts` **不 import `withAccount`**——本次唯一改了导出签名的东西恰好不在 e2e 的 import 面。
- **类型层守卫有牙齿**：`tsconfig.json` `include:["src"]` 覆盖 `*.vitest.ts`，故 `@ts-expect-error`
  由 `tsc` 强制；它另外在快照树里放一条无用指令验证了 `TS2578` 确实会报。
- §3.1"车头是 `withAccount`"经独立核实为真，扩围是必要的而非范围蔓延。
- 替代方案逐个比过，确认本方案是对的中间点（什么都不做 / 只改 `withAccount` / 一步做成
  `runLaunch(plan)` 各自的问题，后者与账本 F03 的既有结论一致：不建议顺手做）。
- **一个我没写进计划的额外收益**：探针证明"给 bag 加一个值为 undefined 的新字段"**不会**让
  那 25 条既有断言连坐转红；而位置参数时代加一个恒传 `undefined` 的尾参会让它们**全部**因
  arity 转红。即这次改造顺带让"加第 4 个维度"不必再改测试。

## 7. 工程审计结果（Phase E）

- **主计划是否仍自洽**：是。共享面账本两行需更新（见 Phase F）：`remote-launch.ts` 那行的
  "保持位置参数签名"**已遵守**（7 个导出零改）；`remote-launch-run.ts` 那行的最终形态
  "6 个 executor → 单一 `runLaunch(plan)`"**本功能不实现**，但清掉了它两个前置障碍之一。
- **是否引入拖累后续功能的耦合**：没有。相反，B02 已经在等这个改动兑现——B 段摸底发现
  `cc-spawn` 的 `CC_BUS_ID` 会在 tmux 边界被吃掉（R08 同型），修法正是**给 `ccm` 加一个新维度**，
  那将是成功标准② 的第二次真实架构验收，届时 `LaunchModifiers` 直接受用。
- **是否有应现在就做的统一重构**：审计提出的"`LaunchContext.passThrough` 纯透传子集"能真正闭合
  "零改 builder"那一半。**判定为暂不做**：它要求区分"需解析"与"纯透传"两类修饰，而今天
  纯透传的只有 `modelOverride` 一个（`configDir`/`accountName` 都要经 `accountOf` 解析）——
  **一个元素撑不起这个抽象**，正是 R12/注册表那条同型教训（"提前建等于为假想需求设计"）。
  等 B02 的 `--bus-id` 进来，纯透传就有第二个元素，那时再抽才被真实需求证成。已登记 **R15**。
- **工程健康度**：tsc 0 / npm test 701（+4：类型层 2 + bag→ctx 1 + 并列路径 modelOverride 1）/
  cargo 390 不受影响（纯 TS）/ 7 套真机 e2e 131 条全绿 / CI 四道真门禁按原样命令过。

## 8. 签收
- [x] 通过代码审计（无阻塞项；4 条重要全部修完并各自复跑变异确认转红）
- [x] 通过工程审计
- [x] 主计划已据此更新（含变更记录 + 新登记 R15）
