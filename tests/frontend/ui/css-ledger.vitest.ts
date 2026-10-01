/**
 * `S25` —— **把 CSS 的对账接进判据**（件 8）。
 *
 * ## 这条为什么存在
 *
 * 「**这 7248 行里没有任何一条机器检查。** 而这个仓有 44,166 行护栏代码
 * 专门做各种对账 —— **唯一没有对账的地方，恰好是 bug 最密的地方。**」
 * 四类问题全是现打可复现的：未定义变量 16 个、死规则 13 个、z-index 18 个量级无刻度、
 * 布局根没人对账。那个 bug 与那个浮层压按钮，都是从这儿长出来的。
 *
 * 本文件装的是**这一面的第一批机检**。量具（遍历 ＋ 词法 ＋ 两个方向的账）住
 * `tests/evidence/S25-class-ledger.ts`，**本文件只登记与判**。分工的理由写在那份文件的头注里
 * （一句话：`scanning-guard-registry.vitest.ts` 的 `WALKER_CEILING` 不许测试文件再多一个遍历者）。
 *
 * ## 装了四格，另有一格**明确没装**
 *
 * | 格 | 判什么 | 形态 | 分母 |
 * |---|---|---|---|
 * | ① | `z-index` 只许写 `var(--z-*)` | **恒等**（一处裸数字都不许） | 判过的 `z-index` 声明条数 |
 * | ② | CSS 里的类名有人用（CSS → 代码） | **恒等**（未解释集合 == 登记的已知死规则） | 判过的 CSS 类名个数 |
 * | ③ | 代码挂的类名 CSS 里有规则（代码 → CSS） | **递减棘轮** | 判过的代码侧类名引用个数 |
 * | ④ | `npx stylelint` 的报错总数（`no-descending-specificity` 除外） | **递减棘轮** | 被 lint 的 CSS 文件份数 |
 * | ④b | `no-descending-specificity` 的命中 == 登记的例外 | **两向相等**（多一条红、修掉一条也红） | 登记表条数 |
 *
 * 🔴 **本文件不装第五格（`#app` 布局根）—— 它有自己的家。**
 * 本轮落地时它红在一个真缺陷上（`src/frontend/ui/tabs.ts` 的 `ensureArchiveUi()` 往 `#app` 插的
 * `.tab-archive` 是个没认领格子的 grid item，偷走消息流 35px，`S21 §6 ④` 有真引擎读数），
 * 而 `src/frontend/ui/tabs.ts` / `src/frontend/ui/styles.css` 都不在本轮写区。
 * **同一棵树上另一路（`S24`）把缺陷修了、并把那一格装成了 `tests/frontend/ui/app-grid-claims.vitest.ts`** ——
 * 而且形态比字面写法更对：钉的不是「子元素 == 那三个」（`.tab-archive` 在步 17
 * 之前还得在，钉集合会持续假红），是「**每个 in-flow 直接子元素都认领了一个声明过的具名区域**」。
 * ⇒ 本文件**不复制那一格**（同一个性质两个住址必漂）。这里只登记它在哪。
 *
 * ## 🔴 ③④ 为什么选棘轮而不是等号（这是有意选的，不是偷懒）
 *
 * 本仓给 `shellcheck` 那一格选的是**等号**（`tests/scripts/gate.sh` 的 `gate_shellcheck`：
 * 人群与地板都从 `ci.yml` 现读，份数少于地板就红）。等号强在**双向**：值往上漂会红，
 * 往下漂（判据自己空转、人群缩水）**也会红**。棘轮只拦一个方向 ——
 * 「扫到 0 处」在棘轮下与「全都干净」长得一模一样，这正是本仓反复吃亏的那一族。
 *
 * 这里仍然选棘轮，理由是**并发**：本条落地的同时另有一路在改 `src/frontend/ui/styles.css`
 * （修 `.tab-archive` 那个真缺陷）。等号在并发下是**互相打架**的形状 ——
 * 对方每改一行都可能让我这边红在一个与缺陷无关的数上，而那种红没有信息量。
 * 棘轮对并发是稳的：对方只要不把账做坏，数就只会不变或变小。
 *
 * ⇒ **代价如实记：③④ 拦不住「判据自己空转」**。补偿是每一格都单独配了一条
 * **反空真自检**（分母地板，见 `FLOORS`），那条是等号式的：分母掉下去当场红。
 * 也就是说这里不是"棘轮代替等号"，是"棘轮管方向 ＋ 等号管分母"，两条腿。
 *
 * ## 🔴 ② 的三个已知假阳性 —— 这是本条的正控
 *
 * 上一轮（`S21` 步 3）现打：普查给出的 13 个"死规则"里 **3 个是活的**，三个各属一族。
 * 一把认不出它们的尺子会指挥人去删**正在生效**的样式 ⇒ 比没有尺子更坏。
 * 所以本文件把这三个钉成正控，**并且连"靠哪条机制活下来"一起钉**：
 * 只钉"没被判死"是不够的 —— 那条断言在「尺子把所有东西都判活」时同样绿。
 *
 * | 名字 | 它凭什么活着 | 本条要求的裁决 |
 * |---|---|---|
 * | `.status-first-run-open` | `` `${FIRST_RUN_HINT_CLASS}-open` `` 拼出来 | `const-concat` |
 * | `.status-first-run-dismiss` | 同上 | `const-concat` |
 * | `.katex-display` | `katex/dist/katex.min.css` 自产 | `vendor` |
 *
 * ## 诚实边界
 *
 * - 量具买不到的东西逐条写在 `tests/evidence/S25-class-ledger.ts` 的头注里，这里不复述。
 * - **（变量对账）没装**：它要 `npm i -D stylelint-value-no-unknown-custom-properties`，
 *   而装包要改 `package-lock.json`，不在本轮写区。而且它今天**装上就红** ——
 *   `S21` 步 2 现打剩 1 个未定义变量（`--bg-1`，6 处），那一处正等用户在
 *   `--bg` / `--field-bg` 之间拍板（`S21 §6 ①`）。⇒ 不装，登记在此。
 * - ④ 跑的是**真 stylelint**（本仓少数几条在判据里起外部工具的，先例：`eslint-baseline.vitest.ts`）。
 *   走 node API 而不是 spawn `npx`，顺带躲开 `eslint-baseline` 头注记的那个
 *   「Windows 上 `npx` 真身是 `npx.cmd`、`execFile*` 不套 PATHEXT ⇒ 恒 ENOENT」的坑。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import stylelint from "stylelint";
import { describe, it, expect, vi } from "vitest";
import {
  buildLedger,
  cssClassesOf,
  explain,
  scanStrings,
  stripCodeComments,
  zIndexDeclsOf,
  type Ledger,
} from "../../evidence/S25-class-ledger.ts";
import { REPO_ROOT } from "../../test-support/repo-root.ts";

const TIMEOUT_MS = 120_000;

/** `z-index` 唯一准写的形状。与 `.stylelintrc.json` 里那条 allowed-list 是同一个意思。 */
const Z_OK = /^var\(--z-[a-z0-9-]+\)$/;

/**
 * 🔴 **反空真的地板**。每一格都要能说出「我判过几条」，而不是只说「过了」。
 *
 * 这些数是**地板不是快照**：现打值写在括号里，地板压在它下面留出改动余量。
 * 掉到地板以下 ⇒ 判据的人群缩水了（遍历坏了 / 词法错位 / 文件搬家），当场红。
 * ⚠ 别把地板往上抬成快照 —— 那会让每一次正常改动都红在一个与被守性质无关的数上。
 */
const FLOORS = {
  /**
   * `src` 下的 CSS 文件份数。〔三入口拆分 · 人群改定义〕原先现打 2（`styles.css` ＋ `styles/tokens.css`），
   * 按窗口 / 按层切开之后现打 **10**（`styles.css` ＋ `styles/` 下 9 份：layers · reset · tokens · layout ·
   * shared · settings-shared · main · settings · viewer）。人群的**定义**没变（仍是 `src/**\/*.css` 全体，
   * 与 `npm run lint:css` 的 glob 同一批，由格 ④ 对拍），变的是份数 ⇒ 地板跟着抬到 8：
   * 留在 2 的话，丢掉 8 份文件这条也不会叫。「每份都被某个窗口的 html 链到」住 `tests/frontend/ui/entry-graphs.vitest.ts`。
   * 同一拍现打：类名 780 · 代码侧引用 858 · z-index 39 · 悬空 162 —— 与拆之前**逐项相等**（拆文件只搬家，不改账）。
   */
  cssFiles: 8,
  /** CSS 选择器里的类名个数（现打 777）。 */
  cssClasses: 600,
  /** `z-index` 声明条数（现打 39）。 */
  zIndexDecls: 30,
  /** 代码侧扫出来的类名形 token 个数（现打 2438）。 */
  literals: 1500,
  /** 第三方 CSS 自产的类名个数（现打 184，来自 katex ＋ highlight.js）。 */
  vendorClasses: 80,
  /** 常量拼接才冒出来的 token 个数（现打 2，正是那两个 `.status-first-run-*`）。 */
  constConcat: 1,
  /** 模板拼接派生出的前缀候选个数（现打 30）。 */
  prefixCandidates: 10,
  /** 代码里确实当类名用的引用个数（现打 859）。 */
  usedClasses: 600,
  /** 被扫的代码文件份数（现打 205）。 */
  codeFiles: 150,
} as const;

/**
 * ★ **准拿来解释类名的模板前缀**（那份白名单的落点）。
 *
 * 🔴 **为什么这张表是手写的，而候选是机器派生的**：
 * 量具会把**每一个**「模板里紧挨着 `${}` 的尾 token」都记成候选（现打 30 个）。
 * 里面有垃圾 —— `src/frontend/ui/tab-collections.ts:46` 的 `` `c${Date.now().toString(36)}…` ``
 * 派生出一个 1 字符前缀 `c`，它能一口气"解释"掉 `code-bar` / `code-lang` / `code-copy` /
 * `ccm-alias-gen*` 共 7 个类，**而那 7 个其实各有真住址**（`src/frontend/ui/render.ts:77` 等）。
 * 一个越宽的前缀能掩掉越多真死规则 ⇒ **白名单必须是人写的，且每条要有理由**。
 *
 * 🔴 **与上一轮那份「11 个前缀」清单的关系（这是本轮最要紧的一条读数）**：
 * 登记了 11 个前缀，`S21` 三处说「今天的 11 个不够，至少还要加
 * `FIRST_RUN_HINT_CLASS` 这类常量拼接、第三方 `katex-*`，以及现打到的 8 处模板拼接点
 * （`settings-btn-` · `cc-bus-hooks-` · `cc-bus-online-` · `remote-test-` · `history-chip` ·
 * `bash-output-body` · `block-collapsible` · `paste-block`）」。
 *
 * **本轮现打的答案不是"补到 19 个"，是"那三族根本不该由前缀来管"**：
 * - 常量拼接 → 归 `const-concat`（把 `${CONST}` 填回去再扫），不占前缀格；
 * - 第三方 → 归 `vendor`（从 `import "<包>/….css"` 现读），不占前缀格；
 * - 那 8 处模板拼接点里有 7 处**根本不需要登记** —— 它们拼出来的类名（`settings-btn-secondary`、
 *   `cc-bus-hooks-ok`、`history-chip`…）在别处有**直接字面量**住址，`literal` 那一档先接住了。
 *
 * ⇒ 真正需要靠前缀才解释得通的，现打就是下面这 **10** 条（比那份 11 个清单还少一条：
 * `paste-block` 不在这里，当时的理由见 `KNOWN_DEAD` —— 那条死规则已删）。**每条都带住址与理由，缺一条本表就不该有它。**
 * 〔10 → 7〕`conf-` · `kind-` · `remote-gap-` 三族摘了：它们是**有限枚举状态拼成的类名族**，
 *   按约定改走 `data-conf` / `data-kind`（`css-conventions` ⑧「同一个状态名不许同现于两种载体」逮到的，
 *   `remote-gap-` 那一族同一个值还同时写进了 `data-kind`）⇒ 拼接点与 CSS 类都没了，前缀随之是死条目。
 *
 * ⚠ 本表有两条自检（见「登记表不许有死条目」那一格）：
 * ① 每条前缀今天仍要在代码里派生得出来（候选表里有）；
 * ② 每条前缀今天至少要真解释掉一个 CSS 类 —— 解释不到任何东西的前缀是死条目，
 *    留着只会在下一次悄悄掩掉一个真死规则。
 */
const ALLOWED_PREFIXES: readonly { prefix: string; why: string }[] = [
  {
    prefix: "acct-c",
    why: "`src/frontend/ui/account-color.ts:38` 按账号序号生成 8 个配色位（`acct-c0`…`acct-c7`）。序号是运行时算的，写不出字面量。",
  },
  {
    prefix: "agent-",
    why: "`src/frontend/ui/agents-panel.ts:139` 把 agent 的运行态直接拼成类名（running/done/aborted）。态值来自后端。",
  },
  {
    prefix: "block-diff-",
    why: "`src/frontend/ui/cards/diff.ts:311` 按 diff 行的增删拼 `block-diff-add` / `-del`。",
  },
  {
    prefix: "pf-dot-",
    why: "`src/frontend/ui/views/port-forward.ts:142` 按端口转发的健康状态拼（ok/err）。",
  },
  {
    prefix: "remote-status-",
    why: "`src/frontend/ui/settings/remote-section.ts:163` 按远端探测结果拼（ok/fail/na/unknown）。",
  },
  {
    prefix: "status-",
    why: "`src/frontend/ui/tasks-panel.ts:177` 把任务态拼成类名（running/aborted/completed/in_progress/deleted）。态值直接来自会话记录，是**协议里的字符串**，前端不重新枚举。",
  },
  {
    prefix: "tone-",
    why: "`src/frontend/ui/settings/config-surface-section.ts:333` 按配置面的判定色调拼（ok/bad/unknown）。",
  },
];

/**
 * ★ **已登记的死规则** —— CSS 里写着、今天确实没有任何人挂它。
 *
 * 登记在这儿不是赦免，是**把它变成一条会红的欠账**：修掉（删那几行 CSS）之后
 * 这张表里的条目会变成死条目，下面那条自检会红并叫人来删登记。
 *
 * ⚠ 〔当时〕本轮**不能自己修** —— `src/frontend/ui/styles.css` 在明令禁碰之列（另有一路正在改它）。
 */
// 〔D §D8〕唯一那条 `.paste-block-warning` 删了（规则在 `src/frontend/ui/styles/settings.css`，
//   它的 `warning` 槽早先已移回消费者那儿，`src/frontend/ui/paste-block.ts` 头注写着）⇒ 规则与登记同拍删，本表空。
//   空的是人群不是判据：② 那一格仍是**恒等**（未解释的类名 == 本表），再冒出一条死规则当场红。
const KNOWN_DEAD: readonly { name: string; why: string }[] = [];

/**
 * ★ **`npx stylelint "src/**\/*.css"` 的报错总数上限**。现打 **47**（`S21 §5` 同数）。
 *
 * **只许降。** 47 这个数本身不是目标（其中 43 条 `--fix` 就能自动改），
 * 本格买的是「它不许悄悄变大」—— 在这之前这个数**全仓无人机检**：
 * `tests/frontend/ui/eslint-baseline.vitest.ts` 的头注逐字登记过「本条不管 stylelint，登记在此，不假装覆盖了」，
 * 而 `ci.yml` 那一步是 `npm run lint:css || true`（结构上不会红）。本格接的就是那半格。
 */
// 〔2026-09-24 U1 合并那一拍棘 47 → 39〕三入口 ＋ CSS 拆 10 份 ＋ 层真包进去之后现打 39（现打，不是 47−8 算的）。
// 〔F7b 09-24 棘 39 → 36〕老 SFTP 面板那整段 CSS 退役，带走 `shared.css` 里 3 条 `color-function-alias-notation`
//   （`rgba` → `rgb`；那一份 7 → 4，现打，不是 39−3 算的：全仓 `npx stylelint` 现打 36）。
// `no-descending-specificity` 打开（待拍 2）。它的命中**不进**本棘轮 ——
//   另由 ④b 的两向相等登记表管（比棘轮严）；本上限仍是「除它以外」的报错总数，数值不动（现打 36）。
// 〔棘 36 → 35〕`styles.css` 的 `.status-tasks` 里那条被 `font: inherit` 整条盖掉的 `line-height: 16px` 删了
//   （`css-conventions` ⑨「活规则里的死声明」逮到的；stylelint 的 `declaration-block-no-shorthand-property-overrides` 正是它，现打 35）。
const STYLELINT_CEILING = 35;

/**
 * ★ **`no-descending-specificity` 的例外登记表**（④b · 待拍 2 · 「特异度冲突」）。
 *
 * 这条 lint 抓的是「后写的选择器特异度更低」：读源码的人以为后者覆盖前者，实际前者赢。
 * `@layer` 落地之后它终于可以打开（「`@layer` 本来要买的正是这条 lint 可以打开」）。
 * 打开时现打 **39** 条：写区内能**证明零级联变化**的 6 条当拍修掉（纯挪位，逐对核过先后翻转的规则，
 * 读数在），余下 **33** 条逐条登记在这里。
 *
 * 形态是**两向相等**（命中的多重集 == 本表）：
 * - 新写出一条降序特异度 ⇒ 红（要么改写法，要么登记并写理由 —— 审的人看得见）；
 * - 修掉一条（或那条规则被改写 / 删掉）⇒ 本表出现死条目 ⇒ 红，删掉那一条。
 * 键是「文件 ＋ 后写的选择器 ＋ 先写的选择器」（**不含行号** —— 行号随别处增删漂移，钉它只会持续假红）。
 * `n` ＝ 同一对在同一文件里出现几次（缺省 1）。
 *
 * `kind`：
 * - `list`：同一条规则的选择器列表内部（声明一模一样，谁赢都一样）；
 * - `harmless`：逐条看过，结构上打不到同一个元素，或打到了也是作者要的结果（理由写在 `why`）；
 * - `defect?`：**真冲突，缺陷候选**，交主会话拍（改了会改变可见样式，本机无图形会话不能目视）；
 * - 〔历史〕`U4` / `ST3` 两档：当时在同波别的路的写区里、没逐条判的「未判」。U4 那 10 条 U4 自己判了，
 *   ST3 那 15 条逐条判完（全是 `harmless`），这两档从类型里删了 —— 今天没有「未判」。
 *
 * ⚠ 并发：别的路改到这些规则时，这张表会在合并那一拍红（多了或少了条目）——
 *   那是本条要的红：按现打把对应条目增删并写理由即可。
 */
const DESCENDING_SPECIFICITY_EXCEPTIONS: readonly {
  file: string;
  later: string;
  earlier: string;
  n?: number;
  kind: "list" | "harmless" | "defect?";
  why: string;
}[] = [
  // ── src/frontend/ui/styles.css：本路判过的 7 条 ──
  {
    file: "src/frontend/ui/styles.css",
    later: ".viewer-branch-btn:focus-visible",
    earlier: ".has-branch-btn:hover .viewer-branch-btn",
    kind: "list",
    why: "两者是同一条规则 `…, .viewer-branch-btn:focus-visible { opacity: 0.7 }` 的选择器列表，声明相同。",
  },
  {
    file: "src/frontend/ui/styles.css",
    later: ".viewer-branch-btn:hover",
    earlier: ".has-branch-btn:hover .viewer-branch-btn",
    kind: "harmless",
    why:
      "〔UC2 合并时已修〕同一条规则的选择器列表里补了 `.has-branch-btn .viewer-branch-btn:hover`（0-3-0，写在 0.7 那条之后 ⇒ 悬停按钮时它赢，" +
      "opacity 到得了 1）。这一条仍命中，是因为列表里原来那个 `.viewer-branch-btn:hover`（0-2-0）还在 —— 它今天只管「不在 `.has-branch-btn` 卡片里」" +
      "的按钮，而那种按钮不受 0.7 那条影响，无害。⚠ 视觉没目视。",
  },
  {
    file: "src/frontend/ui/styles.css",
    later: ".branch-fold-wrap .viewer-branch-btn",
    earlier: ".has-branch-btn:hover .viewer-branch-btn",
    kind: "harmless",
    why: "属性不相交：前者只设 `opacity`，后者设 `border-style` / `color` / `border-color`。",
  },
  {
    file: "src/frontend/ui/styles.css",
    later: ".block-thinking .block-summary",
    earlier: ".block-collapsible .block-summary:hover",
    kind: "harmless",
    why: "悬停高亮（背景 ＋ 正文色）压过 thinking 摘要的淡色 —— 悬停反馈本来就该压过变体色，与其余摘要一致。",
  },
  {
    file: "src/frontend/ui/styles.css",
    later: ".block-tool-result-inline > .block-summary",
    earlier: ".block-collapsible .block-summary:hover",
    kind: "harmless",
    why: "同上：悬停高亮压过内联 tool result 摘要的底色与字色。",
  },
  {
    file: "src/frontend/ui/styles.css",
    later: ".block-diff-line code.hljs",
    earlier: ".code-block pre code.hljs",
    kind: "harmless",
    why: "结构上打不到同一个元素：diff 行不在 `.code-block pre` 里（后一条规则头注逐字：`.code-block` 的 transparent 覆盖不 scope 到 `.block-diff`，所以才单写一条防御）。",
  },
  // ── src/frontend/ui/styles.css：tab 规则（U4 写区，未判）──
  { file: "src/frontend/ui/styles.css", later: ".tab-pin", earlier: ".tab:not(.pinned) .tab-pin", kind: "harmless", why: "〔U4 判〕两边属性不相交（前者只设 `display`），先后翻转不改变任何一个声明的胜负" },
  { file: "src/frontend/ui/styles.css", later: ".tab-title", earlier: ".tab.ended .tab-title", kind: "harmless", why: "〔U4 判〕两边属性不相交（前者只设 `display`），先后翻转不改变任何一个声明的胜负" },
  { file: "src/frontend/ui/styles.css", later: ".live-dot", earlier: ".tab.ended .live-dot", kind: "harmless", why: "〔U4 判〕两边属性不相交（前者只设 `display`），先后翻转不改变任何一个声明的胜负" },
  { file: "src/frontend/ui/styles.css", later: ".tab-badge", earlier: ".tab .tab-badge", kind: "harmless", why: "〔U4 判〕两边属性不相交（前者只设 `display`），先后翻转不改变任何一个声明的胜负" },
  { file: "src/frontend/ui/styles.css", later: ".tab-close", earlier: ".tab:not(.ended) .tab-close", kind: "harmless", why: "〔U4 判〕同属性只有 `display`（`n: 2` 里另一处是只设 `font-size` 的那条，不相交）；高特异度那条赢正是作者要的：× 只在已结束时露出，已结束时 ↗ / 📂 藏起来" },
  { file: "src/frontend/ui/styles.css", later: ".tab-close:hover", earlier: ".tab:not(.ended) .tab-close", kind: "harmless", why: "〔U4 判〕两边属性不相交（前者只设 `display`），先后翻转不改变任何一个声明的胜负" },
  { file: "src/frontend/ui/styles.css", later: ".tab-focus", earlier: ".tab.ended .tab-focus", n: 2, kind: "harmless", why: "〔U4 判〕同属性只有 `display`（`n: 2` 里另一处是只设 `font-size` 的那条，不相交）；高特异度那条赢正是作者要的：× 只在已结束时露出，已结束时 ↗ / 📂 藏起来" },
  { file: "src/frontend/ui/styles.css", later: ".tab-focus:hover", earlier: ".tab.ended .tab-focus", kind: "harmless", why: "〔U4 判〕两边属性不相交（前者只设 `display`），先后翻转不改变任何一个声明的胜负" },
  { file: "src/frontend/ui/styles.css", later: ".tab-cwd", earlier: ".tab.ended .tab-cwd", n: 2, kind: "harmless", why: "〔U4 判〕同属性只有 `display`（`n: 2` 里另一处是只设 `font-size` 的那条，不相交）；高特异度那条赢正是作者要的：× 只在已结束时露出，已结束时 ↗ / 📂 藏起来" },
  { file: "src/frontend/ui/styles.css", later: ".tab-cwd:hover", earlier: ".tab.ended .tab-cwd", kind: "harmless", why: "〔U4 判〕两边属性不相交（前者只设 `display`），先后翻转不改变任何一个声明的胜负" },
  // ── src/frontend/ui/styles/settings.css：〔C §2.2 ·  待拍 2〕ST3 那 15 条逐条判完 ──
  //   读法：两条规则的声明逐条对（`postcss` 现打），再到 TS 里核两个选择器能不能落在同一个元素上。
  //   结论 15 条全是 `harmless`（互斥伪类 / 属性不相交 / 结构上打不到同一元素 / 高特异度那条正是作者要的），
  //   没有一条改变可见样式 ⇒ 不改 CSS。⚠ 视觉没目视（本机无图形会话），判的是级联，不是像素。
  {
    file: "src/frontend/ui/styles/settings.css",
    later: ".settings-data-item-open:disabled",
    earlier: ".settings-data-item-open:hover:not(:disabled)",
    kind: "harmless",
    why: "〔AR1 判〕`:hover:not(:disabled)` 与 `:disabled` 互斥（同一颗按钮同一刻只落一边），且两边属性不相交（前者只设底色 / 字色 / 边框色，后者只设 `cursor` / `opacity`）",
  },
  {
    file: "src/frontend/ui/styles/settings.css",
    later: ".remote-machine-legend",
    earlier: ".remote-machine-row .remote-machine-legend",
    kind: "harmless",
    why: "〔AR1 判〕同属性只有 `gap`（与 `display` / `align-items`，值相同）：列表里的机器行（`remote-section.ts` 两处 `.remote-machine-row` 里的 legend）取 6px，机器卡（`machine-card.ts` 的 `fieldset.remote-machine`，不在 row 里）取 8px —— 高特异度那条赢正是作者要的（它头注「S4b：列表里的机器行」就是为行单写的收紧）",
  },
  {
    file: "src/frontend/ui/styles/settings.css",
    later: ".kb-editor-btn-record:disabled",
    earlier: ".kb-editor-btn-record:hover:not(:disabled)",
    kind: "harmless",
    why: "〔AR1 判〕`:hover:not(:disabled)` 与 `:disabled` 互斥（同一颗按钮同一刻只落一边），且两边属性不相交（前者只设底色 / 字色 / 边框色，后者只设 `cursor` / `opacity`）",
  },
  {
    file: "src/frontend/ui/styles/settings.css",
    later: ".kb-editor-btn-reset:disabled",
    earlier: ".kb-editor-btn-reset:hover:not(:disabled)",
    kind: "harmless",
    why: "〔AR1 判〕`:hover:not(:disabled)` 与 `:disabled` 互斥（同一颗按钮同一刻只落一边），且两边属性不相交（前者只设底色 / 字色 / 边框色，后者只设 `cursor` / `opacity`）",
  },
  {
    file: "src/frontend/ui/styles/settings.css",
    later: ".accounts-new-adv > summary",
    earlier: ".accounts-maint-wrap > summary:hover",
    kind: "harmless",
    why: "〔AR1 判〕结构上打不到同一个元素：`.accounts-maint-wrap`（`accounts-section.ts::renderMaintenance` 的 details）与 `.accounts-new-adv`（`account-new-form.ts` 的 details）是两颗不同的 details，子选择器 `>` 只认各自的直接子 summary",
  },
  {
    file: "src/frontend/ui/styles/settings.css",
    later: ".accounts-new-adv > summary::before",
    earlier: ".accounts-maint-wrap[open] > summary::before",
    kind: "harmless",
    why: "〔AR1 判〕同上一条：两颗不同的 details，各自的 summary 各归各",
  },
  {
    file: "src/frontend/ui/styles/settings.css",
    later: ".accounts-maint",
    earlier: ".accounts-maint-wrap .accounts-maint",
    kind: "harmless",
    why: "〔AR1 判〕同属性只有 `margin-top`：`.accounts-maint` 全仓只一处（`renderMaintenance` 里挂在 `.accounts-maint-wrap` 之内）⇒ 恒取 6px；高特异度那条赢正是作者要的（它头注逐字「折叠容器已给了标题，里面那块就不必再顶一个 margin」）",
  },
  {
    file: "src/frontend/ui/styles/settings.css",
    later: ".ccm-alias-gen > summary",
    earlier: ".accounts-maint-wrap > summary:hover",
    kind: "harmless",
    why: "〔AR1 判〕结构上打不到同一个元素：`.ccm-alias-gen` 是 `machine-aliases.ts` 自己建的 details，与 `.accounts-maint-wrap` 不是同一颗，子选择器 `>` 只认各自的 summary",
  },
  {
    file: "src/frontend/ui/styles/settings.css",
    later: ".ccm-alias-gen > summary::before",
    earlier: ".accounts-maint-wrap[open] > summary::before",
    kind: "harmless",
    why: "〔AR1 判〕同上一条",
  },
  {
    file: "src/frontend/ui/styles/settings.css",
    later: ".accounts-wiz-btns button",
    earlier: ".accounts-row-actions button:hover",
    kind: "harmless",
    why: "〔AR1 判〕结构上打不到同一个元素：`.accounts-row-actions` 是账号行的动作条（`accounts-section.ts` 里只装模型输入 / 设默认 / 复制 / 登录 / 展开钮），向导（`.accounts-wizard`）与维护块（`.accounts-maint`）挂在面板 body / 维护折叠里，不在任何一行的动作条之内",
  },
  {
    file: "src/frontend/ui/styles/settings.css",
    later: ".accounts-maint button",
    earlier: ".accounts-row-actions button:hover",
    kind: "harmless",
    why: "〔AR1 判〕同上一条",
  },
  {
    file: "src/frontend/ui/styles/settings.css",
    later: ".accounts-wiz-btns button:disabled",
    earlier: ".accounts-wiz-btns button:hover:not(:disabled)",
    kind: "harmless",
    why: "〔AR1 判〕`:hover:not(:disabled)` 与 `:disabled` 互斥，且属性不相交（前者只设 `border-color`，后者只设 `opacity` / `cursor`）",
  },
  {
    file: "src/frontend/ui/styles/settings.css",
    later: ".accounts-maint button:disabled",
    earlier: ".accounts-wiz-btns button:hover:not(:disabled)",
    kind: "harmless",
    why: "〔AR1 判〕同上一条（跨两个容器也一样：两个伪类互斥）",
  },
  {
    file: "src/frontend/ui/styles/settings.css",
    later: ".paste-block-out",
    earlier: ".ccm-alias-gen-out > .paste-block-out",
    kind: "harmless",
    why: "〔AR1 判〕属性不相交：前者只设 `color`，后者设字体 / 宽度 / 换行 / 横向滚动，先后翻转不改变任何一个声明的胜负",
  },
];

const NDS = "no-descending-specificity";
/** 一条 `no-descending-specificity` 报错 → 登记键（不含行号）。认不出形状就原样返回，让它在等式里显形。 */
function ndsKey(file: string, text: string): string {
  const m = /^Expected selector "(.*?)"(?: \(".*?"\))? to come before selector "(.*?)"(?: \(".*?"\))?, at line \d+/.exec(text);
  return m ? `${file} :: ${m[1]} ⇐ ${m[2]}` : `${file} :: <认不出的报错形状> ${text}`;
}

/**
 * ★ **靠前缀（而不是靠直接住址）才解释得通的 CSS 类名个数**上限。现打 **35**（分母 777）。
 *
 * 🔴 **它堵的是本条最大的一个洞**：前缀是**开区间**。`status-` 这一条在解释
 * `.status-running` 等 5 个真类的同时，也会顺手把**任何**将来变死的 `.status-xxx` 一起掩掉 ——
 * 死规则与活规则在开区间前缀下长得一模一样。
 * 死值验现打过这一形：把 `FIRST_RUN_HINT_CLASS` 改成 `.join("-")` 之后，
 * `.status-first-run-open` 的裁决从 `const-concat` 掉成了 `prefix` —— **识别路径断了，而它照样"活着"**。
 *
 * ⇒ 开区间关不上（关上就要手工枚举每个后缀，那份清单一加新状态就假红），
 *   但**它掩掉的总量可以钉住**：这个数只许降。新添一个被前缀掩掉的类名 ⇒ 当场红，
 *   那时要么它真有住址（那就该落在 `literal` 那一档，说明代码写法变了），要么它就是新的死规则。
 */
const PREFIX_COVERAGE_CEILING = 35;

/**
 * ★ **代码里挂了、CSS 里没有规则的类名个数**上限。现打 **163**（分母 859）。
 *
 * **只许降。** 这 163 个是**混合人群**，如实说清楚，别当成 163 个缺陷：
 * - 大部分是**纯 JS 钩子** —— 挂上去只为 `querySelector` / 事件代理找得到它
 *   （`.sftp-close` · `.pf-start` · `.panorama-back` 这一族），本来就不需要样式；
 * - 一部分是**真悬空** —— 比如当年的 `.settings-btn-secondary`（`src/frontend/ui/settings/**` 53 处在挂它〔W5-AUX 已摘，见下〕），
 *   CSS 里一条规则都没有。〔AR1 现打订正〕上一版说「同族的 `.settings-btn` / `.cc-bus-online` 都 styled ⇒
 *   多半是改名只改了一边」：`git log -S` 两个名字在 CSS 里**从来没有过规则**，`.cc-bus-online` 也没有 ——
 *   不是改名漏了一边，是一开始就只当标记挂。`cc-bus-online-*` 那几个状态类已改成 `data-state`。
 *
 * ⇒ 本格**不区分这两者**（机械上区分不了：「钩子」与「忘了写样式」在语法上一模一样），
 * 它买的只有一件事：**这个数不许再涨**。涨了就说明又多了一个挂着却没规则的类名，
 * 那时要么补样式、要么改成 `data-*` 钩子（状态表达约定正是这条）。
 *
 * ⚠ 它也会在**另一个方向**红：有人删掉了一条 CSS 规则而代码还在挂那个类。那种红是对的。
 */
// 〔D §D8 · 09-25 棘 163 → 147〕起步现打 150（D 审计同数）；`cc-bus-section.ts` 在线状态那三个
//   从没有过规则的类名（`cc-bus-online-unknown` / `-checking` / `-error`）按约定 3 改成 `data-state`
//   ⇒ 现打 147（少的就是这三个；`-yes` / `-no` 由模板拼、本来就不进这一数）。
//   `.settings-btn-secondary`（53 处挂、git 史里从没有过规则、外观即 `.settings-btn` 默认）仍在这 147 里，理由见 `AR1.md §2`。
// 〔AR1 拍板 3 · 09-25 棘 147 → 146〕「删类名」：`.settings-btn-secondary` 从 `src/` 12 份文件里摘掉
//   （现打 55 处字面量 ＋ `panel.ts::makeBtn` 那一处模板拼接；三个按钮助手的 `variant` 空串 = 默认那一种）⇒ 少的就是它这一个。
//   外观不变：它从来没有规则，挂与不挂算出来的样式一样。
// 〔09-29 棘 146 → 141〕设置 → 机器那一行的后端四格补了样式（`settings.css`）⇒
//   `.backend-row` · `-state` · `-kill` · `-exit` · `-health` 这五个从「挂着没规则」里出去，少的就是它们。
const DANGLING_CEILING = 141;

/** 本文件只在这儿读一次盘，后面各格共用。 */
let cached: Ledger | null = null;
function ledger(): Ledger {
  cached ??= buildLedger(REPO_ROOT);
  return cached;
}

/** 印一行带分母的绿话 —— 照 `tests/scripts/gate.sh` 那些 `ok` 行的写法。 */
function denom(cell: string, n: number, what: string): void {
  console.log(`  ok   S25-${cell}  ${n} ${what}`);
}

describe("S25 ⓪ 量具自检（这些不过，下面四格全是空转）", () => {
  it("遍历与词法真的扫到了东西", () => {
    const led = ledger();
    expect(led.cssFiles.length, `只扫到 ${led.cssFiles.length} 份 CSS —— 遍历坏了`).toBeGreaterThanOrEqual(
      FLOORS.cssFiles,
    );
    expect(led.codeFiles.length, "代码文件份数掉到地板以下 —— 遍历坏了").toBeGreaterThanOrEqual(FLOORS.codeFiles);
    expect(led.cssClasses.size, "CSS 类名分母掉到地板以下 —— 选择器抽取坏了").toBeGreaterThanOrEqual(
      FLOORS.cssClasses,
    );
    expect(led.literals.size, "代码侧 token 分母掉到地板以下 —— 词法器错位了").toBeGreaterThanOrEqual(
      FLOORS.literals,
    );
    expect(led.usedClasses.size, "方向 ② 的调用点分母掉到地板以下").toBeGreaterThanOrEqual(FLOORS.usedClasses);
    denom(
      "⓪",
      led.cssClasses.size,
      `个 CSS 类名 · ${led.codeFiles.length} 份代码文件 · ${led.literals.size} 个代码侧 token`,
    );
  });

  it("三条识别路径各自真的有货（任何一条空了，② 的正控就会靠别的机制蒙混过关）", () => {
    const led = ledger();
    expect(
      led.constConcat.size,
      "常量拼接一条都没识别出来 —— `const X = \"…\"` 那步坏了，`.status-first-run-*` 会被判死",
    ).toBeGreaterThanOrEqual(FLOORS.constConcat);
    expect(
      led.vendorClasses.size,
      `第三方类名只收到 ${led.vendorClasses.size} 个（来源：${led.vendorSpecs.join(", ") || "<一个都没找到>"}）` +
        " —— `import \"<包>/….css\"` 那步坏了，或者 node_modules 没装，`.katex-*` 会被判死",
    ).toBeGreaterThanOrEqual(FLOORS.vendorClasses);
    expect(led.prefixCandidates.size, "模板前缀候选一个都没派生出来 —— 词法器的洞识别坏了").toBeGreaterThanOrEqual(
      FLOORS.prefixCandidates,
    );
    denom(
      "⓪",
      led.constConcat.size + led.vendorClasses.size + led.prefixCandidates.size,
      `条识别证据（常量拼接 ${led.constConcat.size} · 第三方 ${led.vendorClasses.size}（${led.vendorSpecs.length} 份） · 前缀候选 ${led.prefixCandidates.size}）`,
    );
  });

  /**
   * 🔴 词法器的**正控 ＋ 死值验**，全部在内存里跑，不碰工作树。
   *
   * 前三条是正控（它必须认得出这三种真实形状，三条都来自本仓现打过的住址），
   * 第四条是死值验（它必须**说得出"不认识"**，不是把什么都判活）。
   */
  it("词法器对四种真实形状给出正确答案", () => {
    // ① 嵌套模板 —— `src/frontend/ui/paste-block.ts:90` 的真形状
    const nested = 'x.className = `paste-block${spec.className ? ` ${spec.className}` : ""}`;';
    const t1 = scanStrings(nested).filter((t) => t.kind === "tpl");
    expect(t1.some((t) => t.kind === "tpl" && t.chunks[0]?.text === "paste-block" && t.chunks[0].hole)).toBe(true);

    // ② 正则字面量里的引号 —— `src/frontend/ui/launcher-diagnostics.ts:149` 的真形状。
    //    词法器若把 `/'/g` 里的引号当字符串开头，整份文件从此错位，后面的 className 全读不到。
    const rx = ["const q = (s) => `'${s.replace(/'/g, `'\\''`)}'`;", 'w.className = "ccm-alias-gen";'].join("\n");
    const got = scanStrings(rx).filter((t) => t.kind === "str" && t.value === "ccm-alias-gen");
    expect(got.length, "正则字面量把词法带错位了 —— 它后面的类名会平白变成死规则").toBe(1);

    // ③ HTML 片段里的 class 属性 —— `src/frontend/ui/render.ts:77` 的真形状
    const html = "const s = `<div class=\"code-bar\"><span class=\"code-lang\"></span></div>`;";
    const chunks = scanStrings(html).flatMap((t) => (t.kind === "tpl" ? t.chunks.map((c) => c.text) : []));
    expect(chunks.join("")).toContain('class="code-bar"');

    // ④ 死值验：注释里的类名不许当成代码在用它（`src/frontend/ui/render.ts:49` 那条 JSDoc 的真形状）
    const commented = ['/** `<code class="language-X">` 只是散文 */', 'e.className = "real-one";'].join("\n");
    expect(stripCodeComments(commented)).not.toContain("language-X");
    expect(stripCodeComments(commented)).toContain("real-one");

    denom("⓪", 4, "种真实形状（嵌套模板 · 正则里的引号 · HTML class 属性 · 注释不算用户）");
  });
});

describe("S25 ① z-index 只许写 var(--z-*)（件 4）", () => {
  it("一处裸数字都没有", () => {
    const led = ledger();
    const decls = led.zIndexDecls;
    // 反空真：扫到 0 条 ⇒ 失败，不是静默绿。
    expect(
      decls.length,
      `只扫到 ${decls.length} 条 \`z-index\` 声明（地板 ${FLOORS.zIndexDecls}）—— 抽取器坏了，本条会零命中地绿`,
    ).toBeGreaterThanOrEqual(FLOORS.zIndexDecls);

    const bare = decls.filter((d) => !Z_OK.test(d.value)).map((d) => `${d.file}:${d.line}  z-index: ${d.value}`);
    expect(
      bare,
      "这些 `z-index` 没走刻度：\n  " +
        bare.join("\n  ") +
        "\n★ 十档刻度定义在 `src/frontend/ui/styles/tokens.css`（`--z-base` … `--z-drag`），逐条判语义的结果在\n" +
        "  `tests/evidence/S21-css-readings.md §3`。**别机械按数值映射** —— 要求逐条判。\n" +
        "★ 这一格同时钉在 `.stylelintrc.json` 的 `declaration-property-value-allowed-list` 上，\n" +
        "  所以 `npm run lint:css` 也看得见它（那条是 advisory，本条才是会红的那个）。",
    ).toEqual([]);
    denom("①", decls.length, "条 z-index 声明（全部走刻度）");
  });

  it("`.stylelintrc.json` 里那条规则还在（散文与判据是两处副本，必须同调）", () => {
    const raw = readFileSync(resolve(REPO_ROOT, ".stylelintrc.json"), "utf8");
    const rules = (JSON.parse(raw) as { rules: Record<string, unknown> }).rules;
    const allow = rules["declaration-property-value-allowed-list"] as Record<string, string[]> | undefined;
    expect(allow, "`.stylelintrc.json` 里没有 `declaration-property-value-allowed-list` —— 那 `npm run lint:css` 就看不见这一格了").toBeTruthy();
    const pats = allow?.["z-index"] ?? [];
    expect(pats.length, "那条规则里没有 `z-index` 这一项").toBeGreaterThan(0);
    // 规则里写的正则与本文件的 `Z_OK` 必须是同一个意思：拿两个真实值对拍，一正一反。
    const asRe = new RegExp(pats[0].replace(/^\/|\/$/g, ""));
    expect(asRe.test("var(--z-modal)"), "stylelint 那条正则连合法值都不认").toBe(true);
    expect(asRe.test("99"), "stylelint 那条正则把裸数字也放过了 —— 它等于没写").toBe(false);
    denom("①", pats.length, "条 stylelint 侧的 z-index 白名单模式（与判据对拍过一正一反）");
  });

  /** 死值验：给量具喂一段**带裸数字**的 CSS，它必须逮到。逮不到 ⇒ 上面那条是空的。 */
  it("死值验：塞一个裸 `z-index: 99`，量具必须逮到", () => {
    const mutant = ".s25-mutant { z-index: 99; }\n.s25-ok { z-index: var(--z-modal); }";
    const decls = zIndexDeclsOf(mutant, "<内存里的变异体>");
    expect(decls.length, "连变异体里的两条都没扫到 —— 抽取器坏了").toBe(2);
    expect(decls.filter((d) => !Z_OK.test(d.value)).map((d) => d.value)).toEqual(["99"]);
    denom("①", 2, "条变异体声明（1 条该红、1 条该绿，都判对了）");
  });
});

describe("S25 ② CSS 里的类名有人用", () => {
  it("🔴 正控：三个已知假阳性，必须活着 —— 而且要靠对的那条机制活着", () => {
    const led = ledger();
    const prefixes = ALLOWED_PREFIXES.map((p) => p.prefix);
    const want: [string, string][] = [
      ["status-first-run-open", "const-concat"],
      ["status-first-run-dismiss", "const-concat"],
      ["katex-display", "vendor"],
    ];
    for (const [name, kind] of want) {
      // 先确认它今天真的还在 CSS 里 —— 不在的话下面那条断言会对着空气成立。
      expect(led.cssClasses.has(name), `\`.${name}\` 已经不在 CSS 里了 —— 正控失去对象，本条会假绿`).toBe(true);
      const v = explain(led, name, prefixes);
      expect(
        v.kind,
        `\`.${name}\` 的裁决是 ${v.kind}，而它应该靠 ${kind} 活着。\n` +
          "★ 只判「没被判死」是不够的：那条断言在尺子把所有东西都判活时同样绿。\n" +
          "  裁决**必须是对的那一种**，否则识别路径断了也看不出来。",
      ).toBe(kind);
    }
    denom("②", want.length, "个已知假阳性（三族各一，裁决与机制都对上了）");
  });

  it("每个 CSS 类名都说得出谁在用它（未解释的 == 登记的已知死规则）", () => {
    const led = ledger();
    const prefixes = ALLOWED_PREFIXES.map((p) => p.prefix);
    expect(led.cssClasses.size, "CSS 类名分母掉到地板以下 —— 本条会零命中地绿").toBeGreaterThanOrEqual(
      FLOORS.cssClasses,
    );

    const tally = { literal: 0, "const-concat": 0, vendor: 0, prefix: 0 };
    const unexplained: string[] = [];
    for (const [name, sites] of led.cssClasses) {
      const v = explain(led, name, prefixes);
      if (v.kind === "unexplained") unexplained.push(`.${name}  ←  ${sites[0]}`);
      else tally[v.kind]++;
    }
    const registered = KNOWN_DEAD.map((d) => `.${d.name}  ←  ${led.cssClasses.get(d.name)?.[0] ?? "<不在 CSS 里>"}`);
    expect(
      unexplained.sort(),
      "这些 CSS 类名今天没有任何人挂它，而且不在 `KNOWN_DEAD` 里：\n  " +
        unexplained.join("\n  ") +
        "\n★ 删之前**先确认它不是假阳性**（上一轮 13 个里有 3 个是活的，`S21` 步 3）：\n" +
        "  ① 是不是 `` `${某个常量}-后缀` `` 拼出来的？② 是不是第三方库自产的？\n" +
        "  ③ 是不是模板拼接？是的话把前缀连**理由**一起加进 `ALLOWED_PREFIXES`。\n" +
        "  ④ 确实是死的 ⇒ 删掉那几行 CSS；删不了（不在写区）⇒ 加进 `KNOWN_DEAD` 并写清凭什么。",
    ).toEqual(registered.sort());

    denom(
      "②",
      led.cssClasses.size,
      `个 CSS 类名（直接字面量 ${tally.literal} · 常量拼接 ${tally["const-concat"]} · 第三方 ${tally.vendor} · 模板前缀 ${tally.prefix} · 已登记死规则 ${KNOWN_DEAD.length}）`,
    );
  });

  it("登记表不许有死条目（前缀要还派生得出来、还真解释着东西；已知死规则要还在 CSS 里）", () => {
    const led = ledger();
    const prefixes = ALLOWED_PREFIXES.map((p) => p.prefix);

    const noSite = ALLOWED_PREFIXES.filter((p) => !led.prefixCandidates.has(p.prefix)).map((p) => p.prefix);
    expect(
      noSite,
      `这些前缀登记着，但代码里已经没有任何 \`\`\`模板\${…}\`\`\` 派生得出它：${noSite.join(", ")}\n` +
        "⇒ 拼接点没了（改写法 / 删组件）。把登记一起删掉 —— 留着的前缀会在下一次悄悄掩掉一个真死规则。",
    ).toEqual([]);

    // 每条前缀今天到底解释了谁：解释不到任何东西 ⇒ 它是死条目。
    const covered = new Map<string, string[]>();
    for (const [name] of led.cssClasses) {
      const v = explain(led, name, prefixes);
      if (v.kind === "prefix") covered.set(v.prefix, [...(covered.get(v.prefix) ?? []), name]);
    }
    const idle = ALLOWED_PREFIXES.filter((p) => !covered.has(p.prefix)).map((p) => p.prefix);
    expect(
      idle,
      `这些前缀登记着，但今天一个 CSS 类都解释不到：${idle.join(", ")}\n` +
        "⇒ 要么对应的样式被删了（那就把登记也删了），要么它本来就不必登记。",
    ).toEqual([]);

    const gone = KNOWN_DEAD.filter((d) => !led.cssClasses.has(d.name)).map((d) => d.name);
    expect(
      gone,
      `这些已知死规则已经不在 CSS 里了：${gone.join(", ")}\n⇒ 有人把它删了（好事），把 \`KNOWN_DEAD\` 里那一条也删掉。`,
    ).toEqual([]);

    // 🔴 前缀是开区间 —— 掩掉的总量必须钉住，理由全文见 `PREFIX_COVERAGE_CEILING`。
    const maskedAll = [...covered.values()].flat().sort();
    expect(
      maskedAll.length,
      `靠前缀才解释得通的 CSS 类名有 ${maskedAll.length} 个 > 棘轮上限 ${PREFIX_COVERAGE_CEILING}（分母 ${led.cssClasses.size}）。\n` +
        "★ 前缀是**开区间**：它掩得住真死规则。这个数只许降。\n" +
        "★ 涨了先问：新增的那个类名在代码里有没有**直接住址**？有 ⇒ 它本该落在 `literal` 档，\n" +
        "  落到 `prefix` 说明代码那边的写法变了（比如拼接取代了字面量），去看那处改动是不是想要的。\n" +
        `当前被掩的清单：\n  ${maskedAll.join(" ")}`,
    ).toBeLessThanOrEqual(PREFIX_COVERAGE_CEILING);

    const lines = [...covered].sort().map(([p, ns]) => `${p}→${ns.length}`);
    denom(
      "②",
      ALLOWED_PREFIXES.length + KNOWN_DEAD.length,
      `条登记（前缀共掩 ${maskedAll.length}/${PREFIX_COVERAGE_CEILING} 个类：${lines.join(" ")}）`,
    );
  });

  /** 死值验：造一个**没人用**的类名，尺子必须判它死。判不出 ⇒ 上面那条是空的。 */
  it("死值验：造一个没人用的类名，尺子必须判它死；同一把尺子对活类要判活", () => {
    const led = ledger();
    const prefixes = ALLOWED_PREFIXES.map((p) => p.prefix);
    const fake = cssClassesOf(".s25-nobody-uses-this { color: red; }\n.code-bar { color: red; }", "<内存里的变异体>");
    expect([...fake.keys()].sort()).toEqual(["code-bar", "s25-nobody-uses-this"]);
    expect(explain(led, "s25-nobody-uses-this", prefixes).kind, "尺子对一个纯造出来的名字也说「有人用」⇒ 它是把永远说活的坏尺子").toBe(
      "unexplained",
    );
    expect(explain(led, "code-bar", prefixes).kind, "同一把尺子对真在用的 `.code-bar` 判死了 ⇒ 它会指挥人删掉在用的样式").toBe(
      "literal",
    );
    denom("②", 2, "个变异体类名（1 个该死、1 个该活，都判对了）");
  });
});

describe("S25 ③ 代码挂的类名，CSS 里有没有规则（递减棘轮）", () => {
  it("悬空的类名引用只许变少", () => {
    const led = ledger();
    expect(led.usedClasses.size, "方向 ② 一个调用点都没扫到 —— 本条会零命中地绿").toBeGreaterThanOrEqual(
      FLOORS.usedClasses,
    );
    const dangling = [...led.usedClasses]
      .filter(([name]) => !led.cssClasses.has(name) && !led.vendorClasses.has(name))
      .map(([name, sites]) => `.${name}  ←  ${sites.slice(0, 2).join(" ")}`)
      .sort();
    expect(
      dangling.length,
      `代码里挂着、CSS 里没规则的类名有 ${dangling.length} 个 > 棘轮上限 ${DANGLING_CEILING}（分母 ${led.usedClasses.size}）。\n` +
        "★ 这是**递减棘轮**：只许降。涨了是两种情况之一 ——\n" +
        "  ① 新挂了一个没有样式的类名 ⇒ 补样式，或按改成 `data-*` 钩子；\n" +
        "  ② 有人删了一条 CSS 规则而代码还在挂它 ⇒ 那是真回归。\n" +
        `当前清单（前 40 条）：\n  ${dangling.slice(0, 40).join("\n  ")}`,
    ).toBeLessThanOrEqual(DANGLING_CEILING);
    denom("③", led.usedClasses.size, `个代码侧类名引用（其中 ${dangling.length} 个 CSS 里没规则，棘轮上限 ${DANGLING_CEILING}）`);
  });
});

describe("S25 ④ stylelint 报错总数（递减棘轮）", () => {
  it("只许变少，且被 lint 的那批文件就是本账扫的那批", async () => {
    const led = ledger();
    const res = await stylelint.lint({
      files: "src/**/*.css",
      cwd: REPO_ROOT,
      configFile: resolve(REPO_ROOT, ".stylelintrc.json"),
    });
    const linted = res.results.filter((r) => !r.source?.includes("node_modules"));

    // 反空真①：真的 lint 到了文件。「扫了 0 份」与「全都干净」在 stylelint 的退出码上一模一样。
    expect(
      linted.length,
      `stylelint 只 lint 到 ${linted.length} 份 CSS —— glob 坏了或文件搬家了，本条会零命中地绿`,
    ).toBeGreaterThanOrEqual(FLOORS.cssFiles);

    // 反空真②：**两处人群必须是同一批**。`package.json` 的 `lint:css` 与本账各走各的 glob，
    // 一边悄悄缩水时，另一边照样绿 —— 这正是本仓反复吃亏的那一形。
    const lintedRel = linted
      .map((r) => (r.source ?? "").slice(REPO_ROOT.length + 1).split("\\").join("/"))
      .sort();
    expect(
      lintedRel,
      "stylelint 扫到的 CSS 与本账扫到的不是同一批 —— 两个 glob 漂了，其中一边在守空气",
    ).toEqual([...led.cssFiles].sort());

    const all = linted.flatMap((r) => r.warnings);
    // `no-descending-specificity` 不进棘轮，由 ④b 的等号登记表管
    const warnings = all.filter((w) => w.rule !== NDS);
    const byRule = new Map<string, number>();
    for (const w of warnings) byRule.set(w.rule, (byRule.get(w.rule) ?? 0) + 1);
    const top = [...byRule].sort((a, b) => b[1] - a[1]).map(([r, n]) => `${r}×${n}`);

    expect(
      warnings.length,
      `stylelint 报错 ${warnings.length} 条 > 棘轮上限 ${STYLELINT_CEILING}（现打基线 47，\`S21 §5\` 同数）。\n` +
        "★ 这是**递减棘轮**：只许降。修好了就把 `STYLELINT_CEILING` 一起调下来，别只改代码不棘紧。\n" +
        "★ 为什么这里是棘轮不是等号：本条落地时另有一路在改 `src/frontend/ui/styles.css`，等号在并发下互相打架。\n" +
        "  代价（拦不住判据空转）由上面那两条反空真接着。理由全文见本文件头注。\n" +
        `逐规则：\n  ${top.join("\n  ")}`,
    ).toBeLessThanOrEqual(STYLELINT_CEILING);

    // 本格新开的那条规则**必须真的在跑**：它今天该是 0 命中，但「规则没开」与「0 命中」
    // 在报错总数上一模一样 ⇒ 上面那条 `.stylelintrc.json` 的对拍是它的另一条腿。
    expect(
      byRule.get("declaration-property-value-allowed-list") ?? 0,
      "z-index 白名单在真 stylelint 下报出了命中 —— 与格 ① 的读数矛盾，两把尺子有一把坏了",
    ).toBe(0);

    denom("④", linted.length, `份 CSS 文件（报错 ${warnings.length}/${STYLELINT_CEILING}，棘轮只许降；另 ${all.length - warnings.length} 条 ${NDS} 归 ④b）`);

    // ── ④b no-descending-specificity 的命中 == 登记的例外（两向，多重集）──
    const got = linted
      .flatMap((r) =>
        r.warnings
          .filter((w) => w.rule === NDS)
          .map((w) => ndsKey((r.source ?? "").slice(REPO_ROOT.length + 1).split("\\").join("/"), w.text)),
      )
      .sort();
    const want = DESCENDING_SPECIFICITY_EXCEPTIONS.flatMap((e) =>
      Array.from({ length: e.n ?? 1 }, () => `${e.file} :: ${e.later} ⇐ ${e.earlier}`),
    ).sort();
    expect(
      got,
      `\`${NDS}\` 的命中与 \`DESCENDING_SPECIFICITY_EXCEPTIONS\` 不等。\n` +
        "★ 多出来的（在 got 不在 want）：新写出了一条「后写的选择器特异度更低」—— 改写法（把低特异度的挪到前面、或把后者写得同样具体），" +
        "或确认无害后登记并写理由。\n" +
        "★ 少了的（在 want 不在 got）：那一条修掉了 / 规则改写了 —— 把登记删掉（本表只许缩，除非新登记写了理由）。",
    ).toEqual(want);
  }, TIMEOUT_MS);

  it("④b 〔UC2〕no-descending-specificity 真的开着，而且真能报（正控：一段降序特异度的夹具必须恰报 1 条）", async () => {
    const cfg = JSON.parse(readFileSync(resolve(REPO_ROOT, ".stylelintrc.json"), "utf8")) as { rules: Record<string, unknown> };
    expect(cfg.rules[NDS], "`.stylelintrc.json` 里 `no-descending-specificity` 没开 —— ④b 的等式会对着 0 条命中零命中地绿").toBe(true);
    const res = await stylelint.lint({
      code: "@layer components {\n  .a .b:hover {\n    color: red;\n  }\n\n  .b {\n    color: blue;\n  }\n}\n",
      codeFilename: resolve(REPO_ROOT, "src/__uc2_probe__.css"),
      configFile: resolve(REPO_ROOT, ".stylelintrc.json"),
    });
    const nds = res.results.flatMap((r) => r.warnings).filter((w) => w.rule === NDS);
    expect(nds.map((w) => ndsKey("probe", w.text)), "仓里的配置对一段典型的降序特异度不报 —— 规则没生效").toEqual(["probe :: .b ⇐ .a .b:hover"]);
    const kinds = new Map<string, number>();
    for (const e of DESCENDING_SPECIFICITY_EXCEPTIONS) kinds.set(e.kind, (kinds.get(e.kind) ?? 0) + (e.n ?? 1));
    denom("④b", [...kinds.values()].reduce((a, b) => a + b, 0), `条登记的例外（${[...kinds].map(([k, n]) => `${k} ${n}`).join(" · ")}），与真 stylelint 的命中两向相等`);
  }, TIMEOUT_MS);

  it("`ci.yml` 里那个数与本文件的上限是同一个值（散文要有一条会红的判据读它）", () => {
    const yml = readFileSync(resolve(REPO_ROOT, ".github/workflows/ci.yml"), "utf8");
    const m = /S25-STYLELINT-CEILING:\s*(\d+)/.exec(yml);
    expect(
      m,
      "`.github/workflows/ci.yml` 里找不到 `S25-STYLELINT-CEILING: <数>` 这个标记 —— 措辞改了？\n" +
        "改了就把本条的正则一起改，别让它零命中地绿（`ci.yml` 里那句「50 项基线」正是这么腐了一年的：\n" +
        "实测 47，而散文一直写着 50，`eslint-baseline.vitest.ts` 的头注逐字登记过「本条不管 stylelint」）。",
    ).toBeTruthy();
    expect(Number(m?.[1]), "`ci.yml` 里的数与 `STYLELINT_CEILING` 漂了").toBe(STYLELINT_CEILING);
    denom("④", 1, "处 CI 侧散文（与判据常量对上了）");
  });
});

// ═══════════════════════════ ⑤ 层真包进去（件 2）═══════════════════════════
//
// 〔三入口拆分那一拍新装〕抬头的订正逐字：「7 个层只有 `reset`/`tokens` 真包进去
// （**无层样式赢过所有有层的**，收益还没到手）」。本格钉的就是那一句的反面，两条：
//
// ⑤a **恒等**：`src` 下每一份 CSS 里的每一条规则，外层 at-rule 链上都有一个 `@layer` ——
//     无层规则的条数 == 0。一条漏网的无层规则会**压过所有有层的**（与特异度无关），
//     正是订正里说的那个病。
// ⑤b **恒等**：用到的每个层名都在 `layers.css` 那句声明里，且那句声明的次序 == 下面这张表。
//     拼错一个层名不会报错 —— 浏览器会**悄悄新开一层、排在所有声明过的层之后**，于是那一块
//     压过一切。这一形没有任何别的东西会叫。
//
// 买到 / 买不到：
// - ✅ 源码层面零无层规则、零野层名、层序与设计一致。构建产物（含第三方 CSS 被
//   `vite.config.ts` 的插件包进 `vendor` 那一步）另由 `tests/frontend/ui/entry-graphs.vitest.ts` 对产物再判一次。
// - ❌ **每条规则进的是不是「对的」那一层**（该进 `states` 的还在 `components`）判不了 ——
//   那要语义。今天 `base` / `states` / `utilities` 三层是空的，理由写在末尾追加的那一节。

/** 层的设计次序（七层 ＋ 第三方那一层 `vendor`，排在 `reset` 之后、我们所有层之前）。 */
const LAYER_ORDER = ["reset", "vendor", "tokens", "base", "layout", "components", "states", "utilities"] as const;

interface LayerScan {
  /** 判过的规则条数（分母）。 */
  rules: number;
  /** 不在任何 `@layer` 里的规则：选择器 → 所在行。 */
  unlayered: string[];
  /** 用到的层名（`@layer x {` 块与 `@layer a, b;` 语句里出现的）。 */
  used: string[];
  /** `@layer a, b, …;` 语句按出现次序。 */
  statements: string[][];
  /** 真装着规则的层（`@layer x { … }` 块的名字）。 */
  blocks: string[];
}

/** 扫一份 CSS：每条规则在不在层里、用了哪些层名。注释与字符串里的花括号不算。 */
export function scanLayers(css: string, file: string): LayerScan {
  const out: LayerScan = { rules: 0, unlayered: [], used: [], statements: [], blocks: [] };
  const stack: string[] = [];
  let buf = "";
  let line = 1;
  let inRule = 0;
  for (let i = 0; i < css.length; i++) {
    const c = css[i];
    if (c === "\n") line++;
    if (c === "/" && css[i + 1] === "*") {
      const end = css.indexOf("*/", i + 2);
      const stop = end === -1 ? css.length : end + 2;
      for (let k = i; k < stop; k++) if (css[k] === "\n") line++;
      i = stop - 1;
      continue;
    }
    if (c === '"' || c === "'") {
      const end = css.indexOf(c, i + 1);
      buf += css.slice(i, end + 1);
      i = end;
      continue;
    }
    if (inRule > 0) {
      if (c === "{") inRule++;
      if (c === "}") inRule--;
      continue;
    }
    if (c === ";") {
      const m = /^@layer\s+([^{]+)$/.exec(buf.trim());
      if (m) {
        const names = m[1].split(",").map((s) => s.trim());
        out.statements.push(names);
        out.used.push(...names);
      }
      buf = "";
      continue;
    }
    if (c === "{") {
      const prelude = buf.trim().replace(/\s+/g, " ");
      buf = "";
      if (prelude.startsWith("@")) {
        const m = /^@layer\s+([\w-]+)$/.exec(prelude);
        if (m) {
          out.used.push(m[1]);
          out.blocks.push(m[1]);
        }
        // @keyframes / @font-face 的内部不是规则
        if (/^@(?:-webkit-)?keyframes\b|^@font-face\b/.test(prelude)) {
          inRule = 1;
          continue;
        }
        stack.push(prelude);
        continue;
      }
      out.rules++;
      if (!stack.some((a) => a.startsWith("@layer "))) out.unlayered.push(`${file}:${line}  ${prelude.slice(0, 60)}`);
      inRule = 1;
      continue;
    }
    if (c === "}") {
      stack.pop();
      buf = "";
      continue;
    }
    buf += c;
  }
  return out;
}

describe("S25 ⑤ 层真包进去（件 2）", () => {
  it("⑤a 每一条规则都在某个 @layer 里（无层规则 == 0）", () => {
    const led = ledger();
    let rules = 0;
    const unlayered: string[] = [];
    for (const f of led.cssFiles) {
      const s = scanLayers(readFileSync(resolve(REPO_ROOT, f), "utf8"), f);
      rules += s.rules;
      unlayered.push(...s.unlayered);
    }
    // 反空真：扫到的规则条数掉到地板以下 ⇒ 扫描器坏了，下面那条零命中地绿
    expect(rules, `只判到 ${rules} 条规则 —— 扫描器坏了`).toBeGreaterThan(900);
    expect(
      unlayered,
      "这些规则不在任何 `@layer` 里 —— **无层样式赢过所有有层的**（与特异度无关），\n" +
        "它们会悄悄压过层里的一切。放进它该在的那一层（多半是 `components`）。",
    ).toEqual([]);
    denom("⑤", rules, `条规则（${led.cssFiles.length} 份 CSS，无层 0）`);
  });

  it("⑤b 层名都在声明里、声明次序 == 设计（拼错的层名会悄悄排到最后、压过一切）", () => {
    const led = ledger();
    const decl = scanLayers(readFileSync(resolve(REPO_ROOT, "src/frontend/ui/styles/layers.css"), "utf8"), "layers.css");
    expect(decl.statements, "`layers.css` 里应该恰好一句 `@layer …;` 声明").toHaveLength(1);
    expect(decl.statements[0], "层的次序与（＋ vendor）不一致").toEqual([...LAYER_ORDER]);
    const wild: string[] = [];
    const used = new Set<string>();
    for (const f of led.cssFiles) {
      const sc = scanLayers(readFileSync(resolve(REPO_ROOT, f), "utf8"), f);
      for (const n of sc.used) if (!(LAYER_ORDER as readonly string[]).includes(n)) wild.push(`${f}: @layer ${n}`);
      for (const n of sc.blocks) used.add(n);
    }
    expect(wild, "这些层名没在声明里 —— 浏览器会给它新开一层、排在所有声明过的层之后").toEqual([]);
    // 分母：今天真有规则落进的层（base / states / utilities 空着，末尾追加的那一节）
    for (const must of ["reset", "tokens", "layout", "components"]) expect(used.has(must), `没有任何文件用到 \`${must}\` 层`).toBe(true);
    // `vendor` 不在源码里 —— 它由 `vite.config.ts` 的插件在构建时包上（产物那一侧由 entry-graphs 判）
    denom("⑤", used.size, `个层真装着规则（${[...used].sort().join(" · ")}），零野层名`);
  });

  /**
   * 件 3：最小重置里要有表单控件那一族。订正里那句「`@layer reset` 里只有
   * `box-sizing` ＋ `html/body`，button/input 那一族没有」就是本条的反面。
   * ⚠ 这是**存在性**钉子：买到「那几条声明在、而且在 `reset` 层里」，**买不到**它们在真窗口里
   * 让哪些控件变了样（要目视；静态普查的读数写在 `reset.css` 那条规则的注释里）。
   */
  it("⑤c reset 层里有表单控件的最小重置（四种控件继承字体与颜色；button 抹底抹边）", () => {
    const css = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/styles/reset.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, " ");
    const body = /@layer\s+reset\s*\{([\s\S]*)\}\s*$/.exec(css.trim())?.[1] ?? "";
    expect(body.length, "reset.css 里切不出 `@layer reset { … }` —— 下面几条对着空串").toBeGreaterThan(100);
    const rules = [...body.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
      sels: m[1].split(",").map((x) => x.trim()),
      decls: m[2].replace(/\s+/g, " "),
    }));
    const find = (sel: string, decl: RegExp): boolean => rules.some((r) => r.sels.includes(sel) && decl.test(r.decls));
    for (const el of ["button", "input", "select", "textarea"]) {
      expect(find(el, /font:\s*inherit/), `reset 层里没有给 \`${el}\` 继承字体`).toBe(true);
      expect(find(el, /color:\s*inherit/), `reset 层里没有给 \`${el}\` 继承颜色`).toBe(true);
    }
    expect(find("button", /background:\s*none/) && find("button", /border:\s*0/), "reset 层里没有抹掉 button 的原生底与边").toBe(true);
    denom("⑤", rules.length, "条 reset 规则（表单控件那一族在）");
  });

  it("⑤ 死值验：扫描器认得出无层规则与野层名，也不把 @keyframes 的帧当规则", () => {
    const bad = scanLayers(
      "@layer components { .ok { color: red; } }\n.leak { color: red; }\n@layer componets { .typo { x: y; } }\n@keyframes k { 0% { opacity: 0; } }",
      "<变异体>",
    );
    expect(bad.rules, "变异体里 3 条规则（帧不算）").toBe(3);
    expect(bad.unlayered.map((u) => u.replace(/^.*?\s{2}/, ""))).toEqual([".leak"]);
    expect(bad.used.filter((n) => !(LAYER_ORDER as readonly string[]).includes(n))).toEqual(["componets"]);
    denom("⑤", 3, "条变异体规则（1 条无层、1 个野层名，都逮到了）");
  });
});

// ═══════════════════════════ ⑥ 容器查询取代写死宽度（件 5）═══════════════════════════
//
// 两件事绑在一起（「两件事都要做，不能只做一件」）：
//   CSS 侧 —— 消息列的宿主 `.stream` 是名为 `stream` 的行内尺寸容器；
//   JS 侧 —— `src/frontend/ui/height-estimate.ts` 的 `COL_W` 不再写死 780，而是在那个容器里实测。
// 本格钉三条：
//   ⑥a `.stream` 的规则里声明了 `container: stream / inline-size`（名字与轴都对）；
//   ⑥b `COL_W` 真的是量出来的：它的初始化里建了 `.stream > .stream-content`、挂进 `#message-stream`、
//       读了宽度 —— 且**没有**再出现 `const COL_W = <数字>` 那一形；
//   ⑥c 量不到时的回退值 == `tokens.css` 的 `--stream-max-width`（原先只有一句注释「若列宽 token 改动需同步这里」）。
// 买到 / 买不到：
// - ✅ 两侧的住址与回退值不漂。
// - ❌ 真引擎里量出来的数对不对、列被压窄时估高是否更准 —— 要真窗口（本机无图形会话）。
// - ❌ 拉窗口之后 `COL_W` 不重算（只在模块求值时量一次），理由写在 `height-estimate.ts` 那条注释里。

function colWInit(src: string): string {
  const i = src.indexOf("const COL_W");
  if (i < 0) return "";
  const end = src.indexOf(";\n", src.indexOf("})(", i));
  return end < 0 ? src.slice(i, i + 200) : src.slice(i, end + 1);
}

describe("S25 ⑥ 容器查询取代写死宽度（件 5）", () => {
  const css = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/styles.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, " ");
  const est = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/height-estimate.ts"), "utf8");

  it("⑥a `.stream` 是名为 stream 的行内尺寸容器", () => {
    // ⚠ 前界用**后行断言**、不吃掉那个 `}`：相邻两条规则共用一个 `}`，吃掉它 ⇒ matchAll 每隔一条漏一条
    //   （`tests/evidence/P21-frontend-invariants.ts` 的 `cssRules` 头注记过同一个坑；本条第一版又踩了一次，
    //   `.stream` 恰好落在被漏掉的那一半里，当场「找不到」）。
    const rules = [...css.matchAll(/(?<=^|[{};])\s*([^{};@]+)\{([^{}]*)\}/g)]
      .map((m) => ({ sel: m[1], body: m[2] }))
      .filter((m) => m.sel.split(",").map((x) => x.trim()).includes(".stream"));
    expect(rules.length, "styles.css 里找不到 `.stream { … }` —— 下面那条对着空气").toBeGreaterThan(0);
    const decl = rules.map((m) => /(?:^|;)\s*container\s*:\s*([^;]+)/.exec(m.body)?.[1].trim()).find(Boolean);
    expect(decl, "`.stream` 没有声明 `container` —— 列宽没有容器可查").toBe("stream / inline-size");
    denom("⑥", rules.length, "条 `.stream` 规则（其中一条声明了 container: stream / inline-size）");
  });

  it("⑥b `COL_W` 是在那个容器里量出来的，不是写死的", () => {
    const init = colWInit(est);
    expect(init.length, "height-estimate.ts 里切不出 `const COL_W …` 那一段").toBeGreaterThan(100);
    expect(/const COL_W\s*(?::\s*number)?\s*=\s*\d+\s*;/.test(est), "`COL_W` 又写成了一个裸数字").toBe(false);
    for (const [what, re] of [
      ["挂进 #message-stream", /getElementById\("message-stream"\)/],
      ["建 .stream", /className\s*=\s*"stream"/],
      ["建 .stream-content", /className\s*=\s*"stream-content"/],
      ["读宽度", /getBoundingClientRect\(\)\.width/],
      ["量完撤掉", /\.remove\(\)/],
    ] as const) {
      expect(re.test(init), `COL_W 的初始化里缺「${what}」`).toBe(true);
    }
    denom("⑥", 5, "处实测要件（挂进消息流 · .stream · .stream-content · 读宽 · 撤掉）");
  });

  /**
   * ⑥d **行为**：上面三条只证明「代码长成了量的样子」，这一条证明「量出来的数真的进了估高」。
   * jsdom 没有布局 ⇒ 把 `getBoundingClientRect` 换成「`.stream-content` 宽 400」，重新求值模块，
   * 再对一张正文卡估高：必须按 400 折行，而不是 780。
   * 反空真：同一段文字按 400 与按 780 算出来的高度必须不同，否则本条对宽度不敏感、判不出任何东西。
   */
  it("⑥d 行为：列被压到 400px 时，估高按 400 折行（探针量完就撤）", async () => {
    const host = document.createElement("div");
    host.id = "message-stream";
    document.body.appendChild(host);
    const orig = Element.prototype.getBoundingClientRect;
    Element.prototype.getBoundingClientRect = function (this: Element): DOMRect {
      const w = this.classList.contains("stream-content") ? 400 : 0;
      return { width: w, height: 0, top: 0, left: 0, right: w, bottom: 0, x: 0, y: 0, toJSON: () => ({}) } as DOMRect;
    };
    try {
      vi.resetModules();
      const m = await import("../../../src/frontend/ui/height-estimate.ts");
      const text = "字".repeat(60) + "x".repeat(400);
      const card = document.createElement("div");
      card.className = "card card-assistant";
      card.innerHTML = `<div class="card-header">h</div><div class="card-body"><div class="block-text"><p>${text}</p></div></div>`;
      const LH = 15 * 1.65;
      const at400 = 22 + m.fallbackTextHeight(text, 15, LH, 400) + 10;
      const at780 = 22 + m.fallbackTextHeight(text, 15, LH, 780) + 10;
      expect(at400, "这段文字按 400 与 780 折行一样高 —— 本条对宽度不敏感").not.toBe(at780);
      expect(m.estimateStreamNodeHeight(card), "估高没用上量出来的列宽").toBeCloseTo(at400);
      expect(host.children.length, "探针没撤掉 —— 消息流里多了一个空的 .stream").toBe(0);
      denom("⑥", 400, "px 的假列宽被估高用上了（对照 780 高度不同）");
    } finally {
      Element.prototype.getBoundingClientRect = orig;
      host.remove();
      vi.resetModules();
    }
  });

  it("⑥c 量不到时的回退值 == tokens.css 的 --stream-max-width", () => {
    const fb = /\}\)\((\d+)\);\s*$/.exec(colWInit(est))?.[1];
    const tok = /--stream-max-width:\s*(\d+)px/.exec(readFileSync(resolve(REPO_ROOT, "src/frontend/ui/styles/tokens.css"), "utf8"))?.[1];
    expect(fb, "切不出 COL_W 的回退值").toBeTruthy();
    expect(tok, "tokens.css 里切不出 --stream-max-width").toBeTruthy();
    expect(Number(fb), "COL_W 的回退值与 --stream-max-width 漂了").toBe(Number(tok));
    denom("⑥", Number(fb), "px（COL_W 回退值 == --stream-max-width）");
  });
});
