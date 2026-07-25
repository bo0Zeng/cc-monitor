# U7 — 设置「账号」组 IA 重排 + 补表格 CSS

> 风险:**中**。依赖 U4(色 token / `.acct-avatar` / 术语)。关联 issue #47。
> 纯视觉 + 信息架构层,**不碰账号语义**(不改解析优先级、不改 setDefaultName 行为、不新增破坏性动作)。

## 现状(动手前实测)

- `src/settings/accounts-section.ts` 454 行,渲染 4 个分支:`hidden`(daemonless)/`needs-update`
  (老 daemon)/`not-enabled`(部署向导)/`ready`(账号表 + 维护区)。
- **20 个 class 零 CSS**(逐个 grep 证实):`settings-accounts` `accounts-bar` `accounts-bar-label`
  `accounts-origin-select` `accounts-refresh` `accounts-body` `accounts-meta` `accounts-table`
  `accounts-row` `accounts-row-mark/-name/-email/-badge/-dir/-actions` `accounts-hint`
  `accounts-not-enabled` `accounts-ne-title/-body` `accounts-info`。
  ⇒ 账号表现在是**裸 div 竖排**:无对齐、无列、无分隔,`configDir` 长路径撑爆宽度。
- A6 的向导/维护区 **已有** CSS(`.accounts-wizard`/`.accounts-maint*`,styles.css:5598+),本轮不重写。

## DoD(可验证)

1. **顶部「当前工作账号」横幅**:ready 态下,账号表上方一条横幅,显当前账号的 `.acct-avatar` 头像
   + 名 + 邮箱 + 一句"管辖范围"说明(新会话 / 没指定过账号的 resume)。无当前账号(全不可选)时
   显中性提示而非空白。
2. **账号表网格化**:`.accounts-table` 用 CSS Grid 定列(★ / 名 / 邮箱 / 状态 / 路径 / 操作),
   列对齐;`configDir` 单行省略号 + `title` 全文;行分隔线 + hover 高亮;`.current` 行左侧强调条。
   每行复用 U4 的 `.acct-avatar`(与 chip / tab 徽章同色 ⇒ 三处肉眼可对应)。
3. **维护区收进 `<details>` 默认折叠**:`.accounts-maint` 包进 collapsible,标题「维护」,
   默认 **closed**——加账号/自检/补链都是低频且带 danger,不该常驻占版面。
4. **补齐 20 个裸 class 的 CSS**,含 `.accounts-bar` 的横向布局与 `.accounts-info` 的降级文案样式。
5. **降级分支零回归**:daemonless / 老 daemon / 未启用向导 / 无远端 四条路径的 DOM 与文案不变
   (只允许获得 CSS)。**必须有测**。
6. 门禁:tsc 0 / npm test 全绿 / build ✓ / 明暗主题各扫一眼(用现有 token,不硬编码颜色)。

## 不做(防范围蔓延)

- 不改账号语义 / 优先级 / `setDefaultName` 行为;不加任何破坏性动作(对齐入口是 U6 的事)。
- 不重写 A6 向导与维护区的**内部**结构(只把整块收进 `<details>`)。
- 不动 `.acct-avatar` / `--acct-cN` token 本身(U4 已定形,只复用)。
- 不做 per-origin 当前账号(账本明列 future)。
- 不碰 Ctrl+K / 快捷键 / 降级休眠(U8)。

## 触及的共享面(对照 ★账本)

| 共享面 | 账本最终形态 | 本功能怎么做 |
|---|---|---|
| `styles.css` | 「补齐裸奔的 `.accounts-*` 网格表 CSS」 | ✅ 正是本功能;新增 20 个 class 的规则,全用既有 token |
| `.acct-avatar` / `--acct-cN` | U4 已定形 | **只复用**,不改定义 |
| `accounts.ts` 纯函数集 | U1/U6 已定形 | 只读 `currentWorkingAccount` / `isSelectable`;**不新增** |
| `AccountChipDeps` | U6 定形(推模型) | 不碰 |

**结论:不引入账本外新共享面。** 若实现中发现要动 `accounts.ts` 语义 → 停,回 Phase B。

## 实现步骤

1. CSS:在 styles.css 的 A6 向导块**之前**插入 `.settings-accounts` 区块(bar / body / meta / table /
   row / 横幅 / info),全部用现有 token(`--bg-*`/`--text-*`/`--border*`/`--warn`/`--acct-*`)。
2. `accounts-section.ts`:`renderTable` 顶部插横幅(新私有方法 `renderCurrentBanner`)。
3. `accountRow`:插入 `.acct-avatar` 头像(在 mark 之后、name 之前)。
4. 维护区包 `<details class="accounts-maint-wrap">`(默认 closed),`<summary>维护</summary>`。
5. 测试:新建 `src/settings/accounts-section.vitest.ts`——4 条降级分支的 DOM 断言 + 横幅在
   无当前账号时的表现 + 维护区默认折叠 + 头像颜色槽与 chip 一致。
6. 变异验证:故意把"维护区默认折叠"改成默认展开、把降级分支改成渲染表格,看测试是否变红。

## DoD 结账(如实,含未足额兑现的)

| DoD | 结账 |
|---|---|
| 1 横幅 | ✅。`def===null` 那支在 ready 分支下**不可达**(deriveUi 保证 accounts≥1、effectiveDefault 必非 null),保留为防御码并已注明。 |
| 2 网格化 + **列对齐** | ✅ **但初版没兑现**——见审计阻塞项,已改 subgrid 修正。 |
| 3 维护区折叠 | ✅,且默认值改为**按状态给**(见审计重要-3)。 |
| 4 「补齐 20 个裸 class」 | ⚠ **18/20**。`.settings-accounts` 与 `.accounts-body` 是纯容器,不需要规则 —— 但 DoD 是字面承诺,如实记 18/20,不当"符合"过掉。 |
| 5 降级零回归 + 有测 | ✅。5 条出口(含"拉取失败")逐条比对旧版,`renderNotEnabled`/`info` 一字未改;每条都断言"不长出 ready 三件套"。 |
| 6 门禁 + 明暗主题 | ⚠ **DoD 前提有误**:本仓**没有浅色主题**(`color-scheme: dark`,全文件无 `prefers-color-scheme`/`[data-theme]`),"明暗各扫一眼"不可执行。已改口径为「零硬编码颜色 + 全部 token 有定义」,并如实记下:`--accent`/`--border-*`/`--overlay-hover`/`--text-faint` **不在 theme.ts 的换肤范围**(TOKENS 只有 11 个),用户把 bg 调浅时行分隔线/hover 会变淡 —— 这是**全仓既有约定**(`.tab.active` 等同款),非本功能引入。 |

## 代码审计结果(Phase D:2 个对抗 agent 并行)

**阻塞 1 项(已修)——「列对齐」写了但不生效**
`display:grid` 打在 **`.accounts-row`(每一行)** 上而 `.accounts-table` 只是 flex column ⇒ **每行是各自独立的 grid**,`auto`/`fr` 轨道按各自那行的内容定宽,列**跨行对不齐**。两个 agent 独立命中。
且这不是边角:当前账号那行**不渲染「设为当前账号」按钮**、in-place 账号不渲染「登录终端」⇒ 行宽本就差 ~85px,
最显眼的 `.current` 行截断点与别行参差。**恰恰是本功能要解决的问题本身**。
→ 改为列定义放 `.accounts-table`、行用 `grid-template-columns: subgrid` 继承;并加
`@supports not (grid-template-columns: subgrid)` 退回 flex(不支持时若不兜,行会塌成单列竖排,比不对齐更糟)。

**重要(已修)**
1. 横幅与表下 hint **逐字重复**同一句管辖范围(IA 重排新增了第三份拷贝却没消重)→ hint 改为只讲横幅没说的(不动远端/不碰凭据/去登录)。
2. 维护区默认折叠在**首次使用**时挡路:刚跑完 A6 部署向导回来正好是「ready + 只有 1 个账号」,此刻唯一该做的就是"加第二个账号",却被折进去 → 默认值改成 `accounts.length < 2`(稳态仍折叠),并加 1 账号 fixture 测锁住。
3. `reload()` 重建 DOM ⇒ `<details>` 每次刷新都被收回、输入到一半的账号名被清空(**U7 引入的新摩擦**)→ 展开态提到实例字段,用户表态后保持。
4. 头像色槽用例**弱绿**:fixture `z`/`b` 恰好都落槽 5(实算证实),把实现改成"永远取同一个名"照样过 → 换成跨槽 fixture(`wei`=0 / `amy`=6)并加前置断言"两者槽位必须不同"。
5. `.none` 语义扭曲:实际含义是"当前账号存在但不可用",与 class 名/注释/`--warn` 取值三处互相矛盾 → 改名 `.unusable`,注释与取值对齐,并给不可用态用 U5 既有的 ghost 头像(不造新视觉概念)。
6. 无断言锚住「行子元素数 == grid 列数」——以后往行里多 append 一个元素,536 个用例仍全绿而布局整体错位 → 补断言 + 注释写明双向绑定。

**建议(已办)**:`.current` 底色与 hover **同值**(当前行 hover 零反馈)→ 改 `color-mix(accent 9%)`;
账号名 64 字符无省略会压没邮箱/路径两列 → 加 ellipsis;路径列权重 **1.2fr > 邮箱 1fr 倒挂**
(isolated 账号的 configDir 逐行只差最后一个字符,邮箱才是分辨"学校号/私人号"的字段)→ 对调为 1.2fr/0.8fr;
「已登录」被涂绿违反决策③「信息才显」(健康态每行都绿=恒真信息,反稀释真正的 warn)→ 健康态改 faint;
`.accounts-not-enabled` 与内层 `.accounts-wizard` **框里套框** → 去掉外框;
`list-style: revert` 是 no-op 且全仓唯一的原生三角 → 改为仓库惯例的 `▸/▾` 自绘 + `user-select:none`;
`.accounts-maint-title` 成死 CSS(其 div 已被 summary 取代)→ 删;
设置窗无 `min_inner_size` 且双层 `overflow:hidden`,拖窄会把最右按钮直接裁掉且点不到 → `.accounts-table` 加 `overflow-x:auto`。

**已知取舍(不修,记档)**
- **行 hover 高亮是"可读性"而非"可点击"暗示**:整行不可点。保留是因为 7 列横向扫读需要行锚点(与 `.tab` 同款视觉语言);
  已用更实的 `.current` 底色把两者区分开。若日后觉得误导,再考虑"整行可点=设为当前账号"。
- **没有复用 `CollapsibleGroup`**(仓里已有、带 localStorage 持久化与 a11y):它产出的是 `settings-group` **顶层组**,
  嵌进 `.settings-accounts` 里语义别扭。这是**计划层**就定的 `<details>`(步骤 4),实现忠实照做;
  代价是维护区展开态不跨窗持久(仅本次会话内保持)。记档,不改。

## 工程审计结果(Phase E)

- **未冒出账本外新共享面**:改动仅 `accounts-section.ts` + `styles.css`,恰是账本 `styles.css` 行预定的
  「补齐裸奔的 `.accounts-*` 网格表 CSS」;唯一新 import 是既有的 `accountAvatarEl`(只做新增消费者,不改定义)。
  `accounts.ts` / `AccountChipDeps` / `tabs.ts` 对齐 API / `restartWithAccount` **一处未碰**。
- **对 U8 的影响 = "加两行守卫"级,但有一个范围决策今天就该拍**:U8 要固化「徽章色仅 ≥2 可选账号 && readyOrigins 才激活」,
  而全仓 `accountAvatarEl` 有 5 个调用点(chip 本体/chip 菜单/tab 徽章/U7 横幅/U7 表格行)。
  **已写进账本的决定**:休眠只作用于 **chip 与 tab 徽章**;**设置账号表(横幅 + 行)是"颜色图例面",恒显豁免** ——
  它是全应用唯一能让用户学到"这个色块=这个账号=这个邮箱"的地方,单账号期休眠掉,等加了第二个号会突然满屏彩块。
  这样 U8 一行都不必回头改 U7。
- **给 U8 的提醒**:`accounts-section.ts` 部署向导里那句「默认账号名(迁移现有默认账号进来)」是
  **cc-acct-iso CLI 的 manifest `isDefault`** 语义,**不是**"当前工作账号",U8 做术语扫尾时别一起改,否则与 CLI 行为脱节。
- **`< 2` 阈值**:U7 维护区默认展开用的 `accounts.length < 2` 与 U8 休眠判据(`isSelectable` 计数 ≥2)是近亲但**不同源**,
  U8 落地时应抽同一个纯函数,避免"一处数总数、一处数可选数"行为分叉。

## 测试结果

- 门禁:`tsc --noEmit` 0 · `npm test` **536 通过**(U7 前 522 → +14)· `npm run build` ✓ **0 警告**。
- **实现中自查出并修掉一个真 bug**:CSS 注释里写了 `-*` 紧跟斜杠 ⇒ **提前闭合注释**,构建报
  `css-syntax-error` 且后半段注释文字漏进样式表。tsc 与 vitest 都看不见 CSS,**只有 build 抓得到** ——
  已在该块头部留下自我提醒。
- 变异验证(4 个,全部被杀):维护区改成恒定默认展开 → 红;daemonless 降级分支改成也渲染表格 → 红;
  行头像固定成同一个名字(颜色不再跟随该行账号)→ 红;往行里多塞一个子元素(与 7 列脱节)→ 红。

## 签收

- [x] 过代码审计(Phase D,2 agent 并行;阻塞项已修并复验)
- [x] 过工程审计(Phase E,含"设置表豁免颜色休眠"的账本决定)
- [x] 主计划已更新(Phase F)
