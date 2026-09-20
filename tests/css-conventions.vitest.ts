/**
 * `S30` —— **CSS 三条约定接进判据**（`设计/40 §7` 步 9 ① · `设计/41 §12` 件 7 · 件 9）。
 *
 * 量具住 `tests/evidence/S30-css-conventions.ts`，**本文件只登记与判**（分家的理由写在那边）。
 * 读数住 `tests/evidence/S30-readings.md`。
 *
 * ## 与 `tests/css-ledger.vitest.ts`（`S25`）的分工 —— 不是第二本账
 *
 * | 文件 | 管哪几件**事实** |
 * |---|---|
 * | `tests/css-ledger.vitest.ts` | 类名的两个方向（CSS↔代码）· `z-index` 只许走刻度 · stylelint 报错总数 |
 * | `tests/app-grid-claims.vitest.ts` | `#app` 的每个 in-flow 直接子元素都认领了具名区域 |
 * | **本文件** | **自定义属性的两个方向** · **`transition` 动的属性** · **`hidden` 与 `display` 不许同框** |
 *
 * 三处互不重叠，同一件事实只有一个住址。
 *
 * ## 装了三格（＋ 一格量具自检）
 *
 * | 格 | 判什么 | 形态 | 出自 |
 * |---|---|---|---|
 * | ⓪ | 量具没坏（剥注释这一步在承重 · 人群没缩水） | 恒等 ＋ 地板 | —— |
 * | ⑤ | 自定义属性对账，**两个方向** | **恒等**（两向集合相等） | `设计/40 §7` 步 9 ① |
 * | ⑥ | `transition` 只许动白名单里那几个属性 | **恒等**（实测集合 == 白名单 ∪ 例外） | `设计/41 §10` · 件 9 |
 * | ⑦ | 会被 `hidden` 切的元素，CSS 里不许裸写 `display` | **恒等**（违例集合 == 登记表） | `设计/41 §7` 约定 1 · 件 7 |
 *
 * ## 🔴 为什么是 vitest 而不是 stylelint 插件 / 规则
 *
 * `设计/40 §7` 步 9 ① 逐字说「装插件 `stylelint-value-no-unknown-custom-properties`」，
 * `设计/41 §10` 逐字说「stylelint 的 `declaration-property-value-allowed-list` 可以拦」。
 * **两条都办不到，理由不是懒：**
 *
 * - 装插件要改 `package.json` ＋ `package-lock.json`；换 stylelint 规则要改
 *   `.stylelintrc.json`。**三份都不在本轮写区**（写区只有 `src/styles.css` ·
 *   `src/styles/` 下的 CSS · 新立的判据文件）。
 * - 而且那个插件**买不到本格 ⑤ 的反方向**：它只看「用了没定义」，
 *   看不见「定义了没人用」——而现打逮到的两条真缺陷恰好全在反方向（见 ⑤c）。
 * - `declaration-property-value-allowed-list` 同理只能拦**值**的形状，
 *   拦不住「例外名单自己腐了」（⑥ 那条「登记的例外必须今天真的还在」）。
 *
 * ⇒ 这不是「多一本账」，是**那件事实今天唯一的住址**。将来真装上插件，
 *   ⑤ 的正方向可以交给它，本文件那一条连同这段说明一起删。
 *
 * ## 在不在门禁的执行链上
 *
 * 在。`tests/scripts/gate.sh` 的 `npm` 那一格跑 `npm test`，链末是
 * `test:dom` ＝ `vitest run`，而 `vitest.config.ts` 的 `include` 是
 * `tests/**\/*.vitest.ts` ⇒ 本文件自动在里面。**不需要往 `gate.sh` 里加行**，
 * 加了反而成了第二处登记。现打验过：故意弄红本文件 ⇒ `npm test` 退出码非零。
 */
import { describe, it, expect } from "vitest";
import {
  cssFacts,
  displayVerdict,
  hiddenSites,
  setPropertyVars,
  themeTokens,
  type CssFacts,
  type HiddenSite,
} from "./evidence/S30-css-conventions.ts";
import { REPO_ROOT } from "./test-support/repo-root.ts";

const FACTS: CssFacts = cssFacts(REPO_ROOT);
const SET_PROP = setPropertyVars(REPO_ROOT);
const THEME = themeTokens(REPO_ROOT);
const HIDDEN: HiddenSite[] = hiddenSites(REPO_ROOT);

const sorted = (xs: Iterable<string>): string[] => [...xs].sort();
const minus = (a: Iterable<string>, b: ReadonlySet<string> | Map<string, unknown>): string[] =>
  sorted([...a].filter((x) => !b.has(x)));

/**
 * 🔴 **反空真的地板。** 主锚全是下面那几条**相等**断言；这些数只回答一个问题：
 * 「这一趟到底判过几条」。人群缩水（遍历坏了 / 词法错位 / 文件搬家）时相等断言会
 * **一起变空而依然相等** —— 那正是本仓反复吃亏的形状，所以每一格都另配一条地板。
 *
 * ⚠ 地板压在现打值下方留余量，**不要抬成快照** —— 抬成快照会让每一次正常改动
 * 都红在一个与被守性质无关的数上。
 */
const FLOORS = {
  /** `src` 下的 CSS 份数（现打 2：`styles.css` ＋ `styles/tokens.css`）。 */
  cssFiles: 2,
  /** CSS 里定义过的自定义属性个数（现打 79）。 */
  definedVars: 60,
  /** CSS 里 `var()` 到的自定义属性个数（现打 74）。 */
  usedVars: 55,
  /** `transition` / `transition-property` 声明条数（现打 17）。 */
  transitionDecls: 12,
  /** CSS 规则条数（现打 1023）。 */
  cssRules: 800,
  /** TS 里 `<元素>.hidden = …` 的处数（现打 33）。 */
  hiddenSites: 25,
  /** 那些处解析出来的类名个数（现打 19）。 */
  hiddenClasses: 14,
} as const;

// ───────────────────────── ⑤ 自定义属性对账（`设计/40 §7` 步 9 ①）─────────────────────────

/**
 * **CSS 定义了、但 CSS 里没有任何 `var()` 读它**的令牌。逐个登记，附理由。
 *
 * 为什么这一向也要判：`设计/40` 只写了正方向（「用了没定义」），而现打逮到的两条
 * 真缺陷全在**反方向** —— 正方向今天是干净的，只判它等于这一格永远绿。
 */
const UNUSED_TOKENS: Readonly<Record<string, string>> = {
  // 🔴 〔2026-09-19〕`--user` / `--assistant` 两条登记**摘掉了** —— 上一版这里逐字写着
  //    「裁完之后把这两行删掉，判据会提醒」。用户当天裁了：**撤掉那两个取色器**。
  //    ⚠ 摘登记的**同拍**把旋钮本身删了（`settings/panel.ts` 与 `theme.ts` 各一处，
  //      原处留了说明）。**只摘登记不删旋钮**会让下面那条「定义了没人用」的恒等断言
  //      当场红 —— 那正是本表存在的意义：它不许你只清账不清事。

  // 下面五条是 `设计/41 §2` 新补的两族里**今天还没有消费者**的部分。
  // 设计要的是「族补齐」（免得下一个人再去猜 `--input-bg` 这种名字），
  // 消费者要等到那些控件下一次被动到时才自然接上 ⇒ 预留是刻意的，不是腐。
  "--field-border": "表单族预留（`设计/41 §2` 族二）；今天输入框的边框还写着 --border-medium",
  "--field-border-focus": "表单族预留；今天 :focus 各处自己写 --accent",
  "--field-placeholder": "表单族预留；今天 ::placeholder 各处自己写 --text-faint",
  "--state-focus": "交互态族预留（`设计/41 §2` 族一）",
  "--state-disabled-opacity":
    "交互态族预留。`设计/41 §2` 要「把散在各处的 0.45/0.5/0.55 收成一个 token」，" +
    "而现打 `opacity: 0.x` 有 46 处、值域 0.35–0.95，**机械分不出哪些是「禁用/变灰」** " +
    "（水印、悬停压暗、幽灵态混在一起）⇒ 收编要逐条读语义，本轮判不了，不硬做",
} as const;

describe("S30 ⓪ 量具自检（这几条不过，下面三格全是空转）", () => {
  it("扫到的人群没有缩水", () => {
    expect(FACTS.cssFiles.length, `只扫到 ${FACTS.cssFiles.length} 份 CSS`).toBeGreaterThanOrEqual(
      FLOORS.cssFiles,
    );
    expect(FACTS.defined.size, "CSS 里一个自定义属性定义都没扫到 ⇒ 词法坏了").toBeGreaterThanOrEqual(
      FLOORS.definedVars,
    );
    expect(FACTS.used.size, "CSS 里一处 var() 都没扫到 ⇒ 词法坏了").toBeGreaterThanOrEqual(
      FLOORS.usedVars,
    );
    expect(FACTS.rules.length, "一条 CSS 规则都没扫到 ⇒ 分块坏了").toBeGreaterThanOrEqual(
      FLOORS.cssRules,
    );
    expect(FACTS.transitionDecls, "一条 transition 都没扫到").toBeGreaterThanOrEqual(
      FLOORS.transitionDecls,
    );
    expect(HIDDEN.length, "一处 `.hidden =` 都没扫到 ⇒ TS 侧词法坏了").toBeGreaterThanOrEqual(
      FLOORS.hiddenSites,
    );
  });

  it("🔴 正控：剥 CSS 注释这一步在承重（不剥的话散文会同时造假与灭真）", () => {
    // 这一格要证明的不是「剥了」，是「不剥就会给出**错的**答案」，而且两个方向都会错。
    //
    // ① 造假：`src/styles/tokens.css` 的头注逐字写着「z-index 只许写 `var(--z-*)`」——
    //    那是散文。不剥的话它变成一个叫 `--z-` 的「用了但从未定义」的变量 ⇒ ⑤① 假红。
    expect(
      FACTS.usedRaw.has("--z-"),
      "注释里那句「z-index 只许写 `var(--z-*)`」不见了 ⇒ 本条正控没了对象，把它与下一条一起重挑一个样本",
    ).toBe(true);
    expect(FACTS.used.has("--z-"), "`--z-` 混进了剥过注释的集合 ⇒ stripCssComments 坏了").toBe(false);

    // ② 灭真：`--user` / `--assistant` 今天**只在注释里**被 `var()` 提到（就是那段说明
    //    它们零消费者的文字）。不剥的话它们会被散文「救活」⇒ ⑤③④ 那条真缺陷静默消失。
    const dead = minus(FACTS.defined.keys(), FACTS.used);
    const deadIfNotStripped = new Set(minus(FACTS.defined.keys(), FACTS.usedRaw));
    expect(
      minus(dead, deadIfNotStripped),
      "「只有剥了注释才看得出是死令牌」的那一族变了 —— 它恰好就是下面 ⑤④ 盯的那两个旋钮。\n" +
        "两边一样 ⇒ 剥注释在这件事上不再承重，⑤ 的反方向可能正被散文蒙着。",
    // 🔴 〔2026-09-19〕这一族**空了** —— 那两个令牌连定义带旋钮一起撤了（用户裁定）。
    // ⚠ 空集让本条退化成「剥注释这件事在这儿不再承重」，判据自己上面那句话就是这么写的
    //   ⇒ 剥注释是否承重，改由 `strippedChars > 1000` 那条兜（它在下一行，没动）。
    ).toEqual([]);

    expect(FACTS.strippedChars, "一个注释字符都没剥掉").toBeGreaterThan(1000);
  });
});

describe("S30 ⑤ 自定义属性对账，两个方向（设计/40 §7 步 9 ①）", () => {
  it("① 正向：CSS 用到而 CSS 里没定义的，必须**恰好**是 TS 现设的那几个", () => {
    // 🔴 右边不是手写清单，是**现读 `style.setProperty("--x", …)` 的结果** ——
    // 手写一份就成了第二本账，而第二本账会腐。
    const fromCss = minus(FACTS.used.keys(), FACTS.defined);
    const fromTs = sorted(SET_PROP.keys());
    expect(
      fromCss,
      "CSS 里 `var(--x)` 到的名字，减掉 CSS 自己定义的，应当**恰好**等于 TS 侧 setProperty 设的那几个。\n" +
        `  CSS 侧多出来的：${minus(fromCss, new Set(fromTs)).join(" ") || "（无）"}\n` +
        `  TS 设了但 CSS 没人用：${minus(fromTs, new Set(fromCss)).join(" ") || "（无）"}\n` +
        "左边多 ⇒ 写错了变量名（那条声明会被浏览器整条丢弃）；右边多 ⇒ setProperty 设了个没人读的名字。",
    ).toEqual(fromTs);
    expect(fromTs.length, "TS 侧一个 setProperty 都没扫到 ⇒ 上面那条会变成「两边都空」的假绿").toBe(2);
  });

  it("② theme.ts 那 14 个旋钮，CSS 里必须都有默认值", () => {
    // 🔴 〔2026-09-19〕14 → **12**：`--user` / `--assistant` 两个取色器按用户裁定撤掉
    //    （它们是零消费者，拖了界面不动）。`设计/41 §8 ②` 那句「就是这 14 个」要跟着订正。
    //    ⚠ 这个数**刻意写死**：它挡的是「新增的两族偷偷混进旋钮」，不是「旋钮不许变」——
    //      真要加减旋钮，就该在这里被逼着改一次、被人看见一次。
    expect(THEME.length, "`设计/41 §8 ②`：旋钮就是这 12 个，新增的两族一个都不许进来").toBe(12);
    expect(
      minus(THEME, FACTS.defined),
      "这些旋钮 CSS 里没有默认值 ⇒ 用户没改过的时候它们是空的，整条属性会被丢弃",
    ).toEqual([]);
  });

  it("③ 反向：CSS 定义了却没人 var() 的，必须**恰好**是登记表里那几个", () => {
    const dead = minus(FACTS.defined.keys(), FACTS.used);
    const registered = sorted(Object.keys(UNUSED_TOKENS));
    expect(
      dead,
      "「定义了但全仓没有一条 CSS 读它」的令牌集合与登记表对不上。\n" +
        `  新冒出来的（要么接上消费者，要么进登记表并写清为什么留）：${minus(dead, new Set(registered)).join(" ") || "（无）"}\n` +
        `  登记了但今天已经有人用了（把那一行删掉）：${minus(registered, new Set(dead)).join(" ") || "（无）"}`,
    ).toEqual(registered);
  });

  it("🔴 ④ 正控：登记表里哪几条是「用户可调却零消费者」，要判得出来", () => {
    // 只断言「它在登记表里」是不够的 —— 那条在「把所有东西都登记一遍」时同样绿。
    // 这里断言的是**裁决**：死令牌里恰好这两个同时也是 theme.ts 的旋钮。
    const dead = new Set(minus(FACTS.defined.keys(), FACTS.used));
    const deadKnobs = sorted(THEME.filter((t) => dead.has(t)));
    expect(
      deadKnobs,
      "「既是用户可调旋钮、又没有任何 CSS 读它」的集合变了。\n" +
        "  🔴 〔2026-09-19〕这个集合今天**应当是空的** —— 原先那两个（`--user`/`--assistant`）\n" +
        "     按用户裁定连旋钮带令牌一起撤了。**再冒出一个就是新造了一个死旋钮**：\n" +
        "     用户能拖、界面不动，而且不会有任何别的东西报错。\n" +
        "这一族是用户能当场感觉到的那种坏：设置里拖了取色器，界面一动不动。\n" +
        "接上消费者之后把这两行从 UNUSED_TOKENS 里删掉，本条会跟着变。",
    ).toEqual([]);
  });
});

// ───────────────────────── ⑥ 动画属性白名单（`设计/41 §10` · 件 9）─────────────────────────

/** `设计/41 §10` 逐字：只许过渡这五个。它们都只走合成/重绘，不触发重排。 */
const TRANSITION_ALLOWED = ["background", "border-color", "color", "opacity", "transform"] as const;

/** 白名单之外、**逐条登记**的例外。登记表不许有死条目（见下面那条断言）。 */
const TRANSITION_EXCEPTIONS: Readonly<Record<string, string>> = {
  "grid-template-rows":
    "`设计/41 §10` 点名的已登记例外：`0fr ↔ 1fr` 是「不用预知内容高度就能平滑展开」的标准技巧，" +
    "每帧重排但只在展开那 200ms。要动尺寸时的另一条路是 transform: scale。",
  none: "`transition: none` 是**关掉**过渡，不是动某个属性 —— 它恰恰是白名单想要的方向。",
} as const;

describe("S30 ⑥ transition 只许动白名单里那几个属性（设计/41 §10 · 件 9）", () => {
  it("实测到的属性集合 == 白名单 ∪ 登记的例外", () => {
    const seen = sorted(new Set(FACTS.transitions.map((t) => t.prop)));
    const allowed = sorted([...TRANSITION_ALLOWED, ...Object.keys(TRANSITION_EXCEPTIONS)]);
    const offenders = FACTS.transitions.filter(
      (t) => !allowed.includes(t.prop),
    );
    expect(
      offenders.map((t) => `${t.file}:${t.line} ${t.decl}`),
      "这些属性不在白名单里。动尺寸/位置类属性会**每帧重排**：\n" +
        "  · 要动尺寸 ⇒ 用 `transform: scale`（`.sftp-bar-fill` 那条 `width` 就是这么治好的，S24）\n" +
        "  · 真有第三条路 ⇒ 写进 TRANSITION_EXCEPTIONS，附住址与理由",
    ).toEqual([]);
    expect(
      seen.length,
      `只扫到 ${seen.length} 种被过渡的属性（现打 7）—— 值的切分坏了，上面那条会零命中地绿`,
    ).toBeGreaterThanOrEqual(5);
  });

  it("登记的例外今天必须真的还在（不许留死条目）", () => {
    const seen = new Set(FACTS.transitions.map((t) => t.prop));
    expect(
      sorted(Object.keys(TRANSITION_EXCEPTIONS).filter((k) => !seen.has(k))),
      "这些例外登记着，但今天 CSS 里一处都没有了 ⇒ 把那一行删掉。\n" +
        "留着的代价是：下次真有人写回来，这条判据会默默放行。",
    ).toEqual([]);
  });

  it("分母：判过的 transition 条数与属性处数", () => {
    expect(FACTS.transitionDecls).toBeGreaterThanOrEqual(FLOORS.transitionDecls);
    expect(
      FACTS.transitions.length,
      "一条 transition 声明都没解析出属性名 ⇒ 上面两条会零命中地绿",
    ).toBeGreaterThan(FACTS.transitionDecls - 1);
  });
});

// ───────────────── ⑦ `hidden` 与 `display` 不许同框（`设计/41 §7` 约定 1 · 件 7）─────────────────

/**
 * **静态解析不出类名的 `.hidden =` 处**，逐处登记。
 *
 * 为什么要登记而不是跳过：跳过 ＝「扫不到就绿」。这三处今天真的推不出来，
 * 但**多一处少一处都要有人看见** —— 多出来的多半是词法器又错位了。
 */
const HIDDEN_UNRESOLVED: Readonly<Record<string, string>> = {
  "src/error-toast.ts:135":
    "`existing.countEl` —— `existing` 是从一张 Map 里取回来的旧 toast，它的 countEl 在别处建的",
  // ⚠ 〔2026-09-19〕`606 → 614`：我在这份文件上方加了一段注释，**行号就漂了**。
  //    这条登记按**裸行号**做键 —— 那是它的固有脆弱：住址没变、内容没变，只因为
  //    上面多了几行就要来改一次。`设计/16 §5.4b` 纪律 4 说的是同一件事
  //    （「按位置认的针」在搬动面前是结构性盲区）。
  //    ⇒ 本条**不改成按内容认**：那要重写抽取器的键，跨出本轮射程；
  //      如实记在这里，等哪一轮真动这张表时一并收。
  // ⚠ 〔步 20 · 2026-09-19〕`614 → 659`：又漂了一次，原因同上（本轮在 `panel.ts`
  //    上游加了字段与注释）。**住址没变、内容没变**，只是行号跟着挪。
  "src/settings/panel.ts:659":
    "`b.el` —— `b` 来自 `this.perMachineBlocks` 数组，元素由各 section 自己建，跨文件",
  // 🔴 〔步 20 · `设计/70 §1.3 C`〕兜底态那块提示的显隐。它的类名是
  //    `skeleton.ts::makeSkeleton` 挂上去的（`settings-hint`），**跨文件** ——
  //    这把尺子刻意只走两跳、不跨文件，所以推不出来。
  //    ⚠ 顺带说清它安不安全：`settings-hint` 在 `src/styles.css` 里**没有 display 规则**
  //      ⇒ UA 的 `[hidden] { display: none }` 不会被压过，那两句不是空写。
  //      这一条是**人工核过的**，不是这把尺子判的 —— 所以它在登记表里，不在绿里。
  "src/settings/panel.ts:1082":
    "`this.perMachineFallbackHint` —— 类名由 `skeleton.ts::makeSkeleton` 挂，跨文件",
  "src/settings/panel.ts:1089":
    "`this.perMachineFallbackHint` —— 同上（兜底态亮出来那一支）",
  // 🔴 〔步 20 · `设计/70 §10.1`〕「足迹」那一块里，本机那一整套的显隐包装。
  //    它**刻意不挂任何类**：只负责显隐、不要样式。挂了类就得在 CSS 里给它写规则
  //    （`css-ledger` 的两条棘轮会要求），而那条规则会是一句纯装饰。
  //    ⇒ 没有类 ⇒ 不可能有「自己身上的裸 display」⇒ 这一格在构造上就是安全的。
  "src/settings/config-surface-section.ts:384":
    "本机那一套的显隐包装，刻意不挂类名（没有类就不会有裸 display 压过 [hidden]）",
  "src/views/history.ts:1638":
    "`e.hidden = updated.hidden` —— 这一处根本不是「切某个组件的显隐」，是在把一条会话记录的 `hidden` 字段往回写",
} as const;

/**
 * **已知违例**：会被 `hidden` 切、而 CSS 又在它自己身上裸写了 `display`。
 *
 * 作者样式里的 `display` 压过 UA 的 `[hidden] { display: none }` ⇒ 那句 `hidden` 是空写。
 */
// 🔴 〔步 17·A · 2026-09-19〕**这张表清空了。**
//   唯一那条 `tab-archive-list` 摘掉 —— **不是修好了，是那个功能整个不存在了**：
//   归档抽屉随用户裁定删除（逐字「没有归档这个东西，不要归档，就是灰 tab。
//   现在的归档是错误的，甚至是 bug 的来源，全部删掉」），
//   而 `设计/30 §A` 抬头本来就写着「已定：删归档抽屉」。
//   ⚠ 那条缺陷（`hidden` 是空写、箭头在骗人）**没有被修，是随载体一起消失的** ——
//     两者在「表上少一行」这件事上长得一模一样，所以写清楚。
//   ⚠ 空表**不等于这格判据没用**：下面那条仍是**恒等**（盘上现打的违例集合 == 本表），
//     再冒出一个同形的当场红。空的是人群，不是判据。
const HIDDEN_KNOWN_BAD: Readonly<Record<string, string>> = {} as const;

describe("S30 ⑦ 会被 hidden 切的元素，CSS 不许在它身上裸写 display（设计/41 §7 约定 1）", () => {
  const resolved = HIDDEN.filter((h) => h.classes !== null);
  const classes = sorted(new Set(resolved.flatMap((h) => h.classes ?? [])));
  const verdicts = new Map(classes.map((c) => [c, displayVerdict(FACTS, c)]));

  it("分母：判过的 `.hidden =` 处数与类名个数", () => {
    expect(HIDDEN.length).toBeGreaterThanOrEqual(FLOORS.hiddenSites);
    expect(
      classes.length,
      "一个类名都没解析出来 ⇒ 下面两条会零命中地绿",
    ).toBeGreaterThanOrEqual(FLOORS.hiddenClasses);
  });

  it("解析不出类名的那几处 == 登记表（不许静默跳过）", () => {
    const unresolved = sorted(HIDDEN.filter((h) => h.classes === null).map((h) => `${h.file}:${h.line}`));
    expect(
      unresolved,
      "「静态推不出它挂的是哪个类」的处数变了。\n" +
        "  多出来的：要么那一处真的推不出（登记进 HIDDEN_UNRESOLVED，写清为什么）、\n" +
        "  要么词法器又错位了（第一版就因为只比最后一个标识符，把 `b.el` 认成了 `this.el`）。",
    ).toEqual(sorted(Object.keys(HIDDEN_UNRESOLVED)));
  });

  it("裸写 display 的类 == 已知违例表", () => {
    const bad = sorted(classes.filter((c) => {
      const v = verdicts.get(c);
      return v !== undefined && v.bare.length > 0 && !v.guarded;
    }));
    const detail = bad
      .map((c) => `  .${c} ⇐ ${(verdicts.get(c)?.bare ?? []).join(" | ")}`)
      .join("\n");
    expect(
      bad,
      "这些元素会被 TS 用 `hidden` 切，而 CSS 又在它们**自己身上**写了 `display` ——\n" +
        "作者样式里的任何一条 `display` 都会压过 UA 的 `[hidden] { display: none }`，\n" +
        "那句 `x.hidden = …` 于是成了空写：**看起来在切，其实从来没切过**。\n" +
        "  修法：补一条 `.<类名>[hidden] { display: none; }`（`.tab-archive` 就是这么修的）。\n" +
        `现打：\n${detail}`,
    ).toEqual(sorted(Object.keys(HIDDEN_KNOWN_BAD)));
  });

  // 🔴 〔步 17·A · 2026-09-19〕**正控从活体换成合成夹具 —— 今天第三次栽在同一课上。**
  //
  // 上一版锚在 `.tab-archive` 上，理由逐字「它是唯一一个**两边都命中**的活样本」。
  // 归档抽屉整个删掉之后，现打：全仓「既有裸 `display` 又有 `[hidden]{display:none}`」
  // 的类 **0 个** ⇒ **正控没有对象了**，而上一版自己的报错里就预言了这句话。
  //
  // ⇒ 换成**合成的 `CssFacts`**：正控不依赖盘上任何一个类，谁也删不掉它。
  //   ⚠ 「挑一个活着的、看起来不会改的样本」这条路本身是错的 —— 每轮都在赌，每轮都输。
  //
  // ⚠ 顺带**变强了**：上一版只有正控（证明「有兜底」不恒真）。这一版正反两控 ——
  //   还证明了「没兜底」也不恒真，否则一把把所有东西都判成「没兜底」的尺子同样过得了。
  it("🔴 正反两控：`displayVerdict` 对合成样本判得出「有兜底」与「没兜底」两种", () => {
    const synth = (rules: { sel: string; body: string }[]): CssFacts => ({
      root: "<合成>",
      cssFiles: [],
      strippedChars: 0,
      defined: new Map(),
      used: new Map(),
      usedRaw: new Map(),
      transitions: [],
      transitionDecls: 0,
      rules: rules.map((r, k) => ({ file: "<合成>", line: k + 1, ...r })),
    });
    const ok = displayVerdict(
      synth([
        { sel: ".synthetic-guarded", body: "display: flex;" },
        { sel: ".synthetic-guarded[hidden]", body: "display: none;" },
      ]),
      "synthetic-guarded",
    );
    expect(ok.bare.length, "裸 display 一条都没认出来 ⇒ 尺子瞎了，上面那格会假绿").toBe(1);
    expect(ok.guarded, "有 `[hidden]{display:none}` 却判成没兜底 ⇒ 会把好的报成坏的").toBe(true);
    const bad = displayVerdict(
      synth([{ sel: ".synthetic-bare", body: "display: flex;" }]),
      "synthetic-bare",
    );
    expect(bad.bare.length, "这一条就是「空写 hidden」的形状，认不出来那格判据没有意义").toBe(1);
    expect(bad.guarded, "没有任何 `[hidden]` 规则却判成有兜底 ⇒ **上面那格恒绿**").toBe(false);
  });

  it("已知违例表不许有死条目（修好了就把登记删掉）", () => {
    const stillBad = new Set(
      classes.filter((c) => {
        const v = verdicts.get(c);
        return v !== undefined && v.bare.length > 0 && !v.guarded;
      }),
    );
    expect(
      sorted(Object.keys(HIDDEN_KNOWN_BAD).filter((k) => !stillBad.has(k))),
      "这些登记成「已知违例」的类今天已经不违例了 ⇒ 把那一行删掉",
    ).toEqual([]);
  });
});
