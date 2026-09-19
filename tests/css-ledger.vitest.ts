/**
 * `S25` —— **把 CSS 的对账接进判据**（`设计/40 §7` 步 9 · `设计/41 §12` 件 8）。
 *
 * ## 这条为什么存在
 *
 * `设计/40 §0` 逐字：「**这 7248 行里没有任何一条机器检查。** 而这个仓有 44,166 行护栏代码
 * 专门做各种对账 —— **唯一没有对账的地方，恰好是 bug 最密的地方。**」
 * 四类问题全是现打可复现的：未定义变量 16 个、死规则 13 个、z-index 18 个量级无刻度、
 * 布局根没人对账。`真相源/71` 那个 bug 与 `设计/10 步 2` 那个浮层压按钮，都是从这儿长出来的。
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
 * | ④ | `npx stylelint` 的报错总数 | **递减棘轮** | 被 lint 的 CSS 文件份数 |
 *
 * 🔴 **本文件不装第五格（`#app` 布局根，`设计/40 §7` 步 9 ③）—— 它有自己的家。**
 * 本轮落地时它红在一个真缺陷上（`src/tabs.ts` 的 `ensureArchiveUi()` 往 `#app` 插的
 * `.tab-archive` 是个没认领格子的 grid item，偷走消息流 35px，`S21 §6 ④` 有真引擎读数），
 * 而 `src/tabs.ts` / `src/styles.css` 都不在本轮写区。
 * **同一棵树上另一路（`S24`）把缺陷修了、并把那一格装成了 `tests/app-grid-claims.vitest.ts`** ——
 * 而且形态比 `设计/40` 的字面写法更对：钉的不是「子元素 == 那三个」（`.tab-archive` 在步 17
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
 * 这里仍然选棘轮，理由是**并发**：本条落地的同时另有一路在改 `src/styles.css`
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
 * - **`设计/40 步 9 ①`（变量对账）没装**：它要 `npm i -D stylelint-value-no-unknown-custom-properties`，
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
import { describe, it, expect } from "vitest";
import {
  buildLedger,
  cssClassesOf,
  explain,
  scanStrings,
  stripCodeComments,
  zIndexDeclsOf,
  type Ledger,
} from "./evidence/S25-class-ledger.ts";
import { REPO_ROOT } from "./test-support/repo-root.ts";

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
  /** `src` 下的 CSS 文件份数（现打 2：`styles.css` ＋ `styles/tokens.css`）。 */
  cssFiles: 2,
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
 * ★ **准拿来解释类名的模板前缀**（`设计/40 §7` 步 9 ② 那份白名单的落点）。
 *
 * 🔴 **为什么这张表是手写的，而候选是机器派生的**：
 * 量具会把**每一个**「模板里紧挨着 `${}` 的尾 token」都记成候选（现打 30 个）。
 * 里面有垃圾 —— `src/tab-collections.ts:46` 的 `` `c${Date.now().toString(36)}…` ``
 * 派生出一个 1 字符前缀 `c`，它能一口气"解释"掉 `code-bar` / `code-lang` / `code-copy` /
 * `ccm-alias-gen*` 共 7 个类，**而那 7 个其实各有真住址**（`src/render.ts:77` 等）。
 * 一个越宽的前缀能掩掉越多真死规则 ⇒ **白名单必须是人写的，且每条要有理由**。
 *
 * 🔴 **与上一轮那份「11 个前缀」清单的关系（这是本轮最要紧的一条读数）**：
 * `真相源/51 §2` 登记了 11 个前缀，`S21` 三处说「今天的 11 个不够，至少还要加
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
 * `paste-block` 不在这里，理由见 `KNOWN_DEAD`）。**每条都带住址与理由，缺一条本表就不该有它。**
 *
 * ⚠ 本表有两条自检（见「登记表不许有死条目」那一格）：
 * ① 每条前缀今天仍要在代码里派生得出来（候选表里有）；
 * ② 每条前缀今天至少要真解释掉一个 CSS 类 —— 解释不到任何东西的前缀是死条目，
 *    留着只会在下一次悄悄掩掉一个真死规则。
 */
const ALLOWED_PREFIXES: readonly { prefix: string; why: string }[] = [
  {
    prefix: "acct-c",
    why: "`src/account-color.ts:38` 按账号序号生成 8 个配色位（`acct-c0`…`acct-c7`）。序号是运行时算的，写不出字面量。",
  },
  {
    prefix: "agent-",
    why: "`src/agents-panel.ts:139` 把 agent 的运行态直接拼成类名（running/done/aborted）。态值来自后端。",
  },
  {
    prefix: "block-diff-",
    why: "`src/cards/diff.ts:311` 按 diff 行的增删拼 `block-diff-add` / `-del`。",
  },
  {
    prefix: "conf-",
    why: "`src/views/panorama.ts:1201` 把全景节点的置信档拼成类名（exact/heuristic/dynamicguess），档位来自 code-picture 的返回。",
  },
  {
    prefix: "kind-",
    why: "`src/settings/data-section.ts:188` 按条目种类拼（dir/file/user）。",
  },
  {
    prefix: "pf-dot-",
    why: "`src/views/port-forward.ts:142` 按端口转发的健康状态拼（ok/err）。",
  },
  {
    prefix: "remote-gap-",
    why: "`src/settings/remote-section.ts:391` 按远端能力缺口的成因拼（missing/unknown）。",
  },
  {
    prefix: "remote-status-",
    why: "`src/settings/remote-section.ts:163` 按远端探测结果拼（ok/fail/na/unknown）。",
  },
  {
    prefix: "status-",
    why: "`src/tasks-panel.ts:177` 把任务态拼成类名（running/aborted/completed/in_progress/deleted）。态值直接来自会话记录，是**协议里的字符串**，前端不重新枚举。",
  },
  {
    prefix: "tone-",
    why: "`src/settings/config-surface-section.ts:333` 按配置面的判定色调拼（ok/bad/unknown）。",
  },
];

/**
 * ★ **已登记的死规则** —— CSS 里写着、今天确实没有任何人挂它。
 *
 * 登记在这儿不是赦免，是**把它变成一条会红的欠账**：修掉（删那几行 CSS）之后
 * 这张表里的条目会变成死条目，下面那条自检会红并叫人来删登记。
 *
 * ⚠ 本轮**不能自己修** —— `src/styles.css` 在明令禁碰之列（另有一路正在改它）。
 */
const KNOWN_DEAD: readonly { name: string; why: string }[] = [
  {
    name: "paste-block-warning",
    why:
      "`src/paste-block.ts:25-31` 的模块头逐字写着：「原先这里有个 `warning` 槽，T03 审计后**移回消费者自己那儿**」——" +
      "槽拆了，`.paste-block-warning`（`src/styles.css:6748`）留在原地。" +
      "现打三把尺子同口径：`src/**` 零引用 · `tests/**` 零引用 · 生产 bundle `.build/dist/assets/*.js` 零命中" +
      "（同一把 bundle 尺子对活类 `paste-block-copy` 读出 1，所以它不是一把永远说 0 的坏尺子）。" +
      "⇒ 真死规则，交给 `src/styles.css` 的所有者删。删完把本条一起删掉。",
  },
];

/**
 * ★ **`npx stylelint "src/**\/*.css"` 的报错总数上限**。现打 **47**（`S21 §5` 同数）。
 *
 * **只许降。** 47 这个数本身不是目标（其中 43 条 `--fix` 就能自动改），
 * 本格买的是「它不许悄悄变大」—— 在这之前这个数**全仓无人机检**：
 * `tests/eslint-baseline.vitest.ts` 的头注逐字登记过「本条不管 stylelint，登记在此，不假装覆盖了」，
 * 而 `ci.yml` 那一步是 `npm run lint:css || true`（结构上不会红）。本格接的就是那半格。
 */
const STYLELINT_CEILING = 47;

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
 * - 一部分是**真悬空** —— 比如 `.settings-btn-secondary`（`src/launcher-diagnostics.ts:693,698`
 *   两处在挂它）与 `.cc-bus-online-unknown`，CSS 里一条规则都没有，
 *   而同族的 `.settings-btn` / `.cc-bus-online` 都styled ⇒ 多半是改名只改了一边。
 *
 * ⇒ 本格**不区分这两者**（机械上区分不了：「钩子」与「忘了写样式」在语法上一模一样），
 * 它买的只有一件事：**这个数不许再涨**。涨了就说明又多了一个挂着却没规则的类名，
 * 那时要么补样式、要么改成 `data-*` 钩子（`设计/41 §7` 的状态表达约定正是这条）。
 *
 * ⚠ 它也会在**另一个方向**红：有人删掉了一条 CSS 规则而代码还在挂那个类。那种红是对的。
 */
const DANGLING_CEILING = 163;

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
    // ① 嵌套模板 —— `src/paste-block.ts:90` 的真形状
    const nested = 'x.className = `paste-block${spec.className ? ` ${spec.className}` : ""}`;';
    const t1 = scanStrings(nested).filter((t) => t.kind === "tpl");
    expect(t1.some((t) => t.kind === "tpl" && t.chunks[0]?.text === "paste-block" && t.chunks[0].hole)).toBe(true);

    // ② 正则字面量里的引号 —— `src/launcher-diagnostics.ts:149` 的真形状。
    //    词法器若把 `/'/g` 里的引号当字符串开头，整份文件从此错位，后面的 className 全读不到。
    const rx = ["const q = (s) => `'${s.replace(/'/g, `'\\''`)}'`;", 'w.className = "ccm-alias-gen";'].join("\n");
    const got = scanStrings(rx).filter((t) => t.kind === "str" && t.value === "ccm-alias-gen");
    expect(got.length, "正则字面量把词法带错位了 —— 它后面的类名会平白变成死规则").toBe(1);

    // ③ HTML 片段里的 class 属性 —— `src/render.ts:77` 的真形状
    const html = "const s = `<div class=\"code-bar\"><span class=\"code-lang\"></span></div>`;";
    const chunks = scanStrings(html).flatMap((t) => (t.kind === "tpl" ? t.chunks.map((c) => c.text) : []));
    expect(chunks.join("")).toContain('class="code-bar"');

    // ④ 死值验：注释里的类名不许当成代码在用它（`src/render.ts:49` 那条 JSDoc 的真形状）
    const commented = ['/** `<code class="language-X">` 只是散文 */', 'e.className = "real-one";'].join("\n");
    expect(stripCodeComments(commented)).not.toContain("language-X");
    expect(stripCodeComments(commented)).toContain("real-one");

    denom("⓪", 4, "种真实形状（嵌套模板 · 正则里的引号 · HTML class 属性 · 注释不算用户）");
  });
});

describe("S25 ① z-index 只许写 var(--z-*)（设计/40 步 9 ④ · 设计/41 件 4）", () => {
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
        "\n★ 十档刻度定义在 `src/styles/tokens.css`（`--z-base` … `--z-drag`），逐条判语义的结果在\n" +
        "  `tests/evidence/S21-css-readings.md §3`。**别机械按数值映射** —— `设计/40 步 6` 逐字要求逐条判。\n" +
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

describe("S25 ② CSS 里的类名有人用（设计/40 步 9 ②）", () => {
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
        "  ① 新挂了一个没有样式的类名 ⇒ 补样式，或按 `设计/41 §7` 改成 `data-*` 钩子；\n" +
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

    const warnings = linted.flatMap((r) => r.warnings);
    const byRule = new Map<string, number>();
    for (const w of warnings) byRule.set(w.rule, (byRule.get(w.rule) ?? 0) + 1);
    const top = [...byRule].sort((a, b) => b[1] - a[1]).map(([r, n]) => `${r}×${n}`);

    expect(
      warnings.length,
      `stylelint 报错 ${warnings.length} 条 > 棘轮上限 ${STYLELINT_CEILING}（现打基线 47，\`S21 §5\` 同数）。\n` +
        "★ 这是**递减棘轮**：只许降。修好了就把 `STYLELINT_CEILING` 一起调下来，别只改代码不棘紧。\n" +
        "★ 为什么这里是棘轮不是等号：本条落地时另有一路在改 `src/styles.css`，等号在并发下互相打架。\n" +
        "  代价（拦不住判据空转）由上面那两条反空真接着。理由全文见本文件头注。\n" +
        `逐规则：\n  ${top.join("\n  ")}`,
    ).toBeLessThanOrEqual(STYLELINT_CEILING);

    // 本格新开的那条规则**必须真的在跑**：它今天该是 0 命中，但「规则没开」与「0 命中」
    // 在报错总数上一模一样 ⇒ 上面那条 `.stylelintrc.json` 的对拍是它的另一条腿。
    expect(
      byRule.get("declaration-property-value-allowed-list") ?? 0,
      "z-index 白名单在真 stylelint 下报出了命中 —— 与格 ① 的读数矛盾，两把尺子有一把坏了",
    ).toBe(0);

    denom("④", linted.length, `份 CSS 文件（报错 ${warnings.length}/${STYLELINT_CEILING}，棘轮只许降）`);
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
