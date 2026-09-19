# `S25` —— 把 CSS 的对账接进判据：读数与死值验

> 〔量于 2026-09-18〕`设计/40 §7` **步 9** ＋ `设计/41 §12` **件 8**。
> 前提是上一轮 `tests/evidence/S21-css-readings.md` 备好的（步 9 那张表）。
> **本篇是"那一刻量到了什么"，不是现状文档**（`tests/evidence/README.md` 那条纪律）。

---

## 0. 一页结论

装了 **4 格**（13 条断言）。全部现打过死值验：**9 把刀，刀刀见红**。
第 5 格（`#app` 布局根）本篇不装 —— 收尾时现打：**同一棵树上另一路（`S24`）已经把它装了**，见 §5。

| 格 | 判什么 | 形态 | 分母（现打） | 状态 |
|---|---|---|---|---|
| ① | `z-index` 只许 `var(--z-*)` | **恒等** | **39** 条 `z-index` 声明 | ✅ 装上，绿 |
| ② | CSS 里的类名有人用（CSS → 代码） | **恒等** | **777** 个 CSS 类名 | ✅ 装上，绿 |
| ③ | 代码挂的类名 CSS 里有规则（代码 → CSS） | **递减棘轮** 163 | **859** 个代码侧引用 | ✅ 装上，绿 |
| ④ | `stylelint` 报错总数 | **递减棘轮** 47 | **2** 份 CSS 文件 | ✅ 装上，绿 |
| ⑤ | `#app` 布局根子元素 | 恒等 | — | ⛔ 本篇不装：**同一棵树上 `S24` 已经装了**，见 §5 |

住址：判据 `tests/css-ledger.vitest.ts` · 量具 `tests/evidence/S25-class-ledger.ts` ·
stylelint 侧 `.stylelintrc.json` · CI 侧标记 `.github/workflows/ci.yml` ·
跑法 `npm run gate:css`（＝ `vitest run tests/css-ledger.vitest.ts`）。

**顺手查出的真缺陷两条**（都不在本轮写区，见 §6）：
`.paste-block-warning` 是真死规则 · `.settings-btn-secondary` 挂着却没有任何 CSS 规则。

---

## 1. 绿行（现打，`npm run gate:css --reporter=verbose`）

```
ok   S25-⓪  777 个 CSS 类名 · 205 份代码文件 · 2438 个代码侧 token
ok   S25-⓪  216 条识别证据（常量拼接 2 · 第三方 184（2 份） · 前缀候选 30）
ok   S25-⓪  4 种真实形状（嵌套模板 · 正则里的引号 · HTML class 属性 · 注释不算用户）
ok   S25-①  39 条 z-index 声明（全部走刻度）
ok   S25-①  1 条 stylelint 侧的 z-index 白名单模式（与判据对拍过一正一反）
ok   S25-①  2 条变异体声明（1 条该红、1 条该绿，都判对了）
ok   S25-②  3 个已知假阳性（三族各一，裁决与机制都对上了）
ok   S25-②  777 个 CSS 类名（直接字面量 738 · 常量拼接 2 · 第三方 1 · 模板前缀 35 · 已登记死规则 1）
ok   S25-②  11 条登记（前缀共掩 35/35 个类：acct-c→8 agent-→3 block-diff-→2 conf-→3
             kind-→3 pf-dot-→2 remote-gap-→2 remote-status-→4 status-→5 tone-→3）
ok   S25-②  2 个变异体类名（1 个该死、1 个该活，都判对了）
ok   S25-③  859 个代码侧类名引用（其中 163 个 CSS 里没规则，棘轮上限 163）
ok   S25-④  2 份 CSS 文件（报错 47/47，棘轮只许降）
ok   S25-④  1 处 CI 侧散文（与判据常量对上了）
```

⚠ 绿行的写法照抄 `tests/scripts/gate.sh` 的 `run_gate`：**每一行都带分母**。
`K-G3 §2` 那条逐字：「一行不带分母的读数，不论数字是几都不算数。」

---

## 2. 🔴 死值验：9 把刀，刀刀见红

**怎么跑的（重点：一个字都没动真工作树）**：`src/styles.css` 本轮明令禁碰（另有一路在改它）。
所以把整棵树 `tar` 到 `$SCRATCH/mirror/`（`node_modules` 用符号链接），**在镜像里下刀**。
`tests/test-support/repo-root.ts` 的 `findRoot()` 是向上找 `package.json` 的
⇒ 在镜像里跑时它自然解析到镜像根，**判据一行都不用改，也不需要开任何环境变量后门**。

| 刀 | 下在哪 | 红在哪几格 | 读数 |
|---|---|---|---|
| 1 | `styles.css` 追加 `.s25-mutant-z { z-index: 99; }` | ①②④ | 「这些 `z-index` 没走刻度」／「没有任何人挂它」／stylelint **48 > 47** |
| 2 | 追加 `.s25-nobody-claims-this { color: red; }` | ② | 「这些 CSS 类名今天没有任何人挂它，而且不在 `KNOWN_DEAD` 里」 |
| 3 | `main.ts` 里挂一个 `className = "s25-dangling-probe"` | ③ | 「悬空 **164 > 163**（分母 860）」 |
| 4 | **把 `styles.css` 整个掏空** | ⓪①②③ 共 **6 条** | 「CSS 类名分母掉到地板以下」／「只扫到 **0** 条 `z-index`（地板 30）」／「正控失去对象，本条会假绿」 |
| 5 | `FIRST_RUN_HINT_CLASS` 改成 `["status","first","run"].join("-")` | ⓪② | 「常量拼接一条都没识别出来」／**「`.status-first-run-open` 的裁决是 `prefix`，而它应该靠 `const-concat` 活着」** |
| 6 | 登记表里塞一个今天解释不到东西的前缀 `settings-btn-` | ② | 「这些前缀登记着，但今天一个 CSS 类都解释不到」 |
| 7 | 把 `ci.yml` 的 `S25-STYLELINT-CEILING: 47` 改成 `50` | ④ | 「`ci.yml` 里的数与 `STYLELINT_CEILING` 漂了：expected 50 to be 47」 |
| 8 | 删掉 `src/render.ts` 里的 `import "katex/dist/katex.min.css"` | ⓪②③ | 「第三方类名只收到 46 个（地板 80）」／**「`.katex-display` 的裁决是 `unexplained`，而它应该靠 `vendor` 活着」** |
| 9 | 追加 `.status-s25-fake-dead { color: red; }`（一个会被 `status-` 前缀掩掉的死规则） | ② | 「靠前缀才解释得通的有 **36 > 35**」 |

### 🔴 刀 4 是**反空真**那一条，单独说

完成判据要求「每条判据都要能报『扫到 0 处』并**当成失败**，不是静默绿」。
把 `styles.css` 掏空之后，**6 条断言红、7 条绿**，红的全是分母地板与正控：

```
CSS 类名分母掉到地板以下 —— 选择器抽取坏了: expected 0 to be greater than or equal to 600
只扫到 0 条 `z-index` 声明（地板 30）—— 抽取器坏了，本条会零命中地绿: expected 0 >= 30
`.status-first-run-open` 已经不在 CSS 里了 —— 正控失去对象，本条会假绿
这些前缀登记着，但今天一个 CSS 类都解释不到：acct-c, agent-, …（10 条全中）
代码里挂着、CSS 里没规则的类名有 857 个 > 棘轮上限 163
```

⇒ **"扫到 0 处"在本条下是失败，不是绿。**

### 🔴 刀 5 与刀 8 是**正控**那一条，也单独说

这两刀证明的是一件比"没被判死"更强的事：**裁决必须是对的那一种**。

- 刀 5 把常量拼接掐断之后，`.status-first-run-open` **仍然"活着"** ——
  它掉进了 `status-` 那个开区间前缀里。如果本条只断言「它没被判死」，**这一刀会静默地绿**。
  正因为断言的是 `kind === "const-concat"`，识别路径一断就当场红。
- 刀 8 同理：`.katex-display` 从 `vendor` 掉成 `unexplained`。

⇒ 这正是上一轮血教训（13 个里 3 个假阳性）在判据层面的对治：
**一把把所有东西都判活的尺子，和一把判得对的尺子，在"没有假阳性"这条断言下长得一模一样。**

---

## 3. 🔴 类名对账：那份"11 个前缀白名单"的现打答案

`S21` 三处写着「今天的 11 个前缀白名单**不够**，至少要再加 `FIRST_RUN_HINT_CLASS` 这类常量拼接、
第三方 `katex-*`，以及本轮现打到的 8 处模板拼接点」。

**本轮现打的答案不是"补到 19 个"，是"那三族根本不该由前缀来管"。**

| 族 | `S21` 的处置建议 | 本轮的落点 | 理由 |
|---|---|---|---|
| 常量拼接 | 加进前缀白名单 | **`const-concat` 档**（把 `${CONST}` 填回去再扫一遍） | 前缀是开区间，而常量拼接拼出来的是**确定的全名**。当成前缀会白白掩掉 `status-first-run-*` 之外的东西 |
| 第三方 `katex-*` | 加进前缀白名单 | **`vendor` 档**（从 `import "<包>/….css"` 现读那几份 CSS 的类名） | 白名单写死 `katex-` 会在换库/加库时静默失效；现读住址不会过期。现打收上来 **184** 个（katex ＋ highlight.js） |
| 8 处模板拼接点 | 逐条登记 | **7 处不必登记** | 它们拼出来的类名（`settings-btn-secondary` · `cc-bus-hooks-ok` · `history-chip` · `bash-output-body` · `block-collapsible` · `remote-test-ok` …）在别处**有直接字面量住址**，`literal` 那一档先接住了 |

### 真正需要登记的，现打是 **10** 条（比那份 11 个清单还少一条）

每条都带住址与理由（逐条理由写在 `tests/css-ledger.vitest.ts` 的 `ALLOWED_PREFIXES` 里，不在这儿复述一份）：

```
acct-c          src/account-color.ts:38                  → 掩 8 个（acct-c0…c7）
agent-          src/agents-panel.ts:139                  → 掩 3 个
block-diff-     src/cards/diff.ts:311                    → 掩 2 个
conf-           src/views/panorama.ts:1201               → 掩 3 个
kind-           src/settings/data-section.ts:188         → 掩 3 个
pf-dot-         src/views/port-forward.ts:142            → 掩 2 个
remote-gap-     src/settings/remote-section.ts:391       → 掩 2 个
remote-status-  src/settings/remote-section.ts:163       → 掩 4 个
status-         src/tasks-panel.ts:177                   → 掩 5 个
tone-           src/settings/config-surface-section.ts:333 → 掩 3 个
                                                           合计 35
```

少的那一条是 **`paste-block`**：`src/paste-block.ts:90` 的
`` `paste-block${spec.className ? ` ${spec.className}` : ""}` `` 确实是个拼接点，
但那个洞里插的东西**永远以空格开头或为空** ⇒ `paste-block` 是个**完整类名**，不是前缀。
把它登记成前缀的代价是：它会掩掉 `.paste-block-warning` —— 而那**正是一条真死规则**（§6 ①）。
⇒ **不登记**，让那条死规则显出来。

### 🔴 前缀是开区间 —— 掩掉的总量本身也钉住了

`status-` 在解释 5 个真类的同时，会掩掉**任何**将来变死的 `.status-xxx`。
开区间关不上（关上要手工枚举后缀，一加新状态就假红），但**掩掉的总量可以钉**：
`PREFIX_COVERAGE_CEILING = 35`，只许降。刀 9 验过它会红。

---

## 4. 🔴 词法器：两处"用正则就会静默给出错答案"的地方

类名对账要从 TS 里取字符串。第一版用 `` /`([^`]*)`/g `` 取模板，**栽了两次，两次都不报错**：

| 形状 | 真住址 | 正则版的答案 | 后果 |
|---|---|---|---|
| **嵌套模板** | `src/paste-block.ts:90`<br>`` `paste-block${spec.className ? ` ${spec.className}` : ""}` `` | 在**内层**反引号收尾，静态段变成 `paste-block${spec.className ? ` | `.paste-block` 被判死 |
| **正则字面量里的引号** | `src/launcher-diagnostics.ts:149`<br>`` `'${s.replace(/'/g, `'\\''`)}'` `` | `/'/g` 里的单引号被当成字符串开头 ⇒ **整份文件词法错位** | 同文件 `:476` 的 `className = "ccm-alias-gen"` 读不到 ⇒ `ccm-alias-gen*` 4 个类平白变死 |

还有两处形状也必须认（否则同样批量假红）：

| 形状 | 真住址 | 处置 |
|---|---|---|
| **HTML 片段里的 `class` 属性** | `src/render.ts:77-79` 的 `` `<div class="code-bar">` `` | token 切分**按类名能用的字符切，不按空白切** ⇒ `class="code-bar"` 里切得出 `code-bar` |
| **注释里的散文不算用户** | `src/render.ts:49` 的 JSDoc 逐字写着 `` `<code class="hljs language-X">` `` | 方向 ② 的正则跑在原文上 ⇒ **先抹注释**。不抹的话 `.language-X` / `.code-pending` 会被当成"代码在用的类" |

⇒ 这四条各有一条断言钉着（判据里那条「词法器对四种真实形状给出正确答案」）。

---

## 5. ⑤ `#app` 布局根断言：**本篇不装 —— 它已经有家了**

`设计/40 §7` 步 9 ③ 要的是：`#app` 的 in-flow 直接子元素 == `{#tab-bar, #message-stream, #status-bar}`。

**本轮开工时它不成立，而且不成立的原因不在本篇这一格。** `S21 §6 ④` 的真引擎实测
（Chromium 153，900×700）：

| | `#app` 的 `grid-template-rows` | `#message-stream` 高 | 抽屉落在哪 |
|---|---|---:|---|
| 没有抽屉 | `676px 24px` | **676** | — |
| 插入抽屉 | `641px 24px **35px**` ← 多出一条隐式行 | **641（−35）** | y=665，**在状态栏下面** |

病灶：`src/tabs.ts` 的 `ensureArchiveUi()` 往 `#app` 插 `.tab-archive`，
而 CSS 给它的只有 `display/flex-direction/gap/padding` —— **没有 `grid-area`、没有 `position`**
⇒ 一个**没认领格子的 grid item**，浏览器给它开了条隐式行，偷走消息流 35px。

`src/tabs.ts` 与 `src/styles.css` 本轮都明令禁碰 ⇒ 本篇原计划是「点名交出去」。
**收尾时现打：同一棵树上另一路（`S24`）已经把缺陷修了，并把这一格装成了
`tests/app-grid-claims.vitest.ts`**（现打随全量套件绿）。

⇒ **本篇不再复制这一格**（同一个性质两个住址必漂）。而且 `S24` 的形态比 `设计/40` 的字面写法更对，
这一点值得记下来：它钉的不是「子元素 == 那三个」——`.tab-archive` 在 `设计/99 §4` 步 17 之前还得在，
钉集合会持续假红、只会训练人去绕过判据；它钉的是**「每个 in-flow 直接子元素都认领了一个
本模式模板里声明过的具名区域」**，于是隐式行数被钉死在 0，两种模式（含 `body.viewer-mode`）都看。

⚠ 这条同时是本轮的一个**并发读数**：两路各自读同一份 `设计/40 步 9`，
`S24` 拿走了 ③，`S25`（本篇）拿了 ②④⑤ 与 stylelint 那半。分工是收尾时才对上的，不是事前协调的。

## 6. 顺手查出来的两条真缺陷（都不在本轮写区）

### ① `.paste-block-warning` 是真死规则 —— `src/styles.css:6748`

`src/paste-block.ts:25-31` 的模块头逐字：「原先这里有个 `warning` 槽，T03 审计后**移回消费者自己那儿**」。
槽拆了，CSS 留在原地。**三把尺子同口径**（照 `S21` 步 3 那张表的取法）：

| 尺子 | `.paste-block-warning` | 对照组 `.paste-block-copy`（活的） |
|---|---|---|
| `src/**` | 0 | 1（`src/paste-block.ts:122`） |
| `tests/**` | 0 | — |
| 生产 bundle `.build/dist/assets/*.js` | **0** | **1** |

⇒ 第三列是那条「有反应 ⇒ 它在生效」的判据：**同一把尺子既读出 0、也读出非 0**，不是永远说 0 的坏尺子。
⇒ 已登记进 `KNOWN_DEAD`，附住址与理由。**删掉那 5 行 CSS 之后，把那条登记一起删**（判据会提醒）。

⚠ 一条诚实边界：bundle 这把尺子对**常量拼接**那一族不可靠 ——
现打 `.status-first-run-open` 在今天的 bundle 里是 **0**（`S21` 当时量到的是 1）。
minify 会把 `` `${X}-open` `` 拆开，名字在字节里根本不连续。
⇒ bundle 只当**旁证**，不当判官；判官是 `src/**` 的词法结果。

### ② `.settings-btn-secondary` 挂着，但 CSS 里一条规则都没有

`src/launcher-diagnostics.ts:693` 与 `:698` 两处在挂它，而 `src/styles.css` 里
`.settings-btn-secondary` **零条规则**（同族的 `.settings-btn` 是 styled 的）。
同形的还有 `.cc-bus-online-unknown`（`src/settings/cc-bus-section.ts:494`）。
⇒ 多半是改名只改了一边。它们已经落在格 ③ 的棘轮人群里（163 之一），**没有单独立格**
（机械上分不清「纯 JS 钩子」与「忘了写样式」，见下）。

---

## 7. 诚实边界（本轮买不到的东西，逐条）

1. **`设计/40 步 9 ①`（变量对账）没装。** 它要 `npm i -D stylelint-value-no-unknown-custom-properties`，
   装包要改 `package-lock.json`，不在写区。而且**今天装上就红**：`S21` 步 2 现打剩 1 个未定义变量
   （`--bg-1`，6 处），正等用户在 `--bg` / `--field-bg` 之间拍板（`S21 §6 ①`）。
2. **格 ② 的"有人用"判准是"字面量出现过"，不是"真的挂上了 DOM"。**
   一个类名只要在 `src/` 任何字符串里出现过（哪怕报错文案）就算活。
   ⇒ **会漏掉一部分真死规则，但不会把活的判成死的** —— 方向是刻意选的：
   假阳性会指挥人删掉在用的样式，假阴性只是少清一点噪音。
3. **格 ③ 的 163 是混合人群**，如实说清：大部分是**纯 JS 钩子**
   （`.sftp-close` · `.pf-start` · `.panorama-back` 这一族，挂上去只为 `querySelector` 找得到），
   一部分是真悬空（§6 ②）。**机械上区分不了** —— 两者语法一模一样。
   ⇒ 这一格买的只有「这个数不许再涨」。
4. **格 ③④ 是棘轮不是等号，拦不住"判据自己空转"。** 理由是并发（另一路在改 `styles.css`，
   等号在并发下互相打架）。代价由每格的分母地板（等号式）补上 —— 见刀 4。
   **对比**：本仓给 `shellcheck` 那一格选的是等号（`gate.sh` 的 `gate_shellcheck`，
   人群与地板都从 `ci.yml` 现读、份数少于地板就红）。等号强在**双向**，这里是主动放弃了一个方向。
5. **只看 `src/`，不看 `tests/`。** 这是刻意的：`S21` 步 3 记过
   `tests/settings/panel-groups.vitest.ts:261` 有一条**正面断言**说 `.settings-group-empty` 不该存在 ——
   把 `tests/` 收进语料，死规则会靠"说它不该存在的那句话"把自己救活。
6. **不认 CSS-in-JS、不认 CSS Modules。** `设计/41` 件 10 真做起来这本账要跟着改。
7. **词法器的正则/除号判定是启发式**，不是 JS 真语法。失效的样子是词法错位 ⇒ 一批类名平白变死
   ⇒ **在判据那边表现为红，不是静默绿**。这条启发式失效时是吵闹的，可以接受。
8. **本轮跑的是 Linux。** 格 ④ 走 stylelint 的 **node API**（不经 `npx`、不过 shell），
   刻意躲开 `tests/eslint-baseline.vitest.ts` 头注记的那个
   「Windows 上 `npx` 真身是 `npx.cmd`、`execFile*` 不套 PATHEXT ⇒ 恒 ENOENT」的坑；
   路径比较统一走 `relative()` ＋ `split("\\").join("/")`，也是照那份头注的第二条。
   **但 Windows 上一次都没跑过** —— 属静态推出来的、未实测。

---

## 8. 同一棵树上的其他读数（现打）

```
npx vitest run tests/css-ledger.vitest.ts   → 13 passed                       ✅
npx vitest run（全量）                       → 133 文件 / 1747 passed / 0 failed ✅
npx tsc --noEmit                            → 干净                            ✅
npx eslint .                                → 7 errors（基线 7，未动）         ✅
npx stylelint "src/**/*.css"                → 47 errors（与本轮棘轮上限同数）   ✅
```

⚠ 本轮**没跑** `tests/scripts/gate.sh`（本轮明令不跑），也**没有 commit**。

## 9. 中间产物住址（都在 /tmp，不进仓）

```
$SCRATCH/s25/
  mirror/        整棵树的副本（node_modules 是符号链接），9 把刀全在这儿下的
  .orig-*.ts/.css/.yml   下刀前的原件，每刀之间用它还原
```
重建法：在仓根 `tar -c --exclude='src/bridge/target' src tests index.html package.json
.stylelintrc.json .github vitest.config.ts tsconfig.json eslint.config.js vite.config.ts | tar -x -C <目标>`，
再 `ln -s <仓根>/node_modules <目标>/node_modules`。
**镜像里跑判据不需要任何环境变量后门** —— `repo-root.ts` 的 `findRoot()` 向上找 `package.json`，
在镜像里自然解析到镜像根。
