# U8 — 可发现性 + 快捷键 + 降级休眠固化

> 风险:**低**。依赖 U1–U7。本轮 account-ux 的最后一个功能,做完进 Phase G。
> 性质:接线 + 文案 + 一条纯函数门控。**不新增编排、不新增破坏性语义**。

## 现状(动手前实测)

- **Ctrl+K**(`main.ts:507-560` `buildCommands`):已有 `账号：切默认为 X`(逐可选账号一条)+ `账号：管理…`。
  术语还是 A3 的「默认」,与 U4 起全 UI 的「当前工作账号」不一致。**没有**对齐类命令。
  命令项支持 `hint`(显示该 action 当前 chord)——已有 `chordHint()` helper。
- **快捷键**(`src/keybindings/actions.ts`):`ACTIONS` 是单一真相源;`default: null, available: true`
  已有先例(`behavior.toggle-auto-follow` 等)⇒ "默认不绑"天然支持。category 只有
  `Tab | Term | App | Beh | Panel` 五种。**没有** account 类 action。
- **新会话下拉**(`settings/remote-section.ts:872-895`):选项 `不指定（用登录默认、不注入账号）`,
  预选当前工作账号。术语同样是旧口径。
- **休眠**:`accountAvatarEl` 全仓 5 个调用点 —— chip 本体(`account-chip.ts:118`)、chip 菜单行(`:226`)、
  tab 徽章(`tabs.ts:1007`)、**U7 横幅 + 表格行**(`accounts-section.ts:271/405`)。
  今天**没有任何** "≥2 可选账号" 的门控,`accounts.ts` 里也没有这个谓词。

## DoD(可验证)

1. **纯函数 `selectableAccounts(state)` / `accountColorsActive(state)` 落 `accounts.ts`** —— 一处定义,
   chip、tab 徽章、U7 的 `< 2` 阈值共用,**不许两处各数各的**(一处数总数、一处数可选数会行为分叉)。
2. **休眠固化**:`≥2 可选账号 && state.available` 才激活账号色系统。作用面**只有 chip 与 tab 徽章**;
   **设置账号表(U7 横幅 + 表格行)恒显豁免**(账本 2026-07-25 已拍板:它是唯一能学到"色块↔账号↔邮箱"
   映射的图例面)。
3. **Ctrl+K**:①术语「默认」→「当前工作账号」;②新增 `账号：把当前会话对齐到当前工作账号…`
   (复用 `tabs.alignSessionToCurrentAccount(sid)`,**仅在该会话确实可对齐时才出现**——用
   `tabs.accountMismatchSids()` 判);③新增 `账号：对齐全部不一致会话（k）…`(复用
   `alignAllToCurrentAccount()`,仅 k>0 时出现)。两条都带 `hint` 显示快捷键。
4. **快捷键**:`account.switch-default`(打开 chip 菜单)与 `account.align-active`(对齐当前 tab),
   **`default: null`**(不抢键位)、`available: true`;新增 category `Acct`。
5. **新会话下拉 label** 术语对齐:「不指定」那条说清后果。
6. **不改 U6/U7 的实现**(账本已为此定型 public API 与豁免规则)。
7. 门禁:tsc 0 / npm test 全绿 / build ✓ **0 警告**;关键属性变异验证;fixture 先验证被测维度确实不同。

## 不做

- 不给 `account.align-active` 绑默认键(破坏性动作不该有默认单键)。
- 不改 A6 部署向导里「默认账号名(迁移现有默认账号进来)」——那是 **cc-acct-iso CLI 的 manifest
  `isDefault`** 语义,不是"当前工作账号",改了会与 CLI 行为脱节(账本已记)。
- 不动 U6 的对齐编排 / U7 的设置 IA。
- 不做 per-origin 当前账号(future)。

## 触及的共享面(对照 ★账本)

| 共享面 | 账本最终形态 | 本功能 |
|---|---|---|
| `accounts.ts` 纯函数集 | U1/U6 已定形,**纯增** | 新增 `selectableAccounts` / `accountColorsActive`(纯函数 + vitest) |
| `tabs.ts` 对齐 public API | U6 已定形,**U8 直接复用不再改** | ✅ 只调用 |
| `AccountChipDeps` | U6 定形(推模型) | 可能加 1 个"打开菜单"入口给快捷键;若加则回写账本 |
| `keybindings/actions.ts` | 账本未列(全仓单一真相源) | 纯增 2 条 + 1 个 category ⇒ **新共享面,需回写账本** |

## 实现步骤

1. `accounts.ts`:加 `selectableAccounts(state)` + `accountColorsActive(state)` + vitest(含 0/1/2 个可选账号、
   available=false 三档)。
2. chip / tab 徽章接门控;**设置表不接**(豁免)。
3. `keybindings/actions.ts` 加 `Acct` category + 两条 action;`main.ts` `dispatcher.bind` 接线。
4. Ctrl+K:术语改名 + 两条对齐命令(带可用性判定 + hint)。
5. 新会话下拉 label 术语对齐。
6. 测试 + 变异验证(把休眠门控去掉 → 单账号时仍显色 ⇒ 应红;把对齐命令的可用性判定去掉 ⇒ 应红)。
7. Phase D(风险低 → 1–2 agent)→ E → F。

## 代码审计结果(Phase D)

**阻塞 1 项(已修,并因此修订了本功能的 DoD)——休眠门控装错了地方**

我把休眠加在了 `updateAccountBadge` 上,而 `hide()` 会连 U6 的 `⇄` 一起关掉;与此同时
`⚠k` / Ctrl+K 两条命令 / 新快捷键**都不睡**(它们走 `alignableCurrent`,没有这个门)。三重问题:
1. **鬼影可达**(不是理论):manifest 有 ≥2 个账号但恰好 1 个可选(另一个 `/logout` 过 / 目录丢了 /
   是 in-place 逃生口),且有个活会话跑在那个不可选账号上 ⇒ chip 显 `⚠1`、tooltip 说"点开菜单可
   一键对齐"、Ctrl+K 有对齐命令,**而所有 tab 上一个徽章一个 ⇄ 都没有** —— 用户无从知道是哪个会话。
   这正是 U6 审计当年点名的"信息与操作不同源"裂缝被重新打开。
2. **打破账本不变量**:MASTERPLAN 把 U6 定型为「单一谓词 `alignableCurrent` 统管 ⇄ 显隐 / ⚠k /
   批量 / Ctrl+K」,我给 `⇄` 并联了第二个门,却没回写账本。
3. **休眠的理由对 tab 徽章根本不成立**:U5 之后徽章是「信息才显」——只有 `detectAccountMismatch`
   为真才渲染,所以它**从来不是**"单账号时的颜色噪音",而是**唯一的 per-session 不一致信号**;
   只有 1 个可选账号时这条信息同样成立、甚至更要紧。

→ **修法 + DoD 修订**:休眠**只留给 chip 那个常显的身份头像**(它显示的是"当前账号是谁",单账号时
确实零信息量)。tab 徽章的门控整个撤掉,连带撤掉 `setSessionAccounts` 的第 6 形参与 `main.ts` 的
per-origin 计算 —— **反而少碰了一个共享面**。一句话规则记进账本:**颜色可以睡,信息和操作不能睡。**

**重要(已修)**
1. **DoD 1 未兑现**:U7 的 `< 2` 阈值仍在数**总**账号数,没接新纯函数——正是计划明令禁止的
   "两处各数各的"。分叉可观测:1 个 isolated + 1 个 in-place ⇒ 总数=2(维护区折叠,当成稳态)
   而可选数=1(色系统休眠,当成"你还只有一个号"),同一屏两个组件对"你有几个账号"给出相反判断。
   → `renderMaintenance(selectableAccounts(state).length)`。
2. **Ctrl+K 的可用性判定不可测**:命令构造长在 `main.ts` 的 `DOMContentLoaded` 闭包里(局部 const、
   无导出),把判定改成恒 true 也不会红 —— 计划第 6 步要求的变异验证**根本做不到**。
   → 抽出纯函数 `account-commands.ts::buildAccountCommands` + 11 条 vitest。
3. **`actions.vitest.ts` 的 chord 冲突守卫留了个洞**:我跳过了 `available:false`,而 dispatcher 的
   `rebuildChordTable` **不看 available**,照样把它写进 chord 表("冲突时后定义的赢"),派发时又直接
   `return`。净效果:一条未上线 action 能把同键的**已上线**快捷键彻底打哑,而守卫是绿的。
   → 不再跳过,并加一条"未上线的不得占用默认 chord"。
4. **chip 头像休眠零覆盖**(U4 引入头像时就没测)。补测时我第一版在测试里**重抄了一遍渲染分支**
   —— 那等于测自己、删掉实现里的门控也不会红(正是本轮反复吃亏的同型)。已改成 mock 掉两个数据源、
   **走真实 `refresh()` 路径**。
5. **S1 合成 click**:`element.click()` 对 `display:none` 元素照样派发,能开出一个
   `getBoundingClientRect()` 全 0、飘到视口外的菜单(看不见却吞下一次点击);今天碰不到只是因为
   `pickPrimaryOrigin` 过滤了 daemonless,**是巧合不是设计** → 加 `openMenu()` 显式入口 + 隐藏时早退。
6. **S3 快捷键静默拒绝** → 不可对齐时给一条 4s info toast(不弹 confirm、不猜)。
7. **S4 `activeSid` 被闭包冻住**:命令面板开着时 tab 可能被自动跟随切走,执行的会是"打开面板那一刻
   的会话" → `run` 里重新取 `tabs.activeSessionId()`。
8. **术语残留**:`remote-section.ts` 的「账号（默认＝当前账号）」label;`history.ts` 的降级文案
   「改用**默认账号** resume」**语义也已过时**(U3 之后走的是 `resolveFollowAccount`:lastAccount →
   当前工作账号 → 基座) → 两处都改。

**已知取舍(记档,不改)**
- **chip 菜单行的头像不休眠**(只有 chip 本体的图标休眠):菜单是账号选择器 = 图例面,与设置账号表
  豁免同理。5 个 `accountAvatarEl` 调用点最终只有 1 个接门控,其余 4 个(chip 菜单 / tab 徽章 /
  设置横幅 / 设置表格行)豁免 —— 已写进账本,别留给下轮人考古。
- **Esc 会同时关 chip 菜单和背后的 overlay**(chip 菜单没走 `pushOverlay`,自 A3 起如此;dispatcher 的
  overlay.close 不 stopPropagation)。U8 的快捷键让这条路径更容易走到,但它是**既有缺陷**、修它要动
  A3 的菜单生命周期 —— 超出 U8 范围,已登记进 `.claude/planned-build/BACKLOG.md` §A。
- **`acct-align-all` 没有 hint**:`ACTIONS` 里没有 `account.align-all`(批量是破坏性的,不给它键位),
  所以 DoD 3 的"两条都带 hint"改口径为"能对应到 action 的才带";`acct-default-*` 已补上
  `account.switch-default` 的 hint,教学闭环由那条承担。

## 工程审计结果(Phase E)

- **未冒出账本外新共享面**;反而因阻塞项的修法**撤回**了一个(`setSessionAccounts` 第 6 形参没了)。
  新增 `account-commands.ts` 是纯新增模块(无跨功能消费者)。`keybindings/actions.ts` 新增 `Acct`
  category + `CATEGORY_ORDER` 导出属新共享面,已回写账本。
- **顺手堵了一个与本功能无关的静默坑**:`editor.ts` 里 category 显示顺序是**手抄数组**,TS 不会因为
  `Category` 多一个成员而报错 —— 漏加会让整组快捷键在编辑器里静默消失(用户既看不到也绑不了)。
  已提成 `actions.ts::CATEGORY_ORDER` 单一真相源 + 覆盖完整性测。
- **U9(可选,解钉)**:U6 已记过"解钉后若 live 仍在旧号上跑,是否还算不一致"需先定义。U8 未触及,
  该问题原样留给 U9/用户决定。

## 测试结果

- 门禁:`tsc --noEmit` 0 · `npm test` **564 通过**(U7 的 536 → +28)· `npm run build` ✓ **0 警告**。
- 变异验证(4 个,全部被杀):去掉 chip 休眠门控 → 红;`CATEGORY_ORDER` 漏掉 `Acct` → 红;
  去掉单会话对齐的可用性判定 → 红;`accountColorsActive` 改成数总数 / 删 `available` → 红。
- fixture 纪律(U7 教训):休眠用例先断言 `accounts.length === 2` 再断言休眠,证明差异确实来自
  "可选数"而非"总数"。

## 签收

- [x] 过代码审计(Phase D,阻塞项已修并复验;**DoD 2 的作用面据审计修订**)
- [x] 过工程审计(Phase E)
- [x] 主计划已更新(Phase F)
