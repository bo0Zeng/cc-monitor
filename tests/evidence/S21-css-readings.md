# S21 · `设计/40` 九步 ＋ `设计/41` 十一件：做了什么、没做什么、凭什么

> 对应 `设计/99 §4` 步 **21**（路 J）。写区只有 `src/styles.css` ＋ CSS 文件族 ＋
> `tests/evidence/`。零 TS、零 Rust、零 `tests/` 判据文件改动。
>
> **复算全部读数**（从仓根）：
> ```bash
> bash tests/evidence/U-scale2-run.sh                      # 两个真引擎重打真高 → 刷金标准
> npx vitest run tests/scale2-height-truth.vitest.ts       # 秤 2 门禁
> npx vitest run tests/scale3-one-screen-gate.vitest.ts    # 秤 3 门禁
> npx tsc --noEmit
> npx stylelint "src/**/*.css"
> ```

---

## 0. 一页结论

| | 改前 | 改后 |
|---|---|---|
| CSS 文件族 | `src/styles.css`（7085 行，单文件） | `src/styles.css`（6986）＋ **`src/styles/tokens.css`（170）** |
| 用了但从未定义的变量 | **15 个 / 37 处**（3 处无兜底 ⇒ 属性被整条丢弃） | **1 个 / 6 处**（`--bg-1`，**刻意留着**，见 §冲突） |
| 死规则（TS 零引用） | 13 个类 | **0** |
| `!important` | 11 | **0** |
| 裸数字 `z-index` | 37 条规则、18 个量级、最大 100001 | **0**；39 处全是 `var(--z-*)`，10 档刻度 |
| 层叠上下文 | `#message-stream` / `#tab-bar` 都不是 ⇒ 子孙逃出去压叔伯 | 两个都是（`--z-base`） |
| 级联层 | 0 处 `@layer` | 七层已声明，`reset` / `tokens` 已包 |
| `@container` / `:has()` | 0 | 0（**没做**，见 §件 5） |
| stylelint 报错数 | 50 | **47**（一条没多；搬进 `tokens.css` 的那 3 条顺手修掉 ⇒ `tokens.css` **零报错**） |

🔴 **真高与 DOM 指纹：83 张卡逐条比对，`trueContentBox` / `trueBorderBox` / `htmlHash` /
`estBrowser` / `estAppliedBrowser` / `padBorder` **一个字节都没变**。**（表在 §4）

---

## 1. `设计/40 §7` 九步逐步交代

### 步 1 ✅ 部分做 —— 三处无兜底修了两处，第三处卡在 40/41 冲突上

| 住址（改前行号） | 改前 | 改后 |
|---|---|---|
| `styles.css:880` `.command-bar-input` | `font-size: var(--font-size)` | `var(--font-size-base)` ✅ |
| `styles.css:3915` `.remote-machine-toggle` | `color: var(--text-muted)` | `var(--text-2)` ✅ |
| `styles.css:5837` `.panorama-ann-input` | `background: var(--bg-1)` | **没动** ⚠ 见 §冲突 |

前两处是「属性被浏览器整条丢弃」的真 bug：命令栏输入框原来用 `input` 的 UA 默认字号
（约 13.33px）且不跟随用户改的字号设置；远端机器 toggle 原来该变灰的地方没变灰。

### 步 2 ✅ 做 —— 14 个近似名统一到真 token，31 处

```
--font-size     → --font-size-base   1     --mono          → --font-mono     2
--text-muted    → --text-2           1     --overlay-active→ --state-active  1  ← 新族
--danger        → --error            7     --ok            → --success       1
--fg-dim        → --text-2           4     --link          → --color-link    1  ← 新增
--input-bg      → --field-bg         4  ← 新族   --text-dim → --text-2       1
--text-3        → --text-2           3     --font          → --font-base     1
--fg            → --text             2     --hover         → --state-hover   2  ← 新族
```
顺手把这 31 处的**硬编码兜底一并删掉**（`var(--fg, #ddd)` 那种）：兜底在的时候，
「用户换主题这 31 处一动不动」（`真相源/51 §1` 记的那条）。现在它们跟着 `theme.ts` 走。

⚠ 这带来一次**有意的取色变化**：`var(--danger, #e06c75)` → `var(--error)` ＝ `#d96868`。
方向是对的（跟随主题），但确实不是同一个红。

现打核对：
```
剩余未定义变量: 1
  --bg-1   6 处，无兜底 1        ← 刻意留着
```

### 步 3 ✅ 做 —— 删 13 个死规则。**但普查那份名单是错的，改前先纠了**

🔴 **`真相源/51 §2` 那 13 个里有 2 个是活的**，而且正是这个仓治过的那一族
（「靠字符串拼出来的名字，直查搜不到」）：

```ts
// src/first-run-hint.ts:65,162,170
export const FIRST_RUN_HINT_CLASS = "status-first-run";
open.className  = `${FIRST_RUN_HINT_CLASS}-open`;      // ⇒ .status-first-run-open
close.className = `${FIRST_RUN_HINT_CLASS}-dismiss`;   // ⇒ .status-first-run-dismiss
```
⇒ **这两条没删。** 顺带还查出第三个假阳性：`.katex-display` 由
`node_modules/katex/dist/katex.css` 产出，任何「CSS 类 ⊆ TS 字面量」的尺子都会把它判死。
⇒ **`设计/40` 步 9 那条类名对账脚本，白名单里必须额外登记：`FIRST_RUN_HINT_CLASS`
这种常量拼接 ＋ 第三方库自产的 `katex-*`。**（今天的 11 个前缀白名单不够。）

**死值验**（先证明它今天真的没有生效者，再删）。三把尺子，同一批名字：

| 名字 | `src/**/*.ts` ＋ `index.html` | 257 个 `*.rs` | `tests/**` | **生产 bundle `.build/dist/assets/*.js`** | 裁 |
|---|---|---|---|---|---|
| 13 个死的（见下） | 0 | 0 | 0 | **0** | 删 |
| `.status-first-run-open` | 0（直查） | 0 | 0 | **1** | **留** |
| `.status-first-run-dismiss` | 0（直查） | 0 | 0 | **1** | **留** |
| `.katex-display` | 0 | 0 | 0 | **3** | **留** |

⇒ 第四列就是那个「有反应 ⇒ 它在生效」的判据：**同一把尺子既判出 0，也判出非 0**，
所以它不是一把永远说 0 的坏尺子。

实删的 13 个：
```
.accounts-bar-label              .panorama-file-note
.cc-bus-agent-meta               .panorama-file-searchbtn
.cc-bus-hooks-out                .settings-cc-profiles      ┐
.cc-bus-source                   .settings-cc-profile-card  │ 一整个组件
.history-group-icon              .settings-cc-profile-head  │ 5 条，被重写掉了
.settings-group-empty            .settings-cc-profile-name  │ 样式留在原地
                                 .settings-cc-profile-path  ┘
```
三条单独说：
- **`.settings-group-empty`** —— `tests/settings/panel-groups.vitest.ts:261` 有一条
  **正面断言** `expect(document.querySelector(".settings-group-empty")).toBeNull()`，
  逐字写着「F82b 那个『留空占位』组已随 S2 一并消失」。**判据自己说它不该存在。**
- **`.cc-bus-hooks-out`** —— `tests/settings/cc-bus-hooks-section.vitest.ts:163` 的注释说
  「输出面现在由 `buildPasteBlock` 产出，`cc-bus-hooks-out` 落在它的根节点上」，
  **但现打调用处（`cc-bus-hooks-section.ts:203`）根本没传 `className`** ⇒ 那句注释是旧的，
  类名落不到任何节点上。（⚠ 这条注释是 `tests/` 里的，不在写区，没改，**记在这里**。）
- **`.settings-cc-profile-*`** —— 只删了 `s / -card / -head / -name / -path` 这 5 条；
  同前缀的 `-status` / `-badge` / `-buttons` / `-warn` **是活的**（`settings/cc_integration.ts` 现打在用），
  原地留了一条注释说明这件事，免得下一个人按前缀一把梭。

### 步 4 ✅ 已经是做过的 —— 本轮只核，没动

`.user-inputs-toggle` 今天**有三条规则**（`styles.css` 现打 `.user-inputs-toggle{}` /
`:hover:not(:disabled)` / `:disabled`），`views/user-input-panel.ts:75` 在挂它。
`设计/10` 步 2（浮层挪位 `top: 8px → 40px`）也已落地。⇒ **这一步前面的轮次已经做完。**
本轮给它加了一句：那个 `top: 40px` 现在是第二道保险（第一道见步 6）。

### 步 5 ❌ **没做 —— 必须动 TS，超出写区，上报**

```ts
// src/main.ts:425-429   ← 不在写区
const applyCollapse = (): void => {
  document.body.classList.toggle("tabbar-collapsed", window.innerWidth < 980);
};
window.addEventListener("resize", applyCollapse);
applyCollapse();
```
CSS 那一半（`body.tabbar-collapsed` 的 7 条规则改挂 `@media (max-width: 979px)`）
**刻意也没做** —— 只改 CSS 不删监听 = 同一个断点两处实现，比今天更坏。
⇒ 这一步要 CSS ＋ `main.ts` **同拍**，请拍板是否放开 `main.ts` 那 5 行。

现打复核（确认这一步的前提今天还成立）：`tabbar-collapsed` 这个类在全仓
**只被写、从没被读**（仅 `main.ts:426` 一处）⇒ 删了监听改媒体查询是等价的。

### 步 6 ✅ 做 —— 10 档刻度 ＋ 两个层叠上下文，**并且有真引擎的死值验**

刻度进 `src/styles/tokens.css`（`--z-base/sticky/chrome/float/view/overlay/menu/modal/toast/drag`
= 0/1/2/3/100/200/300/400/500/600）。37 条规则**逐条判语义**替换，全表在 §3。
裸数字 `z-index` 现在是 **0 处**。

🔴 **顺带那条（真正治根的一条）**：
```css
#message-stream { position: relative; z-index: var(--z-base); overflow: hidden; }
#tab-bar        { position: relative; z-index: var(--z-base); … }
```

**死值验**（`stack/probe.js`，两个真引擎各跑 old/new）。做法：按 `index.html` 的真结构建
`#app > {#tab-bar, #message-stream, #status-bar}` ＋ 一个 `.settings-trigger`，
流里放一个 `.live-user-inputs.active`，**把 `设计/10` 步 2 那个止血的 `top: 40px`
退回原来的 `8px`**，强制两者重叠，然后问浏览器 `document.elementFromPoint`：

| CSS | `#message-stream` 的 z-index | 重叠点上最上面的是谁 |
|---|---|---|
| **old**（把那一行摘掉） | `auto` | `live-user-inputs active` ← **浮层压住按钮，连点击一起吃掉** |
| **new**（今天） | `0` | `settings-trigger` ← **按钮赢** |

Chromium 153 与 WebKitGTK 2.52.6 **两个引擎结论相同**。
⇒ 这就是 `真相源/50 §5.2` 那个「点设置/历史没反应」的**结构性根**，不再靠错开位置止血。

### 步 7 ❌ **没做 —— 必须动 TS，超出写区，上报**

`.sftp-bar-fill` 的 `transition: width 0.15s linear` 要换成 `transform: scaleX()`，
但宽度是 **JS 逐帧写内联样式**的：
```ts
// src/sftp/panel.ts:341   ← 不在写区
bar.style.width = `${(ratio * 100).toFixed(1)}%`;
```
只改 CSS 会让进度条**完全不动**。⇒ 要 CSS ＋ `sftp/panel.ts` 同拍，请拍板。

### 步 8 ✅ 做 —— 11 处 `!important` 全摘，**先在两个真引擎里验过**

先按特异度算：`.acct-avatar.ghost` 是 (0,2,0)、`.acct-avatar.ghost.acct-cN` 是 (0,3,0)，
而被压的 `.acct-cN` 只有 (0,1,0) ⇒ **本来就该赢**，与源码顺序无关。
第 11 处 `body.tabbar-collapsed .tab .tab-close` 是 (0,3,1)，被压的
`.tab-close,.tab-focus,.tab-cwd{display:inline-flex}` 是 (0,1,0) ⇒ 同理。

**死值验**（`imp/probe.js`）：拿真构建出来的 CSS 做 A/B——
A = 带 `!important`，B = 把那 11 处摘掉（katex 自带的那处保留不动），
在真引擎里建 8 个 slot × {实心, 幽灵} ＋ 折叠态 tab 的 5 个子元素，逐个读 computed：

| 引擎 | A vs B 的 computed 差异 |
|---|---|
| Chromium 153 | **0 处** |
| WebKitGTK 2.52.6 | **0 处** |

**反向变异自检**（证明这支探针不是瞎的）：把 `.acct-avatar.ghost` 的
`background: transparent` 改成 `magenta` ⇒ 探针**当场看见 8 处差异**。

探针同时确认那条折叠规则**是活的**（不是空跑）：
`tab-title` 在 `collapsed-off` 是 `block`、`collapsed-on` 是 `none`；`tab-focus` 是 `flex` → `none`。

### 步 9 ❌ **没做 —— 出写区，上报**（这是本轮最大的一块欠账）

`设计/40 §7` 步 9 那五件，落点全在写区外：

| | 做什么 | 落点 | 为什么没做 |
|---|---|---|---|
| ① | 变量对账 | `package.json`（装 `stylelint-value-no-unknown-custom-properties`）＋ `.stylelintrc.json` | 两个文件都不是 CSS |
| ② | 类名对账脚本 | 新建 `tests/*.vitest.ts` | 写区只给了 `tests/evidence/` |
| ③ | 布局根对账 | 同 ② | 同上 |
| ④ | `z-index` 只许 `var(--z-*)` | `.stylelintrc.json` 的 `declaration-property-value-allowed-list` | 同 ① |
| ⑤ | **CSS lint 从 advisory 改成阻断** | CI 配置 | 同 ① |

⚠ **但本轮把这五条的前提都备好了**，接手的人基本是填表：
- ① 的白名单要包含 `theme.ts` 那 14 个 ＋ JS `setProperty` 的 2 个（`--fork-depth` / `--tab-bar-w`）；
  现打全仓只有这 16 个是 CSS 外定义的。
- ② 的前缀白名单**今天的 11 个不够**，至少要再加：`FIRST_RUN_HINT_CLASS` 这类**常量拼接**
  （见步 3）、第三方库自产的 `katex-*`，以及本轮现打到的这些拼接点：
  `settings-btn-${variant}` · `cc-bus-hooks-${tone}` · `cc-bus-online-${yes|no}` ·
  `remote-test-{ok,err}` · `history-chip ${extra}` · `bash-output-body ${cls}` ·
  `block-collapsible ${cls}` · `paste-block${spec.className}`（`accounts-rc-paste` 从这来）。
- ③ 的断言 🔴 **今天不成立，会当场红** —— 见 §6 ④（`.tab-archive` 还在，而且真的在咬人）。
- ④ 现在**一次就能通过** —— 裸数字 `z-index` 已经是 0 处。

---

## 2. `设计/41 §12` 十一件逐件交代

| # | 件 | 裁 | 本轮 |
|---|---|---|---|
| 1 | 补两族 token ＋ 清单单独成文件 | ✅ 做 | **✅ 全做** |
| 2 | `@layer` 级联层 | ✅ 做 | **🟡 做了第一刀**（`reset`/`tokens`），见下 |
| 3 | 最小重置（含 button/input） | ✅ 做 | **❌ 没做**，见下 |
| 4 | 三个容器成层叠上下文 ＋ z-index 刻度 | ✅ 做 | **✅ 做**（= 步 6） |
| 5 | 容器查询取代写死宽度 | ✅ 做 | **❌ 没做**，见下 |
| 6 | 文件按层 ＋ 按窗口切 | ✅ 做 | **🟡 只切出 `tokens.css`**（件 1 那一半） |
| 7 | 状态表达约定 | ✅ 做 | **🟡 纪律写进 `tokens.css` 抬头；机检出写区** |
| 8 | 对账上 lint | ✅ 做 | **❌ 出写区**（＝ 步 9） |
| 9 | 动画属性白名单 | ✅ 做 | **🟡 现状已合规（除那一条）；lint 出写区** |
| 10 | 渐进式 CSS Modules | ✅ 做（摊开） | **❌ 没做**（要动 TS，且设计说摊开） |
| 11 | 主题预设 / 亮色 | ❌ 不做 | 不做（用户 2026-09-16 拍板） |

### 件 1 ✅ 全做

新建 **`src/styles/tokens.css`**（170 行）。抬头逐字写着「**组件里只许用这个文件里的名字**」，
并把命名语法、禁止清单、状态约定（件 7）、主题那条「推迟不产生债」一并写在那里。

新增三族：
```css
--state-hover / --state-active / --state-focus / --state-disabled-opacity   /* 交互态 */
--field-bg / --field-border / --field-border-focus / --field-placeholder    /* 表单控件 */
--color-link                                                               /* 零散那三个之一 */
--z-base … --z-drag（10 档）                                                /* 层叠刻度 */
```
⚠ `--color-link` 取的是**今天那处硬编码兜底的同值 `#6cb6ff`**，刻意**不**用 `--accent`
（赤陶橙当链接色读不出「可点」）。`设计/41 §2` 给的是二选一，这里选了不改观感的那个。

⚠ 按 `设计/41 §8` ②：新增的都用具体 hex，**一个都没进 `theme.ts` 的 14 个旋钮**。

🔴 **禁止清单里发现一条自相矛盾，已在 `tokens.css` 里就地标注**：
`设计/41 §2` 的禁止清单含「`--bg-1/2/3`（编号背景）」，但 **`--bg-2` 是今天活着的真 token，
而且是 `theme.ts` 那 14 个用户可调项之一**。同一节又说「现有短名没问题，别为了整齐去动」。
⇒ 读法只能是「禁止清单管**新增**」，落 lint 时 `--bg-2` 必须显式豁免，否则当场假红。

### 件 2 🟡 做了第一刀：声明七层 ＋ 包了 `reset` / `tokens`

```css
@layer reset, tokens, base, layout, components, states, utilities;
@import "./styles/tokens.css" layer(tokens);
@layer reset { *{box-sizing:border-box} html,body{…} }
```

🔴 **为什么只包这两层，而这恰恰是对的**：`@layer` 的规则是「**无层的样式赢过所有有层的样式**」。
所以分批包**只能从优先级最低的那头开始** —— 没包的那 6900 行落进隐含的「无层桶」，
而那个桶的位置正好在七层**之后**。⇒ 今天的实际次序是
`reset < tokens < (其余全部，相对次序与改前逐字相同)`，**与设计要的次序一致，且对渲染零影响**。
反过来先包 `components`、把 `layout` 留在外面，次序就倒过来了。
`设计/41 §3` 那句「可以分批包（先 reset/tokens/layout，components 最后）」说的就是这个方向。

零影响的两条证明（现打）：
① 全文 `box-sizing` 只有 2 处声明，另一处是具体元素（特异度远高于 `*`）；
② 全文只有 1 个 `:root`，且 `html`/`body` 上没有别处声明 `font-*`/`color`/`background`。
真引擎复核：`body` 的 computed `box-sizing` = `border-box`，`--bg`/`--text`/`--font-size-base`
等 token 在两个引擎里都读得到（`imp/res-*.json` 的 `tokens` 段）。

**`layout` 没包**，因为布局根的规则**散在四处**（`#app:105` · `#message-stream:118 和 :1418` ·
`#tab-bar:268` · `#status-bar:121 和 :2380`），包它必须搬规则 ⇒ 改源码顺序 ⇒ 风险与收益不成比例。
这件事该和**件 6（按层切文件）**一起做，而件 6 绑三入口拆分。**不是忘了，是次序。**

### 件 3 ❌ 没做 —— 最小重置

`设计/41 §12` 自己把它排在**第二批**，逐字「**与三入口拆分一起做**（那时本来就要过一遍全局样式），
并且**逐窗口目视**」，还标了「中（有视觉风险，要目视）」。

三条不做的理由：
1. **这个环境做不了目视**（无头机，没有 GUI）。而它的风险**恰恰是机器看不见的那一类**：
   现有 1021 条规则里若有哪条按钮只写了背景、`border` 靠 UA 默认撑着，
   加 `border: 0` 之后它就**长得不对但不报错**（`设计/40 §8` 自己点名的第一条「没查的」）。
2. 它点名要治的那个具体 bug —— `.user-inputs-toggle` 用浏览器默认按钮样式渲染在暗主题上 ——
   **前面的轮次已经单独修掉了**（步 4）。⇒ 现在做它是「防下一次」，不是「止血」。
3. **但 `reset` 这一层的空壳本轮已经建好**（件 2），内容加进去是一次纯增量。

### 件 4 ✅ 做 —— 见步 6

### 件 5 ❌ 没做 —— 容器查询

`设计/41 §12` 排第二批，逐字「与 `设计/10` 的『COL_W 从容器实测』一起」，
而 `height-estimate.ts` **不在写区**（明令禁碰）。只做 CSS 那一半会得到
「卡片按列宽排版、估高仍按写死的 780 算」——**两边更不一致**。

⚠ 顺带留一条**动手前必须先验的坑**（本轮查出来的，别人接手时省一次事故）：
`container-type: inline-size` 会带上 **layout containment**，
而 layout containment 让该元素成为**其后代里 `position: fixed` 元素的包含块**。
`#message-stream` 的子树里有悬浮层（`.live-user-inputs` 是 `absolute`，但将来并进
Ctrl+F 那块面板时形态会变）。⇒ 给 `#message-stream` 加 `container-type` 之前，
必须先清点它子树里所有 `fixed`/`absolute` 定位的东西。

### 件 6 🟡 只切出了 `tokens.css`

`设计/41 §9` 那棵树（`reset.css`/`base.css`/`layout.css`/`components/*`/`entries/*`）
逐字「**与 `设计/00 §3.6` 的三入口拆分是同一件事，必须一起做**」，而三入口要动
`vite.config.ts` ＋ 入口 HTML ＋ TS —— 全在写区外。
本轮只落了件 1 要的那一份（`tokens.css`），并**验证了这条路是通的**：
vite 的 postcss-import 会把 `@import … layer(tokens)` 正确内联并包成 `@layer tokens{…}`
（生产构建产物里现打可见），所以将来按层切文件**不增加请求数、也不丢层**。

### 件 7 🟡 纪律落了，机检没落

三条约定逐字写进 `src/styles/tokens.css` 抬头（「不参与布局用 `hidden` 属性、
CSS 里禁止对这些元素写 `display`」「占位不可见用 `visibility`」「状态用 `data-*`，
且**减的那一半也要有判据**」）。
判据（「同一个状态名不得同时出现在两种载体里」）要新建测试文件 ⇒ 出写区，**没做**。

### 件 8 ❌ 出写区 —— 见步 9

### 件 9 🟡 现状已合规，白名单落 lint 出写区

现打全文 `transition` 动的属性：
```
color 7 · background 7 · border-color 4 · transform 4 · opacity 3
grid-template-rows 2（已登记的例外：0fr↔1fr 展开动画） · none 1
width 1  ← 唯一一条违反白名单的，就是步 7 那条，卡在 TS 上
```
⇒ 白名单 `transform/opacity/background/color/border-color` **今天只差那一条**。

### 件 10 ❌ 没做 —— 渐进式 CSS Modules

要动 TS（`import s from "./x.module.css"`），且 `设计/41` 自己说「摊开做，不设期限、
别一次迁 800 处」。本轮没有新组件，所以没有自然的落点。

### 件 11 ❌ 不做 —— 用户 2026-09-16 已裁

---

## 3. z-index：37 条逐条的判语义结果

`--z-base:0 · --z-sticky:1 · --z-chrome:2 · --z-float:3 · --z-view:100 ·
--z-overlay:200 · --z-menu:300 · --z-modal:400 · --z-toast:500 · --z-drag:600`

| 选择器 | 旧 | 新令牌 | 新值 | |
|---|---:|---|---:|---|
| `.settings-trigger` `.history-trigger` `.panorama-trigger` `.sftp-trigger` `.grid-monitor-trigger` | 2 | `--z-chrome` | 2 | 同值 |
| `.viewer-branch-btn` | 2 | `--z-chrome` | 2 | 同值 |
| `.search-session-header` | 1 | `--z-sticky` | 1 | 同值 |
| `#tab-bar-resizer` | 3 | `--z-float` | 3 | 同值 |
| `.live-user-inputs` | 3 | `--z-float` | 3 | 同值（但现在关在 `#message-stream` 圈子里） |
| `.panorama-tooltip` | 3 | `--z-float` | 3 | 同值 |
| `.tasks-popover` | 50 | `--z-float` | 3 | ↓ |
| `.settings-panel` | 100 | `--z-view` | 100 | 同值 |
| `.history-view` `.panorama-view` `.agent-records-viewer-mount` | 150 | `--z-view` | 100 | ↓ |
| `.grid-monitor` | 50 | `--z-view` | 100 | ↑ |
| `.cc-bus-view` | 40 | `--z-view` | 100 | ↑ |
| `.kb-editor-overlay` `.launcher-back` `.pane-preview-overlay` `.pf-overlay` `.import-preview-back` | 200 | `--z-overlay` | 200 | 同值 |
| `.sftp-overlay` | 100000 | `--z-overlay` | 200 | ↓ |
| `.inbox-overlay` | 40 | `--z-overlay` | 200 | ↑ |
| `.panorama-loading, .panorama-message` | 4 | `--z-overlay` | 200 | ↑（圈内，在 `.panorama-view` 上下文里） |
| `.tab-context-menu` `.history-context-menu` | 99999 | `--z-menu` | 300 | ↓ |
| `.sftp-host-picker` | 100001 | `--z-menu` | 300 | ↓ |
| `.account-picker` | 1000 | `--z-menu` | 300 | ↓ |
| `.settings-cc-modal-backdrop` `.fork-ask-backdrop` | 10000 | `--z-modal` | 400 | ↓ |
| `.command-bar` | 10002 | `--z-modal` | 400 | ↓ |
| `.sftp-edit-back` | 5 | `--z-modal` | 400 | ↑（圈内，在 `.sftp-overlay` 上下文里） |
| `#ccm-toast-stack` | 9999 | `--z-toast` | 500 | ↓ |
| `.settings-info-tooltip` | 10001 | `--z-toast` | 500 | ↓ |
| `.tab-drag-ghost` | 1000 | `--z-drag` | 600 | ↓ |
| `.tab-bar-resize-guide` | 50 | `--z-drag` | 600 | ↑ |
| **新增** `#message-stream` `#tab-bar` | — | `--z-base` | 0 | 层叠上下文 |

**次序变了的对**：32 个根上下文里的元素，两两 496 对，**次序变了 100 对**。
绝大多数是永远不会同屏的组合（右键菜单 vs SFTP 全屏面板那种）。**要人看的这几条**：

| 变化 | 判断 |
|---|---|
| `.tab-bar-resize-guide` 从 50 → 600，**现在压得过 `.settings-panel`** | ✅ **这是修 bug** —— `真相源/51 §3 ②` 逐字：「设置面板开着时拖 tab 栏，那条参考线被面板盖住」 |
| `#ccm-toast-stack`（500）现在**压得过模态背板**（400），以前 9999 < 10000 | ✅ **刻意** —— 刻度表把 toast 放在 modal 之上；被背板盖住的提示条是没用的提示条 |
| `.settings-info-tooltip`（500）现在**压不过**？不：它也是 500，但从「压过 `.command-bar`(10002)」变成「被 `.command-bar`(400) 压不过 / 平手偏上」 | ⚠ 两者同屏概率低（一个是设置里的 `?`，一个是 Ctrl+K 面板）。**记一笔，别当没看见** |
| `.tasks-popover` 50 → 3，与 `#tab-bar-resizer`(3) **打平**，谁上由 DOM 顺序定 | ⚠ 只在 tab 栏被拖到很宽时才可能重叠；resizer 平时透明（只在 `:hover` 才着色）。**记一笔** |
| `.sftp-overlay` 200 现在**压不过** `.tab-context-menu` 300 / `.tab-drag-ghost` 600 | 🟡 SFTP 是全屏面板，盖着 tab 栏 ⇒ 那两个根本开不出来。低风险 |
| `.cc-bus-view` 40 → 100，与 `.settings-panel` 打平 | 🟡 它是「从设置里搬出来的顶层运营视图」（`main.ts:512` 注释），打平后由 DOM 顺序定，后开的在上 —— 这正是想要的 |

---

## 4. 🔴 真高变化表（逐 class）＋ DOM 指纹

**做法**：`bash tests/evidence/U-scale2-run.sh` 在改 CSS **之前**跑一遍（拿基线）、
**之后**再跑一遍，逐行比对 `tests/evidence/U-scale2-truth-golden.json` 的 83 行。

先证明这条流水线是**确定性**的（不然「变了」就分不清是 CSS 还是噪音）：
**改动前连跑两遍，83 行 × 6 个字段，差异 0 处。**

| class | n | 修前真高 content-box | 修后真高 | Δ | 为什么 |
|---|---:|---|---|---|---|
| `card-api-error` | 3 | 49.38 … 244.56 | 49.38 … 244.56 | **0** | 见下 |
| `card-api-retry` | 5 | 17.05 | 17.05 | **0** | 见下 |
| `card-assistant` | 28 | 46.80 … 8876.45 | 46.80 … 8876.45 | **0** | 见下 |
| `card-bash-input` | 3 | 18.59 | 18.59 | **0** | 见下 |
| `card-bash-output` | 6 | 41.19 … 504.59 | 41.19 … 504.59 | **0** | 见下 |
| `card-compact` | 2 | 34.59 | 34.59 | **0** | 见下 |
| `card-slash` | 3 | 18.59 | 18.59 | **0** | 见下 |
| `card-tool-group` | 23 | 34.59 | 34.59 | **0** | 见下 |
| `card-user` | 10 | 21.69 … 195.19 | 21.69 … 195.19 | **0** | 见下 |

同样逐行比对、同样 0 处差异的还有：`trueBorderBox` · `padBorder` · `estBrowser` ·
`estAppliedBrowser`；B 段的 5 行 `contain-intrinsic-size` 盒模型读数也逐位相同。

### 🔴 DOM 指纹（`htmlHash`）：**83 行一个都没变**

指纹变 = 改到了 DOM 结构而不是样式。本轮**零 TS 改动**，指纹不变是应该的 —— 这一格在这里
是**反向验证**：它证明上面那张「真高全 0」不是因为语料/渲染变了导致两边都漂，而是真的没动。

### 为什么真高一格都没变（这才是要解释的那件事）

任务书预期「你动 CSS，真高就变」。本轮**没变**，逐条说清楚：

| 本轮改了什么 | 会影响布局高度吗 |
|---|---|
| 包 `@layer reset` / `@layer tokens` | ❌ 不改任何一条声明的**胜负**（证明见件 2 那两条），只改它们住在哪一层 |
| `:root` 搬进 `tokens.css` | ❌ 打包后逐字还是同一个 `:root`，token 值逐个现打核过 |
| 新增 3 族 token | ❌ **纯新增**，本轮只有 `--field-bg`/`--state-hover`/`--state-active`/`--color-link` 被 31 处引用中的 8 处用到，且全是颜色 |
| 14 个假变量改真名（31 处） | ❌ 逐处核过：**没有一处落在 `.card-*` / `.stream-content` / `.code-block` / `.block-*` 上**。改的是 sftp / launcher / accounts / inbox / settings / panorama 那几片，且 **27 处是 `color`/`background`、3 处是 `font-family`、1 处是 `font-size`** —— 那 1 处是 `.command-bar-input`，不在流里 |
| 删 13 个死规则 | ❌ 按定义它们**匹配不到任何元素**（这正是步 3 那三把尺子验的） |
| z-index 换成刻度 | ❌ `z-index` 不参与布局，只决定画在谁上面 |
| `#message-stream`/`#tab-bar` 加 `z-index`/`position:relative` | ❌ `position:relative` 不脱流、不改 grid 定尺；`z-index` 只改层叠 |
| 摘 11 处 `!important` | ❌ 死值验证明 computed 逐格相同（而且全是 `background`/`color`/`display`，前两个不影响高度） |

⇒ **「CSS 改了但真高没变」在这里是可解释的，不是秤失灵。**
反证：本轮那支反向变异探针能看见 8 处 computed 差异 ⇒ 探针链路是活的；
而秤 2 自己的变异自检（`U-scale2-mutation-log.txt`）此前 9 变异 8 当场红。

### `P90_CEILING` 那三格 0.3 —— 没红，也**没碰**

任务书提醒「改完 CSS 那几格可能会红，红了不许靠调上限变绿」。
现打：`card-assistant` / `card-bash-input` / `card-slash` 三格**都没红**
（真高没变 ⇒ 误差分母没变 ⇒ 估值也没变 ⇒ p90 逐位相同）。
**`P90_CEILING` 与 `EXCEEDS_DESIGN_GATE` 一个字都没改**（那个文件本来就不在写区）。

---

## 5. 门禁与检查（全部现打）

```
npx vitest run tests/scale2-height-truth.vitest.ts   → 12 passed   ✅
npx vitest run tests/scale3-one-screen-gate.vitest.ts→  9 passed   ✅
npx tsc --noEmit                                     → 干净        ✅
npx vitest run（全量）                                → 1703 passed / 14 skipped / 131 文件中 1 失败
npx stylelint "src/**/*.css"                         → 47 errors（改前 50，advisory 不阻断）
npx vite build（产物落 /tmp，不碰仓里 .build/）        → ✓ built，CSS 144.7KB → 141.2KB
```

⚠ **全量里那 1 个失败是 `tests/scale1-render-cost.vitest.ts`，不是我改坏的**：
它是 `beforeAll` 钩子 10s 超时（131 个文件并行时的抢占），**单独跑 4.53s、14 tests 全过**。
而且秤 1 压根不加载 CSS。（秤 1 也在明令禁碰之列，一个字没动。）

stylelint 50 → 47 的分解：拆文件那一刻是 `styles.css` 47 ＋ `tokens.css` 3 = **50，与改前同数**
（三条是 `--acct-ink0/5/6` 的 `#ffffff`，跟着 `:root` 一起搬过去的，不是新造的）；
随后把那 3 条改成 `#fff`（纯等价）⇒ **`tokens.css` 零报错**，总数 47。
⇒ **本轮一条新 lint 报错都没引入**，还净少 3 条。

---

## 6. 🔴 要用户拍板的四件

### ① `设计/40` 与 `设计/41` 对 `--bg-1` 给了**不同目标**（纪律 1，没自己挑）

- **`设计/40 §7` 步 1/步 2 逐字**：`styles.css:5967 var(--bg-1) → var(--bg)`、`--bg-1→--bg`
- **`设计/41 §2` 逐字**：「这一族缺失正好解释了 `.command-bar-input` / `.panorama-ann-input`
  那两条坏规则（一个抓 `--font-size`、一个抓 `--bg-1`）—— **它们都是在找一个不存在的『输入框』族**」
  ⇒ 按这个读法 `.panorama-ann-input` 该是 `--field-bg`（`#1f1e1b`），不是 `--bg`（`#2b2a27`）

两个目标**差一个可见的色阶**。而现打的 6 处硬编码兜底**站在 41 那一边**：

| 住址 | 今天实际渲染的颜色 | `--bg` = `#2b2a27` | `--field-bg` = `#1f1e1b` |
|---|---|---|---|
| `.mcp-server-json` | `#1e1a16` | 明显变亮 | 几乎不变 |
| `.status-cmdk kbd` | `#1e1a16` | 明显变亮 | 几乎不变 |
| `.status-first-run-{open,dismiss}:hover` | `#1e1a16` | 明显变亮 | 几乎不变 |
| **`.panorama-ann-input`（无兜底）** | **透明（属性被丢弃）** | 比页面底色亮 | 比页面底色暗 |
| `.account-picker` | `#1c1a17` | 明显变亮 | 几乎不变 |
| `.accounts-wiz-preview` | `#1c1a17` | 明显变亮 | 几乎不变 |

⚠ `真相源/51 §1` 的表里这一格写的是「它**多半**想指的是 `--bg`」—— 真相源自己就带着「多半」。
⇒ **这 6 处一处没动。** 请裁：`--bg` / `--field-bg`（或 `--bg-2`）/ 逐处分别判。
裁完是一条 `sed`，本轮所有配套（新族已在、lint 前提已备）都已经就位。

### ② 步 5 与步 7 要放开两处 TS（各 5 行以内），请拍板

| 步 | 要改的 TS | 为什么不能只改 CSS |
|---|---|---|
| 5 | `src/main.ts:425-429` 删 `applyCollapse` ＋ resize 监听 | 只加媒体查询 = 同一断点两处实现，比今天更坏 |
| 7 | `src/sftp/panel.ts:341` `bar.style.width` → `transform: scaleX()` | 只改 CSS 进度条会完全不动 |

### ③ 步 9 / 件 8（对账上 lint）整块出写区，请拍板落点

要碰 `package.json`（装插件）＋ `.stylelintrc.json`（开规则 ＋ z-index 白名单）＋
新建一个 `tests/*.vitest.ts`（类名与布局根对账）＋ CI 配置（advisory → 阻断）。
前提已经全部备好（见步 9 那张表），**尤其是那份前缀白名单今天的 11 个不够** ——
不补全，类名对账装上去第一天就会把 `.status-first-run-*` 和 `.katex-*` 判成死规则。

---

### ④ 🔴 顺手查出的活缺陷：`真相源/71` 那个病灶**今天还在**，而真相源以为它没了

`真相源/51 §4` 逐字「**删掉归档抽屉之后，`#app` 的 in-flow 子元素恰好是那三个认领过格子的**」
—— 这句话的前提是「归档抽屉已经删了」。**现打：它还在。**

```ts
// src/tabs.ts:3279-3307  ensureArchiveUi()
const wrap = document.createElement("div"); wrap.className = "tab-archive";
const parent = this.barEl.parentElement;          // = #app
parent.insertBefore(wrap, this.barEl.nextSibling); // ⇒ #app 的 in-flow 直接子元素
```
而 `styles.css` 里 `.tab-archive` 只有 `display/flex-direction/gap/padding` ——
**没有 `grid-area`、没有 `position`** ⇒ 它是一个**没认领格子的 grid item**。

**真引擎实测**（Chromium 153，900×700 视口，`archive/probe.js`，照 `tabs.ts` 现打的插法）：

| | `#app` 的 `grid-template-rows` | `#message-stream` 高 | `#status-bar` 的 y | 抽屉落在哪 |
|---|---|---:|---:|---|
| 没有抽屉 | `676px 24px` | **676** | 676 | — |
| 插入抽屉 | `641px 24px **35px**` ← 多出一条隐式行 | **641**（−35） | 641 | y=665，**在状态栏下面**，x=0 |

⇒ 两条真实后果：**① 消息流被偷走 35px 高；② 抽屉画在状态栏之下**（本该在 tab 栏下面）。

**为什么本轮没修**（三条，请拍板）：
1. 它不在九步/十一件里 —— 九步里跟它沾边的是**步 9 ③那条断言**，而断言整块出写区。
2. CSS-only 的修法要**改 `#app` 的 grid 模板**（加一条具名行 `archive`），
   而那块正好带着一条踩过坑的警告（`body.viewer-mode #app` 那段逐字说「必须只定义两行，
   否则 message-stream 会落进多出来的那行被压成 0 高」）⇒ 动它属于重新设计布局根，不是执行九步。
3. `设计/99 §4` 步 **17**（`设计/30` tab 三刀）本来就要处置归档区。现在加个补丁可能白做。

⇒ 请裁：**(a)** 放开 `#app` 的 grid 模板让我按 `"tabs main" / "archive main" / "status status"`
就地修；**(b)** 归给步 17 一起办；**(c)** 先在 `设计/40` 步 9 ③ 的断言里登记成**已知红**。

---

## 7. 探针与中间产物的住址（都在 /tmp，不进仓）

```
/tmp/claude-1000/…/scratchpad/s21/
  golden-BEFORE.json        改 CSS 之前的金标准（比对基线）
  styles-BEFORE.css         改之前的 styles.css
  run-baseline.log          改之前那次 U-scale2-run.sh 的全文（确定性基线）
  run-after.log             改之后那次
  imp/{A,B,C}.css probe.js probe-{A,B,C}.html res-*-{chromium,webkitgtk}.json
                            步 8 的死值验 ＋ 反向变异自检
  stack/{old,new}.css probe.js probe-{old,new}.html res-*.json
                            步 6 的层叠上下文死值验（elementFromPoint）
  dist-check/               生产 vite build 的产物（验 @layer 没被打包丢掉）
```
两个 runner 仍住 `/tmp/cv-reparent-probe/`（`run_chromium.mjs` / `run_webkitgtk.py`），
被清掉了怎么重建见 `tests/evidence/U-scale2-run.sh` 末尾。
