# MASTERPLAN — 账号切换 UX/UI 完善(account-ux 子族)

> 承接已交付的 account-isolation A0–A6(v3.2.0)。本轮**只做 UX/UI 完善**,不改隔离/管线底座。
> 三视角设计 agent(UX 交互 / UI 视觉 / 架构数据模型)独立产出、已交叉对比收敛。
> 归并 #68(多账号集成体验)。**不触发发版**(纯前端 + config.json + 复用既有只读命令,daemon 零改)。

## 目标(Goals)
- **G1 当前工作账号**:把 `config.json accounts.defaultName` 从"仅预选新会话对话框"升格为唯一的**「当前工作账号」**——resume / 新会话默认都跟随它(attach 物理不可变,只"照见+给对齐出口")。
- **G2 账号解析优先级**:统一为纯函数 `显式选号 > 会话 lastAccount(粘性) > 当前工作账号 > 基座`;每级过 `isSelectable` 否则下沉;终点 null=基座(逐字节旧行为)。
- **G3 少打断/少手动**:普通 Resume 不再落基座而"跟随";切号入口从"只有右键"提升到 chip / tab hover / Ctrl+K;活会话不一致**可见**(徽章)+ **一键对齐**(复用破坏性重启)。
- **G4 视觉可辨**:账号稳定身份(圆角方块头像 + 不透明 CVD 安全色),一眼看出"当前是谁 / 每个会话在谁 / 谁不一致"。

## 非目标(防蔓延)
- **不做本地 Windows 切号**(A7,远端优先;`origin===null` 全程不接账号逻辑)。
- **不做"运行中会话热切账号"**(物理下限:CLAUDE_CONFIG_DIR 启动读死;对齐=破坏性重启,非热切)。
- **不新增任何 daemon/Rust 写命令**(daemon 只读铁律零妥协);不碰用户 `~/.claude`;不改 `cc-<sid8>` 名。
- **不改隔离/共享策略、manifest schema、A0–A6 已交付行为**。
- **不 push / 不发版**(本轮纯前端,commit 仅作本地检查点;发版由用户单独拍板)。

## 架构 / 关键设计(三视角收敛)
```
状态栏 chip ── 当前工作账号(defaultName 升格)+ ⚠k 不一致计数
     │
     ▼ 拨号(非破坏,不重启任何东西)
accounts.ts ── resolveFollowAccount(纯函数:explicit>last>current>base)
     │            currentWorkingAccount(effectiveDefault 语义别名)
     │            detectAccountMismatch(live≠current 纯函数)
     │            account-color(FNV-1a(name)%8 → 稳定色 slot)
     ▼
withAccount(origin, name|null, run, {follow?}) ── 新增 opt-in follow 第三态
     │   name=string → 显式(A4 不变);name=null 无 follow → 基座(A4 逐字节不变)
     │   name=null + follow → 解析 last>current>base 后注入(新路径)
     ▼
resume/新会话 各站点接线 ── 徽章"信息才显"(live 实心/last 幽灵/一致或未知=无)
                          不一致:tab hover ⇄ + chip 汇总浮层 → restartWithAccount(对齐)
```
- **守逐字节回归契约的结构性保证**:`remote-launch.ts` builder 层**零改动**(契约锚点=有无 configDir 字符串,与 accountName 无关)→ `remote-launch.test.ts` 全部原样绿。
- **新逻辑归属 accounts.ts**(账号模型单一真相),纯函数优先、vitest 锁死。
- **withAccount 与 restartWithAccount 的刻意分离不打破**:follow 只进 withAccount(降级默认起语义),对齐只进 restartWithAccount(中止语义),共享同批原语无逻辑漂移。

## ★共享面账本(最终形态 + 与 A4/A5 差异)
| 共享面 | A4/A5 已定形态 | 本轮最终形态 | 差异 |
|---|---|---|---|
| `remote-launch.ts` builder/runner | 4 runner/3 builder 透传 configDir?,空=逐字节旧 | **零改动** | 无(硬保证) |
| `accounts.ts withAccount` | `(origin, string\|null, run, opts)` null=基座 | additive 增 `opts.follow?:{lastAccount?}`;null/string 两态字节不变 | 新增 follow 第三态 |
| `accounts.ts` 纯函数集 | effectiveDefault/accountConfigDir/isSelectable/sessionBadge/shouldShowAccountBadge | 新增 `resolveFollowAccount` / `currentWorkingAccount` / `detectAccountMismatch` / **`alignableCurrentAccount`**(U6:current 过 isSelectable,mismatch 一路统一用它);`sessionBadge` 返回值加 `source:'live'\|'last'\|'unknown'` | 纯增 + sessionBadge 加字段 |
| `config.json` schema | `accounts.defaultName`(全局) | **不变**,概念更名"当前工作账号",消费面扩大到 resume/新会话 | 语义/文案,无 schema 变更(per-origin 留 future) |
| `history.rs` last_account/list_last_accounts | 存/读,仅喂徽章源② | **Rust 零改动**;前端新增消费者(follow 回填 resume)+ follow-resume 成功照旧 recordLastAccount ⇒ 会话账号自动 sticky | 无 Rust 变化 |
| tabs `setSessionAccounts` | `(rows, emailByName, lastByS, readyOrigins)` | additive 增 `currentByOrigin`(供 mismatch) | 加形参 |
| `tmux_send_keys`/`kill_remote_tmux` | A5 已定 | **零改动**,对齐直接复用 | 无 |
| `restartWithAccount`(A5 编排) | `Promise<void>`;`confirm?` 早在 A5 首版就是可注入点 | **语义零改动**,仅签名 additive:返回 `Promise<boolean>`(true=真走完 kill+resume)。U6 批量据此报**真实**成败(原按发起数报,最坏"0 成功 + 成功汇总") | 返回值 additive;老调用点忽略即可 |
| `tabs.ts` 对齐 public API(U8 复用面) | — | `alignSessionToCurrentAccount(sid):Promise<boolean>` / `accountMismatchSids():string[]` / `countAccountMismatches()` / `alignAllToCurrentAccount()`;内部单一谓词 `alignableCurrent(sid,tab)` 统一 ⇄ 显隐 / ⚠k / 批量 / Ctrl+K | **U6 一次定型**,U8 直接用不再改 |
| `account-chip.ts AccountChipDeps`(★账本外冒出,U6 补记) | A3:`{openSettings}` | `{openSettings, alignAll?, onDefaultChanged?}` + `updateMismatchBadge(count)` **推**模型(照 `tabs.onActiveUsageChanged → usageHud.setActive` 惯例,chip 不反拉 TabManager);chip 构造移到 `tabs` 之后 | 加 2 回调 + 1 推入口 |
| 新 `src/account-color.ts` | — | 纯函数(FNV-1a%8)+ `.acct-avatar` CSS token | 新增文件 |
| `styles.css` | `.tab-acct-badge` 实色药丸;`.accounts-*` 表**无 CSS** | `.acct-avatar` 头像 + `--acct-cN/inkN` 8 色 token;`.tab-align-btn`(**hover 才露面**:JS 打 `.is-eligible`、CSS 管显隐——默认 150px tab 栏里固定图标已占 ~132px,常驻会把标题挤没)+ `.status-account-mismatch` + `.account-picker-action.danger`;补齐裸奔的 `.accounts-*` 网格表 CSS | 新增视觉层 |
| DESIGN §2/§8 | 三种切号语义 / 存储归属 | 补注:①默认账号升格"当前工作账号";②lastAccount 优先级高于① | 文档回写 |

## Features(拆分 + 顺序 + 依赖)
> 三份 agent 的拆分(UX A8x / 架构 Bx / UI 1-6)已合并去重为下列 8+1。

- **U1 地基:纯函数层(解析器 + 账号色 + 徽章 source)**。`resolveFollowAccount`/`currentWorkingAccount`/`detectAccountMismatch`/`account-color(FNV-1a%8)` + `sessionBadge` 加 `source`。全纯函数 + vitest + 老 config 兼容测。依赖:无。**风险:低**。
- **U2 withAccount follow 模式**。additive `opts.follow` 第三态;A4 null/explicit 测原样绿 + follow 新套件 + 迁移守卫测;builder 测未改仍绿。依赖:U1。**风险:中**(须证 A4 零回归)。
- **U3 接线:resume/新会话跟随当前账号**。`resumeTab`/`resumeTabTmux`(归档分支)/`history.runResume`/`history.runNewSession` 远端/`remote-section` 对话框默认接 follow;"不指定"/显式不变;tabs.vitest 默认-resume 断言按契约演进改写+注释。依赖:U2。**风险:中**(站点多 + 默认行为 delta)。
- **U4 「当前工作账号」语义面 + chip 升级**。术语"默认账号"→"当前账号"全 UI 改;chip 显账号头像+名+`⚠k`;切号 toast 三句式(变/不变);设置 label。依赖:U1。**风险:低-中**。
- **U5 tab 徽章升级(信息才显 + live/last 层次 + 头像)**。==当前账号→无徽章;≠当前 live→实心头像;last→幽灵头像;未知→无(退 tooltip)。`.tab-acct-badge`→`.acct-avatar`。依赖:U1、U4。**风险:中**。
- **U6 不一致检测 + 一键对齐**。tab hover `⇄`(仅活会话+精确 @ccm_sid)+ chip `⚠k` 汇总浮层 + 批量对齐(空闲/回合中两步确认);均复用 `restartWithAccount`;"current 不可选不对齐"测。依赖:U1、U4、U5。**风险:中高**(破坏性路径)。
- **U7 设置账号组 IA 重排 + 补表格 CSS**。顶部"当前账号"横幅 + 网格化账号表(填补裸 `.accounts-*` CSS)+ 维护区收进 collapsible 默认折叠;不破未启用向导/降级分支。依赖:U4。**风险:中**(关联 #47)。
- **U8 可发现性 + 快捷键 + 降级润色**。Ctrl+K 文案改名 + 新增"对齐会话到当前账号…"命令;快捷键 `account.switch-default`/`account.align-active`(默认不绑);新会话下拉 label;单账号/降级休眠固化(徽章色系统仅 ≥2 可选账号 && readyOrigins 才激活)。依赖:U1–U7。**风险:低**。
- **U9(可选)解钉"让此会话跟随当前账号"**。清 sid 的 lastAccount → 之后跟当前号。依赖:U1。**风险:低**。

**顺序**:U1 → U2 → U3(行为线);U1 → U4 →(U5 → U6)(视觉/对齐线);U4 → U7;全部 → U8。文档回写(账本/DESIGN)并入各功能 Phase F。

## 安全 / 验证(每功能 DoD 硬门)
- **纯函数优先**:解析器/色引擎/mismatch 全 table-driven vitest。
- **回归契约硬门**:`remote-launch.test.ts` 一行不改保持绿(builder 未动)= 逐字节守法证据;`accounts.vitest.ts` withAccount null/explicit 老套件保持绿 + follow 新套件 + 迁移守卫测。
- **破坏性对齐**:继承 `restartWithAccount` §5.2 失败语义(kill 失败中止防双进程 / 超时降级);批量逐会话独立 + 回合中二次确认。
- **实测门禁(防终端污染纪律)**:每步 Read 回盘核实;tsc/vitest/cargo **重定向到文件再 Read + grep 计数**,绝不信内联"绿"、绝不 watch;门禁命令用 pipefail。每功能收尾:tsc0 / vitest 全绿 / build ✓ / 明暗主题各扫一眼 / 真机零改动(不碰 ~/.claude)。

## 开放决策(呈用户,审批时定)
> 加★ 的 4 条经 AskUserQuestion 请用户拍板;其余为**推荐默认**,用户不反对即照做。
1. ★**优先级**:lastAccount(会话粘性)是否高于当前工作账号?(推荐:是——拨号只管新/无主会话,搬号走显式对齐,不静默改写老会话烧缓存。)
2. ★**无主会话 resume**:无 lastAccount 的"陌生来源"会话默认是否跟随当前账号?(推荐:是——实现"拨号即生效";否则只有本工具带账号起过的才跟随。)
3. ★**徽章信息才显**:==当前账号的会话是否不挂徽章(出现彩色头像=不一致)?(推荐:是——tab 栏更干净、不一致更醒目。)
4. ★**批量"全部对齐 k"**:是否要批量破坏性对齐入口(空闲默认纳入、回合中二步确认)?(推荐:是。)
- 推荐默认(不单独问):当前账号 = **全局复用 defaultName**(per-origin 留 future);术语全 UI 改"默认→当前";头像=**圆角方块**;色=**hash FNV-1a%8**(per-account 覆盖留 future);withAccount 信号=**additive opts.follow**;`resumeTabTmux` 归档分支**纳入**;批量进度=**每会话 toast + 末尾汇总** MVP;对齐**含"先压缩上下文"变体**(复用 compactFirst)。

## 变更记录
- 2026-07-24 建 account-ux 主计划(Phase A)。三视角设计 agent(UX/UI/架构)独立产出并交叉收敛。待用户拍板 4 决策 + 主计划 → 授权后 /loop 全自动跑 U1→U9 + Phase G。
- 2026-07-24 **用户批准主计划 + 4 决策全选推荐项**(粘性优先 / 无主会话跟随 / 徽章信息才显 / 批量对齐两步确认)。授权全自动 loop 连续跑。进 U1。
- 2026-07-25 **U6 Phase F 账本回写**(3 个对抗审计 agent 的结论):
  ① `restartWithAccount` 由「零改动」→ 返回值 additive(`Promise<boolean>`),因为批量必须报真实成败;
  ② 补记**账本外冒出**的共享面 `account-chip.ts AccountChipDeps`,并按本仓既有惯例(`onActiveUsageChanged`)
     定型为**推**模型 —— 原实现让 chip 反拉 TabManager + 前向引用,靠「中间恰好没有 await」的隐式不变量
     避 TDZ,已改掉;
  ③ `accounts.ts` 增 `alignableCurrentAccount`(current 过 `isSelectable`),兑现 U6 DoD「current 不可选不对齐」;
  ④ 预定 `tabs.ts` 对齐 public API 的最终形态,**U8 的 Ctrl+K/快捷键直接复用、不必再改**(避免下轮打补丁);
- 2026-07-25 **U8 Phase F 账本回写**:
  ① **休眠规则最终形态(据 D 审计修订)**:`accountColorsActive`(≥2 **可选**账号 && available)
     **只作用于状态栏 chip 那个常显的身份头像**。全仓 5 个 `accountAvatarEl` 调用点里,
     **只有 chip 本体接门控**;chip 菜单行 / tab 徽章 / 设置横幅 / 设置表格行**全部豁免**。
     **规则一句话:颜色可以睡,信息和操作不能睡。** 理由:tab 徽章自 U5 起是「信息才显」——
     只在 `detectAccountMismatch` 为真时才渲染,所以它不是"单账号时的颜色噪音",而是**唯一的
     per-session 不一致信号**;在那里休眠会造成 chip 报 ⚠k、Ctrl+K 有对齐命令、而 tab 上一个
     徽章一个 ⇄ 都没有的鬼影(U6 审计点名过的"信息与操作不同源"裂缝)。
     ⇒ MASTERPLAN 第 52 行「U6 单一谓词 `alignableCurrent` 统管 ⇄ 显隐 / ⚠k / 批量 / Ctrl+K」
     **仍然成立、未被并联第二个门**(实现中一度打破,已撤回)。
  ② `accounts.ts` 纯函数集再增 `selectableAccounts` / `accountColorsActive`;
     **"你有几个账号"全仓一律数可选数**(U7 维护区默认展开阈值已改接同一函数,不再数总数)。
  ③ 新共享面 `keybindings/actions.ts`:新增 `Acct` category + `CATEGORY_ORDER` 导出
     (原为 editor.ts 里手抄的数组——漏加会让整组快捷键在编辑器里**静默消失**且 TS 不报错)。
  ④ 新模块 `account-commands.ts`(Ctrl+K 账号命令的纯函数构造):原逻辑长在 main.ts 闭包里,
     "命令何时出现"完全不可测。以后往 Ctrl+K 加账号命令一律加在这里。
  ⑤ 遗留(不在本轮范围):chip 菜单没走 `pushOverlay`,Esc 会同时关掉它和背后的 overlay(A3 起既有)。
- 2026-07-25 **U7 Phase F 账本回写**:
  ① `styles.css` 行的「补齐裸奔的 `.accounts-*`」已兑现(**18/20**——`.settings-accounts`/`.accounts-body`
     是纯容器不需要规则,如实记账不充数);列定义放**表**上、行用 `subgrid`(+`@supports` 退回 flex),
     **别再把 grid 打在行上**——那样每行各自成 grid,列跨行对不齐;
  ② **新决定(供 U8 直接用,免得回头改 U7)**:U8 的「徽章色系统仅 ≥2 可选账号 && readyOrigins 才激活」
     休眠规则**只作用于 chip 与 tab 徽章**;**设置账号表(横幅 + 表格行)是"颜色图例面",恒显豁免** ——
     它是全应用唯一能学到"色块↔账号↔邮箱"映射的地方,单账号期休眠会导致加第二个号时突然满屏彩块。
     另:U7 用的 `accounts.length < 2`(维护区默认展开)与 U8 的可选账号计数应抽**同一个纯函数**,别分叉。
  ③ 记档:本仓**没有浅色主题**(`color-scheme: dark`,无 `prefers-color-scheme`),且 `theme.ts` 的 TOKENS
     只覆盖 11 个 token,`--accent`/`--border-*`/`--overlay-hover`/`--text-faint` 不可换肤 ——
     以后功能的 DoD 别再写"明暗主题各扫一眼",改写"零硬编码颜色 + var 全部有定义"。
- 2026-07-25 U6 记录(续)⑤ U6 计划口径的两处偏离已记档:`⇄` 回到计划定的 **hover**(兼修布局挤压);「⚠k 汇总浮层」本轮用
     两步 confirm 顶替(已逐行列出会话→目标账号),**浮层顺延 U8**;对齐的 `compactFirst` 变体本轮不做
     (右键菜单已有)。
