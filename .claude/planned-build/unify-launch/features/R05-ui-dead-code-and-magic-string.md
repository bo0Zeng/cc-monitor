# 功能计划 — R05 UI 层：删死代码 + 收敛真重复 + 魔法串类型化

## 0. 先把账本那条声明缩到它站得住的范围

STATUS §R 的 R05 行原写：「删 `launch-menu.ts` 从未被渲染过的 container 组；
**5 处独立账号菜单实现收敛**；`"__base__"` 魔法串类型化」。

**中间那条经核实是过度声称，本功能不按它做**（理由在下面，这是本计划的第一个决定）。

### 核实：所谓「5 处独立账号菜单实现」其实已经共享了该共享的东西

| 处 | 取账号数据的方式 | 渲染形态 |
|---|---|---|
| `tabs.ts::buildResumeSubmenu` | ← `enumerateModifierGroups`（内部 `fetchAccounts`+`isSelectable`） | 右键三级级联 flyout |
| `tabs.ts::buildRestartSubmenu` | 同上（`realAccounts`，已排除 `__base__`） | 右键二级 flyout（danger） |
| `account-chip.ts` | `fetchAccounts` + `isSelectable`（`:137` / `:234`） | chip 下拉的 picker 行 |
| `settings/remote-section.ts` | `fetchAccounts` + `isSelectable`（`:822-824`） | `<select>` 下拉 |
| `settings/accounts-section.ts` | `fetchAccounts` + `isSelectable`（`:113` / `:371`） | 设置页表格行 |

**「哪些账号可选」这条业务判断早就是单一来源了**——F05 把它收进 `isSelectable`，五处全用它。
剩下不同的只有**渲染形态**：级联菜单项 / picker 行 / `<option>` / 表格行。
这四种是**不同的交互载体**，不是同一段逻辑被抄了四遍；强行收敛会造出一个既要吐 `TabMenuItem`
又要吐 `<option>` 又要吐表格行的通用渲染器，那是**新增**一层抽象、不是消除重复。

依据同一条既有判断准则（见记忆化的教训：**拆/合由具体架构病证成，不由"看起来像重复"证成**），
本功能**不做**这条。若日后真出现"某处漏了 `isSelectable`"这类不一致，那才是要治的病。

## 1. 本功能真正要做的三件（每件都有实测证据）

### ① 删 `launch-menu.ts` 的 container 组——**它是死代码**
证据（`grep -rn enumerateModifierGroups src/ --include=*.ts`）：
- 全仓**唯一生产调用点** `tabs.ts:2325`，第二参**硬编码** `"tmux"`；
- 紧接着 `:2327` 只做 `groups.find(g => g.id === "account")`——**container 组从未被读取**；
- 其余调用点全在 `launch-menu.vitest.ts`，且只有它（`:47`）跑过 `"none"` 分支
  → 那条分支**只被测试驱动过，生产从未走到**；
- 因第二参恒 `"tmux"`，container 组里的 `selected` 标志也恒定退化。

- [ ] 删 `ModifierGroup.id` 的 `"container"` 变体与产出它的那段
- [ ] `enumerateModifierGroups` 的第二参（`currentContainerKind`）随之删除——它唯一的用途就是
      给 container 组算 `selected`
- [ ] `launch-menu.vitest.ts` 里针对 container 组 / `"none"` 分支的用例一并删（它们测的是死代码）
- [ ] **函数改名**：不再枚举"若干组"，实际只产出 account 一组 → 名字要如实
      （候选 `enumerateAccountModifiers`；最终名在实现时定，但不得保留"Groups"这个复数假象）

### ② 收敛 `tabs.ts` 与 `launch-menu.ts` 里**逐字相同**的容器 label
证据：`tabs.ts:2265-2266` 的 `{ label: "tmux" }` / `{ label: "直连（不建 tmux）" }`
与 `launch-menu.ts:72-73` 的 label **逐字相同**，两处各写一遍。
①删掉后 `launch-menu.ts` 那份消失，`tabs.ts` 那份成为唯一来源——
- [ ] 确认删除后不再有第二处 label（即 ① 顺带解决 ②，不需要额外抽公共常量；
      **抽一个只有一个消费者的常量是无收益的间接层**）

### ③ `"__base__"` 类型化
证据：`launch-menu.ts:61` 产出这个裸串，`tabs.ts:2275` / `:2338` 各自 `=== "__base__"` 消费。
跨文件、无类型约束——**拼错一个字符 tsc 抓不到**，行为是"基座选项静默变成一个普通账号名"
（又是 R11/R08 那族"看起来生效了，只是用错了号"的形状）。
- [ ] 导出一个 `const BASE_OPTION_ID = "__base__"` 并在三处引用它；
      `ModifierOption.id` 的类型收成 `string & {}` 之外的合适形态，让"是基座还是账号名"可判别
      （实现时定：优先考虑把 `id` 改成判别联合 `{kind:"base"} | {kind:"account"; name:string}`，
      那样连 `=== BASE_OPTION_ID` 这个比较本身都不需要了——**但只在不引起大面积改动时才这么做**，
      否则退回常量方案）

## 2. DoD

- [ ] 上述 ①②③ 全部落地
- [ ] **UI 行为逐字节不变**：三级级联的菜单项文案、顺序、`danger` 标记、`enabled` 条件全不变
- [ ] `tabs.vitest.ts` 既有 DOM 测试全绿（它们覆盖 flyout 展开态、代次守卫、restart 阈值）
- [ ] `launch-menu.vitest.ts` 删完死代码用例后，剩下的**必须仍在验 account 组的真实行为**
      （不能删成一个空壳套件）
- [ ] tsc 0 / `npm test` 全绿 / 7 套真机 e2e 不受影响（本功能纯前端 UI，不碰命令构造）

**不做什么**：
- **不**做"5 处账号菜单收敛"（§0 已论证）
- **不**碰 `isSelectable` / `fetchAccounts` / `withAccount`（账号数据层，F05 已收敛）
- **不**改任何菜单文案（F09 Phase D 的 UX 审计逐条定过）
- **不**动 `restartingSids` / 代次守卫 / safe-triangle 悬停逻辑（F09 修过的坑）

## 3. 测试策略

- 删死代码的风险是"删多了"：靠 tsc（`ModifierGroup.id` 收窄后，任何残留的 `"container"` 引用
  都会编译失败）+ `tabs.vitest.ts` 的 DOM 断言兜住。
- ③ 的变异检查：把 `BASE_OPTION_ID` 的值改成 `"__base_typo__"` →
  若 `realAccounts` 的过滤仍"正常"（基座混进真实账号列表）说明类型化没生效、断言不足。
- 收尾核对 `git status` 确认 `shared/ccm` / 两个 e2e driver / 三个渲染器零 diff。

## 4. 代码审计结果（Phase D）
（待填）

## 5. 工程审计结果（Phase E）
（待填）

## 6. 签收
- [ ] 通过代码审计（无阻塞项）
- [ ] 通过工程审计
- [ ] 主计划已据此更新（含变更记录）
