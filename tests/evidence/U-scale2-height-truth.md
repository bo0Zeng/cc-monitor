# 秤 2 · 估高精度对拍（`设计/17 §6` 表第 2 行）—— 现打读数，2026-09-18

**一句话**：`设计/17 §5.3` 那句「**"估得准不准"今天零读数**」今天不成立了 ——
两个真引擎、83 张真卡片，逐卡的 `(class, 估值, 真值, 相对误差)` 在下面。
**结论不好看**：9 个卡型里 **5 个够不着**设计自己给的门槛「p90 相对误差 < 30%」，
其中最值钱的 `card-assistant`（正文卡）是 **系统性 ~2× 虚高**。

复算：

```bash
bash tests/evidence/U-scale2-run.sh                        # 真浏览器那一半（两个引擎各跑一遍 + 刷新金标准）
npx vitest run tests/scale2-height-truth.vitest.ts         # 门禁那一半（12 格，今天全绿）
npx vitest run tests/scale2-height-truth.vitest.ts --reporter=verbose --disable-console-intercept   # 连读数一起打
```

---

## 0. 真浏览器是怎么起来的

`设计/17 §6` 对秤 2 逐字要求「**必须真浏览器**（jsdom 无布局引擎）」。
路子**照抄 `真相源/90 §3.1/§3.7`**，没有重新发明；那边已经证明过这台机器上两个引擎都起得来。

| | 怎么起的 | 身份 |
|---|---|---|
| **Chromium 153.0.8010.12**（headless shell） | Playwright（`~/.cache/ms-playwright/`，G 路装的那份还在），`node /tmp/cv-reparent-probe/run_chromium.mjs` | Blink，**生产 WebView2 的同引擎家族** ⇒ 取它当金标准 |
| **WebKitGTK 2.52.6** | PyGObject + gi `WebKit2-4.1`，**必须 `xvfb-run` + `WEBKIT_DISABLE_COMPOSITING_MODE=1`**（否则 web process 起不来），`python3 /tmp/cv-reparent-probe/run_webkitgtk.py` | **Linux 侧 Tauri/wry 的同一引擎** ⇒ 取它当交叉核对 |

- 系统浏览器仍然一个都没有；`WebKitWebDriver` / `tauri-driver` 仍然没有 ⇒ **WebdriverIO 那条路今天仍然走不通**（`真相源/90 §3.1` 那条订正照样成立）。
- 被测页面 = `src/styles.css` 原样 + `src/cards/index.ts` 的 `renderMessage` 原样建出的卡，由 vite 打成一个 IIFE（`tests/evidence/U-scale2-vite.config.ts`，产物只落 `/tmp/scale2-height-truth/dist`，不进仓）。
- **没有动 `src/` 一个字**（含变异自检那一段，见 §6）。

### 环境读数（这段必须留 —— 真高是"这台机器这套字体"的真高）

```
UA(Chromium)  Mozilla/5.0 (X11; Linux x86_64) … HeadlessChrome/153.0.8010.12
UA(WebKitGTK) Mozilla/5.0 (X11; Ubuntu; Linux x86_64) … Version/60.5 Safari/605.1.15
视口 900×700 · dpr=1 · .stream-content 实测列宽 780px（= height-estimate.ts 的 COL_W，对得上）
CSS token: --font-size-prose 15px · --line-height-prose 1.65 · --font-size-xs 11px · --font-size-small 12px
```

🔴 **`--font-prose` / `--font-base` / `--font-mono` 里声明的字体，本机一个都没装**
（`fc-match "Source Serif 4"` / `"PingFang SC"` / `"Inter"` / `"JetBrains Mono"` **四个全部落到
`Noto Sans CJK SC`**）。⇒ 下面所有正文卡的真高是 **Noto Sans CJK 的真高**，
不是生产 Windows WebView2 上 Segoe UI / 微软雅黑的真高。**跨平台那一格今天没有**（见 §7 第 1 条）。

---

## 1. 语料

### 🔴 2026-09-18 改判：仓里**没有**真对话正文

初版逐字照着 `设计/17 §6` 当时那条「**不要造合成数据。用真机 `~/.claude/projects`**」做，
结果把 **74 条真实会话记录（57 544 字符正文 ＋ 本机绝对路径 ＋ 会话 id）** 冻进了
`tests/__fixtures__/`，而那 74 条**全部来自当时正在进行的那次会话本身**。
用户裁决逐字：「**这是测试啊 / 不应该进**」。

⇒ `设计/17 §6` 的数据源纪律已改判成「**结构照真的，内容一律合成**」，
本篇的语料随之重打。做法是**逐字符同形替换**（`U-scale2-sample-records.ts`）：

| 量 | 保不保 | 怎么保的 |
|---|---|---|
| 字符数 / 行数 / 每行长度 / 显示宽度 | **逐位相同** | 一个字符换一个字符；全角换全角 |
| 断词位置（决定折行）· markdown 结构 | **逐位相同** | 空格与标点原样留下 |
| 块的种类与数量 · CJK:ASCII 比例 | **逐位相同** | 按字符类映射，类不跨界 |
| **正文语义** | **全毁** | 字母/数字/汉字换成只跟位置有关的填充字符 |

**换语料前后 p90 几乎没动**（`card-assistant` 96.8/105.9 → 97.6/104.1 ·
`card-user` 10.7/14.3 → 10.7/14.3 · `card-tool-group` 9.8/11.8 → 9.8/11.8）——
这本身就是「同形替换不改排版」的实证：正文换成填充字符，估高与真高**一起**不动。

⚠ **残留的可识别信息只有「词长序列 ＋ 标点分布」**。折行必须靠词边界，词长抹了这杆秤就废了。
这是刻意付的代价，不藏。采样器自带两道自检，不过就抛、绝不落一份脏语料：
① **形状自检**（换完每一项结构量与原记录逐位相同）
② **泄漏自检**（每一段 ≥4 的字母串要么是填充轮产物、要么在那张打印得出来的保留词表里）。
逐字保留的只有 CLI 协议字面（`<bash-input>` 等）、工具名、模型 id、代码块语言标签 —— 全部会打印。

### 张数

| | 来源 | 张数 |
|---|---|---|
| **真形语料** | 结构采自 `~/.claude/projects/-home-zbl----claudecode-frontend/`、正文全合成的 **69 条**记录，按 `设计/17 §6` 秤 1 的字节桶（`<2K/2-8K/8-32K/32-128K/>128K`）**按预测卡型分层**均匀采样，冻进 `tests/__fixtures__/scale2-height-records.jsonl`（441 KB；7 条渲染成 skip 不建卡） | 62 |
| **手工构造** | `card-api-retry` ×5 · `card-api-error` ×3 · `card-bash-input` ×3 · `card-bash-output` ×6 · `card-slash` ×3 · `card-compact` ×1 | 21 |
| | | **83** |

⚠ **那 21 张为什么必须手工构造**：全部 8 个项目 **32 674 条**记录里
`system.subtype == "api_error"` **0 条**、bash 模式 `<bash-input>` **0 条**、`<command-name>`（slash）只有 **2 条**。
`设计/17 §6` 数据源纪律与 `真相源/90 §5.3` 都说过这件事，本轮复核一次，成立。
⇒ **这 5 个卡型的读数带着「构造体长得像不像真的」这个前提**，别当成真会话上的读数。

### 🔴 订正：初版那条「631 KB 的 max 尾记录」是虚的

初版写着语料「含一条 **631 KB** 的 max 尾记录」。**那是一颗截图的 base64。**
`renderResultContent` 对 image 块只吐一行 `[image image/png]`，从不展开 ——
**那颗 blob 对高度的贡献是 0**。

现打全量普查（8 148 条候选记录，去掉 image 块的 base64 之后）：

```
max 41K · 41K · 36K · 36K · 34K · 34K      p99 13.0K   p90 3.9K   超 128K 的：（无）
```

⇒ **`>128K` 那一桶根本没有人，`32-128K` 也只有 6 条。**
初版的长尾覆盖是**虚的**；这一版把 image 的 `data` 整个丢掉（只留 `media_type`，
它决定那一行的宽度），语料从 1.1 MB 降到 441 KB，而**长尾覆盖一点没少** ——
本来就没有那条尾巴。

### 读数会飘：治法从"跳过活文件"改成"冻前缀"

采样源里有一个**正在被写的 jsonl（当前这次会话自己的）**，每跑一次它都长大一点 ⇒
同一天两次采样，语料从 86 张变 87 张、`card-assistant` 的 p90 从 100.8% 变 96.9% ——
**读数在动而被测代码一行没改**。

治法：**活文件只读它的前 4 000 行**（追加写永远不改已写过的行 ⇒ 这个前缀是冻结的），
行数不够就整份跳过。
⚠ 中途试过「整份跳过活文件」，当场丢掉了最大的那一桶 —— 而设计点名要的就是 p99 与 max。
**"跳过活文件"本来是为了稳定，不是为了隐私**；隐私已经由同形替换解掉了，所以正确的做法是冻前缀。

采样器仍**默认不重跑**（`U-scale2-run.sh` 要 `--resample`）：语料一旦冻结就当金标准的一部分。

---

## 2. 主表 —— 各 class 的相对误差分布

口径：`|估值 − 真高| / 真高`。
- **估值** = 浏览器实际用的那个数：`Math.max(24, round(estimateStreamNodeHeight(el)))`；估不出高的按 CSS 兜底 `120`。
- **真高（content-box）** = `getBoundingClientRect().height − padding − border`。
  取 content-box 是因为 §3 实测 `contain-intrinsic-size` 就是 content-box。
- **真高（border-box）** 一栏比的是「估值**实际占掉的位置**（= 估值 + padding + border）vs 卡的真实外框」——
  这是它对 `scrollHeight` 的真实贡献。

### Chromium 153（Blink · 生产 WebView2 同引擎家族）← 金标准

| card class | n | 语料 | content-box p50 | p90 | max | border-box p50 | p90 | max | 方向 |
|---|--:|---|--:|--:|--:|--:|--:|--:|---|
| `card-api-error` | 3 | 构造 | 19.0% | **70.7%** | 83.6% | 13.9% | 65.1% | 77.9% | 全部**虚低** |
| `card-api-retry` | 5 | 构造 | 40.8% | **40.8%** | 40.8% | 30.2% | 30.2% | 30.2% | 全部虚高 |
| `card-assistant` | 28 | 真形 | 48.1% | **97.6%** | **103.9%** | 48.1% | 97.6% | 103.9% | 全部虚高 |
| `card-bash-input` | 3 | 构造 | 72.1% | **72.1%** | 72.1% | 43.8% | 43.8% | 43.8% | 全部虚高 |
| `card-bash-output` | 6 | 构造 | 0.6% | 8.6% | 15.2% | 0.5% | 8.2% | 14.8% | 双向 |
| `card-compact` | 2 | 真形+构造 | 9.8% | 9.8% | 9.8% | 9.3% | 9.3% | 9.3% | 全部虚高 |
| `card-slash` | 3 | 构造 | 82.9% | **82.9%** | 82.9% | 50.4% | 50.4% | 50.4% | 全部虚高 |
| `card-tool-group` | 23 | 真形 | 9.8% | 9.8% | 9.8% | 9.3% | 9.3% | 9.3% | 全部虚高 |
| `card-user` | 10 | 真形 | 0.5% | 10.7% | 10.7% | 0.3% | 5.3% | 5.3% | 双向 |

### WebKitGTK 2.52.6（Linux 侧 Tauri/wry 同一引擎）← 交叉核对

| card class | n | content-box p50/p90/max | border-box p50/p90/max |
|---|--:|---|---|
| `card-api-error` | 3 | 16.7% / 69.8% / 83.1% | 12.1% / 64.2% / 77.3% |
| `card-api-retry` | 5 | 41.2% / 41.2% / 41.2% | 30.4% / 30.4% / 30.4% |
| `card-assistant` | 28 | 51.2% / 104.1% / 106.7% | 同左 |
| `card-bash-input` | 3 | 68.4% / 68.4% / 68.4% | 41.9% / 41.9% / 41.9% |
| `card-bash-output` | 6 | 1.6% / 10.0% / 15.1% | 1.4% / 9.3% / 14.7% |
| `card-compact` | 2 | 11.8% / 11.8% / 11.8% | 11.1% / 11.1% / 11.1% |
| `card-slash` | 3 | 88.9% / 88.9% / 88.9% | 53.3% / 53.3% / 53.3% |
| `card-tool-group` | 23 | 11.8% / 11.8% / 11.8% | 11.1% / 11.1% / 11.1% |
| `card-user` | 10 | 3.2% / 14.3% / 14.3% | 2.5% / 7.0% / 7.0% |

**两引擎一致性**：83 张卡里 **4 张**真高差 >5%（门禁里那一格逐条核过，上限是 83/3 ≈ 27 张）。
⚠ 换成真形语料之后这个数从 0 变成 4 —— **不是语料坏了**：字体一换正文卡的真高就会变（§见"量不到什么"第 2 条），
两引擎的 fallback 字体不同，正文卡上差几个百分点是**真实存在**的。
初版那个 0 是旧语料碰巧，不是规律。所以那一格从来就是"点名 + 计数上限"，不是"必须零"。
**没有哪一条结论靠单引擎撑着**：9 个卡型的 p90 排序两引擎完全一致。

### 🔴 门禁判定

`设计/17 §6` 给秤 2 的门槛逐字：「**各 class 的 p90 相对误差 < 30%**」。

- **达标（4 个）**：`card-bash-output` · `card-compact` · `card-tool-group` · `card-user`
- **不达标（5 个）**：`card-api-error` · `card-api-retry` · `card-assistant` · `card-bash-input` · `card-slash`

这 5 个登记在 `tests/scale2-height-truth.vitest.ts::P90_CEILING` 的「超标段」＋
`EXCEEDS_DESIGN_GATE` 名单里。**登记不是豁免**：那个名单是 `toEqual` 的，
**变长变短都红** —— 多一个卡型退步要红，修好一个也要回来改这张表。

### 两条系统性成因（5 个超标里，4 个是同一个病）

**① 常数写的是 border-box，而 `contain-intrinsic-size` 吃的是 content-box。**

| class | 常数 | padding+border | content-box 真高 | 相对误差 |
|---|--:|--:|--:|--:|
| `card-api-retry` | 24 | 6 | **17.05** | +40.8% |
| `card-bash-input` | 32 | 12 | **18.60** | +72.1% |
| `card-slash` | 34 | 12 | **18.60** | +82.9% |

三条的差额分别是 6 / 12 / 12 —— **恰好各是一份 padding**。
源码注释自陈「不加 padding/border：`contain-intrinsic-size` 是 content-box」，
**那句话本身是对的**（§3 实测），**错的是常数**：24/32/34 都是按 border-box 手算的。
⇒ 把三条常数各减去一份 padding（24→17 / 32→19 / 34→19），三个 class 立刻进 30% 内。
⚠ **本轮不改**（U 路是秤不是修）；改完必须重跑 `U-scale2-run.sh` 刷金标准。

**② `card-assistant` 的 ~2× 虚高：`extractProseText` 把 markdown 产物里的排版换行当成了硬断行。**

12 张 assistant 卡的顶层块合计：

```
提取文本里的 \n          2726 个
DOM 里真的 <br>             0 个
BLOCK_TAGS 块边界         961 个
⇒ 剩下 1765 个 \n 没有任何 DOM 依据
```

来源是 `marked` 输出的 HTML 里**标签之间 / 段落内部的排版换行**（源码折行、表格每个 `<td>` 一行）。
浏览器把它们折叠成空格，而 `textHeight` 用 `whiteSpace: "pre-wrap"` 喂给 pretext ⇒ **每一个都算一次硬断行**。
表格最狠：一行 4 个单元格 → 被算成 4 行以上。最差的三张：

```
real#18（最差的一张）  估值 6604px   真高(content-box) 3239.6px   → +103.9%
  textLen 2933 · hardBreaks 182 · <br> 0 · BLOCK_TAGS 块 62
real#14               估值 6574px   真高 3239.6px   → +102.9%   （textLen 2933 · hardBreaks 182 · 块 62）
real#10               估值 4818px   真高 2402.9px   → +100.5%   （textLen 2659 · hardBreaks 144 · 块 42）
```

⚠ `real#14` 与 `real#18` 的 textLen / hardBreaks / 块数**逐位相同**，真高也相同 ——
它们本来就是真会话里两条近乎重复的记录（同形替换保长度，所以重复关系也原样保住了）。

**拆账证明锅不在别处**：折叠 `<details>` 那一支估得准（9.8%），纯常数卡也准。**误差全在正文那一条通道上。**
⚠ 代码块那一支（`codeBlockHeight`）的逐卡拆账是在**旧语料**上打的（real#1 221 vs 真 248、
real#2 462 vs 真 543，差 10–15%），**没有在新语料上重打** —— 卡 id 已经换了一批，
那两行数字只能当方向性参考，不许当现打读数引用。

> 这条是 `height-estimate.ts` 里 R1 注释那次修正**矫枉过正**的结果：
> 原来的 `textContent` 吞掉 `<br>`/块边界 ⇒ 8 倍低估；现在的提取器保留了**所有** `\n`
> 并按 `pre-wrap` 当硬断行 ⇒ 2 倍虚高。两头都不对。
> **修法方向**（本轮不改，只登记）：块边界与 `<br>` 插 `\n`，**其余位置的换行归一成空格**。

⚠ **降级路与主路几乎一样**：jsdom 里 pretext 必然失效走 `fallbackTextHeight`（0.52em 均宽算术），
它给出的 `card-assistant` p90 是 **99.8%**（p50 48.8% / max 104.8%），与真浏览器里 pretext 的 97.6% 只差 2 个点
⇒ **这个 2× 不是 pretext 的锅**，换掉度量库救不了它。

---

## 3. 悬案① ——「`contain-intrinsic-size` 是 content-box」到底成不成立

**做法**：一张真的 `card-api-retry`（padding `3px 12px`）放在 `top: 500000px`、**从没渲染过**的位置，
inline 写 `contain-intrinsic-size: auto N px`，读它的 `getBoundingClientRect().height`。
读到 `N` ⇒ border-box；读到 `N + padding + border` ⇒ content-box。**二选一，没有第三种。**

| 声明 | padding+border | 实测高度 | 判定 |
|---|--:|--:|---|
| `auto 24px`（`card-api-retry` 今天的常数） | 6 | **30** | ⇒ **content-box** |
| `auto 100px`（拉开差距好判读） | 6 | **106** | ⇒ **content-box** |
| 不写（落 CSS 兜底 `auto 120px`） | 6 | **126** | ⇒ **content-box** |
| `auto 32px`（`card-bash-input`） | 12 | **44** | ⇒ **content-box** |
| `auto 40px`（`card-api-error`） | 18 | **58** | ⇒ **content-box** |

**两个引擎读数完全一致。**

### 答案

> **`height-estimate.ts` 那句「不加 padding/border：`contain-intrinsic-size` 是 content-box」——语义层面是对的。
> 不自洽的是常数：24 / 32 / 34 / 40 全是按 border-box 手算的，写进一个吃 content-box 的属性里。**
> A 路登记的那条「两者不自洽」**成立，而且方向定下来了**：
> **不是注释错，是常数多了一份 padding。**

**`card-api-retry` 的真高到底是多少**（本轮直接问的那一问）：

| | 值 | 说明 |
|---|--:|---|
| border-box 真高 | **23.047px**（Chromium）/ **23.0px**（WebKitGTK） | R 路手算的 23.05 ✅ **对的** |
| content-box 真高 | **17.047px** | = `font-size 11px × line-height 1.55`（computed 实测 `17.05px`） |
| padding+border | **6px** | `padding: 3px 12px`，无上下 border |
| 常数 24 落地后**实际占的位置** | **30px** | ⇒ 对真高 23.05 是 **+30.2% 虚高**，不是手算推的 +4% |

---

## 4. 悬案② —— `applyIntrinsicSize` 的 `Math.max(24, …)` 地板

**读数**：83 张卡里只有 **2 张**被地板顶起来（`real#30` / `real#37`），都是 `card-user`，估值 21.7 → 写成 24。

**答案**：

> **`card-api-retry → 24` 这条常数，在"写进 style 的数字"这个层面上，
> 与地板值撞在一起完全是巧合 —— 而且它确实改变了行为。**
>
> 拆清楚三件事：
> 1. **地板碰不到它**。`Math.max(24, 24) === 24`：地板只对估值 **< 24** 的卡生效，
>    而 `card-api-retry`(24) / `card-bash-input`(32) / `card-slash`(34) / `card-api-error`(40) 四个常数都 ≥24。
>    ⇒ 说「常数 = 地板值 ⇒ 等于没改」是**把两条路混了**。
> 2. **它真正改掉的是「写不写 inline style」**。改之前 `card-api-retry` 走 `return null` ⇒
>    **不写 inline style** ⇒ 由 `styles.css` 的 `contain-intrinsic-size: auto 120px` 接管；
>    改之后写 `auto 24px`。**24 vs 120，差 5 倍**，这才是那条常数的效果。
>    实测：视口外这张卡实际占的位置从 **126px** 降到 **30px**。
> 3. **但它没有把误差修到位**。30px vs 真高 23.05px ⇒ 仍然 **+30%**（见 §3），
>    而按 content-box 语义本该写 **17**。

（变异自检 M2 把那一行删掉 ⇒ 落回 120 ⇒ `card-api-retry` 的 p90 从 40.8% 跳到 **603.8%**，门禁红。
这就是"改了常数确实改变了行为"的机检版本。）

---

## 5. 悬案③ —— R 路那个「n=17 是洞、n=34 仍红」还成不成立

R 路自陈：三段边界表**建在「真高 = 手算 23.05px、估值 = 24px」这个前提上**。秤 2 把两个数都量了：

| 量 | 手算前提 | 秤 2 实测 | |
|---|--:|--:|---|
| 一张 `card-api-retry` 的真高（border-box） | 23.05 | **23.047** | ✅ 手算对的 |
| 它在视口外对 `scrollHeight` 的贡献 | 24 | **30** | ❌ 差一份 padding |

把 24 换成 30，重跑同一套门控算术（`clientHeight=800` · `round < 4` · 每轮 150 条，
口径与 R 路逐字对齐：**两边都不算 margin**）：

| | 红点（不足一屏） |
|---|---|
| R 路的手算前提（估值 24 / 真高 23.05） | `1–8` ∪ **{17}** ∪ **{34}** |
| **秤 2 实测（估值 30 / 真高 23.047）** | `1–11` ∪ **`[14,17]`** ∪ **`[27,34]`** |

### 答案

> **① 「n=17 是洞」仍然成立 —— 但它不再是孤点，变成 `[14,17]` 一整条带。**
> **② 新长出一条 `[27,34]` 红带**：估值虚高 30% ⇒ 27 张卡就被判「满了」，而 27×23 = 622 < 800。
> **③ 最低那一段从 `n ≤ 8` 扩到 `n ≤ 11`。**
> **④ 🔴 `设计/17 §6` 那句「取 **18–30** 条/150 最稳」在实测值下是错的 —— 27–30 全红。**
> **实测下整段都成立的密度是 `18–26`**（或 `12–13`，或 `≥35`）。

⚠ 敏感性：把 `.card-api-retry` 的 `margin: 4px 0` 算进去（估值侧真高侧同时加），
红带整体左移到 `1–9` ∪ `[12,14]` ∪ `[24,29]` —— **上面四条结论的方向一条都不变**。

这段是**机检**的，不是散文：`tests/scale2-height-truth.vitest.ts` 最后那个 describe 现算，
`card-api-retry` 的真高与「auto 24px 实际占多少」都从金标准里取，估值常数一动它就跟着动。

---

## 6. 变异自检 —— 这杆秤在"估错了"时真的会红吗

**做法**：把整棵 `src/` + `tests/` `cp -a` 到 `/tmp/scale2-mutation/`（`node_modules` 软链），
**在副本里**改常数、跑门禁、还原。**不在真仓改** —— 本轮六路并发，真仓 `src/` 有别的路在动，
两秒钟的窗口也不该开。脚本最后一格现打 `git status --porcelain src/height-estimate.ts`，
⇒ **真仓的 `src/height-estimate.ts` 全程干净，一个字没改。**

复算：`bash tests/evidence/U-scale2-mutation.sh`
原文（全文在 `tests/evidence/U-scale2-mutation-log.txt`，本脚本覆盖重写）：

```
######## M0 · 基线（未变异）                              Tests  12 passed (12)

######## M1 · card-api-retry 常数 24 → 120                Tests  2 failed | 10 passed
  × ★ 现算路：每个 class 都在登记上限内
      card-api-retry：p90=603.8% > 上限 45%（n=5）
  × ★ 常数型卡：jsdom 现算的估值必须与真浏览器逐位相同
      syn#0（card-api-retry）现算 120 ≠ 金标准 24   （×5）

######## M2 · 删掉 card-api-retry 那一行（退回 return null ⇒ CSS 120 兜底）
  × 同 M1 两格                                            Tests  2 failed | 10 passed

######## M3 · SUMMARY_H 38 → 52                           Tests  2 failed | 10 passed
      card-compact：p90=50.3% > 上限 30%（n=2）
      card-tool-group：p90=50.3% > 上限 30%（n=23）
      real#29（card-compact）现算 52 ≠ 金标准 38 · real#40（card-tool-group）同

######## M4 · LH_PROSE ×1.65 → ×1.20（往**真值方向**改）  Tests  12 passed (12)   ← 见下面「它抓不住什么」

######## M5 · COL_W 780 → 390                             Tests  1 failed | 11 passed
      card-assistant：p90=133.1% > 上限 120%（n=28）
      card-user：p90=54.9% > 上限 30%（n=10）

######## M6 · LH_PROSE ×1.65 → ×2.40                      Tests  1 failed | 11 passed
      card-assistant：p90=180.0% > 上限 120%（n=28）

######## M7 · BASH_OUTPUT_MAX_LINES 20 → 60               Tests  1 failed | 11 passed
      syn#14（card-bash-output）现算 529 ≠ 金标准 428

######## M8 · card-slash 34 → 19（往真值方向改一个常数）  Tests  1 failed | 11 passed
      syn#17/18/19（card-slash）现算 24 ≠ 金标准 34   （19 被 Math.max(24,…) 顶成 24）

######## M9 · BASH_OUTPUT_HEADER_H 19 → 90                Tests  2 failed | 10 passed
      card-bash-output：p90=115.8% > 上限 30%（n=6）
      syn#11 现算 113 ≠ 金标准 42 · syn#12 现算 197 ≠ 126 · syn#13/14 现算 499 ≠ 428
                                                          （上表为换语料后重跑的原文）

######## 全部还原后复跑                                   Tests  12 passed (12)
```

**9 个变异里 8 个当场红。**

### ⚠ 它抓不住什么（这段不完整本身就是缺陷）

1. **M4 绿是真的绿**：`LH_PROSE ×1.65 → ×1.20` 把正文估值**拉向真值**
   （因为今天正文是 2× 虚高，砍行高刚好抵消一部分）⇒ 精度门禁没有理由红。
   **代价**：只要 `card-assistant` 还挂着 1.2 的上限，任何"碰巧抵消"的改动都能悄悄进来。
   ⇒ **§2 那条 ~2× 虚高修掉、上限拧回 0.3 之前，正文路的变异灵敏度是打折的。**
2. **M7 第一次跑是绿的**，因为当时语料里**没有 21–30 行的 bash 输出**
   （`buildOutputPre` 超 30 行只留头 20 行 ⇒ `min(行数, 20)` 那个常数不可达）。
   补了一条 25 行的构造体之后才红。⇒ **常数可达性要靠语料保证，光有门禁不够。**
   这条也顺带量出一个真问题：25 行那张卡的估值被 `min(…, 20)` 压到 20 行，**低估 15.2%**。
3. **正文路（pretext）的常数改动，门禁只能从"精度变差"这一侧抓**。
   金标准里存的 `estBrowser` 是探针跑那一刻算的，改常数不会让它变 ——
   所以它只用来出主路的读数，不当灵敏度来源。**灵敏度全在「现算路」那两格上。**

---

## 7. 我判不了的

1. **生产 Windows WebView2 上的真高。** 本篇全部真高是这台 Linux 无头机 + **Noto Sans CJK 替身字体**下的。
   `Source Serif 4` / `PingFang SC` / `Microsoft YaHei` / `Inter` / `JetBrains Mono` 一个都没装。
   字体一换，`card-user`/`card-assistant` 的真高一定变（常数型细条卡不变 —— 它们只吃 font-size × line-height）。
   **怎么才能知道**：在一台装了那几个字体的机器（或把字体打进探针页面）上重跑 `U-scale2-run.sh`，比两份金标准。
2. **`设计/17 §5` 第 2 条「一次强制布局读多少钱」。** 本篇一次都没测，也**不给它一个数**。
   它在「分不清」里挂着，本篇不动它。
3. **多条 tool-group 合并成一张外壳那一档。** 本篇的口径是**一条记录一张外壳**；
   生产里 TabManager 会把连续多条合并进同一张折叠卡。折叠态 summary 高度与 units 数量无关
   （`estimateStreamNodeHeight` 的第一支就是这么写的），所以**估计影响不大，但没量过**。
4. **`card-api-retry` / `card-api-error` / `card-bash-input` / `card-bash-output` / `card-slash` 的构造体像不像真的。**
   真语料里 0 条（§1）。文案长度、是否多行、stderr 有没有 —— 全是假设。
   `card-api-error` 那 3 张尤其：**虚低 83.6% 那一张是我自己编的一段长报错**，
   真实的 API 报错文案有多长，没有样本。
5. **pretext 0.0.8 → 0.0.9 那一跳的影响。** 本篇只量了 0.0.9（`package.json` 钉死的那个版本）。
   源码头注要求的「解钉升级须重跑估值精度对照」现在**有工具了**（`U-scale2-run.sh`），
   但**没有 0.0.8 的对照读数**，所以那一跳到底动没动 layout 行为，今天仍然判不了。
6. **`MEASURE_PREFIX_CHARS = 2400` 前缀外推的误差。** 语料里最大那条记录是 **41 KB**
   （去掉 image 的 base64 之后的真实上限，见 §1 的订正），而它渲染成的是 `card-tool-group`
   （折叠态，估值走 `SUMMARY_H` 常数，根本不走文本路）。
   ⇒ **超长正文卡这一档没采到样本**，`设计/17 §5.3` 那条「183× 线性放大（无界、双向）」
   **今天仍然零读数**。
   ⚠ 而且这一档**采不到** —— 真机上就没有那么长的正文记录（p99 13.0 KB / max 41 KB）。
   要量它只能**专门构造**一条超长 `assistant` 正文记录，那就回到"构造体像不像真的"那个前提里了。

---

## 8. 产物清单

| 文件 | 是什么 |
|---|---|
| `tests/scale2-height-truth.vitest.ts` | 门禁 + 读数（12 格，jsdom 里跑；**估值现算、真高读金标准**） |
| `tests/scale2-height-corpus.ts` | 语料构造器（真浏览器侧与门禁侧**共用**，否则两边不是同一张卡） |
| `tests/__fixtures__/scale2-height-records.jsonl` | 冻结的**真形**语料（69 条，441 KB；结构真、正文全合成，见 §1） |
| `tests/evidence/U-scale2-sample-records.ts` | 采样器（`npx tsx …`，默认不跑，见 §1） |
| `tests/evidence/U-scale2-probe-entry.ts` | 真浏览器探针（A/A′/B/C 四段） |
| `tests/evidence/U-scale2-vite.config.ts` | 探针打包配置（产物只落 /tmp） |
| `tests/evidence/U-scale2-report.ts` | 出表器 + 金标准刷新器 |
| `tests/evidence/U-scale2-run.sh` | 一键复算（②打包 → ③Chromium → ④WebKitGTK → ⑤出表） |
| `tests/evidence/U-scale2-mutation.sh` | 变异自检（在 `/tmp` 副本里改常数，真仓 `src/` 不碰） |
| `tests/evidence/U-scale2-mutation-log.txt` | 变异自检的原文读数 |
| `tests/evidence/U-scale2-truth-golden.json` | **真高金标准**（83 行 × 两引擎） |
| `tests/evidence/U-scale2-corpus-shape.json` | 语料的**形状指纹**（原记录 vs 落盘逐项相同）＋ 逐字保留词表 |
| `tests/evidence/U-scale2-height-truth.md` | 本文件 |

`src/` **一个字没改**。
