# S24 · 归档抽屉就地止血 ＋ 步 5 / 步 7 ＋ `--bg-1` 裁定：做了什么、凭什么

> 接 `tests/evidence/S21-css-readings.md` §6 那四件要用户拍板的事。本轮把其中三件办了，
> 外加一条 S21 顺手查出的**活缺陷**（`.tab-archive` 偷高度 + 画到状态栏下面）就地止血。
>
> 写区：`src/styles.css` · `src/styles/tokens.css` · `src/tabs.ts`（本轮**一个字没动**，见 §1.4）·
> `src/main.ts`（5 行）· `src/sftp/panel.ts`（1 行）· 新建 `tests/app-grid-claims.vitest.ts` ·
> 本文件。**没有**碰 `src/height-estimate.ts` / `tests/scale2-*` / `tests/evidence/U-scale2-*` /
> `src/cards/` / 任何 `.rs` / `.github/` / `package.json`。
>
> **复算全部读数**（从仓根）：
> ```bash
> npx vitest run tests/app-grid-claims.vitest.ts     # 新判据（10 条）
> npx vitest run tests/scale2-height-truth.vitest.ts # 秤 2 金标准（＝「没碰到卡」的绊线）
> npx vitest run                                     # 全量
> npx tsc --noEmit
> npx stylelint "src/**/*.css"
> ```
> 真引擎读数怎么复算见 §6（探针源码逐字在那一节）。

---

## 0. 一页结论

| | 改前 | 改后 |
|---|---|---|
| `#app` 插入归档抽屉后的 `grid-template-rows` | `641px 24px **35px**`（多一条**隐式行**） | `676px 0px 24px`（三条**具名**行，抽屉认领 `archive`） |
| 插抽屉前后 `#message-stream` 高 | 676 → **641**（被偷 35px） | 676 → **676**（一个像素不差） |
| 抽屉画在哪 | y=665，**在状态栏（y=641）下面** | y=543，在 tab 栏正下方，**不在状态栏下面** |
| 3 个归档会话时 tab 那一列宽 | 150 → **296px**（消息流被挤窄 146px） | 150 → **150**（不变） |
| 0 个归档会话时抽屉 | 照样占 35px（`tabs.ts` 那句 `hidden` 是空写） | `display: none`，占 **0** |
| viewer 窗口 | 同样被偷 133px（`真相源` 里没人记过这一格） | 抽屉 `display:none`，消息流 **644.41 → 644.41** |
| 「`#app` 子元素都认领了格子」有没有机检 | **没有** | `tests/app-grid-claims.vitest.ts`，**两种模式都钉**，6 个变异全咬 |
| 980px 断点 | `main.ts` 一个 `resize` 监听挂 body 类 | `@media (width < 980px)`，JS 监听整条删除 |
| `.sftp-bar-fill` 进度 | `transition: width`（每帧重排） | `transform: scaleX()`（只走合成） |
| `--bg-1`（用了但从未定义） | 6 处 | **0 处**，全部落到 `--field-bg` |
| stylelint | 47 errors | **47**，**逐条直方图完全相同**（§5） |
| `npx tsc --noEmit` | 干净 | 干净 |
| 全量 vitest | 133 文件 / 1747 条 | **133 文件 / 1747 条 全过** |
| 秤 2 金标准 | 绿 | **绿**（＝ 本轮没碰到卡的盒模型） |

🔴 **两个真引擎**（Chromium 153.0.8010.12 · WebKitGTK 2.52.6）× **两个窗宽**（900 / 1200）×
**改前/改后/生产构建产物** 全部对过，结论一致。

---

## 1. 🔴 ① 归档抽屉：就地止血

### 1.1 病灶（S21 §6 ④ 已记，本轮复现并扩大了读数）

```ts
// src/tabs.ts  ensureArchiveUi()
const wrap = document.createElement("div"); wrap.className = "tab-archive";
const parent = this.barEl.parentElement;           // = #app
parent.insertBefore(wrap, this.barEl.nextSibling); // ⇒ #app 的 in-flow 直接子元素
```
而改前 `.tab-archive` 的 CSS 只有 `display / flex-direction / gap / padding`
—— **没有 `grid-area`、没有 `position`** ⇒ **一个没认领格子的 grid item**，
浏览器只能给它开一条**隐式行**。

### 1.2 真引擎实测（改前，两个引擎逐位相同）

900×700 与 1200×700 读数**完全一样**（这个缺陷与窗宽无关）：

| 场景 | `#app` 的 `grid-template-rows` | `#message-stream` 高 | 消息流左边界 x | 抽屉 |
|---|---|---:|---:|---|
| 没抽屉 | `676px 24px` | 676 | 150 | — |
| 抽屉 + **0 个归档** | `641px 24px **35px**` | **641（−35）** | 150 | y=665 w=150 h=35，**在状态栏下面** |
| 抽屉 + 3 个归档 | `543px 24px **133px**` | **543（−133）** | **296.14（−146）** | y=567 w=296.14 h=133，**在状态栏下面** |

🔴 **S21 没读到的两格，本轮读到了**：

1. **0 个归档会话时它照样占 35px。** `tabs.ts:3473` 逐字写着
   `this.archiveWrap.hidden = archived === 0;`，注释是「一条都没有就整块不显：
   一个空抽屉只会占地方」。但 `.tab-archive { display: flex }` 是**作者层**声明，
   压过 UA 的 `[hidden] { display: none }` ⇒ **那句 `hidden` 一直是空写**。
   而「0 个归档」正是绝大多数用户的常态 ⇒ 这 35px 是**天天在偷**。
2. **抽屉会把整列撑宽。** `tabs`/`archive` 那一列是 `auto`（intrinsic），列宽取该列所有
   item 的 max-content，而抽屉里装的是会话标题 ⇒ 3 个归档会话就把列从 150px 撑到
   **296.14px**，消息流跟着右移、变窄 146px。

### 1.3 viewer 窗口也中招（这一格以前没人读过）

`main.ts` 的 `bootstrapViewer` **照样 `new TabManager(tabBar, …)`**，所以抽屉在 viewer
里也会被插进 `#app`；而 viewer 的模板只有 `top / main / status` 三行：

| 场景 | rows（改前） | `#message-stream` 高 |
|---|---|---:|
| viewer，没抽屉 | `31.59px 644.41px 24px` | 644.41 |
| viewer，3 个归档 | `31.59px **511.41px** 24px **133px**` | **511.41（−133）** |

### 1.4 修法（**只动 CSS，`src/tabs.ts` 一个字没改**）

```css
/* #app：多一条具名行给抽屉。main 跨 tabs/archive 两行 ⇒ 抽屉开多高，消息流都不变 */
#app {
  grid-template-areas:
    "tabs main"
    "archive main"
    "status status";
  grid-template-columns: auto minmax(0, 1fr);
  grid-template-rows: minmax(0, 1fr) auto 24px;
}
.tab-archive {
  grid-area: archive;                  /* ① 认领格子 */
  width: var(--tab-bar-w, 150px);      /* ② 别把 auto 列撑宽（同 #tab-bar-resizer 的跟法）*/
  max-height: 40vh;                    /* ③ 别把 1fr 那行压成 0 */
  overflow-y: auto;
  display: flex; …
}
.tab-archive[hidden] { display: none; }   /* ④ 让 tabs.ts 那句 hidden 真的生效 */
body.viewer-mode .tab-archive { display: none; }  /* ⑤ tab 栏都藏了，抽屉是它的附属 */
```

⚠ **`#tab-bar-resizer` 与六个 trigger 不受影响**：它们是 `position: absolute`，
不是 grid item，也不开隐式行（abspos 子元素永远不产生隐式轨道）。

### 1.5 真引擎复测（改后）

| 引擎 / 窗宽 | 没抽屉 | 抽屉+0 归档 | 抽屉+3 归档 | 抽屉在状态栏下面？ |
|---|---:|---:|---:|---|
| Chromium @900 | 676 | **676** | **676** | 否（y=481，状态栏 y=676） |
| Chromium @1200 | 676 | **676** | **676** | 否（y=543） |
| WebKitGTK @900 | 676 | **676** | **676** | 否（y=475） |
| WebKitGTK @1200 | 676 | **676** | **676** | 否（y=543） |
| **生产构建产物** ×4 | 676 | **676** | **676** | 否 |

消息流左边界 x 三种场景下也逐位相同（@1200 都是 150，@900 都是 44 ＝折叠态）。
viewer：`31.59px 644.41px 24px` → 插抽屉后**一模一样**，抽屉 `display: none`。

### 1.6 ⚠ 止血 ≠ 做完，外加一条**查到但刻意没修**的

- 步 17（`设计/30` 三刀）仍要把「抽屉」这个概念整个处置掉。本轮只解「偷高度 + 画错地方」。
- 🔴 **`.tab-archive-list` 上有一模一样的 `hidden` 空写**：`tabs.ts:3474`
  `this.archiveList.hidden = collapsed;`，而 `.tab-archive-list { display: flex }`
  同样压过 `[hidden]` ⇒ **「折叠」这个交互今天从来没生效过**（默认就是折叠态，
  但归档 tab 照样全列出来）。
  **没修** —— 那是归档的**交互**，属于步 17 的活，不该在止血这一刀里顺手重做。
  如实登记在这里，别让它变成第二次「帐本说没了、现实还在」。

---

## 2. 步 5：980px 断点从 JS 搬进 CSS

### 2.1 改了什么

`src/main.ts` 删掉 5 行（`applyCollapse` + `resize` 监听 + 首帧调用），
`styles.css` 那 7 条 `body.tabbar-collapsed …` 规则改挂 `@media (width < 980px)`。

前提复核（现打）：`tabbar-collapsed` 这个类全仓**只被写、从没被读**，唯一读者就是那几条
CSS 规则 ⇒ 删监听是等价替换。

⚠ 断点写成 `(width < 980px)` 而不是 `(max-width: 979px)`，两条理由：
① 与原来的 JS 判据 `window.innerWidth < 980` **逐字等价**（`max-width: 979px` 会漏掉
979.5px 这种分数宽）；② `stylelint-config-standard` 的 `media-feature-range-notation`
认的就是这个写法（写成 `max-width` 会**多出一条 lint 报错**，实测 47 → 48）。
范围语法在仓库基线（Chromium ≥117 / WebKitGTK ≥2.42）之内。

### 2.2 🔴 那个坑：选择器前缀不能直接去掉

`body.tabbar-collapsed X` 去掉前缀就**少一个类**，而其中三条**靠那一个类**才压得过
后文的同族规则。**实测**（Chromium @900，把前缀整个去掉当变异）：

| computed 格 | 改前·挂类（＝折叠态） | S24 实现 | **若直接去掉前缀** | |
|---|---|---|---|---|
| `#tab-bar` width | 44px | 44px | 44px | |
| `#tab-bar-resizer` display | none | none | none | |
| `.tab` justify-content | center | center | center | |
| **`.tab` padding** | `0px 4px` | `0px 4px` | **`0px 8px 0px 10px`** | 🔴 失效 |
| `.tab-title` display | none | none | none | |
| `.tab-close` display | none | none | none | |
| `.live-dot` display | block | block | block | |
| **`.tab.tab-bg::before` position** | static | static | **absolute** | 🔴 失效 |
| `.tab.tab-bg::before` margin-right | 2px | 2px | 2px | |
| **`.tab.archived .live-dot` display** | block | block | **none** | 🔴 失效 |
| **`.tab.archived .live-dot` background** | `rgb(107,102,94)` | `rgb(107,102,94)` | **`rgb(217,104,104)`** | 🔴 失效 |

⇒ 所以前缀换成了 **`body:not(.viewer-mode)`**：`:not(.viewer-mode)` 同样算**一个类**，
逐条特异度与改前**完全相同**；语义上也正好 —— viewer 里 `#tab-bar` 本就 `display:none`，
折叠样式在那儿是空跑。

### 2.3 死值验（两个引擎 × 11 个 computed 格）

| 比较 | 差异格数 |
|---|---|
| **改前**「挂类」 vs「不挂类」 | **9**（证明这支探针量的是活的东西） |
| **S24 @900** vs 改前「挂类」 | **0** ← 窄窗行为逐格等价 |
| **S24 @1200** vs 改前「不挂类」 | **0** ← 宽窗行为逐格等价 |
| S24 @900 vs 把断点改成 `(width < 1px)` | **5** ← 断点没生效就看得见 |
| S24 @900 vs 去掉 `body:not(.viewer-mode)` 前缀 | **4** ← 见 §2.2 |

Chromium 与 WebKitGTK 两个引擎的这四行结论相同。

---

## 3. 步 7：进度条 `width` → `transform: scaleX()`

CSS（`.sftp-bar-fill`）：`width: 0` + `transition: width .15s` →
`width: 100%` + `transform: scaleX(0)` + `transform-origin: left center` + `transition: transform .15s`。
TS（`sftp/panel.ts:341`）：`bar.style.width = …%` → `bar.style.transform = scaleX(…)`。

### 死值验：2×2，**每个单边都死**（这就是「必须同拍」的证据）

400px 轨道，推到 40%，等 transition 跑完再量填充盒的 `getBoundingClientRect().width`：

| | JS 写 `style.width = "40%"` | JS 写 `style.transform = "scaleX(0.4)"` |
|---|---:|---:|
| **改前 CSS** | **160px** ✅（今天的样子） | **0px** 🔴 完全不动 |
| **改后 CSS** | **0px** 🔴 完全不动 | **160px** ✅ |

⇒ 只改 CSS 不改 TS，或只改 TS 不改 CSS，用户看到的都是**进度条一动不动**。
两个引擎 + 生产构建产物，四组读数全部相同。

顺带：`设计/41` 件 9 那张动画属性白名单（`transform/opacity/background/color/border-color`）
**今天起零违例** —— `width` 那唯一一条就是这个。

---

## 4. `--bg-1` → `--field-bg`（6 处）

裁定理由（`设计/40 §7` 与 `设计/41 §2` 原先给了两个目标，差一个看得见的色阶）：
**兜底色是当初写它的人真正想要的颜色，是证据；`真相源/51` 那句「多半想指的是 `--bg`」是猜。**

| 住址 | 改前 | 今天实际渲染 | `--bg` `#2b2a27` | `--field-bg` `#1f1e1b` |
|---|---|---|---|---|
| `.mcp-server-json` | `var(--bg-1, #1e1a16)` | `#1e1a16` | 明显变亮 | 几乎不变 ✅ |
| `.status-cmdk kbd` | `var(--bg-1, #1e1a16)` | `#1e1a16` | 明显变亮 | 几乎不变 ✅ |
| `.status-first-run-{open,dismiss}:hover` | `var(--bg-1, #1e1a16)` | `#1e1a16` | 明显变亮 | 几乎不变 ✅ |
| **`.panorama-ann-input`** | `var(--bg-1)`（**无兜底**） | **透明**（属性被整条丢弃） | 比页面底色亮 | 比页面底色暗 ✅ |
| `.account-picker` | `var(--bg-1, #1c1a17)` | `#1c1a17` | 明显变亮 | 几乎不变 ✅ |
| `.accounts-wiz-preview` | `var(--bg-1, #1c1a17)` | `#1c1a17` | 明显变亮 | 几乎不变 ✅ |

顺带修掉「第三处无兜底」那个 bug：**全景批注输入框从此有背景**（`设计/40 §7` 步 1 的第三条，
S21 因为 40/41 冲突没敢动的那一处）。

现打核对：`src/**/*.css` 里 `--bg-1` **0 处**，`--field-bg` 10 处。
真引擎读到 `--field-bg = #1f1e1b`（证明 `@import … layer(tokens)` 那条链是活的）。

**两篇文档已同拍改对**（criterion 6，不留两个说法）：
- `设计/40 §7` 步 1、步 2 两处 `--bg-1 → --bg` 改成 `→ --field-bg`，并加一段裁定理由；
- `设计/40 §0` 那格「删归档后 `#app` 干净」改成「**2 例**」并注明那个前提没有发生；
- `设计/40 §7` 步 9 ③ 的断言措辞从「子元素集合 == 那三个」改成性质式（见 §5.2）；
- `src/styles/tokens.css` 的 field 族抬头记下裁定与理由。

---

## 5. 新判据：`tests/app-grid-claims.vitest.ts`

### 5.1 钉的是什么

不是「`#app` 的子元素 == {tab-bar, message-stream, status-bar}」（`设计/40 §7` 步 9 ③ 的原措辞
—— **那个集合今天就是错的**，抽屉确实在，钉它会持续假红），
而是那条**真正要成立的性质**：

> `#app` 的**每一个** in-flow 直接子元素，都认领了一个**本模式模板里声明过的**具名区域。

认领了 ⇒ 不开隐式行 ⇒ 隐式行数被钉死在 0，**而不用谁记得回来数行**。
**普通模式与 `body.viewer-mode` 两种模式都钉** —— `styles.css` 里那条踩坑警告
（「必须只定义两行，否则 message-stream 会落进多出来的那行被压成 0 高」）从此有机器在看。

三把尺子：**A** 人群从源码派生（找「往 `#app` 插节点」的调用点）· **B** 表里的
`expr → selector` 映射与源码对账 · **C** 认领对账（会红的那条）。三把各带一次**自检**
（喂合成输入，确认它看得见本该看见的东西）。

### 5.2 变异台（`npx vitest run tests/app-grid-claims.vitest.ts`，逐条现打）

| | 植入的缺陷 | 结果 |
|---|---|---|
| M0 | 不变异（基线） | 🟢 10 条全绿 |
| **M1** | **摘掉 `.tab-archive` 的 `grid-area`**（＝ 把今天这个缺陷放回去） | 🔴 **红**（3 条同时红：认领对账 + 尺 C 自检 + 「没有空转的行」） |
| M2 | 把 `.tab-archive` 从 viewer-mode 的隐藏名单里拿掉 | 🔴 红（viewer 模式认领对账） |
| M3 | `grid-template-rows` 多一条轨道 | 🔴 红（模板自洽） |
| M4 | 往 `#app` 里新插一个没登记的子元素 | 🔴 红（尺 A） |
| M5 | 抽屉在 TS 里改名、CSS 不知道 | 🔴 红（尺 B） |
| M6 | 给 `#app` 加一条没人认领的具名行 | 🔴 红（空转的行） |
| — | 还原后复核 | 🟢 绿 |

### 5.3 ⚠ 写这条判据时踩的一个坑（值得记）

仓里有一条只降不升的棘轮「测试里做目录遍历的文件只许变少」
（`tests/scanning-guard-registry.vitest.ts`，上限 11，**已满**），而抬那个上限是
**另一个文件的决定**，不该由一条新判据顺手替它做 ⇒ 人群改用 `import.meta.glob`
（构建期展开，不新增遍历者，而且结构上读不到自己）。

🔴 **改完仍然红** —— 红在**解释「我为什么不用目录遍历」的那句注释上**：
那条棘轮是按**子串**数的，所以连在注释里写出那几个 API 的名字都会把自己算成一个遍历者。
这正是这个仓治过的「判据在自己的登记表里找到自己」，只是高了一层。
⇒ 注释改成不点名 API。**这件事也说明那条棘轮的尺子偏粗**（它数的是「文本里出现过」
而不是「真的在遍历」），登记在这里，不在本轮的写区内。

### 5.4 它管不了什么（诚实边界）

- 尺 A 只认得今天实际存在的四种插法；有人用第五种形状（例如把节点交给一个工具函数再插）
  塞进 `#app`，这把尺子看不见。
- 这是**静态对账**，不是渲染：它保证「每个 item 都认领了一个声明过的区域」，
  不保证那个区域的位置好看 —— 那一格靠本文件的真引擎读数。
- `position: absolute/fixed` 的那几个画在哪，它不管（它们不是 grid item，不开隐式行）。

---

## 6. 门禁、检查与复算

### 6.1 全部现打

```
npx vitest run tests/app-grid-claims.vitest.ts      → 10 passed          ✅ 新判据
npx vitest run tests/scale2-height-truth.vitest.ts  → 17 passed          ✅ 金标准绊线
npx vitest run（全量）                               → 133 文件 / 1747 条 全过  ✅
npx tsc --noEmit                                    → 干净               ✅
npx stylelint "src/**/*.css"                        → 47 errors（改前 47）✅
npx vite build --outDir <scratch>                   → ✓ built            ✅
```

⚠ 秤 2 那条是**绊线**不是目标：本轮三件都不该碰到卡的盒模型
（抽屉在 `#app` 层 · 两处 TS 与卡无关 · `--bg-1` 只是颜色），它绿 ＝ 确实没碰到。
**金标准一个字节都没动**（另一路正在重刷它，本轮不碰）。

⚠ `npx tsc --noEmit` 期间 `tests/evidence/S25-class-ledger.ts` 报过语法错 ——
那是**另一路 agent 在飞的文件**，不在本轮写区，本轮零改动。过滤掉它之后本轮是干净的。

### 6.2 stylelint：逐条直方图**完全相同**

不是只对总数，是把改前/改后各自按规则名出直方图逐行 diff：

```
29 color-function-alias-notation · 6 declaration-property-value-keyword-no-deprecated
 5 at-rule-empty-line-before     · 3 no-duplicate-selectors
 2 comment-whitespace-inside     · 1 media-feature-range-notation
 1 import-notation               · 1 declaration-block-no-shorthand-property-overrides
⇒ diff 为空
```
（`media-feature-range-notation` 那一条是**改前就有的**、在别处；本轮新加的
`@media (width < 980px)` 正是它认的写法，故没多出报错。写成 `max-width: 979px`
实测会变成 48。）

### 6.3 改动规模

| 文件 | 改动 |
|---|---|
| `src/styles.css` | 6986 → 7061 行（+118 / −43，其中**大半是注释**：病史 + 踩坑 + 读数住址） |
| `src/styles/tokens.css` | field 族抬头加一段裁定理由 |
| `src/main.ts` | −6 / +5（删掉 `applyCollapse` + `resize` 监听，换成一段「别加回来」的注释） |
| `src/sftp/panel.ts` | −1 / +6（一行改写 + 「必须同拍」的注释） |
| `src/tabs.ts` | **0** |
| `tests/app-grid-claims.vitest.ts` | 新建 665 行 |

### 6.4 真引擎探针怎么复算

两个 runner 仍住 `/tmp/cv-reparent-probe/`（`run_chromium.mjs` / `run_webkitgtk.py`，
取值约定 `window.__DONE` + `window.__RESULT`）。被清掉了怎么重建见
`tests/evidence/U-scale2-run.sh` 末尾。本轮多要一个「窗宽可变」的口子：

```js
// run_cr.mjs —— 与 run_chromium.mjs 唯一的差别是 viewport 从 argv 来
import { chromium } from '/tmp/cv-reparent-probe/node_modules/playwright/index.mjs';
import { writeFileSync } from 'node:fs'; import { pathToFileURL } from 'node:url';
const [html, out, w = '900', h = '700'] = process.argv.slice(2);
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: +w, height: +h } });
await page.goto(pathToFileURL(html).href);
await page.waitForFunction('window.__DONE === true', null, { timeout: 300000 });
writeFileSync(out, await page.evaluate('window.__RESULT')); await browser.close();
```
WebKitGTK 侧只把 `win.set_default_size(900, 700)` 换成 `int(os.environ['PW'])`，
其余照 `U-scale2-run.sh` 的启动方式（`xvfb-run` + 关合成，否则 web process 起不来）。

探针页面：`<link rel="stylesheet" href="./styles.css">`（把 `src/styles.css` 与
`src/styles/tokens.css` 按相对路径复制过去，`@import … layer(tokens)` 在 `file://`
下能正常解析 —— 读到 `--field-bg = #1f1e1b` 就是这条链活着的证明）。

探针主体（**照 `index.html` 的真结构建 `#app`，再照 `tabs.ts:ensureArchiveUi` 的插法插抽屉**）：

```js
function buildApp(opts) {
  document.body.className = opts.viewer ? "viewer-mode" : (opts.collapsedClass ? "tabbar-collapsed" : "");
  document.body.innerHTML = "";
  const app = document.createElement("div"); app.id = "app";
  const bar = document.createElement("div"); bar.id = "tab-bar";
  const stream = document.createElement("main"); stream.id = "message-stream";
  const status = document.createElement("div"); status.id = "status-bar";
  app.append(bar, stream, status); document.body.appendChild(app);
  const rz = document.createElement("div"); rz.id = "tab-bar-resizer"; app.appendChild(rz);
  for (const c of ["settings-trigger","history-trigger","panorama-trigger","sftp-trigger","grid-monitor-trigger"]) {
    const b = document.createElement("button"); b.className = c; b.textContent = " "; app.appendChild(b);
  }
  if (opts.viewer) {                                  // main.ts bootstrapViewer
    const top = document.createElement("div"); top.className = "viewer-topbar";
    const t = document.createElement("span"); t.className = "viewer-topbar-title"; t.textContent = "abcd1234";
    top.appendChild(t); app.insertBefore(top, app.firstChild);
  }
  for (let i = 0; i < 3; i++) {                       // 三个真 tab，让 #tab-bar 有内容
    const tb = document.createElement("div"); tb.className = "tab";
    const dot = document.createElement("span"); dot.className = "live-dot";
    const ti = document.createElement("span"); ti.className = "tab-title"; ti.textContent = "会话 " + i;
    const cl = document.createElement("button"); cl.className = "tab-close"; cl.textContent = "x";
    tb.append(dot, ti, cl); bar.appendChild(tb);
  }
  let wrap = null;
  if (opts.archive) {                                 // tabs.ts:3279-3307 逐字照抄
    wrap = document.createElement("div"); wrap.className = "tab-archive";
    const toggle = document.createElement("button"); toggle.className = "tab-archive-toggle";
    toggle.textContent = "▸ 已归档 " + opts.archivedCount;
    const list = document.createElement("div"); list.className = "tab-archive-list";
    for (let i = 0; i < opts.archivedCount; i++) {
      const tb = document.createElement("div"); tb.className = "tab archived";
      const dot = document.createElement("span"); dot.className = "live-dot";
      const ti = document.createElement("span"); ti.className = "tab-title"; ti.textContent = "旧会话 " + i;
      tb.append(dot, ti); list.appendChild(tb);
    }
    wrap.append(toggle, list);
    bar.parentElement.insertBefore(wrap, bar.nextSibling);
    wrap.hidden = opts.archivedCount === 0;           // tabs.ts:3473
    list.hidden = true;                               // tabs.ts:3474（默认折叠）
  }
  return { app, bar, stream, status, wrap };
}
```
量的是 `getComputedStyle(app).gridTemplateRows / gridTemplateAreas`、四个盒子的
`getBoundingClientRect()`、抽屉的 `display` / `gridArea`、以及
「抽屉的 y ≥ 状态栏的 y」这条布尔。步 5 那 11 个 computed 格与步 7 的 2×2 见 §2.3 / §3
（进度条那一组必须 `await` 400ms 等 transition 跑完再量 —— 不等的话两边都读成 0，
会得到一个「哪边都没动」的假结论）。

### 6.5 中间产物住址（都在 scratchpad，不进仓）

```
<scratchpad>/s24/
  styles-BEFORE.css / styles-AFTER.css / main-BEFORE.ts / panel-BEFORE.ts
  rig/{probe.html, probe.js, styles.css, styles/tokens.css}
  run_cr.mjs / run_wk.py                      ← 窗宽可变的两个 runner
  res-{BEFORE,AFTER}-{cr,wk}-{900,1200}.json  ← 主读数矩阵
  res-BUILT-{cr,wk}-{900,1200}.json           ← 生产构建产物的同一批读数
  res-MUT-bp-cr-900.json                      ← 断点改成 (width < 1px)
  res-MUT-naked-cr-900.json                   ← 去掉 body:not(.viewer-mode) 前缀
  mutate.sh / mutation-log.txt                ← §5.2 那张变异台
  dist-check/                                 ← vite build 产物（不落 .build/）
```

---

## 7. 交回去的两件（本轮没做，不是忘了）

1. **`.tab-archive-list` 的 `hidden` 同样是空写** ⇒ 归档抽屉的「折叠」从来没生效过。
   属于归档**交互**，归步 17（`设计/30` 三刀），见 §1.6。
2. **`tests/scanning-guard-registry.vitest.ts` 那条棘轮的尺子偏粗**（按子串数，
   连注释里提到 API 名都算）。不在本轮写区，见 §5.3。
