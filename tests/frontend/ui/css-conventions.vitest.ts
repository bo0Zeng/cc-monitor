/**
 * `S30` —— **CSS 三条约定接进判据**（件 7 · 件 9）。
 *
 * 量具住 `tests/evidence/S30-css-conventions.ts`，**本文件只登记与判**（分家的理由写在那边）。
 * 读数住 `tests/evidence/S30-readings.md`。
 *
 * ## 与 `tests/frontend/ui/css-ledger.vitest.ts`（`S25`）的分工 —— 不是第二本账
 *
 * | 文件 | 管哪几件**事实** |
 * |---|---|
 * | `tests/frontend/ui/css-ledger.vitest.ts` | 类名的两个方向（CSS↔代码）· `z-index` 只许走刻度 · stylelint 报错总数 |
 * | `tests/frontend/ui/app-grid-claims.vitest.ts` | `#app` 的每个 in-flow 直接子元素都认领了具名区域 |
 * | **本文件** | **自定义属性的两个方向** · **`transition` 动的属性** · **`hidden` 与 `display` 不许同框** |
 *
 * 三处互不重叠，同一件事实只有一个住址。
 *
 * ## 装了三格（＋ 一格量具自检）
 *
 * | 格 | 判什么 | 形态 | 出自 |
 * |---|---|---|---|
 * | ⓪ | 量具没坏（剥注释这一步在承重 · 人群没缩水） | 恒等 ＋ 地板 | —— |
 * | ⑤ | 自定义属性对账，**两个方向** | **恒等**（两向集合相等） | |
 * | ⑥ | `transition` 只许动白名单里那几个属性 | **恒等**（实测集合 == 白名单 ∪ 例外） | · 件 9 |
 * | ⑦ | 会被 `hidden` 切的元素，CSS 里不许裸写 `display` | **恒等**（违例集合 == 登记表） | 约定 1 · 件 7 |
 * | ⑧ | 同一个状态名不许同现于类名与 `data-*` 两种载体（名字形 ＋ 同一处写两遍形） | **恒等**（违例集合 == 登记表，今天空）＋ 正反两控 | 一般形式 |
 * | ⑨ | 活规则里的死声明（同选择器 · 同条件 · 被级联里赢的那一条整条盖掉） | **恒等**（死声明集合 == 登记表，今天空）＋ 正反两控 | |
 *
 * ## 🔴 为什么是 vitest 而不是 stylelint 插件 / 规则
 *
 * 说「装插件 `stylelint-value-no-unknown-custom-properties`」，
 * 说「stylelint 的 `declaration-property-value-allowed-list` 可以拦」。
 * **两条都办不到，理由不是懒：**
 *
 * - 装插件要改 `package.json` ＋ `package-lock.json`；换 stylelint 规则要改
 *   `.stylelintrc.json`。**三份都不在本轮写区**（写区只有 `src/frontend/ui/styles.css` ·
 *   `src/frontend/ui/styles/` 下的 CSS · 新立的判据文件）。
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
} from "../../evidence/S30-css-conventions.ts";
import { stripCodeComments } from "../../evidence/S25-class-ledger.ts";
import { REPO_ROOT } from "../../test-support/repo-root.ts";

const FACTS: CssFacts = cssFacts(REPO_ROOT);
const SET_PROP = setPropertyVars(REPO_ROOT);
const THEME = themeTokens(REPO_ROOT);
const HIDDEN: HiddenSite[] = hiddenSites(REPO_ROOT);

const sorted = (xs: Iterable<string>): string[] => [...xs].sort();
const minus = (a: Iterable<string>, b: ReadonlySet<string> | Map<string, unknown>): string[] =>
  sorted([...a].filter((x) => !b.has(x)));


// ───────────────────────── ⑤ 自定义属性对账─────────────────────────

/**
 * **CSS 定义了、但 CSS 里没有任何 `var()` 读它**的令牌。逐个登记，附理由。
 *
 * 为什么这一向也要判：只写了正方向（「用了没定义」），而现打逮到的两条
 * 真缺陷全在**反方向** —— 正方向今天是干净的，只判它等于这一格永远绿。
 */
const NEW_TOKEN = "规范新立、组件件待接";
const UNUSED_TOKENS: Readonly<Record<string, string>> = {
  // 🔴 `--user` / `--assistant` 两条登记**摘掉了** —— 上一版这里逐字写着
  //    「裁完之后把这两行删掉，判据会提醒」。用户当天裁了：**撤掉那两个取色器**。
  //    ⚠ 摘登记的**同拍**把旋钮本身删了（`settings/panel.ts` 与 `theme.ts` 各一处，
  //      原处留了说明）。**只摘登记不删旋钮**会让下面那条「定义了没人用」的恒等断言
  //      当场红 —— 那正是本表存在的意义：它不许你只清账不清事。

  // 下面五条是新补的两族里**今天还没有消费者**的部分。
  // 设计要的是「族补齐」（免得下一个人再去猜 `--input-bg` 这种名字），
  // 消费者要等到那些控件下一次被动到时才自然接上 ⇒ 预留是刻意的，不是腐。
  // 规范 V1–V12 新立的令牌里，通用组件还没接上的（组件件落地一个摘一个）。
  "--dur-float-out": NEW_TOKEN,
} as const;

describe("S30 ⓪ 量具自检（这几条不过，下面三格全是空转）", () => {
  it("扫到的人群没有缩水", () => {
    expect(FACTS.cssFiles.length, `只扫到 ${FACTS.cssFiles.length} 份 CSS`).toBeGreaterThan(0);
    expect(FACTS.defined.size, "CSS 里一个自定义属性定义都没扫到 ⇒ 词法坏了").toBeGreaterThan(0);
    expect(FACTS.used.size, "CSS 里一处 var() 都没扫到 ⇒ 词法坏了").toBeGreaterThan(0);
    expect(FACTS.rules.length, "一条 CSS 规则都没扫到 ⇒ 分块坏了").toBeGreaterThan(0);
    expect(FACTS.transitionDecls, "一条 transition 都没扫到").toBeGreaterThan(0);
    expect(HIDDEN.length, "一处 `.hidden =` 都没扫到 ⇒ TS 侧词法坏了").toBeGreaterThan(0);
  });

  it("🔴 正控：剥 CSS 注释这一步在承重（不剥的话散文会同时造假与灭真）", () => {
    // 这一格要证明的不是「剥了」，是「不剥就会给出**错的**答案」，而且两个方向都会错。
    //
    // ① 造假：`src/frontend/ui/styles/tokens.css` 的头注逐字写着「z-index 只许写 `var(--z-*)`」——
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
    // 🔴 这一族**空了** —— 那两个令牌连定义带旋钮一起撤了。
    // ⚠ 空集让本条退化成「剥注释这件事在这儿不再承重」，判据自己上面那句话就是这么写的
    //   ⇒ 剥注释是否承重，改由 `strippedChars > 1000` 那条兜（它在下一行，没动）。
    ).toEqual([]);

    expect(FACTS.strippedChars, "一个注释字符都没剥掉").toBeGreaterThan(0);
  });
});

describe("S30 ⑤ 自定义属性对账，两个方向", () => {
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
    expect(fromTs.length, "TS 侧一个 setProperty 都没扫到 ⇒ 上面那条会变成「两边都空」的假绿").toBeGreaterThan(0);
  });

  it("② theme.ts 那 14 个旋钮，CSS 里必须都有默认值", () => {
    // 旋钮只许落在这几族里：新补的令牌族不许偷偷混进旋钮（真要开一族旋钮，就在这里被逼着改一次、被人看见一次）。
    const KNOB_FAMILIES = ["--font-", "--bg", "--card", "--text", "--success", "--warn", "--error", "--live"];
    expect(
      THEME.filter((t) => !KNOB_FAMILIES.some((f) => t === f || t.startsWith(f))),
      "这些旋钮不在旋钮族里 —— 新补的令牌族不许混进旋钮",
    ).toEqual([]);
    expect(THEME.length, "一个旋钮都没读到 ⇒ 下面那条会零命中地绿").toBeGreaterThan(0);
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
        "  🔴 这个集合今天**应当是空的** —— 原先那两个（`--user`/`--assistant`）\n" +
        "     连旋钮带令牌一起撤了。**再冒出一个就是新造了一个死旋钮**：\n" +
        "     用户能拖、界面不动，而且不会有任何别的东西报错。\n" +
        "这一族是用户能当场感觉到的那种坏：设置里拖了取色器，界面一动不动。\n" +
        "接上消费者之后把这两行从 UNUSED_TOKENS 里删掉，本条会跟着变。",
    ).toEqual([]);
  });
});

// ───────────────────────── ⑥ 动画属性白名单（件 9）─────────────────────────

/** 只许过渡这五个。它们都只走合成/重绘，不触发重排。 */
const TRANSITION_ALLOWED = ["background", "border-color", "color", "opacity", "transform"] as const;

/** 白名单之外、**逐条登记**的例外。登记表不许有死条目（见下面那条断言）。 */
const TRANSITION_EXCEPTIONS: Readonly<Record<string, string>> = {
  "grid-template-rows":
    " 点名的已登记例外：`0fr ↔ 1fr` 是「不用预知内容高度就能平滑展开」的标准技巧，" +
    "每帧重排但只在展开那 200ms。要动尺寸时的另一条路是 transform: scale。",
  none: "`transition: none` 是**关掉**过渡，不是动某个属性 —— 它恰恰是白名单想要的方向。",
} as const;

describe("S30 ⑥ transition 只许动白名单里那几个属性（件 9）", () => {
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
    expect(FACTS.transitionDecls).toBeGreaterThan(0);
    expect(
      FACTS.transitions.length,
      "一条 transition 声明都没解析出属性名 ⇒ 上面两条会零命中地绿",
    ).toBeGreaterThan(FACTS.transitionDecls - 1);
  });
});

// ───────────────── ⑦ `hidden` 与 `display` 不许同框（约定 1 · 件 7）─────────────────

/**
 * **静态解析不出类名的 `.hidden =` 处**，按「文件::接收者表达式」登记（同一份文件里切同一个元素的几处是一格；不认行号）。
 *
 * 为什么要登记而不是跳过：跳过 ＝「扫不到就绿」。多出一个推不出的元素要有人看见 —— 多出来的多半是词法器又错位了。
 */
const HIDDEN_UNRESOLVED: Readonly<Record<string, string>> = {
  "src/frontend/ui/find-strip.ts::row.state":
    "命中行下那一句 `row.state`（`s.fsState`）—— 它写了 display:flex，CSS 里另有 `.fsState[hidden]` 收住",
  "src/frontend/ui/find-strip.ts::state":
    "命中行下那一句（`s.fsState`，建时的局部名）—— 它写了 display:flex，CSS 里另有 `.fsState[hidden]` 收住",
  "src/frontend/ui/find-strip.ts::this.strip":
    "会话内查找的命中清单 `strip`（`s.fsStrip`）—— 它写了 display:flex，CSS 里另有 `.fsStrip[hidden]` 收住",
  "src/frontend/ui/kit/dialog.ts::err":
    "填值框的错误句 `err`（`s.error`）—— 开时收起",
  "src/frontend/ui/kit/dialog.ts::failLine":
    "表单框提交没成那一行 `failLine`（`s.dlgFail`，自己不写 display）—— 没出错 / 改了一格时收起",
  "src/frontend/ui/kit/dock.ts::this.el":
    "底部抽屉网格那一格 `this.el`（`s.dockSlot`，自己不写 display；竖排的 flex 在里层 `s.dock`）—— 收着时 hidden",
  "src/frontend/ui/kit/fold.ts::body":
    "折叠块正文 `body`（`s.body`）",
  "src/frontend/ui/kit/toast.ts::countEl":
    "合流计数（`s.count`，建时的局部名）—— 建时收起",
  "src/frontend/ui/kit/toast.ts::same.countEl":
    "合流计数 `same.countEl`（`s.count`）—— 合进第二条时出现",
  "src/frontend/ui/launch-slot.ts::this.panel":
    "占位标签页那一页 `this.panel`（`s.slotPanel`）—— 建时收起",
  "src/frontend/ui/new-session.ts::accountRow.root":
    "「账号」那一行 `accountRow.root`（`s.nsRow`）—— 建时收起",
  "src/frontend/ui/new-session.ts::agentRow.root":
    "「agent」那一行 `agentRow.root`（`s.nsRow`）—— 建时收起（那台能起的多于一家才出）",
  "src/frontend/ui/new-session.ts::note":
    "一格下那一行说明 / 错误 `note`（`s.nsNote`）—— 建时收起",
  "src/frontend/ui/new-session.ts::tmuxRadio.label":
    "「tmux 里」那一张单选 `tmuxRadio.label`（`s.nsPlace`）—— 那台没 tmux 时收起",
  "src/frontend/ui/new-session.ts::tmuxRow.root":
    "「tmux 会话名」那一行 `tmuxRow.root`（`s.nsRow`）—— 放在终端窗口时收起",
  "src/frontend/ui/settings/ext-section.ts::this.drawer":
    "`this.drawer` —— 构造时收起",
  "src/frontend/ui/settings/resume-select.ts::this.custom":
    "恢复命令下拉旁「自定义…」那一格 —— 类名 `settings-input settings-input-mono` 在构造器里挂，那两类不写 display",
  "src/frontend/ui/settings/machine-state.ts::el":
    "`el` 是调用方交进来的问题行（列表那一行与卡头各一个 `.machine-problem`，跨文件）；`settings.css` 里 `.machine-problem[hidden] { display: none }` 人工核过",
  "src/frontend/ui/settings/panel.ts::b.el":
    "`b.el` —— `b` 来自 `this.perMachineBlocks` 数组，元素由各 section 自己建，跨文件",
  "src/frontend/ui/settings/panel.ts::this.claudeDirRestart":
    "Claude 目录那一行下「重启 cc-monitor 后生效 ［现在重启］」—— 类名 `diag-restart` 在 `buildDataGroup` 里挂，那一类不写 display",
  "src/frontend/ui/settings/panel.ts::errorAt":
    "`errorAt` —— 参数，类名 `settings-row-error` 由调用方建的那一句挂",
  "src/frontend/ui/settings/panel.ts::this.perMachineFallbackHint":
    "`this.perMachineFallbackHint` —— 类名由 `skeleton.ts::makeSkeleton` 挂，跨文件",
  "src/frontend/ui/status-messages.ts::this.dot":
    "「消息」那一枚右上的琥珀点 `this.dot`（`s.smDot`）—— 建时收起；CSS 里另有 `.smDot[hidden]` 收住",
  "src/frontend/ui/terminal-page.ts::this.followBox":
    "终端页「回到最新」外面那一格 `this.followBox`（`s.termFollow`：只定位、不写 display）—— 贴着底时收起",
  "src/frontend/ui/terminal-page.ts::this.liveTag":
    "终端页头上「● 实时 / 接入实时」`this.liveTag`（`s.termLive`：不写 display，圆点在 `::before` 上）—— 没在实时时收起",
  "src/frontend/ui/terminal-page.ts::this.snapGroup":
    "终端页头上「画面几点 ＋ 重新看」那一组 `this.snapGroup`（`s.termSnap`：不写 display）—— 实时中整组收起",
  "src/frontend/ui/terminal-page.ts::this.snapTag":
    "终端页头上「仅快照」`this.snapTag`（`s.termTag`：不写 display）—— 那台能实时就收起",
  "src/frontend/ui/views/agent-window.ts::this.pill":
    "「↓ 新内容」那一枚 `this.pill`（kit 按钮，再挂 `sv.svPill`）—— 没有新内容时收起",
  "src/frontend/ui/views/session-find.ts::this.box":
    "面板本体 `this.box`（`s.sfPanel`）—— 建时收起",
  "src/frontend/ui/views/session-find.ts::this.outline.panel":
    "`this.outline.panel` —— 大纲清单那块，由 `UserInputPanel` 建（类 `.user-inputs`；`styles.css` 里那条规则头注逐字「绝不许出现 display」）",
  "src/frontend/ui/views/session-find.ts::this.searchPane":
    "「搜索」那一页 `this.searchPane`（`s.sfSearch`）—— 切到「大纲」时收起",
  "src/frontend/ui/views/session-viewer.ts::head":
    "头 `head`（`sv.svHead`）—— 窗口那一形不画（细顶栏担）；它写了 display:flex，CSS 里另有 `.svHead[hidden]` 收住",
  "src/frontend/ui/views/session-viewer.ts::this.bannerEl":
    "头下那一条 `bannerEl`（`sv.svBanner`）—— 没有错误条时收起",
  "src/frontend/ui/views/session-viewer.ts::this.emptyEl":
    "0 条消息的空态那一格 `emptyEl`（`sv.svEmpty`）—— 有消息时收起；它写了 display:flex，CSS 里另有 `.svEmpty[hidden]` 收住",
  "src/frontend/ui/views/session-viewer.ts::this.loadingEl":
    "读取中的骨架外层 `loadingEl`（`sv.svLoading`）",
  "src/frontend/ui/views/session-viewer.ts::this.newPill":
    "「↓ 新内容」`newPill`（kit 按钮 ＋ `sv.svPill`）—— kit 按钮写了 display，CSS 里另有 `.svPill[hidden]` 收住",
  "src/frontend/ui/views/session-viewer.ts::this.said.panel":
    "「你说过的话」清单 `this.said.panel` —— 由 `UserInputPanel` 建（类 `.user-inputs`，`styles.css` 那条规则头注逐字「绝不许出现 display」）",
  "src/frontend/ui/views/session-viewer.ts::this.streamEl":
    "消息流 `streamEl`（`stream session-viewer-stream`，全局类）—— 0 条消息时让位给空态；`.stream` / `.session-viewer-stream` 都不写 display",
  "src/frontend/ui/views/session-viewer.ts::this.toolsEl":
    "工具行 `toolsEl`（`sv.svTools`）—— 窗口那一形 Ctrl+F 才露；它写了 display:flex，CSS 里另有 `.svTools[hidden]` 收住",
  "src/frontend/ui/views/session-viewer.ts::tools":
    "工具行（`sv.svTools`，建时的局部名）—— 窗口那一形建时收起；它写了 display:flex，CSS 里另有 `.svTools[hidden]` 收住",
} as const;

/**
 * **已知违例**：会被 `hidden` 切、而 CSS 又在它自己身上裸写了 `display`。
 *
 * 作者样式里的 `display` 压过 UA 的 `[hidden] { display: none }` ⇒ 那句 `hidden` 是空写。
 */
// 🔴 **这张表清空了。**
//   唯一那条 `tab-archive-list` 摘掉 —— **不是修好了，是那个功能整个不存在了**：
//   归档抽屉删了（用户原话「没有归档这个东西，不要归档，就是灰 tab。
//   现在的归档是错误的，甚至是 bug 的来源，全部删掉」），
//   而抬头本来就写着「已定：删归档抽屉」。
//   ⚠ 那条缺陷（`hidden` 是空写、箭头在骗人）**没有被修，是随载体一起消失的** ——
//     两者在「表上少一行」这件事上长得一模一样，所以写清楚。
//   ⚠ 空表**不等于这格判据没用**：下面那条仍是**恒等**（盘上现打的违例集合 == 本表），
//     再冒出一个同形的当场红。空的是人群，不是判据。
const HIDDEN_KNOWN_BAD: Readonly<Record<string, string>> = {} as const;

describe("S30 ⑦ 会被 hidden 切的元素，CSS 不许在它身上裸写 display（约定 1）", () => {
  const resolved = HIDDEN.filter((h) => h.classes !== null);
  const classes = sorted(new Set(resolved.flatMap((h) => h.classes ?? [])));
  const verdicts = new Map(classes.map((c) => [c, displayVerdict(FACTS, c)]));

  it("分母：判过的 `.hidden =` 处数与类名个数", () => {
    expect(HIDDEN.length).toBeGreaterThan(0);
    expect(
      classes.length,
      "一个类名都没解析出来 ⇒ 下面两条会零命中地绿",
    ).toBeGreaterThan(0);
  });

  it("解析不出类名的那几处 == 登记表（不许静默跳过）", () => {
    const unresolved = sorted(new Set(HIDDEN.filter((h) => h.classes === null).map((h) => `${h.file}::${h.recv}`)));
    expect(
      unresolved,
      "「静态推不出它挂的是哪个类」的元素变了。\n" +
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

  // 🔴 **正控从活体换成合成夹具 —— 今天第三次栽在同一课上。**
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

// ───────────────── ⑧ 同一个状态名不许同现于两种载体（一般形式）─────────────────
//
// 要求：「状态只用**一种**载体承载，不许同一个状态在类名、`data-*`、内联 `style` 三处各写一遍」，
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
  "src/frontend/ui/cards/brief.ts · classList.add(s.briefMore)":
    "CSS Modules：给 kit 按钮挂派活框里［展开全部］的版位类；哈希过的类名，不是状态名",
  "src/frontend/ui/kit/icon.ts · classList.add(s.icon)":
    "图标件：svg 元素的 `className` 不是串、只能走 classList；`s.icon` 是 CSS Modules 哈希名，与 `data-*` 撞不了名",
  "src/frontend/ui/tab-bar-drag.ts · classList.toggle(cls, on)":
    "拖拽落点标记只动新旧两个（P3）：`cls` 是 `drop-before` / `drop-onto` 之一（同文件的常量），不是一个状态名的载体选择",
  "src/frontend/ui/usage-hud.ts · classList.add(s.hudChip)":
    "CSS Modules（`usage-hud.module.css`）：`s.hudChip` 是构建时哈希过的类名（数字等宽那一条），不是状态名；预警态走 kit chip 的 `data-intent`",
  "src/frontend/ui/views/agent-window.ts · classList.add(s.awMark)":
    "CSS Modules（agent 窗口）：给 kit 建的元素挂本页的版位类；哈希过的类名，不是状态名",
  "src/frontend/ui/views/agent-window.ts · classList.add(s.awWhyAct)": "同上（状态不明那一句后的按钮）",
  "src/frontend/ui/views/agent-window.ts · classList.add(sv.svPill)": "同上（「↓ 新内容」借只读查看器的样式）",
  "src/frontend/ui/views/history-rows.ts · classList.add(s.hvDot)":
    "CSS Modules（`history.module.css`）：给 kit 建的元素（状态点 · 徽标 · 按钮 · 提示条）挂本页的版位类；哈希过的类名，不是状态名",
  "src/frontend/ui/views/history-rows.ts · classList.add(s.hvGroupNew)": "同上（项目头上的［＋ 新会话］）",
  "src/frontend/ui/views/history-rows.ts · classList.add(s.hvMachine)": "同上（机器标签）",
  "src/frontend/ui/views/history-rows.ts · classList.add(s.hvStar)": "同上（星标图标）",
  "src/frontend/ui/views/history-rows.ts · classList.add(s.hvStrip)": "同上（列表顶的警示条）",
  "src/frontend/ui/views/history.ts · classList.add(s.hvCount)": "同上（「筛选」上的数字徽标）",
  "src/frontend/ui/views/history.ts · classList.add(s.hvViews)": "同上（「按时间 | 按项目」那一条分栏）",
  "src/frontend/ui/views/history.ts · classList.add(s.hvFilterBtn)": "同上（「筛选」按钮：窄档只剩图标）",
  "src/frontend/ui/views/history.ts · classList.add(s.hvToList)": "同上（窄档内容头左端的「← 列表」）",
  "src/frontend/ui/views/session-viewer.ts · classList.add(sv.svPill)": "「↓ 新内容」那颗 kit 按钮叠上自己的定位类（不是状态名；露 / 收由 `hidden` 管）",
  "src/frontend/ui/kit/split-button.ts · classList.add(s.splitMain)":
    "CSS Modules（`split-button.module.css`）：给拆分按钮的两半（kit 按钮）挂拼接用的版位类；哈希过的类名，不是状态名",
  "src/frontend/ui/kit/split-button.ts · classList.add(s.splitMore)": "同上（▾ 那一半）",
} as const;

/** 名字形的已知违例（今天空：立格那一拍逮到的两处已改）。 */
const STATE_NAME_KNOWN_BAD: Readonly<Record<string, string>> = {} as const;
/** 同一处写两遍的已知违例（今天空：立格那一拍逮到的一处已改）。 */
const STATE_SAME_WRITE_KNOWN_BAD: Readonly<Record<string, string>> = {} as const;

describe("S30 ⑧ 同一个状态名不许同现于类名与 data-* 两种载体（一般形式）", () => {
  it("分母：扫过的份数与两种载体的人群（地板只防人群塌成空集，主锚是下面的恒等）", () => {
    expect(CARRIERS.scanned.ts, "一份 TS 都没扫到").toBeGreaterThan(0);
    expect(CARRIERS.scanned.css, "CSS 份数塌了").toBeGreaterThan(0);
    expect(CARRIERS.scanned.html, "三扇窗口的 html 没扫全").toBe(WINDOWS.size);
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
      "这些状态名同时当类名与 `data-*` 用 ——：状态只用一种载体。\n" +
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

// ───────────────── ⑨ 活规则里的死声明─────────────────
//
// 把它列在「没查的」里，逐字「**活规则里的死声明** —— 规则在用，但某条声明被后面的规则覆盖了」。
// 本格收的是**机械上判得死**的那一形（量具 `S30-css-conventions.ts::deadDeclarations`，判定写在那边的头注）：
// 同一个选择器（逐字 ⇒ 特异度相同、命中同一批元素）· 同一条件链（`@media` / `@supports` / `@container`）·
// 另一条声明整条盖住它的属性（同名，或后写的简写盖先写的长写）· 按级联那一条赢（重要性 → 层 → 窗口清单里的文件先后 →
// 同文件里的规则先后 → 同规则里的声明先后）· 而且在**每一扇**载入它那份文件的窗口里都这样。
//
// 🔴 立格那一拍现打逮到一处，当拍删掉（外观不变 —— 死声明从没生效过）：`src/frontend/ui/styles.css` 的 `.status-tasks` 里
//   `line-height: 16px` 紧跟着被 `font: inherit` 整条盖掉（stylelint 的 `declaration-block-no-shorthand-property-overrides`
//   同一处；`css-ledger` ④ 的上限随之 36 → 35）。若本意是 16px 要挪到 `font` 之后 —— 那会改可见样式，要目视，没做。
//
// ⚠ 买不到（如实）：
//   · **跨选择器的覆盖**（`.a` 与 `.b` 落不落在同一个元素上要真 DOM）—— 那一族连同「特异度相同、后者赢而本意是前者」仍要逐条读（第二条）；
//   · **条件不同的不判**（`@media` 里那条只在条件成立时盖）；
//   · 简写表只收常用的那几族（`SHORTHAND`），表外的简写当它只盖同名那一条 ⇒ 少判死、不多判；
//   · CSS Modules（`usage-hud.module.css`）不进任何窗口清单 ⇒ 不在射程；第三方 CSS（`vendor` 层）不在 `src/` ⇒ 不在射程。

const DECL_ANOMALIES: string[] = [];
const DECLS: CssDecl[] = cssDeclarations(REPO_ROOT, DECL_ANOMALIES);
const WINDOWS = windowSheets(REPO_ROOT);
const LAYERS = layerOrder(REPO_ROOT);

/** 死声明的已知违例（今天空：立格那一拍逮到的一处已删）。键：`文件:行 选择器 { 属性 }`。 */
const DEAD_DECL_KNOWN: Readonly<Record<string, string>> = {} as const;

describe("S30 ⑨ 活规则里的死声明", () => {
  it("分母与量具自检：声明条数 · 三扇窗口的清单 · 层序 · 规则体里没有原生嵌套", () => {
    expect(DECLS.length, "声明条数塌了 ⇒ 下面那条恒等会零命中地绿").toBeGreaterThan(0);
    expect([...WINDOWS.keys()].sort()).toEqual(["index.html", "settings.html", "viewer.html"]);
    for (const [w, sheets] of WINDOWS) expect(sheets[0], `${w} 的 CSS 清单第一份不是 layers.css —— 层序读错了`).toBe("src/frontend/ui/styles/layers.css");
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
