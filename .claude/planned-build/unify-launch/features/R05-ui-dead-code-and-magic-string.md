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

### Phase D 审计订正：**结论对，但这份核实不完整——而且漏掉的那处正好有"该治的病"**

审计独立核对后确认：**没有任何一处漏了 `isSelectable`**（chip 那处"不过滤只置灰"是有意的，
它同时是状态显示面），所以"造一个既吐 `TabMenuItem` 又吐 `<option>` 又吐表格行的通用渲染器
= 新增抽象而非消除重复"这个论证**成立，否决的结论保留**。

但上表**少了 2 处**，实为 **7 处**：
- **第 6 处 `src/views/history.ts::appendAccountResumeItems`（`:1685`）**——history 会话行右键的
  「用账号 X resume」。同样 `fetchAccounts` + `isSelectable`（`:1694`）、同样 ≥2 阈值（`:1695`）。
  它与 tabs 的 Resume flyout **是语义上同一个动作**，只是长在另一个视图里。
- **第 7 处 `src/account-commands.ts::buildAccountCommands`（`:37`）**——Ctrl+K 的「设 X 为当前账号」。

**并且第 6 处存在一条真的功能不一致**（我已独立复核）：history 的默认 resume 走 `follow`
（`views/history.ts:1509`），即"有 ≥1 可选账号 → follow 会注入某号"这条前提在 history 完全成立；
但那个菜单里**没有任何基座逃生口**——`grep -rn "基座" src/views/` **零命中**。
而这正是 F01 步骤2 给 tabs 加基座项的全部理由（防 #75）。对照：
tabs 有「基座（不隔离）」、设置页新会话对话框有「不指定（用远端登录的基座账号…）」，**只有 history 没有**。
计划文里找不到任何"history 不给基座逃生口"的记录决策 → **看起来是遗漏而非决定**。

→ 故本节改判为：**维持"不造通用渲染器"的否决（理由不变）**，但把这条不一致
**登记为独立条目 R16**（不在 R05 做：给 history 加一个菜单项是行为变化，该走自己的 DoD 与 UX 判断）。
顺带登记两条更轻的（同样不在本功能做）：「≥2 可选账号」这条业务规则写在三处
（`launch-menu.ts` / `tabs.ts` / `views/history.ts`）；同一概念在三处三种文案。
**这三条才是"账号菜单真正该收敛"的东西——收的是规则与文案，不是渲染器。**

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
- [x] **UI 行为逐字节不变——除一处**：三级级联的菜单项文案、顺序、`danger` 标记、`enabled`
      条件全不变；**唯一例外是"账号名恰为 `__base__`"这个保留名碰撞，那是修 bug**（见 §4 ③）。
      Phase D 审计订正：原文只写"逐字节不变"会把这处盖住，将来做 drift 对账时会误以为 R05
      什么行为都没动。
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

独立对抗性 agent（70 次工具调用 / 13 个变异 / `git archive HEAD` 快照差分实测）。
**0 阻塞 + 5 重要，全部已修**；它同时**确认了三件事的判定全部成立**，并把 ③ 的价值抬高了一档。

### 重要（已修）1：本功能唯一实质重写的那一行**零测试覆盖**——三个变异全部存活
`tabs.ts` 的 `opt.kind === "base" ? containerLeaves(undefined, true) : containerLeaves(opt.name, false)`
是 R05 唯一改写行为的语句。审计做了三个变异（每次先 `diff` 确认落在代码行上），**全绿**：
- 基座分支 `useBase: true → false`（= #75 逃生口退化成 follow 注入）→ 147 passed
- 具名账号分支 `opt.name → undefined`（= 点「用 b resume」实际跟随默认号）→ 147 passed
- 具名账号分支 `useBase: false → true` → 147 passed

即**把"基座逃生口"和"点哪个账号就用哪个账号"这两条语义整个改坏，仓里没有一条测试会红**。
既有 DOM 测试只断言 label 集合与叶子**数量**（`containerLeafCount === 8`），
从不点开某个账号的叶子看真实调用参数；tsc 也帮不上忙（两个分支都类型正确）。
同类还有 `accountOptions.length > 0 → >= 0`（空账号也加分隔线）也存活——
分隔线渲染成 `.tab-context-menu-divider`，而测试的 `menuLabels()` 只扫 `.tab-context-menu-item`。

→ 已补 6 条行为测试（含审计给的 `clickLeafUnder` 工具函数，现有测试缺的正是它）。
**复跑三个变异全部转红**（M1 红1 / M2 红2 / M7 红1）。

### 重要（已修）2：注释指向已删除的符号，就在本次改动的函数里
`appendAccountMenuItems` 上方还留着「container 组这里不消费……`currentContainerKind` 传恒定值即可」
——这个参数已被本改动删除。R04 刚把"注释纪律变成类型上做不到的事"当一等事，这里正好是反例。
另两处（`buildRestartSubmenu` 头注、`realAccounts` 上方）仍用 `__base__` 描述机制，
而现在的机制是 `kind === "account"`。已全部订正。

### 重要（已修）3：`doc/INVARIANTS.md` §38 结尾段现在是错的
§38 是**规范性**章节（"供未来任何新轴参考，防止被当成 bug 顺手修掉"），结尾原写
「account 组手写调 `fetchAccounts`，**container 组手写两个硬编码值**——两者形式不同」。
`enumerateModifierGroups` 已改名、container 组已删、那半个论证已经搬去 `tabs.ts::containerLeaves`。
指着不存在的符号讲道理比历史记录严重。已同步，并指出**论证本身反而更强**：
container 轴的可选值本就固定为两个字面量、不需要"现查"，所以它根本不需要一个发现层
——这恰恰印证 §38 的结论。另 `features/F09-ui-convergence.md` 的验收勾里"容器组恒 2 项"
那半条 DoD 已由 R05 撤销，已就地标注。

### 重要（已修）4：DoD「UI 行为逐字节不变」不准确
有一处边缘行为**确实变了**（且是变好，见下 ③）。这个项目的账本里反复出现"过度声称→事后订正"
（R03/R04 都栽过），这句属同一形状：不写出来，将来做 drift 对账会认为 R05 什么行为都没动。
已改成"除『账号名恰为 `__base__`』这一保留名碰撞外逐字节不变；该情形是修 bug"。

### 重要（已修）5：§0 的否决**结论对、核实不完整**——漏掉的那处正好有"该治的病"
见 §0 的「Phase D 审计订正」段：实为 **7 处**不是 5 处；且 `views/history.ts` 的账号 resume 菜单
**没有基座逃生口**（`grep -rn "基座" src/views/` 零命中），而它的默认 resume 走 `follow`
——正是 #75 那个场景。已登记 **R16**（不在 R05 做：加菜单项是行为变化，该走自己的 DoD）。

### 建议（已落实）
- `tabs.ts` 的 `if (realAccounts.length < 2) return;` 是**不可达守卫**
  （`realAccounts.length ∈ {0} ∪ [2,∞)`，因为具名账号只在 `selectable.length >= 2` 时整批 push）
  ——保留作 belt-and-braces，但已加注释说明，别让后人以为这里在独立执行阈值。
- `launch-menu.ts` 改用既有的 `selectableAccounts(state)`——它自己的注释就写着
  「计数一律走它，**别各处再 filter 一遍**」，而这里原先手写了同一个 filter。一行的事。
- 头注点明"本文件现在只负责账号轴，容器两项住 `tabs.ts::containerLeaves`"。
- **计数订正**：HEAD 是 **9** 条测试不是 10 条（删 3 + 新增 1 = 7）。
- `expect(JSON.stringify(opts)).not.toContain("__base__")` 在其它断言存在的前提下恒真，
  属装饰性断言（无害，但不算保护）——真正有效的是同一条里的 `kind === "base"` 与 `!("name" in opts[0])`。

### 审计独立确认为真、且比我给的证据更强的部分
- **container 组死代码判定成立，而且比我写的更强**：它用 `git archive HEAD` 快照独立查，
  逐一排除了动态 key 访问 / 解构 / 遍历 groups / 序列化 / barrel 再导出（**此仓 `export *` 零命中**）
  / 跨语言引用（全快照 grep `launch-menu` 只命中 tabs + 自身测试 + 文档）。
  并发现**消费面比我说的还窄**：`ModifierGroup.label` 从未被渲染、`ModifierOption.title`
  从未被写也从未被读、`selected` 只有 container 组写过无人读——
  **这次连带删掉了三个零消费者的字段**，是"删对了"的正面证据。
- **"删的是死代码不是扩展点"**：它给了三条判据（曾有的第二个组本身是死的 → 这个形状从未被真实
  使用过；唯一消费者做的 `find(g => g.id === "account")` 表达的恰恰是"我只要 account 那一组"；
  §38 已就 container/agent 两条轴拍过板、且全部计划文里搜不到第三条 UI 修饰轴的立项）。
  并指出回退成本有界（三处），符合推论① "加第 N+1 个维度应是 +1 的代价"。
- **判别联合与仓内既有词汇一致**：`accounts.ts` 早有
  `AccountResolution = {kind:"account"|"base"|"unavailable"}`。用同一套 `kind` 是**沿用惯例**，
  不是新造抽象——这条比我计划里写的理由更强。
- **`a.id → a.name` 三处全部改到位**，且 `label` 那两处原本就是 `a.label`、没有被误改（逐条对照 HEAD）。
- **③ 修的是真 bug，双向实测**（这条把 ③ 的价值抬高了一档）：
  `settings/acct-deploy.ts::validateAcctName` 放行 `[A-Za-z0-9._-]`（**下划线在内**），
  只禁首字符 `-`/`.` → `__base__` **通过校验**、是可达的名字；何况账号也能由 `cc-acct-iso`
  在 app 之外直接建、根本不过这道校验。改造前实测：点那个真实账号 → `configDir: undefined`
  （静默落基座、用户选的号被吞），**且** `realAccounts` 把它一起过滤掉 → Restart 入口**凭空消失**。
  改造后两条皆绿。已把这两条落成回归锚点。
- **行为等价的正面证据**：审计用 HEAD 快照跑我新写的行为用例 → 4 passed；换回现状 → 4 passed。
  两边同绿 = 除 `__base__` 那个碰撞外行为确实没动。

### 审计自己排除的假信号（值得记，避免我误读它的表格）
三个变异存活但**不是覆盖缺口**：`!accountsAvailable ||` 冗余（`selectable` 只在 available 时才填，
两条件恒同结果，HEAD 同样冗余、非本次引入）；`realAccounts.length < 2` 不可达；
`a.name → a.label` 语义等价（account 项恒 `label === name`）。它把这三条单列，没混进缺口清单。

## 5. 工程审计结果（Phase E）

- **主计划是否仍自洽**：是。§38 已同步（见重要 3）；账本 `src/tabs.ts` 那一行的最终形态
  （"菜单支持二级 flyout；徽章多账号即常显；对齐全套删除"）不受影响。
- **是否引入拖累后续功能的耦合**：没有。反而给 B03 减负——驾驶舱要列 bus 上的 agent，
  账号选择那套 UI 不会被卷进来。
- **是否有应现在就做的统一重构**：审计指出的三条（R16 的基座逃生口缺失、「≥2」阈值散在三处、
  同一概念三种文案）**都不在本功能做**，理由是它们是**规则与文案**层面的收敛，
  且第一条是行为变化。已登记，不留在脑子里。
- **工程健康度**：tsc 0 / npm test **705**（+6 行为测试）/ coverage 地板过 /
  `ccm-print-parity` 12 + `ccm-cli` 44 全绿 / `shared/ccm` 与 `src-tauri/` 与两个 e2e driver 零 diff。

## 6. 签收
- [x] 通过代码审计（0 阻塞；5 重要全部修完，三个曾存活的变异已复跑确认转红）
- [x] 通过工程审计
- [x] 主计划已据此更新（含变更记录 + 新登记 R16）
