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
 * | ⑧ | 同一个状态名不许同现于类名与 `data-*` 两种载体（名字形 ＋ 同一处写两遍形） | **恒等**（违例集合 == 登记表，今天空）＋ 正反两控 | `设计/41 §7` 一般形式〔W5-AUX〕 |
 * | ⑨ | 活规则里的死声明（同选择器 · 同条件 · 被级联里赢的那一条整条盖掉） | **恒等**（死声明集合 == 登记表，今天空）＋ 正反两控 | `设计/40 §8`〔W5-AUX〕 |
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
  cssDeclarations,
  cssDeclsOf,
  deadDeclarations,
  layerOrder,
  stateCarriers,
  stateCarriersOfCss,
  stateCarriersOfTs,
  stateNameCollisions,
  themeTokens,
  windowSheets,
  type CssDecl,
  type CssFacts,
  type HiddenSite,
  type StateCarriers,
} from "./evidence/S30-css-conventions.ts";
import { stripCodeComments } from "./evidence/S25-class-ledger.ts";
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
  "--state-hover":
    "交互态族（`设计/41 §2` 族一）。〔F7b 09-24〕今天的两个消费者（老 SFTP 面板的按钮与行悬停）" +
    "随面板整段 CSS 退役；族不拆 —— 同 `--state-focus`，等下一个悬停态自然接上",
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
  // 〔SE2〕查找面板按模式切大纲清单的开合（大纲那一半的 `hidden` 从 `UserInputPanel` 自己手里交给了面板）。
  "src/views/session-find.ts:237": // 〔C4b〕行号 −1：两条类型 import 并成一条（`../session-reads`）·〔CP2b〕+1：加了 copyText 的 import
    "`this.outline.panel` —— 大纲清单那块，由 `UserInputPanel` 建（类 `.user-inputs`；`styles.css` 里那条规则头注逐字「绝不许出现 display」）",
  "src/error-toast.ts:136": // 〔CP2b〕+1：加了 copyText 的 import
    "`existing.countEl` —— `existing` 是从一张 Map 里取回来的旧 toast，它的 countEl 在别处建的",
  // ⚠ 〔2026-09-19〕`606 → 614`：我在这份文件上方加了一段注释，**行号就漂了**。
  //    这条登记按**裸行号**做键 —— 那是它的固有脆弱：住址没变、内容没变，只因为
  //    上面多了几行就要来改一次。`设计/16 §5.4b` 纪律 4 说的是同一件事
  //    （「按位置认的针」在搬动面前是结构性盲区）。
  //    ⇒ 本条**不改成按内容认**：那要重写抽取器的键，跨出本轮射程；
  //      如实记在这里，等哪一轮真动这张表时一并收。
  // ⚠ 〔步 20 · 2026-09-19〕`614 → 662`：又漂了一次，原因同上（本轮在 `panel.ts`
  //    上游加了字段与注释）。**住址没变、内容没变**，只是行号跟着挪。
  // ⚠ 〔`P12` · 2026-09-21〕`662 → 668` / `1085 → 1091` / `1092 → 1098`：**又漂了一次**，
  //    原因同上 —— 本轮在 `panel.ts` 上游加了一行 import ＋ 建面板时多挂了一条常驻条
  //    （「配置里有不认识的键」那条）。**住址没变、内容没变**，只是行号跟着挪；
  //    三处逐处现打核过（`b.el` / 两处 `perMachineFallbackHint`），语义一字未动。
  // ⚠ 〔ST1 · 2026-09-24〕`668 → 791` / `1091 → 1250` / `1098 → 1259`：**又漂了一次**，原因同上 ——
  //    本轮在 `panel.ts` 上游加了延后加载（`loadableBlock`）与关窗接管（`installWindowLifecycle` /
  //    拦截条）。三处逐处现打核过：仍是 `b.el` 与两处 `perMachineFallbackHint`，语义一字未动。
  // ⚠ 〔AL1 合并 · 2026-09-24〕`791 → 788` / `1250 → 1259` / `1259 → 1268`：**又漂了一次**，原因同上 ——
  //    AL1 在 `panel.ts` 上游把 import 收成一行、per-machine 表里多挂「别名」一块、删「行为」组里那段别名挂载。
  //    三处逐处现打核过：仍是 `b.el` 与两处 `perMachineFallbackHint`，语义一字未动。
  // ⚠ 〔第四波 ST2 · 2026-09-24〕又漂了（多拍）：步 14 / 删顶层「改动足迹」/ 步 15 在 `panel.ts` 上游
  //    改了 buildBody（后端四格寄居、应用下挂三个子页、漂移记账那块）。住址与语义一字未动，只是行号跟着挪；
  //    三处照旧由脚本按「`b.el.hidden =` / 两处 `perMachineFallbackHint.hidden =`」现打。
  // 〔合并 C4a〕这一批行号随 C4a 在同文件里加的 import（origin 判定那一行）各挪一两行，住址与语义一字未动。
  // 〔AL1c · 4B〕`802 → 792` / `1306 → 1288` / `1319 → 1301`：又漂了一次 ——「终端集成」一块并进「别名」，
  //    `panel.ts` 上游删了它的 import / OS 门常量 / 挂载那几行与「不适用」替身。三处照旧是 `b.el` 与两处
  //    `perMachineFallbackHint`，语义一字未动。
  // 〔AS2 · 4B〕`792 → 793` / `1288 → 1297` / `1301 → 1310`：`panel.ts` 多了资产目录那一行 import 与那一块登记，三处语义一字未动。
  // 〔W5-UI ＋ CFG1 合并〕W5-UI 那三处 +4 与 CFG1 +3 叠加（`panel.ts` 785 → 792 · 1289 → 1296 · 1302 → 1309；`history.ts` 1612 → 1616），语义一字未动。原注：〔W5-UI〕`785 → 789` / `1289 → 1293` / `1302 → 1306`：`panel.ts` 多一行 import（应用内对话框）＋ 选目录失败那处 catch 多三行出声，三处语义一字未动。
  "src/settings/panel.ts:820": // 〔CP2b〕同上 · 〔LR2〕−7：逃生口 forceLaunchPayloadRenderer 那段缓存字段删了 · 〔FIX2〕+35：openInner 拆出两个私有方法
    "`b.el` —— `b` 来自 `this.perMachineBlocks` 数组，元素由各 section 自己建，跨文件",
  // 🔴 〔步 20 · `设计/70 §1.3 C`〕兜底态那块提示的显隐。它的类名是
  //    `skeleton.ts::makeSkeleton` 挂上去的（`settings-hint`），**跨文件** ——
  //    这把尺子刻意只走两跳、不跨文件，所以推不出来。
  //    ⚠ 顺带说清它安不安全：`settings-hint` 在 `src/styles.css` 里**没有 display 规则**
  //      ⇒ UA 的 `[hidden] { display: none }` 不会被压过，那两句不是空写。
  //      这一条是**人工核过的**，不是这把尺子判的 —— 所以它在登记表里，不在绿里。
  "src/settings/panel.ts:1325": // 〔CP2b〕行号 −6：字面量进表后几段多行拼接收成一行 · 〔LR2〕再 −7（同上） · 〔AL2〕+1（别名管理器多传一行 origin） · 〔FIX2〕+35
    "`this.perMachineFallbackHint` —— 类名由 `skeleton.ts::makeSkeleton` 挂，跨文件",
  "src/settings/panel.ts:1338": // 〔CP2b〕同上 · 〔LR2〕−7 · 〔AL2〕+1 · 〔FIX2〕+35
    "`this.perMachineFallbackHint` —— 同上（兜底态亮出来那一支）",
  // 🔴 〔步 20 · `设计/70 §10.1`〕「足迹」那一块里，本机那一整套的显隐包装。
  //    它**刻意不挂任何类**：只负责显隐、不要样式。挂了类就得在 CSS 里给它写规则
  //    （`css-ledger` 的两条棘轮会要求），而那条规则会是一句纯装饰。
  //    ⇒ 没有类 ⇒ 不可能有「自己身上的裸 display」⇒ 这一格在构造上就是安全的。
  // 〔第四波 ST2〕漂移记账按机器分（这一拍只做本机）那两处（本机那一整套的包装 · 远端那一句）
  //    〔ST3〕随账按机器分一起退场：本机与远端同一套 DOM，这一块不再切任何显隐 ⇒ 两行删掉。
  // 〔第四波 ST2〕远端也有真栏之后，这个包装本机与远端都用；显隐切两处：`applyOriginGate`（摆出来）
  //    与 `showUnanswered`（远端那台答不了时收起来）。同一个包装、同一个理由。
  // 〔合并 RM1a〕440/447 → 436/443：`readFootprint` 去掉那一道 `as unknown as`（命令签名本来就收 `{ origin }`），上移 4 行。
  // 〔OSA〕436/443 → 437/444：上方多一行 import（`$PROFILE` 备份改问本机后端），那一处本身没动。
  "src/settings/config-surface-section.ts:415": // 〔CP2b〕行号 −1：字面量进表后收行 ·〔TL3〕+1：`answersFor` 回声那一行上面加了一行注释
    "那一整套的显隐包装（本机与远端都用），刻意不挂类名（没有类就不会有裸 display 压过 [hidden]）",
  "src/settings/config-surface-section.ts:422": // 〔CP2b〕同上 ·〔TL3〕同上 +1
    "同一个包装，远端那台答不了时收起来（`showUnanswered`）",
  // 〔C4d〕行号随上方历史清单那几段改走通道挪了（1635 → 1617），那一处本身没动。
  // 〔W5-UI〕`1612 → 1613`：`history.ts` 多一行 import（应用内对话框）；〔FW1〕再 +5：`liveInTabs` 那一格；
  // 〔LOC1b · 4D〕上方「重新索引」按钮与等索引那一族删掉 ⇒ 行号挪了（合并主线后现打 1505）。那一处本身没动。
  // 〔W5-AUX〕主线 1504 → 1506：上游搜索命中那一格改走 `data-kind`（多一行注释、一句拆两句），这一处本身没动。
  // 〔FIX3〕1515 → 1516：`history.ts` 多一行 import（`launch-arrival`），这一处本身没动。
  "src/views/history.ts:1516":
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
        "  修法：补一条 `.<类名>[hidden] { display: none; }`。\n" +
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

// ───────────────── ⑧ 同一个状态名不许同现于两种载体（〔W5-AUX〕`设计/41 §7` 一般形式）─────────────────
//
// 要求住址：`设计/41 §7` 逐字「状态只用**一种**载体承载，不许同一个状态在类名、`data-*`、内联 `style` 三处各写一遍」，
// 以及同节「**没判的**：一般形式『同一个状态名不得同时出现在两种载体里』今天没有判据」—— 本格补的就是这一句。
// 量具 `tests/evidence/S30-css-conventions.ts::stateCarriers`（两种形的定义与取法写在那边的头注）。
//
// 🔴 立格那一拍现打逮到三处，**当拍改掉，没有登记**（外观不变：选择器特异度逐条同档或无竞争者，见 `W5-AUX.md §2`）：
//   · 名字形 `conf`：`views/panorama.ts` 把置信档拼成 `conf-<值>` 类名族，而全景图里同一个状态名走 `data-conf` ⇒ 改走 `data-conf`；
//   · 名字形 `kind`：`views/history.ts` · `settings/data-section.ts` 拼 `kind-<值>` 类名族，而另三处走 `data-kind` ⇒ 改走 `data-kind`；
//   · 同一处写两遍：`settings/remote-section.ts` 的缺口行把 `g.kind` 同时拼进类名 `remote-gap-<kind>` 又写进 `data-kind` ⇒ 类名那一份摘掉。
//
// ⚠ 买不到（如实）：
//   · **内联 `style` 那一种载体不在这里**：它没有「状态名」可比（`style.display = …` 写的是属性不是名字）。
//     「会被 `hidden` 切的元素不许裸写 `display`」是 ⑦ 管的那一形，其余「拿内联 style 表达状态」本格看不见。
//   · 类名族只认「`<名>-${…}` 紧挨着洞」那一形；族名与属性名不同的（`remote-gap-${g.kind}` 对 `data-kind`）只有「同一处写两遍」
//     那一形认得出，而它要求**同一个接收者、同一个值表达式逐字相同**（换个局部变量转一手就看不见）。
//   · 修饰类只取 CSS 复合选择器里挂在后面的那几个（`.tab.ended` 的 `ended`）；只由 TS `className` 字面量挂、CSS 里单独成规则的状态类，
//     名字形看不见（要么经 `classList` 切 —— 那一族收了 —— 要么就是组件类，不是状态）。

const CARRIERS: StateCarriers = stateCarriers(REPO_ROOT, stripCodeComments);

/**
 * `classList.*(…)` 实参里一个字符串字面量都没有的调用点 —— 静态认不出切的是哪个类。逐处登记。
 * 键是「文件 · 调用原文」（不按行号 —— ⑦ 那张表逐轮漂行号的教训）。
 */
const STATE_CLASS_UNRESOLVED: Readonly<Record<string, string>> = {
  "src/tab-bar-drag.ts · classList.toggle(cls, on)":
    "拖拽落点标记只动新旧两个（P3）：`cls` 是 `drop-before` / `drop-onto` 之一（同文件的常量），不是一个状态名的载体选择",
  "src/usage-hud.ts · classList.remove(s.high)":
    "CSS Modules（`usage-hud.module.css`）：`s.high` 是构建时哈希过的类名，不进全局命名空间，与 `data-*` 撞不了名",
  "src/usage-hud.ts · classList.toggle(s.high, rounded >= 80)": "同上（逼近自动 compact 时的预警态）",
} as const;

/** 名字形的已知违例（今天空：立格那一拍逮到的两处已改）。 */
const STATE_NAME_KNOWN_BAD: Readonly<Record<string, string>> = {} as const;
/** 同一处写两遍的已知违例（今天空：立格那一拍逮到的一处已改）。 */
const STATE_SAME_WRITE_KNOWN_BAD: Readonly<Record<string, string>> = {} as const;

describe("S30 ⑧ 同一个状态名不许同现于类名与 data-* 两种载体（设计/41 §7 一般形式）", () => {
  it("分母：扫过的份数与两种载体的人群（地板只防人群塌成空集，主锚是下面的恒等）", () => {
    expect(CARRIERS.scanned.ts, "一份 TS 都没扫到").toBeGreaterThanOrEqual(150);
    expect(CARRIERS.scanned.css, "CSS 份数塌了").toBeGreaterThanOrEqual(FLOORS.cssFiles);
    expect(CARRIERS.scanned.html).toBe(3);
    // 现打（立格那一拍）：类名那一侧 89 个状态名 · `data-*` 那一侧 61 个。
    expect(CARRIERS.classNames.size, "类名那一侧一个状态名都没收到 ⇒ 名字形恒绿").toBeGreaterThanOrEqual(60);
    expect(CARRIERS.dataNames.size, "`data-*` 那一侧一个都没收到 ⇒ 名字形恒绿").toBeGreaterThanOrEqual(40);
    // 活样本锚：量具认得出今天真在用的两种载体（`data-state` 是 AR1 把在线状态三个类改过去的那一处）。
    expect(CARRIERS.dataNames.has("state"), "`data-state` 都没认出来 ⇒ 取法坏了").toBe(true);
    expect(CARRIERS.classNames.has("open"), "`classList` 切的 `open` 都没认出来 ⇒ 取法坏了").toBe(true);
  });

  it("解析不出类名的 classList 调用 == 登记表（不许静默跳过）", () => {
    expect(
      sorted(new Set(CARRIERS.unresolved)),
      "「静态认不出切的是哪个类」的调用点变了：多出来的要么登记进 STATE_CLASS_UNRESOLVED（写清为什么它不是状态名的第二种载体），\n" +
        "要么把实参写成字面量；少了的把登记删掉。",
    ).toEqual(sorted(Object.keys(STATE_CLASS_UNRESOLVED)));
  });

  it("名字形：两种载体都出现过的状态名 == 已知违例表", () => {
    const hits = stateNameCollisions(CARRIERS);
    const detail = [...hits].map(([k, v]) => `  ${k}\n    ${v.join("\n    ")}`).join("\n");
    expect(
      sorted(hits.keys()),
      "这些状态名同时当类名与 `data-*` 用 —— `设计/41 §7`：状态只用一种载体。\n" +
        "  有限枚举（几种值）⇒ 走 `data-<名>`；只有有 / 无两态的布尔开关 ⇒ 走类名。改成一种，别两种都留。\n" +
        `现打：\n${detail}`,
    ).toEqual(sorted(Object.keys(STATE_NAME_KNOWN_BAD)));
  });

  it("同一处写两遍：同一接收者、同一值表达式既进类名又进 data-* == 已知违例表", () => {
    expect(
      sorted(CARRIERS.sameWrite),
      "同一个值写进了两种载体（类名模板的洞 / `classList.toggle` 的第二个实参，与 `dataset.x` / `setAttribute(\"data-x\")`）。\n" +
        "  留一种：CSS 与判据都照那一种选。",
    ).toEqual(sorted(Object.keys(STATE_SAME_WRITE_KNOWN_BAD)));
  });

  // 正反两控：合成样本，不依赖盘上任何一个活样本（⑦ 那条「活样本会被删掉」的教训）。
  it("🔴 正反两控：量具对合成样本认得出两种形、也不乱报", () => {
    const fresh = (): Pick<StateCarriers, "classNames" | "dataNames" | "unresolved" | "sameWrite"> => ({
      classNames: new Map(),
      dataNames: new Map(),
      unresolved: [],
      sameWrite: [],
    });
    // 名字形 · 阳：`is-` 前缀剥掉后与 `dataset.open` 同名。
    const a = fresh();
    stateCarriersOfTs("<合成>", 'x.classList.add("is-open");\ny.dataset.open = "1";\n', a);
    expect([...stateNameCollisions(a).keys()], "`is-open` 与 `data-open` 是同一个状态名，认不出 ⇒ 名字形恒绿").toEqual(["open"]);
    // 名字形 · 阳（类名族）：`tone-${…}` 族与 `[data-tone` 选择器同名。
    const b = fresh();
    stateCarriersOfTs("<合成>", "el.className = `pill tone-${t}`;\n", b);
    stateCarriersOfCss("<合成>.css", '.pill[data-tone="ok"] { color: red; }\n', b);
    expect([...stateNameCollisions(b).keys()], "类名族 `tone-${…}` 与 `data-tone` 同名，认不出 ⇒ 立格逮到的那两处会漏").toEqual(["tone"]);
    // 名字形 · 阳（CSS 修饰类）：`.row.busy` 与 `data-busy`。
    const c = fresh();
    stateCarriersOfCss("<合成>.css", ".row.busy { opacity: .5 }\n[data-busy] { cursor: wait }\n", c);
    expect([...stateNameCollisions(c).keys()]).toEqual(["busy"]);
    // 名字形 · 阴：名字不同 ⇒ 不报（「什么都报」的坏尺子过不了这一格）。
    const d = fresh();
    stateCarriersOfTs("<合成>", 'x.classList.add("open");\ny.dataset.shut = "1";\nz.className = `row kind-${k}`;\n', d);
    stateCarriersOfCss("<合成>.css", ".row.busy {}\n[data-idle] {}\n", d);
    expect([...stateNameCollisions(d).keys()], "名字不同也报 ⇒ 这把尺子不可信").toEqual([]);
    // 同一处写两遍 · 阳 / 阴。
    const e = fresh();
    stateCarriersOfTs("<合成>", "li.className = `gap gap-${g.kind}`;\nli.dataset.kind = g.kind;\n", e);
    expect(e.sameWrite.length, "同一接收者同一值写了两遍，认不出 ⇒ `remote-gap` 那一形会漏").toBe(1);
    const f = fresh();
    stateCarriersOfTs("<合成>", "li.className = `gap gap-${g.kind}`;\nother.dataset.kind = g.kind;\nli.dataset.kind = g.other;\n", f);
    expect(f.sameWrite, "接收者或值不同也报 ⇒ 这把尺子不可信").toEqual([]);
  });
});

// ───────────────── ⑨ 活规则里的死声明（〔W5-AUX〕`设计/40 §8`）─────────────────
//
// 要求住址：`设计/40 §8` 把它列在「没查的」里，逐字「**活规则里的死声明** —— 规则在用，但某条声明被后面的规则覆盖了」。
// 本格收的是**机械上判得死**的那一形（量具 `S30-css-conventions.ts::deadDeclarations`，判定写在那边的头注）：
// 同一个选择器（逐字 ⇒ 特异度相同、命中同一批元素）· 同一条件链（`@media` / `@supports` / `@container`）·
// 另一条声明整条盖住它的属性（同名，或后写的简写盖先写的长写）· 按级联那一条赢（重要性 → 层 → 窗口清单里的文件先后 →
// 同文件里的规则先后 → 同规则里的声明先后）· 而且在**每一扇**载入它那份文件的窗口里都这样。
//
// 🔴 立格那一拍现打逮到一处，当拍删掉（外观不变 —— 死声明从没生效过）：`src/styles.css` 的 `.status-tasks` 里
//   `line-height: 16px` 紧跟着被 `font: inherit` 整条盖掉（stylelint 的 `declaration-block-no-shorthand-property-overrides`
//   同一处；`css-ledger` ④ 的上限随之 36 → 35）。若本意是 16px 要挪到 `font` 之后 —— 那会改可见样式，要目视，没做。
//
// ⚠ 买不到（如实）：
//   · **跨选择器的覆盖**（`.a` 与 `.b` 落不落在同一个元素上要真 DOM）—— 那一族连同「特异度相同、后者赢而本意是前者」仍要逐条读（`40 §8` 第二条）；
//   · **条件不同的不判**（`@media` 里那条只在条件成立时盖）；
//   · 简写表只收常用的那几族（`SHORTHAND`），表外的简写当它只盖同名那一条 ⇒ 少判死、不多判；
//   · CSS Modules（`usage-hud.module.css`）不进任何窗口清单 ⇒ 不在射程；第三方 CSS（`vendor` 层）不在 `src/` ⇒ 不在射程。

const DECL_ANOMALIES: string[] = [];
const DECLS: CssDecl[] = cssDeclarations(REPO_ROOT, DECL_ANOMALIES);
const WINDOWS = windowSheets(REPO_ROOT);
const LAYERS = layerOrder(REPO_ROOT);

/** 死声明的已知违例（今天空：立格那一拍逮到的一处已删）。键：`文件:行 选择器 { 属性 }`。 */
const DEAD_DECL_KNOWN: Readonly<Record<string, string>> = {} as const;

describe("S30 ⑨ 活规则里的死声明（设计/40 §8）", () => {
  it("分母与量具自检：声明条数 · 三扇窗口的清单 · 层序 · 规则体里没有原生嵌套", () => {
    // 现打（立格那一拍）：4028 条声明、1017 条带声明的规则。
    expect(DECLS.length, "声明条数塌了 ⇒ 下面那条恒等会零命中地绿").toBeGreaterThanOrEqual(3000);
    expect([...WINDOWS.keys()].sort()).toEqual(["index.html", "settings.html", "viewer.html"]);
    for (const [w, sheets] of WINDOWS) expect(sheets[0], `${w} 的 CSS 清单第一份不是 layers.css —— 层序读错了`).toBe("src/styles/layers.css");
    expect(LAYERS, "层序读出来不是 layers.css 那一句").toEqual(["reset", "vendor", "tokens", "base", "layout", "components", "states", "utilities"]);
    expect(DECL_ANOMALIES, "规则体里又开了块 ⇒ 本量具拆声明的前提（不嵌套）不成立，下面的判定不可信").toEqual([]);
  });

  it("死声明 == 已知违例表", () => {
    const dead = deadDeclarations(DECLS, WINDOWS, LAYERS).map(
      (d) => `${d.file}:${d.line} ${d.sels.join(", ")} { ${d.prop} }`,
    );
    expect(
      sorted(dead),
      "这些声明在它所在的规则里从来不生效：同一个选择器、同一条件下，级联里赢的另一条把它的属性整条盖掉了（每一扇载入它的窗口都一样）。\n" +
        "  删掉它外观不变；若本意是它，那是另一件事（要改顺序 / 改写法，会改可见样式，要目视）。",
    ).toEqual(sorted(Object.keys(DEAD_DECL_KNOWN)));
  });

  it("🔴 正反两控：合成样本上判得出死、也不乱判", () => {
    const one = (css: string, windows: Map<string, string[]> = new Map([["w", ["a.css"]]])): string[] =>
      deadDeclarations(cssDeclsOf("a.css", css), windows, ["base", "components", "utilities"]).map(
        (d) => `${d.sels.join(",")} ${d.prop}:${d.value}`,
      );
    // 阳：同规则里后写的简写盖先写的长写（立格逮到的那一形）。
    expect(one("@layer components { .x { line-height: 1; font: inherit; } }")).toEqual([".x line-height:1"]);
    // 阳：同文件后一条同选择器规则盖前一条；`url("data:…;…")` 里的分号不许把声明拆歪。
    expect(one('@layer components { .x { color: red; background: url("data:a;b,c"); } .x { color: blue; } }')).toEqual([".x color:red"]);
    // 阳：层序靠后的赢，不看书写先后。
    expect(one("@layer utilities { .x { padding: 2px } } @layer base { .x { padding: 1px } }")).toEqual([".x padding:1px"]);
    // 阴：条件不同不判；选择器表只被盖掉一半不判。
    expect(one("@layer components { .x { color: red } @media (width < 9px) { .x { color: blue } } }")).toEqual([]);
    expect(one("@layer components { .y, .z { color: red } .y { color: blue } }")).toEqual([]);
    // 重要性先于书写先后：先写的带 `!important` ⇒ 它活着，死的是后写的那条。
    expect(one("@layer components { .x { color: red !important } .x { color: blue } }")).toEqual([".x color:blue"]);
    // 阴：先写的长写盖不了后写的简写（方向反了）。
    expect(one("@layer components { .x { font: inherit; line-height: 1; } }")).toEqual([]);
    // 窗口：盖它的那一份只在一扇窗口里 ⇒ 另一扇里它是活的 ⇒ 不判死；两扇都有 ⇒ 判死。
    const two = (ws: Map<string, string[]>): string[] =>
      deadDeclarations(
        [...cssDeclsOf("a.css", "@layer components { .x { color: red } }"), ...cssDeclsOf("b.css", "@layer components { .x { color: blue } }")],
        ws,
        ["components"],
      ).map((d) => `${d.file} ${d.prop}:${d.value}`);
    expect(two(new Map([["w1", ["a.css", "b.css"]], ["w2", ["a.css"]]])), "另一扇窗口里它还活着，却判死了").toEqual([]);
    expect(two(new Map([["w1", ["a.css", "b.css"]], ["w2", ["a.css", "b.css"]]]))).toEqual(["a.css color:red"]);
  });
});
