# 功能计划 — F09 UI 收敛：动作 × 修饰

> 对应主计划功能清单中的 F09。本文件是该功能从规划到签收的全程记录。

## 0. Phase B 决策过程（两个独立 Plan agent 论证 R12）

用户 2026-07-27 授权"具体决策由本席开 agent 讨论分析后决定"。就 R12（container/agent 两条轴要不要收进
`LAUNCH_DIMENSIONS` 注册表）开了两个独立 Plan agent，分别论证对立方向：

- **Agent A**（论证"扩大注册表覆盖面"）：分轴给出结论——`container.kind`（tmux/none 二元开关）
  建议收编成 `CONTAINER_DIMENSION`；`container.mode`（`create-or-attach`/`send-into`/`attach-only`）
  **不该**被当成"修饰"，因为它是点击那一刻现查远端状态派生出来的值（`tabs.ts::resumeTabTmuxInner`
  的探测-派发逻辑），用户从未也不该在 UI 上选它；agent 轴**不该**本轮收编。
- **Agent B**（论证"维持三轴三机制，只在 UI 层收敛"）：给出独立证据——`codex-phase2` 已是独立在建
  的计划，且 Codex 的 resume 走 `remote-daemon-proto` 的 `--resolve` RPC，**完全不经过**
  `LaunchPlan`/`ccm` 管线；container 的判别式（`kind`/`mode`）从 `buildLaunchPlan` 起就是直接
  透传定型的载体字段，不是"追加一段 env/args"的效果，塞进"就地改 IR、绝不拼字符串"这个契约
  语义别扭；提出 `doc/INVARIANTS.md` 新增判断准则（§38）区分"该走注册表"vs"该硬编码"的轴。
- **两者的共识**（独立得出，非其中一方单方面主张）：
  1. **agent 轴本轮不做**——前端零 codex 消费能力，`AGENT_PROFILE` 是单例常量非查找表；即便做了，
     resume/attach 对已存在会话没有"换 agent"的自由度（sid 对应特定 agent 的 JSONL 格式），
     只有 `new` 动作能真的用上，打破"修饰对任意动作正交"的故事；且已有独立计划轨道
     （agent-profile.ts 头注的 MA-multi-agent-adapter）负责这件事，不该被 F09 顺手夹带。
  2. **`restart` 不该塞进 `LaunchAction` 第四变体**——它是"kill 旧会话 → 等退出/等 compact →
     resume 新会话"的**编排**（`account-restart.ts::restartWithAccount`），带 confirm/awaitCompact/
     awaitExit 回调式协调,不是"一条命令怎么渲染"。UI 上摆成与 resume/new/attach 同级的一级菜单项，
     但底层继续调用编排函数，不经 `buildLaunchPlan`。
  3. **R8 核实为安全**：`e2e/restart-cmd-driver.ts`/`e2e/resume-cmd-driver.ts` 只 import
     `src/account-restart.ts`/`src/accounts.ts` 的具名导出（`restartWithAccount`/
     `detectAccountMismatch`/`accountConfigDir`），**没有一行 import `tabs.ts`**——只要 F09 不改
     `account-restart.ts`/`accounts.ts` 的导出签名，删 tabs.ts 里的 UI 层封装/对齐全套不触及
     e2e driver 的 import 面。

**本席综合两版方案后的取舍：采纳 Agent B 的"维持三轴三机制"作为 R12 的解法**，理由：
- Agent A 收编 `container.kind` 的论据本质是"面向未来第二种容器后端（abduco/dtach）的可扩展性"——
  但那是 `session-backend.ts` 头注里的**推测性**阶段②，未立项、未排期。为一个未确定会发生的未来
  改造正在稳定工作、已被 #76 真机验收锁死的 `canRenderCli` 分支，不符合"不为假设的未来需求设计"
  的原则。
- Agent A 自己也承认：即便收编，`canRenderCli` 的 `send-into` 强制降级检查逻辑和验证方式
  （临时删除、确认恰好 2 条测试转红）完全不变，只是换了个位置——**没有换来真实的简化，纯粹多一层
  间接**。维持现状是零风险、零多余改动的选择，且完全不用碰这条审计过的安全相关代码。
- Agent B 的方案不牺牲任何 F09 的 UI 收敛目标——`enumerateModifierGroups` 无论 container 是不是
  "维度"，产出的 flyout 结构和用户可见行为完全一样,唯一区别只在这一个函数内部几行代码怎么取值。
- R12 **不会被"关闭"**，只会被**降级为已归档的设计决策**——本 Phase B 决策把 `doc/INVARIANTS.md`
  §38 的判断准则写清楚，未来任何人再问"这条新轴该不该进注册表"，有据可查，不再是每次重新审视的
  开放问题。这是"open → accepted with documented rationale"，不是"root cause fixed"，MASTERPLAN
  §6 R12 行的措辞照此写。

## 1. 目标与验收标准（DoD）

- **目标**：把 `tabs.ts` 里因"直连/tmux × 账号/基座 × 归档/存活"正交维度被压平成组合展开而产生的
  约 8-10 个 resume 家族菜单项，收敛成 **1 个 resume 一级菜单项 + 二级 flyout 修饰**（账号组 +
  容器组）；账号徽章从"仅不一致时才显示的警示信号"改为"账号数 ≥2 时恒显示的身份标识"（R7 的语义
  反转，显式说明信息去哪）；删除对齐全套（⇄ 按钮/`alignAll`/`countAccountMismatches`/Ctrl+K 对齐
  命令/`account.align-active` 快捷键）。
- **验收标准**（可验证、可勾选）：
  - [x] `doc/INVARIANTS.md` 新增 §38（注册表 vs 硬编码轴的判断准则）
  - [x] 新文件 `src/launch-menu.ts`：`ModifierOption`/`ModifierGroup`/`enumerateModifierGroups`
        纯函数 + 配套 vitest（账号组 <2 可选账号不出现；容器组恒 2 项；`selected` 标记正确）
  - [x] `TabMenuItem` 支持二级 flyout（`submenu?: TabMenuItem[]`），`showTabContextMenu`/
        `makeTabMenuButton` 支持悬停+点击展开（MASTERPLAN §2.6 已拍板的交互方式）
  - [x] `tabs.ts` 的归档远端 tab 右键菜单：`Resume（直连）`/`Resume（tmux）`/`用基座 resume（直连）`/
        `用基座 resume（tmux）`/每账号 resume 项，收敛成 1 个 `Resume` 一级项 + 账号/容器二级 flyout
  - [x] `tabs.ts` 的存活远端 tab 右键菜单：每账号"重启"/"先压缩再重启"项，收敛进 `Resume` 的
        flyout（选中账号 = 用该账号重启；"先压缩"作为该 flyout 项下的次级 danger 选项或独立小项，
        具体交互在 Phase C 实现时定，不预先锁死 DOM 细节）
  - [x] `attach`/`preview`/`kill` 三个 F51/F60/F79 加的独立动作**不动**（它们不是 resume 家族的
        组合展开，MASTERPLAN §2.6 的收敛范围只针对 resume）
  - [x] `updateAccountBadge`：`shouldShowAccountBadge` 门通过时恒显示账号身份头像（不再要求
        `detectAccountMismatch` 为真才显示）；不一致态仍有视觉区分（沿用 `ghost`/实心区分 live/
        last 来源的既有机制，不新增视觉语言）
  - [x] 删除：`tabs.ts` 的 `alignBtn` DOM 元素+其创建/append/click 监听、`alignableCurrent`/
        `alignSessionToCurrentAccount`/`accountMismatchSids`/`accountMismatches`/
        `countAccountMismatches`/`alignAllToCurrentAccount`/`aligningBatch` 字段
  - [x] 删除：`src/account-commands.ts` 的 `acct-align-active`/`acct-align-all` 命令构造 +
        `AccountCommandsInput.alignSession`/`alignAll` 字段
  - [x] 删除：`src/main.ts` 里喂给 `buildAccountCommands` 的 `alignSession`/`alignAll` 回调 +
        `dispatcher.bind("account.align-active", ...)` 整块
  - [x] 删除：`src/keybindings/actions.ts` 的 `account.align-active` ACTIONS 条目
  - [x] 删除：`styles.css` 的 `.tab-align-btn`/`.is-eligible` 相关规则
  - [x] 相关测试同步更新（`tabs.vitest.ts` 23 处引用、`account-commands.vitest.ts` 18 处引用、
        `keybindings/actions.vitest.ts` 相关断言）——删除失效用例，为新行为（恒显徽章/flyout
        结构）补新用例
  - [x] tsc 0 / npm test 全绿 / cargo test 379 不变（本功能不碰 Rust） / 全部既有 e2e 套件不变
        （本功能不碰 tmux/shell 命令构造，`test:tmux-target`/`ccm-cli`/`ccm-acceptance`/
        `ccm-print-parity`/`tmux-guarded` 应逐字节不受影响）
  - [x] `resume-suite.sh`/`restart-suite.sh` 重跑作为回归确认（R8 核实的 import 面不变不能替代
        真机行为回归——按 MASTERPLAN 教训清单第 2 条，光核 import 面不够）
  - [x] `remote-launch.test.ts`/两个 e2e driver 全程 `git status` 核对零 diff
- **明确不做什么**（防范围蔓延）：
  - 不做 agent 轴的 UI 修饰项（见 §0 共识①）
  - 不把 `restart` 塞进 `LaunchAction`/`buildLaunchPlan` 渲染管线（见 §0 共识②）
  - 不把 `container`/`agent` 收进 `LAUNCH_DIMENSIONS` 注册表（见 §0 采纳的 Agent B 方案）
  - 不做 `remote-launch-run.ts` 的"单一 `runLaunch(plan)`"合并（MASTERPLAN 账本已明确这是比
    F03 大得多的一次性重构，不建议顺手做）
  - 不保留/不重建 `alignAllToCurrentAccount` 的批量一键对齐能力——用户需要对齐多个会话时，
    走新的逐会话 flyout（多点几次），批量捷径不做等价替代（这是本功能唯一一处"能力缩减而非
    等价重排"）。**Phase D UX 审计指出本条最初的论证有问题**：初稿援引 MASTERPLAN §0 推论③
    "自定义在组合层，不在实现层"来支持这个决定，但推论③谈的是**实现层的组合爆炸**（不要为
    第 N 个定制需求在 `buildLaunchPlan`/`LaunchAction` 里加特判）；而 `alignAllToCurrentAccount`
    是完全建立在已有正交原语（逐会话调用既有的 `restartWithAccount`）之上的一层薄编排，保留
    它**不会**给渲染管线增加任何组合复杂度，跟"container/agent 收不收进注册表"（R12，那条援引
    才站得住）根本不是同一类问题。**如实的理由是**：这是一次单纯的产品范围/复杂度取舍，不是
    架构强制——`alignAllToCurrentAccount` 原本带有 idle/busy 两桶分流、两步确认+代价文案、
    `aligningBatch` 重入防护、真实成败汇总这一整套安全工程；F09 的核心目标是"resume/restart
    菜单收敛成动作+修饰"，重建一套与新 flyout 结构等价安全等级的批量入口，工作量与 F09 本体
    实现步骤相当，判定为超出本轮范围，留给用户按需求提出后再做（不是"架构不允许"，是"这轮
    先不做，做了代价不小，需求不明确前不预先造"）。
  - 不改 `session-backend.ts`（container 后端仍是 tmux/none 两态，不接 abduco/dtach）
  - 不改 `AGENT_PROFILE` 单例结构（留给 MA-multi-agent-adapter）

## 2. 与主计划的对接

- **触及的共享面**（对照主计划 §3 账本）：`src/tabs.ts`（F04,F09 行）、`e2e/restart-cmd-driver.ts`
  （F03,F09 行——只读验证，不改其 import 面）、`src/launch-plan.ts`/`launch-dimensions.ts`
  （本功能**不**修改，明确决策见 §0）。
- **遵循的最终形态设计**：`src/tabs.ts` 账本行最终形态"菜单支持二级 flyout；徽章多账号即常显；
  对齐全套删除"——本计划严格按此实现，未偏离。
- **新引入、需登记进账本的共享面**：`src/launch-menu.ts`（新文件，`ModifierGroup`/
  `enumerateModifierGroups`）——F09 独有消费者（`tabs.ts` 的菜单构造），暂不登记进 F03 那条
  "F03 新增"的 IR 账本行（它不碰 `LaunchPlan`/`LaunchContext`，是纯 UI 层发现函数），登记进新一行。
- **本功能的边界**：不改 `src/account-restart.ts`/`src/accounts.ts` 的导出签名（R8 依赖此）；
  不改 `src/launch-plan.ts`/`launch-dimensions.ts`/两个渲染器（R12 决策依此）；不改
  `session-backend.ts`/`agent-profile.ts` 的结构。

## 3. 接口 / 契约设计

```ts
// src/launch-menu.ts —— 新文件，纯模块（同 session-backend.ts/agent-profile.ts 范式）

export interface ModifierOption {
  id: string;              // 稳定 key：账号名 / "__base__" / "tmux" / "none"
  label: string;
  selected?: boolean;       // 当前 ctx 是否已是这个值
  title?: string;
}

export interface ModifierGroup {
  id: "account" | "container";
  label: string;
  options: ModifierOption[];
}

/** 枚举一个远端 origin 当前可用的修饰组。account 组值域来自 accounts.ts（<2 可选账号时
 *  整组不出现，同今天 appendAccountMenuItems 的早退逻辑）；container 组值域固定硬编码
 *  两项（tmux/none）——它的值域不随 LAUNCH_DIMENSIONS 增减而变化,是独立的一条轴
 *  （R12 决策，见 §0）。 */
export async function enumerateModifierGroups(
  origin: string,
  currentContainerKind: "tmux" | "none",
): Promise<ModifierGroup[]>;
```

`TabMenuItem`（`tabs.ts:3102`）新增：
```ts
interface TabMenuItem {
  // ...既有字段不变...
  submenu?: TabMenuItem[]; // F09：二级 flyout；缺省 = 无 flyout（今天所有既有项的行为不变）
}
```

`updateAccountBadge` 的显示条件从
```
if (!shouldShowAccountBadge(...)) return hide();
... if (!b || !b.account) return hide();
... if (!detectAccountMismatch(b.account, current)) return hide();  // ← 删除这一行
```
改为：门通过 + 账号已知 → 恒显示（不再要求不一致）。一致态与不一致态的视觉区分沿用既有的
"live 实心/last 幽灵"机制（`ghost: b.source === "last"`），不新增第三种视觉语言；`tooltip`
措辞按"这是本会话的账号身份"改写，不一致时追加"与当前账号不同"提示（但不再是唯一触发显示的
条件）。

## 4. 实现步骤（严格顺序执行，逐条勾选）

- [x] 步骤 1：`doc/INVARIANTS.md` 新增 §38（判断准则，见 §0 Agent B 给出的三条 checklist：
      效果能否完全表达成 env/args/identity 追加、沉默态能否按 §37 判据归类、影响半径是否
      仅限本次渲染不跨子系统）——纯文档改动，验证：`git diff` 走查措辞准确、不引用不存在的符号。
- [x] 步骤 2：新建 `src/launch-menu.ts` + `src/launch-menu.vitest.ts`（`ModifierOption`/
      `ModifierGroup`/`enumerateModifierGroups`，account 组内部调 `fetchAccounts`/`isSelectable`，
      container 组硬编码两项）——验证：`npx vitest run src/launch-menu.vitest.ts` 绿，覆盖
      <2/≥2 可选账号两态、`selected` 标记。
- [x] 步骤 3：`TabMenuItem` 加 `submenu?`；`makeTabMenuButton`/`showTabContextMenu` 支持展开二级
      flyout（悬停 300ms 延迟 + 点击均可触发，Esc/点外部同现有一级菜单一起收起）——验证：`tsc`
      通过；现有一级菜单（无 `submenu` 的项）行为逐字节不变（既有 `tabs.vitest.ts` 全绿，因为
      `submenu` 缺省即无 flyout）。
- [x] 步骤 4：`tabs.ts` 的 `contextmenu` 监听器改造——归档远端 tab 的 `Resume（直连）`/
      `Resume（tmux）`/`用基座 resume` 系列 + `appendAccountMenuItems` 的归档分支，收敛成
      1 个 `Resume` 项 + `submenu`（走 `enumerateModifierGroups`，`onClick` 按选中的
      account/container 组合调用对应的 `resumeTab`/`resumeTabTmux` 变体）——验证：手工在
      `npm run dev` 里对一个归档远端 tab 右键，确认菜单只剩 `Resume` 一项、hover 展开正确的
      账号+容器组合，点击后行为与旧版对应组合逐一核对一致。
- [x] 步骤 5：存活远端 tab 的账号重启系列（`appendAccountMenuItems` 的非归档分支）同样收敛进
      `Resume` 的 flyout——验证：同步骤 4 的手工核对方式，额外核对"先压缩再重启"这个 danger
      选项的呈现方式（次级确认或 flyout 内独立小项，Phase C 现场定）。
- [x] 步骤 6：`updateAccountBadge` 改为恒显示（门通过+账号已知即显示，不再要求不一致）——
      验证：`tabs.vitest.ts` 补新用例（一致态下也应显示头像，且无 `.is-eligible`/⇄ 相关状态）。
- [x] 步骤 7：删除对齐全套——`tabs.ts` 的 `alignBtn`/`alignableCurrent`/
      `alignSessionToCurrentAccount`/`accountMismatchSids`/`accountMismatches`/
      `countAccountMismatches`/`alignAllToCurrentAccount`/`aligningBatch`；
      `account-commands.ts` 的 `acct-align-active`/`acct-align-all`/`alignSession`/`alignAll`；
      `main.ts` 对应回调与 `account.align-active` 绑定；`keybindings/actions.ts` 的
      `account.align-active` 条目；`styles.css` 的 `.tab-align-btn`/`.is-eligible`——验证：
      `tsc` 0（确认无孤儿引用）；删除/改写对应失效测试；`grep -rn "alignAll\|countAccountMismatches\|align-active" src/` 只剩预期之外的 0 个命中（历史注释里的"⚠k"提及可保留或一并清理，
      视 Phase D 审计意见）。
- [x] 步骤 8：全量门禁——`tsc`/`npm test`/`cargo test`/全部既有 e2e 套件（用 `set -o pipefail`
      + 重定向到文件 + grep 核实，不信内联回显）；额外重跑 `resume-suite.sh`/`restart-suite.sh`
      作为真机行为回归（R8 核实的是 import 面静态不变，不能替代行为验证）。
- [x] 步骤 9：`git status --short` 核对 `remote-launch.test.ts`/`e2e/resume-cmd-driver.ts`/
      `e2e/restart-cmd-driver.ts`/`src-tauri/` 零 diff。

## 5. 测试策略

- **单元**：`launch-menu.vitest.ts`（新）；`tabs.vitest.ts` 改写对齐/菜单相关用例；
  `account-commands.vitest.ts` 删对齐相关用例；`keybindings/actions.vitest.ts` 删
  `account.align-active` 断言。
- **集成 / E2E**：本功能不碰 tmux/shell 命令构造，不新增 e2e 套件；重跑既有
  `resume-suite.sh`/`restart-suite.sh` 作真机行为回归确认（步骤 8）。
- **属性 / 快照**：无（本功能不涉及 CLI 输出/解析）。
- **本功能覆盖率 / 门禁要求**：同仓库既有标准（`npm test`/`tsc`/`cargo test` 三者全绿）。
- **修 bug 时**：先写复现的失败测试再修（沿用本仓库既有纪律）。

## 5.1 实现期修正（Phase C 过程中发现，非计划外扩权）

- **`resumeTabTmux`/`resumeTabTmuxInner` 补齐显式选号能力**：实现步骤 4 时发现，这两个方法此前
  `withAccount` 恒传 `null`——tmux 版 resume 从未支持"显式选一个非默认账号"，只支持"跟随默认"或
  "显式基座"。这是账号×容器没做到真正正交的一个实现缺口（旧扁平菜单里"把此会话切到账号 X
  （resume）"这一项也确实只有直连版，从未有 tmux 版，反映的正是这同一个缺口）。既然 F09 的整个
  目的就是让这两条轴在 UI 上正交，继续保留这个缺口会让 flyout 里"账号 X + tmux"这个组合是假的
  （UI 能选、点了却退化成跟随默认）。已给两个方法加 `accountName?: string` 参数（与 `resumeTab`
  直连版的参数顺序/语义对齐），两处 `withAccount` 调用从 `null` 改传 `accountName ?? null`，
  `follow` 逻辑同步改成 `accountName || useBase ? undefined : {...}`（与 `resumeTab` 同款），并补
  `onUnselectable` toast（此前 tmux 路径完全没有，因为此前 accountName 恒 null 用不上；现在
  accountName 是真参数，缺这个回调会让"选了一个刚好失效的账号"静默退化，不吭声）。
- **Resume flyout 的最终交互形状**：没有做成"一次性列出全部 account×container 笛卡尔积"（那正是
  MASTERPLAN §0 想消灭的东西），而是做成两级级联——`Resume` → [顶层 `tmux`/`直连`两个叶子，跟随
  默认账号，对应旧版 plain「Resume（tmux/直连）」] + [账号组的每一项（基座/具名账号），各自再嵌
  一层 `tmux`/`直连` 子选择]。这样"+1 不是 ×2"体现在：账号组的选项数量增减只影响这一层的条目数，
  不影响顶层结构；新增第三条轴（如果将来真的要给某个 environment 维度也开放 UI 修饰）只需要在
  某一级再嵌一层，不需要重新设计整个结构。
- **account 组阈值订正**：`launch-menu.ts::enumerateModifierGroups` 最初写成"≥2 可选账号才出现
  account 组"，实现中对照原始 `appendAccountMenuItems` 代码发现这与旧行为不符——旧版在**恰好 1 个**
  可选账号时就已经给归档会话一个显式"用基座 resume"逃生口（F01 步骤2，防 #75：只有 1 个账号也可能
  被 follow 误注入，老会话需要不隔离的退路），只有"具名切换到某个账号"这个更细的选项才要求 ≥2。
  已订正为：≥1 时出现 account 组（恒含 `__base__`），≥2 时才追加具名账号——修正后重新过了
  `launch-menu.vitest.ts`。

## 6. 代码审计结果（Phase D）

两个独立 agent 并行审（后端架构 + UX），prompt 均自包含带 MASTERPLAN §0 全文。

- **正确性**：后端架构审计 0 阻塞——R12 决策实施完整性（`launch-plan.ts`/`launch-dimensions.ts`/
  两个渲染器/`account-restart.ts`/`accounts.ts` 导出签名零改动）、`resumeTabTmux` 新增
  `accountName` 参数的线通（两处 `withAccount`/`follow`/`onUnselectable` 与直连版逐项对齐）、
  代次守卫、删除完整性均实测验证通过。UX 审计发现 **1 阻塞**：`updateTabContextMenuItem` 用
  `old.replaceWith(btn)` 整体替换 DOM，账号数据异步到达时会让用户已经 hover 展开的 Resume
  flyout 无预警收起（鼠标没动，浏览器不会因 DOM 替换重新派发 `mouseenter`）——直接命中 R4
  "悬停+点击都可触发"这条契约，越熟练的用户越容易踩中。**已修**：替换前记下 `is-open`
  态，替换后原样带回；补一条用假计时器复现"先 hover 展开、后台数据才到达"时序的回归测试。
- **计划符合度**：Phase C 的 9 个实现步骤逐条对应，无夹带计划外改动；`resumeTabTmux` 补显式
  选号是计划内登记的"实现期修正"（§5.1），非未经计划的范围蔓延。
- **架构符合度（有无引入耦合/打补丁）**：R12 决策（container/agent 不进注册表）严格执行，
  `launch-menu.ts` 是独立发现层，未耦合进 `LaunchDimension` 遍历；`restart` 未被塞进
  `LaunchAction`。两个审计都确认这一点。
- **代码质量**：UX 审计额外发现 3 条重要 + 2 条建议——① 悬停展开 150ms 延迟但收起零延迟，
  命中经典"safe triangle"问题（账号数≥2 时用户斜向移动鼠标去够账号自己的 submenu，路径中途
  经过下一个账号项会被误判"已离开"）——已修：收起也加 250ms 可取消延迟，与展开对称；
  ② 三级级联无视口边缘碰撞检测，tab-bar 可拖到 340px 宽（`main.ts::clampW` 硬上限）时窄窗口
  会让最深一级 flyout 溢出变死菜单——已修：展开前 `getBoundingClientRect` 检测，右侧不够时
  加 `.flip-left` 向左展开；③ ⇄ 按钮删除后，`restartingSids` in-flight 守卫对用户完全不可见，
  重复点击"直接重启"静默无反应——已修：命中守卫时给 toast + Restart 一级项在重启中禁用。
  后端审计另发现 2 条重要：① `openTimer` 在菜单被外部关闭（点外部/Esc）时未被清理，回调对
  已 detach 节点操作（功能无害但不整洁）——已修：模块级 `pendingMenuTimers` 数组，
  `closeTabContextMenu` 统一清空；② `enumerateModifierGroups` 未复刻旧版
  `if (!a.configDir) continue` 静默隐藏过滤——判定为有意的行为改进（显示账号 + 点击后走
  `onUnselectable` 给明确反馈，优于静默隐藏），已在 `launch-menu.ts` 加注释 + 补锁定测试
  防止以后被"顺手"改回静默隐藏。
- **处置**（修了什么 / 重审结论）：1 阻塞 + 5 重要全部修复并补测试锁定（详见
  git log 本次 commit 前的工作记录）；另外落地 3 条建议（Resume flyout 视觉分隔线、
  Restart 账号项 title 说明无基座选项、设置页批量对齐下线提示）+ 重写了 §1"不做什么"
  里对批量对齐删除的论证（原论证误用 MASTERPLAN 推论③，UX 审计指出后已改写为诚实的
  范围/复杂度取舍说明，见 §1）+ 6 处引用已删除 UI（⇄/批量对齐）的过时注释订正 +
  `alignableCurrentAccount` 重命名为 `currentAccountForBadge`（名实相符）。全部修复后
  重跑：tsc 0 / npm test 648 / cargo test 379 / 全部既有 e2e 套件（含 resume-suite 17/
  restart-suite 24 真机回归）绿；`remote-launch.test.ts`/两个 e2e driver/`src-tauri/`
  全程零 diff。无未处置的阻塞项，Phase D 通过。

## 7. 工程审计结果（Phase E）
- **主计划是否仍自洽**：是。R7（徽章语义反转）/R8（对齐全套删除安全性）/R12（container/agent
  注册表决策）三项风险均已按计划落地并在 MASTERPLAN §6 更新措辞（R7/R12 降级为已归档的设计
  决策而非"关闭"，R8 标记已核实安全）。Feature Inventory F09 行、tabs.ts 共享面账本行均已
  同步实际落地内容。
- **是否引入拖累后续功能的耦合/技术债**：没有发现新增耦合。`src/launch-menu.ts` 是独立于
  `LaunchDimension` 的薄发现层，只被 `tabs.ts` 消费，边界清晰；`TabMenuItem.submenu`/`divider`
  是 `tabs.ts` 内部菜单组件的能力扩展，未渗透到其它模块。
- **是否有应现在就做的统一重构（避免后续打补丁）**：没有。F10（剩余账号 UX：面板砍卡片/
  加号一键化/用量）依赖 F09，但 F09 触及的是 tab 右键菜单 + 徽章 + `account-commands.ts`/
  `keybindings`，F10 触及的是设置面板的账号卡片/加号入口/用量显示——两者共享面很窄（只有
  `accounts-section.ts` 里 F09 新加的一行下线提示），不存在"F09 打了补丁、F10 要来解开"的
  预见重叠。
- **工程健康度（结构/文档漂移/构建测试）**：Phase D 审计已发现并清理了 6 处引用已删除 UI 的
  过时注释（详见 §6），修复后未发现新的文档-代码漂移。构建/测试全绿（见 §6 末尾门禁数字）。
  批量对齐删除后的"能力去哪了"这个问题目前靠 §1 的诚实文档 + 设置页一行静态提示解决——
  本仓库没有 changelog/首次运行提示机制是一个更大范围的既有缺口，不在 F09 范围内新建。
- **反馈到主计划的改动（→ Phase F）**：MASTERPLAN §6 的 R7/R8/R12 三行措辞更新（本节已提前
  确认内容准确，Phase F 落笔）；§1 功能清单 F09 行状态更新为"完成"；§3 共享面账本 `tabs.ts`
  行补充 F09 实际落地内容；§7 变更记录追加一条。

## 8. 签收（Sign-off）
- [x] 通过代码审计（无阻塞项——UX 审计发现的 1 阻塞已修复并补测试锁定）
- [x] 通过工程审计
- [x] 主计划已据此更新（含变更记录）
