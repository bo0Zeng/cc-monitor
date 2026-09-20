# `S30` —— 步 21 收尾：`设计/40` 步 9 ① ＋ `设计/41` 件 7/件 9 的机检，与四件做不了的

> 〔量于 2026-09-19〕对应 `设计/99 §4` 步 **21**（路 J）。
> 接 `tests/evidence/S21-css-readings.md`（九步第一刀）· `S24`（止血 ＋ 步 5/7 ＋ `--bg-1`）·
> `S25`（类名与 z-index 对账）。**本篇是"那一刻量到了什么"，不是现状文档**
> （`tests/evidence/README.md` 那条纪律）。
>
> 写区：`src/styles.css` · `src/styles/tokens.css` · 新建 `tests/css-conventions.vitest.ts` ＋
> `tests/evidence/S30-css-conventions.ts` ＋ 本文件。**零 `.rs`、零 TS 生产代码、
> 没有改 `tests/evidence/` 下任何已有的 `.md`、没有改 `CHANGELOG.md`、没有改 `gate.sh`**
> （为什么不用改 `gate.sh`，见 §4）。
>
> **复算全部读数**（从仓根）：
> ```bash
> npx vitest run tests/css-conventions.vitest.ts --reporter=verbose
> npx vitest run                     # 全量
> npx tsc --noEmit
> npx eslint .
> npx stylelint "src/**/*.css"
> bash tests/scripts/gate.sh
> ```

---

## 0. 一页结论

| | 本轮之前 | 本轮之后 |
|---|---|---|
| `设计/40 §7` 步 9 ①（变量对账） | ❌ 没装（`S21`/`S25` 都记着「要装插件，出写区」） | ✅ 装了，**两个方向都是恒等式** |
| `设计/41` 件 7（状态表达约定） | 🟡 纪律写在 `tokens.css` 抬头，**零机检** | ✅ 约定 1 有机检（约定 2/3 仍无，如实写在抬头） |
| `设计/41` 件 9（动画属性白名单） | 🟡 现状合规，但**没有任何东西在看** | ✅ 恒等式机检，**两向**（白名单外红 · 例外表腐了也红） |
| CSS 侧的判据格数 | 4（`css-ledger`）＋ 10（`app-grid-claims`） | ＋**14 条**（`css-conventions`，3 格 ＋ 1 格量具自检） |
| 全量 vitest | 134 文件 / 1746 条 | **135 文件 / 1760 条**，全过 |
| stylelint | 47 errors | **47**（一条没多） |
| `npx tsc --noEmit` / `npx eslint .` | 干净 / 7（基线） | 干净 / **7**（基线未动） |

🔴 **顺手逮到一条用户看得见的真缺陷**（不是本轮改出来的，是一直如此、只是以前没人对账）：
**设置里的「用户色」「Claude 色」两个取色器拖了没有任何反应** —— 见 §2。

🔴 **九步今天全部有着落了**（步 9 ① 是最后一格）。**十一件还欠四件，四件的前提各是什么、
为什么今天不成立，逐条现打在 §5** —— 其中件 2 的后半有一条**新查出来的硬前提**，
`S21` 当时给的理由（「等件 6」）是对的但不是全部。

---

## 1. 装了三格 ＋ 一格量具自检（14 条断言）

住址：判据 `tests/css-conventions.vitest.ts` · 量具 `tests/evidence/S30-css-conventions.ts`。

| 格 | 判什么 | 形态 | 分母（现打） |
|---|---|---|---|
| ⓪ | 量具没坏：人群没缩水 ＋ **剥 CSS 注释这一步在承重** | 地板 ＋ 恒等正控 | 2 份 CSS · 79 个定义 · 74 个 `var()` 名 · 1023 条规则 · 33 处 `.hidden =` |
| ⑤ | 自定义属性对账，**两个方向** | **恒等**（两向集合相等） | 同上 ＋ TS 侧 2 个 `setProperty` 名 ＋ `theme.ts` 14 个旋钮 |
| ⑥ | `transition` 只许动白名单那五个 | **恒等**（实测集合 == 白名单 ∪ 例外，且例外不许是死条目） | 17 条 `transition` 声明 / 29 处属性 / 7 种属性 |
| ⑦ | 会被 `hidden` 切的元素，CSS 不许在它身上裸写 `display` | **恒等**（违例集合 == 登记表） | 33 处 `.hidden =` → 19 个类名 ＋ 3 处解析不出（逐处登记） |

### 绿行（`npx vitest run tests/css-conventions.vitest.ts --reporter=verbose`，14/14）

```
✓ S30 ⓪ 扫到的人群没有缩水
✓ S30 ⓪ 🔴 正控：剥 CSS 注释这一步在承重（不剥的话散文会同时造假与灭真）
✓ S30 ⑤ ① 正向：CSS 用到而 CSS 里没定义的，必须恰好是 TS 现设的那几个
✓ S30 ⑤ ② theme.ts 那 14 个旋钮，CSS 里必须都有默认值
✓ S30 ⑤ ③ 反向：CSS 定义了却没人 var() 的，必须恰好是登记表里那几个
✓ S30 ⑤ 🔴 ④ 正控：登记表里哪几条是「用户可调却零消费者」，要判得出来
✓ S30 ⑥ 实测到的属性集合 == 白名单 ∪ 登记的例外
✓ S30 ⑥ 登记的例外今天必须真的还在（不许留死条目）
✓ S30 ⑥ 分母：判过的 transition 条数与属性处数
✓ S30 ⑦ 分母：判过的 `.hidden =` 处数与类名个数
✓ S30 ⑦ 解析不出类名的那几处 == 登记表（不许静默跳过）
✓ S30 ⑦ 裸写 display 的类 == 已知违例表
✓ S30 ⑦ 🔴 正控：`.tab-archive` 必须被判成「写了 display 但有 [hidden] 兜底」
✓ S30 ⑦ 已知违例表不许有死条目（修好了就把登记删掉）
```

### ⑤ 现打的两侧

```
CSS 里定义过的自定义属性            79
CSS 里 var() 到的自定义属性（剥注释后） 74
用了但 CSS 里没定义                  --fork-depth · --tab-bar-w
TS 侧 style.setProperty("--x", …)    --tab-bar-w  (src/main.ts:373, :403)
                                     --fork-depth (src/views/history.ts:1958)
theme.ts 的旋钮                      14 个，CSS 里全部有默认值（0 个缺）
定义了但 CSS 里没人 var()            7 个（2 个是缺陷 + 5 个是预留，见 §2）
```

🔴 **右边那一列不是手写清单，是现读 `setProperty(` 的结果。** 手写一份就是第二本账，
而第二本账会腐 —— 这一点在 ⑥ 的例外表上另有一条专门的判据盯着（「登记的例外必须今天真的还在」）。

### ⑥ 现打的属性直方图

```
color 7 · background 7 · transform 5 · border-color 4 · opacity 3
grid-template-rows 2   ← 登记的例外（0fr↔1fr 展开动画）
none 1                 ← 登记的例外（transition: none 是**关掉**过渡）
width 0                ← S24 已治好（改成 transform: scaleX）
```
⇒ `设计/41 §10` 那条白名单**今天零违例**，所以这一格装上就是绿的。
**恰恰因为它今天是绿的才值得装** —— 一条今天就红的判据会被人绕过，一条今天绿的判据挡的是明天。

### ⑦ 现打

```
src/**/*.ts 里 `<元素>.hidden = …`   33 处
其中静态解析得出类名的               30 处 → 19 个类名
解析不出的                           3 处（逐处登记，见判据里的 HIDDEN_UNRESOLVED）
19 个类里，CSS 在它自己身上写了 display 的  2 个
  .tab-archive        有 `[hidden]` 兜底 ✅（S24 补的）   ← 本格的正控
  .tab-archive-list   没有兜底           ❌ ← 真缺陷，登记为已知违例，见 §3
```

---

## 2. 🔴 顺手逮到的真缺陷：两个用户可调的取色器**拖了没反应**

`设计/40` 只要求查「用了但没定义」那一向。本轮把**反向**也做成恒等式（「定义了但没人用」），
而两条真缺陷全在反向 —— 只判正向的话这一格永远绿。

现打的链子，逐跳都有住址：

| 跳 | 住址 | 现打 |
|---|---|---|
| 设置界面给了两个取色器 | `src/settings/panel.ts:120-121` | `{ key: "user", label: "用户色", type: "color" }` · `{ key: "assistant", label: "Claude 色" }` |
| 它们映到两个 CSS 变量 | `src/theme.ts:46-47` | `{ key: "user", cssVar: "--user" }` · `{ key: "assistant", cssVar: "--assistant" }` |
| 改动时真的写进了 DOM | `src/theme.ts:80` | `root.style.setProperty(token.cssVar, str)` |
| 而 CSS 里读它的地方 | `src/styles.css` ＋ `src/styles/tokens.css` | **0 处**（`var(--user)` 0 · `var(--assistant)` 0） |

⇒ **用户拖这两个取色器，界面上不会有任何变化。**

⚠ **不是本轮改出来的**：`git show main:src/styles.css | grep -c 'var(--user'` = **0**，
`var(--assistant)` 同样 0。一直如此，只是以前这一面**没有任何对账**（`设计/40 §0` 逐字：
「这 7248 行里没有任何一条机器检查……唯一没有对账的地方，恰好是 bug 最密的地方」）。

⚠ `tokens.css` 里那句「兼容，仍可用于 slash 卡左条等」是**当时的打算不是今天的事实**，
本轮改成了现状 ＋ 一条指向判据的指针。

### 为什么本轮不修

该让**哪些元素**跟着这两个旋钮走，是产品决定：用户气泡的底色（今天走 `--card`）？
卡片左边条？名字的颜色？`设计/40` 与 `设计/41` 都没有写过一句。
⇒ **不自己挑。** 已登记进判据的 `UNUSED_TOKENS`，并配了一条**正控**专盯
「既是 `theme.ts` 旋钮、又零消费者」这一族（现打恰好这两个）——
再多一个旋钮变成空转就会当场红；接上消费者之后要把登记删掉，判据也会提醒。

### 另外五个「定义了没人用」的，是预留不是腐

`--field-border` · `--field-border-focus` · `--field-placeholder` · `--state-focus` ·
`--state-disabled-opacity` —— `设计/41 §2` 新补的两族里还没接上消费者的部分，逐个登记了理由。

🔴 **其中 `--state-disabled-opacity` 那一条要单独说：`设计/41 §2` 要求把散在各处硬写的
`0.45 / 0.5 / 0.55` 收成这一个 token，而现打 `src/styles.css` 里 `opacity: 0.x` 有 46 处、
值域 0.35–0.95，里面混着水印、悬停压暗、幽灵态、禁用态**。
「哪几处是『禁用/变灰』」**机械分不出来**（语法上一模一样），收编要逐条读语义 ＋ 统一到同一个值
（0.55 → 0.45 是看得见的变化）。⇒ **这一条本轮判不了，不硬做**，如实登记在判据的登记表里。

---

## 3. 🔴 ⑦ 逮到的那一条：`.tab-archive-list` 的 `hidden` 是空写

`S24 §1.6 / §7` 已经点过名（「交回去的两件」之一）。本轮把它**从散文变成了会红的登记**。

```
src/tabs.ts:3474   this.archiveList.hidden = collapsed;
src/styles.css     .tab-archive-list { display: flex; … }   ← 作者层，压过 UA 的 [hidden]{display:none}
```
⇒ 归档抽屉的「折叠」**从来没生效过**：默认是折叠态（`tabs.ts:3278` 逐字「默认折叠，缺省 `"1"`」），
按钮画着 `▸ 已归档 N`（`▸` ＝「点开」），而归档 tab 照样全列出来 —— **箭头在骗人**。

### 为什么本轮不修（两条，不是忘了）

1. 修法只有一行 CSS（`.tab-archive-list[hidden] { display: none; }`），**但它改的是用户看得见的交互**，
   而仓里对这件事有**两句相反的话，在同一个文件里**：
   - `src/tabs.ts:3278` 逐字：「P7a-1：归档区 UI 建一次。**默认折叠**（缺省 `"1"`，与 agents/tasks 面板同形态）。」
   - `src/tabs.ts:3294-3299` 那段头注逐字：「宁可退回改之前的样子（归档 tab **灰着留在主栏**），
     也不要让它凭空不见……**『灰着但还在』正是用户对归档的全部期待**。」
   ⇒ **该听哪一句是产品裁定，不是工程问题。** 按纪律不自己挑。
2. `设计/99 §4` 步 **17**（`设计/30` tab 三刀）本来就要把「归档」整个处置掉，现在打补丁可能白做 ——
   这也正是 `S24` 当时把它交回去的理由，本轮没有推翻它，只是**给它上了一把会红的锁**。

⚠ 同形的 `.tab-archive` 已由 `S24` 修掉（那一条是「偷高度 ＋ 画到状态栏下面」，不涉交互）。
它现在是本格的**正控**：判据要求它被判成「写了 `display` **但有** `[hidden]` 兜底」——
只断言「今天没有新违例」是不够的，那条在「尺子把所有东西都判成有兜底」时同样绿。

---

## 4. 🔴 死值验：12 把刀，刀刀见红；还原后 14/14 复绿

**怎么跑的（一个字都没动真工作树）**：照 `S25` 那条取法，把树 `tar` 到
`$SCRATCH/s30/mirror/`（`node_modules` 用符号链接），**在镜像里下刀**。
`tests/test-support/repo-root.ts` 的 `findRoot()` 向上找 `package.json`
⇒ 在镜像里跑时自然解析到镜像根，判据一行不用改、也不需要任何环境变量后门。

| 刀 | 下在哪 | 红在哪几条 |
|---|---|---|
| 1 | `styles.css` 加 `.s30-mutant-a { color: var(--nope-not-a-token) }` | ⑤① |
| 2 | `main.ts` 把 `setProperty("--tab-bar-w"` 改名 | ⑤①（TS 侧登记一漂，两向立刻分叉） |
| 3 | `tokens.css` 删掉 `--live` 的定义（它是 14 个旋钮之一） | ⑤① ＋ ⑤② |
| 4 | `tokens.css` 新增一个没人用的 `--s30-probe-token` | ⑤③ |
| 5 | `styles.css` 里给 `--user` 接上一个消费者 | ⓪正控 ＋ ⑤③ ＋ ⑤④ |
| 6 | `styles.css` 加 `transition: width 0.2s linear` | ⑥ |
| 7 | 把两处 `transition: grid-template-rows` 换成 `opacity` | ⑥「例外表不许有死条目」 |
| 8 | 给 `.ccm-toast-count`（会被 `hidden` 切）裸写 `display: inline` | ⑦「裸写 display 的类 == 登记表」 |
| 9 | 摘掉 `.tab-archive[hidden] { display: none }` | ⑦ 违例表 ＋ **⑦ 正控** |
| 10 | `tabs.ts` 加一处解析不出类名的 `.hidden =` | ⑦「解析不出的那几处 == 登记表」 |
| 11 | **把 `styles.css` 整个掏空** | **11 条红 / 3 条绿**（见下） |
| 12 | 把量具的 `stripCssComments` 改成恒等（**尺子自己坏掉**） | ⓪正控 ＋ ⑤①③④ ＋ ⑦×2，共 6 条 |

还原后复跑：**14 passed，一条都没红。**

### 🔴 刀 11 是反空真那一条，单独说

把 `styles.css` 掏空之后，红的是**分母地板与正控**，不是「扫不到就绿」：

```
✗ ⓪ 只扫到 … 个定义/var()/规则 —— 掉到地板以下
✗ ⓪ 正控：`--z-` 那条散文没了 ⇒ 剥注释这一步失去对象
✗ ⑤① 两向集合分叉（CSS 侧空了，TS 侧还有 2 个）
✗ ⑤③④ 全部 79 个定义变成「死令牌」/ 正控的两个旋钮没了对象
✗ ⑥ 一条 transition 都没有 ⇒ 例外表整张变成死条目 ＋ 分母地板破
✗ ⑦ 违例表与正控双双失去对象
```
⇒ **「扫到 0 处」在本条下是失败，不是绿。**

### 🔴 刀 12 是「尺子自己坏掉」那一条，也单独说

`stripCssComments` 是这把尺子的承重墙，而它坏掉的样子**两个方向都是错的**：

- **造假**：`tokens.css` 抬头逐字写着「z-index 只许写 `var(--z-*)`」——
  不剥注释它会变成一个叫 `--z-` 的「用了但从未定义」的变量 ⇒ ⑤① 假红。
- **灭真**：`--user` / `--assistant` 今天**只在注释里**被 `var()` 提到（就是 §2 那段说明），
  不剥注释它们会被自己的说明文字「救活」⇒ **§2 那条真缺陷静默消失**。

⇒ ⓪ 那条正控断言的不是「剥了」，是「**不剥就会给出错的答案，而且错在这两个具体的地方**」。

---

## 5. 判据在不在门禁的执行链上 —— **在，而且现打验过**

`tests/scripts/gate.sh` 的 `npm` 那一格跑 `npm test`，链末是 `test:dom` ＝ `vitest run`，
而 `vitest.config.ts` 的 `include` 是 `tests/**/*.vitest.ts` ⇒ 本判据自动在里面。

**不需要往 `gate.sh` 里加行，加了反而是第二处登记。** 现打的证明（在镜像里）：

```
$ printf '\n.s30-chain-probe { color: var(--nope-not-a-token); }\n' >> src/styles.css
$ npm test ; echo rc=$?
 Test Files  3 failed | 132 passed (135)
      Tests  5 failed | 1755 passed (1760)
rc=1
```
⇒ 只往 CSS 里塞一行，`npm test` 就红了，而 `gate.sh` 的 `npm` 那一格吃的就是它的退出码。

---

## 6. 🔴 十一件还欠四件：逐件的前提今天成不成立

> 纪律：前提是假的就如实说，不硬做。下面每一条都带现打证据。

### 件 2（`@layer` 的后半：包 `base` / `layout` / `components`）—— **前提是假的，而且不只是 `S21` 说的那条**

`S21` 记的理由是「要等件 6（按层切文件），而件 6 绑三入口拆分」。那条仍然成立，
但**本轮现打到一条更硬的**：

```
src/render.ts:5   import "highlight.js/styles/github-dark-dimmed.css";
src/render.ts:6   import "katex/dist/katex.min.css";
```
这两份第三方样式表是**从 TS 里 import 的 ⇒ 它们是「无层」的**。而 `@layer` 的规则是
「**无层的样式赢过所有有层的样式**」。今天 `styles.css` 的 6900 行也无层，两边靠
**声明顺序 ＋ 特异度**分胜负，其中有三条是我们**刻意压第三方**的：

```
src/styles.css:1620   .card-body .katex-display { … }
src/styles.css:1710   .code-block pre code.hljs { … }
src/styles.css:2312   .block-diff-line code.hljs { … }
```
⇒ 一旦把 `styles.css` 的其余部分包进任何一层，**这三条会立刻输给第三方的无层规则**
（层再高也压不过无层）。要先把那两份第三方 CSS 也塞进一层（`@import … layer(vendor)`），
而那两行 `import` 住在 **`src/render.ts`**，是 TS，**不在写区**。

⇒ **这不是「等件 6」那种排期问题，是一条硬前置。** 记在这里，接手的人省一次踩坑。

### 件 3（最小重置）—— 前提不成立，理由与 `S21` 相同，本轮复核过一遍

`设计/41 §12` 自己把它排在第二批，逐字「**与三入口拆分一起做**（那时本来就要过一遍全局样式），
并且**逐窗口目视**」，风险标「中（有视觉风险，要目视）」。
本环境是无头机，**做不了目视**；而它的风险恰恰是机器看不见的那一类
（某个按钮只写了背景、`border` 靠 UA 默认撑着，加 `border: 0` 之后「长得不对但不报错」）。
它点名要治的那个具体 bug（`.user-inputs-toggle`）**前面的轮次已单独修掉**（`S21` 步 4）
⇒ 现在做它是「防下一次」不是「止血」。**`reset` 那一层的空壳已经建好，内容加进去是纯增量。**

### 件 5（容器查询）—— 前提不成立

`设计/41 §12` 逐字「与 `设计/10` 的『`COL_W` 从容器实测』一起」，而
`src/height-estimate.ts` 是 TS，不在写区。只做 CSS 那一半会得到
「卡片按列宽排版、估高仍按写死的 780 算」——**两边比今天更不一致**。
⚠ `S21` 留的那条动手前必须先验的坑仍然有效（`container-type: inline-size` 带 layout
containment ⇒ 它会变成子树里 `position: fixed` 元素的包含块），本轮没有新读数。

### 件 6（文件按层 ＋ 按窗口切）—— 前提不成立

要动 `vite.config.ts` ＋ 入口 HTML ＋ TS，全在写区外。
本轮只再确认一条既有事实：`@import "./styles/tokens.css" layer(tokens)` 这条路是通的
（`S21` 已在生产构建产物里验过），所以将来按层切文件不增加请求数、也不丢层。

### 件 10（渐进式 CSS Modules）—— 前提不成立

要动 TS（`import s from "./x.module.css"`），且 `设计/41` 自己说「摊开做、不设期限、
别一次迁 800 处」。本轮没有新组件，没有自然落点。

### 件 11（主题预设 / 亮色）—— **用户 2026-09-16 已裁：不做**

---

## 7. 诚实边界（这把尺子买不到的，逐条）

1. **⑤ 的「CSS 里定义过」是集合式的，不看层叠作用域。** `.foo { --x: 1 }` 定义的 `--x`
   在本尺眼里就算「CSS 里有」，哪怕真正用它的是另一棵子树（那种情况运行时仍然拿不到值）。
   `设计/40 §7` 步 9 ① 要的就是集合式那条（逐字：「CSS 里 `var(--x)` 的集合 ⊆ (CSS 定义 ∪ …)」）。
2. **⑤ 的反向判准是「CSS 里没人 `var()` 它」，不是「它对渲染没有贡献」。**
   一个被 `var()` 引用、但那条规则本身匹配不到任何元素的 token，本尺看不出来。
3. **⑥ 只认静态字面量的属性名。** 值里拿 `var(--something)` 当属性名的写法看不见 ——
   现打 0 处，属潜伏不是现患。
4. **⑦ 的 TS 侧不是真语法解析**：靠正则 ＋「离它最近的那一次上游赋值」。
   失效的样子是**解析不出**（落进 `HIDDEN_UNRESOLVED` 那条恒等式，多一处少一处都红），
   不是解析错 —— 失效是吵闹的。两个具体的坑逐字写在量具头注里（同名局部变量跨函数串味 ·
   接收者只比最后一个标识符会把 `b.el` 认成 `this.el`，**两个都是第一版真踩过的**）。
5. **⑦ 只看主语。** `.a .b { display: … }` 压的是 `.b` 自己，所以只有选择器**最后一个复合**
   里出现那个类才算数；祖先位置出现不算。
6. **三格都只看 `src/`，不看 `tests/`。** 与 `S25` 同一条取法（理由见那边：
   把 `tests/` 收进语料，一条「断言它不该存在」的测试会把死规则救活）。
7. **本轮跑的是 Linux，Windows 上一次都没跑过。** 路径比较统一走 `relative()` ＋
   `split("\\").join("/")`（照 `tests/eslint-baseline.vitest.ts` 头注那两条），
   但那是静态推出来的、未实测。
8. **没有任何真引擎读数。** 本轮改的 CSS 只有注释（48 行增 / 2 行改，见 §8），
   不改任何一条声明的胜负 ⇒ 没有重打秤 2 的必要，也没打。

---

## 8. 本轮的改动规模与其余检查（全部现打）

```
src/styles.css         +11 行（全是注释：`.tab-archive-list` 那条违例的逐字交代）
src/styles/tokens.css  +39 / -2 行（抬头补两段约定 ＋ `--user`/`--assistant` 的现状订正）
tests/css-conventions.vitest.ts        新建 373 行（判据 ＋ 三张登记表）
tests/evidence/S30-css-conventions.ts  新建 300 行（量具）
tests/evidence/S30-readings.md         新建（本文件）
—— 零 `.rs`，零 TS 生产代码，零 `gate.sh`，零 `CHANGELOG.md`，
   没有改 tests/evidence/ 下任何已有的 .md
```

```
npx vitest run tests/css-conventions.vitest.ts → 14 passed                       ✅
npx vitest run（全量）                          → 135 文件 / 1760 passed / 0 失败   ✅
npx tsc --noEmit                               → 干净（rc=0）                     ✅
npx eslint .                                   → 7 errors（基线 7，未动）          ✅
npx stylelint "src/**/*.css"                   → 47 errors（棘轮上限 47，未动）     ✅
bash tests/scripts/gate.sh                     → 见 §9
```

⚠ **一条环境读数，不是代码问题，但会咬下一个在 worktree 里干活的人**：
本轮是在 `.claude/worktrees/<id>/` 这个 git worktree 里跑的，而它**自己没有 `node_modules`**
（`npx` 靠向上找父目录解析到主检出那一份，所以工具跑得起来）。
但 `tests/css-ledger.vitest.ts` 的第三方类名那一档是 `readFileSync(join(root, "node_modules", …))`
—— `root` 是 worktree 根 ⇒ **读不到，`.katex-*` 会被判死，4 条断言当场红**。
⇒ 处置：在 worktree 根把 `node_modules` 软链到主检出那一份（`S25 §9` 的镜像也是这个做法）。
**这不是 `css-ledger` 的缺陷**（在正常检出里它是对的），是「worktree 少一步 `npm ci`」。

## 9. 中间产物住址（都在 scratchpad，不进仓）

```
$SCRATCH/s30/
  mirror/      整棵树的副本（node_modules 是符号链接），12 把刀全在这儿下的
  cuts.py      12 把刀的脚本：逐刀下、逐刀跑、逐刀还原，最后复跑一遍证明还原干净
  chain.log    「判据在不在门禁执行链上」那一趟 npm test 的全文
```
重建镜像：在仓根
`tar -c --exclude=node_modules --exclude=target src tests index.html package.json .stylelintrc.json vitest.config.ts tsconfig.json eslint.config.js vite.config.ts | tar -x -C <目标>`，
再 `ln -s <主检出>/node_modules <目标>/node_modules`。
