/**
 * S24〔`设计/40 §7` 步 9 ③ · `设计/41 §5`〕：**`#app` 的每个 in-flow 直接子元素都要认领一个格子。**
 *
 * # 病史（这条判据是被一个活缺陷逼出来的，不是防御性编程）
 *
 * `TabManager.ensureArchiveUi()` 把归档抽屉 `.tab-archive` 插成 `#tab-bar` 的**兄弟**
 * ⇒ 它是 `#app` 的 in-flow 直接子元素。而 `.tab-archive` 的 CSS 里**既没有 `grid-area`、
 * 也没有 `position`** ⇒ 一个**没认领格子的 grid item**，浏览器只好给它开一条**隐式行**。
 *
 * 真引擎实测（Chromium 153 + WebKitGTK 2.52.6，900×700，读数见
 * `tests/evidence/S24-css-readings.md`）：
 *
 * | | `#app` 的 `grid-template-rows` | `#message-stream` 高 | 抽屉落在哪 |
 * |---|---|---:|---|
 * | 没抽屉 | `676px 24px` | 676 | — |
 * | 插入抽屉 | `641px 24px **35px**` | **641（−35）** | y=665，**在状态栏下面** |
 *
 * ⇒ 用户当场看得见的两件事：消息流被偷 35px；抽屉画在状态栏之下。
 * 而 `真相源/51 §4` 当时写的是「删掉归档抽屉之后 `#app` 就干净了」——
 * **那句话的前提没有发生**，帐本和现实分了岔，没有任何机器在对。
 *
 * # 为什么是这条性质，而不是「`#app` 的子元素 == 那三个」
 *
 * `设计/40 §7` 步 9 ③ 原文写的是「`#app` 的 in-flow 直接子元素集合 ==
 * {`#tab-bar`, `#message-stream`, `#status-bar`}」。**那个写法今天就是错的**：
 * `.tab-archive` 确实在那里，而且步 17 之前它还得在。钉一个会持续假红的集合，
 * 只会训练人去绕过判据。
 *
 * ⇒ 这里钉的是**那条真正要成立的性质**：*每一个* in-flow 直接子元素都认领了一个
 * **本模式模板里声明过的**具名区域。认领了就不会开隐式行 —— 隐式行数因此被钉死在 0，
 * 而不用谁记得回来数行。踩过的那条坑（`styles.css` 里 `body.viewer-mode #app` 那段
 * 逐字：「必须只定义两行，否则 message-stream 会落进多出来的那行被压成 0 高」）
 * 从此有机器在看，**两种模式都看**。
 *
 * # 三把尺子
 *
 * - **尺 A（人群从源码派生）**：扫 `src/**\/*.ts`，找出所有「往 `#app` 里插节点」的调用点。
 *   发现新的插入点 ⇒ 当场红，逼人把它登记进下面那张表（然后尺 C 会问它认领了没有）。
 * - **尺 B（住址对账）**：表里每一行的「插入的那个表达式」必须真的被赋过那个 class/id。
 *   —— 防止表上写着 `.tab-archive` 而源码早改名了这种「判据守着一个不存在的东西」。
 * - **尺 C（认领对账，会红的那条）**：按 `src/styles.css` 逐个判「认领了没有」。
 *
 * 三把尺子各带一次**自检**（拿一段合成输入喂进去，确认它看得见本该看见的东西）——
 * 尺子自己坏掉的时候会永远说「没问题」，那比没有判据更坏。
 *
 * # 它管不了什么（诚实边界）
 *
 * - ⚠ 尺 A 只认得今天实际存在的四种插法（见 `APP_INSERT_SHAPES`）。有人用第五种形状
 *   （比如先把节点交给一个工具函数再插）把东西塞进 `#app`，这把尺子看不见。
 * - ⚠ 这是**静态对账**，不是渲染。它保证「每个 item 都认领了一个声明过的区域」，
 *   不保证那个区域的位置好看 —— 那一格靠 `tests/evidence/S24-css-readings.md` 的真引擎读数。
 * - ⚠ 不管 `position: absolute` 的那几个画在哪：它们不是 grid item，不开隐式行，
 *   到此为止。
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, it, expect } from "vitest";
import { REPO_ROOT } from "./test-support/repo-root.ts";

// ── 0. 人群表（`住址` 由尺 A 机械对账，`选择器` 由尺 B 机械对账）───────────────

interface AppChild {
  /** 插入点所在文件（相对仓根）。 */
  readonly file: string;
  /** 被插进 `#app` 的那个表达式，**逐字**照抄源码。 */
  readonly expr: string;
  /** 它在 CSS 里的住址。 */
  readonly selector: string;
  /** 它在哪几种模式下真的存在于 `#app` 里。 */
  readonly modes: readonly Mode[];
  /** 为什么是这几种模式。 */
  readonly why: string;
}

type Mode = "default" | "viewer";
const BOTH: readonly Mode[] = ["default", "viewer"];

/** `index.html` 里静态写死的那三个（不经过任何 JS 插入，故不在尺 A 的人群里）。 */
const STATIC_CHILDREN: readonly AppChild[] = [
  {
    file: "index.html",
    expr: '<div id="tab-bar">',
    selector: "#tab-bar",
    modes: BOTH,
    why: "静态 DOM；viewer 里靠 display:none 退出 grid",
  },
  {
    file: "index.html",
    expr: '<main id="message-stream">',
    selector: "#message-stream",
    modes: BOTH,
    why: "静态 DOM",
  },
  {
    file: "index.html",
    expr: '<div id="status-bar">',
    selector: "#status-bar",
    modes: BOTH,
    why: "静态 DOM",
  },
];

/** JS 插进 `#app` 的那些。`expr` 必须与源码逐字相同 —— 尺 A 拿它对账。 */
const INSERTED_CHILDREN: readonly AppChild[] = [
  {
    file: "src/main.ts",
    expr: "tasksPanel.popoverElement",
    selector: ".tasks-popover",
    modes: ["default"],
    why: "bootstrapMain 建；viewer 路不建",
  },
  {
    file: "src/main.ts",
    expr: "agentsPanel.popoverElement",
    selector: ".tasks-popover",
    modes: ["default"],
    why: "同上（className 是 `tasks-popover agents-popover`，认领靠前者）",
  },
  {
    file: "src/main.ts",
    expr: "resizer",
    selector: "#tab-bar-resizer",
    modes: ["default"],
    why: "bootstrapMain 建；viewer 路不建（CSS 另有一条兜底隐藏）",
  },
  {
    file: "src/main.ts",
    expr: "settingsTrigger",
    selector: ".settings-trigger",
    modes: ["default"],
    why: "六个顶栏 trigger 都只在 bootstrapMain 建",
  },
  {
    file: "src/main.ts",
    expr: "historyTrigger",
    selector: ".history-trigger",
    modes: ["default"],
    why: "同上",
  },
  {
    file: "src/main.ts",
    expr: "panoramaTrigger",
    selector: ".panorama-trigger",
    modes: ["default"],
    why: "同上",
  },
  {
    file: "src/main.ts",
    expr: "gridTrigger",
    selector: ".grid-monitor-trigger",
    modes: ["default"],
    why: "同上",
  },
  {
    file: "src/main.ts",
    expr: "sftpTrigger",
    selector: ".sftp-trigger",
    modes: ["default"],
    why: "同上",
  },
  {
    file: "src/main.ts",
    expr: "topbar",
    selector: ".viewer-topbar",
    modes: ["viewer"],
    why: "只有 bootstrapViewer 建它，而它第一件事就是挂 body.viewer-mode",
  },
  {
    file: "src/tabs.ts",
    expr: "wrap",
    selector: ".tab-archive",
    modes: BOTH,
    why: "🔴 viewer 也 new 一个 TabManager（main.ts bootstrapViewer）⇒ 两种模式都会插",
  },
];

const ALL_CHILDREN = [...STATIC_CHILDREN, ...INSERTED_CHILDREN];

// ── 1. 尺 A：从源码派生「往 #app 里插节点」的调用点 ──────────────────────────

/**
 * 今天实际存在的四种插法。**这是这把尺子的全部视野**，边界写在文件抬头。
 *
 * ① `document.getElementById("app")?.appendChild(X)` —— 直接取、直接插
 * ② `APP.appendChild(X)` / `APP.insertBefore(X, …)` —— `APP` 是绑到 `#app` 的局部名
 * ③ `const APP = <barEl>.parentElement` 之后的 ②   —— `#tab-bar` 的父节点**就是** `#app`
 * ④ `X.insertBefore(Y, <barEl>.nextSibling)`        —— 插成 `#tab-bar` 的兄弟 ⇒ 同为 `#app` 的孩子
 */
const APP_INSERT_SHAPES = ["直接 getElementById", "app 局部名", "barEl.parentElement", "barEl 的兄弟"] as const;

interface Site {
  readonly file: string;
  readonly expr: string;
}

/** 绑到 `#app` 的局部名（含经 `barEl.parentElement` 拿到的）。 */
function appBindings(src: string): Set<string> {
  const names = new Set<string>();
  // 形状 ②：const appEl = document.getElementById("app");
  for (const m of src.matchAll(
    /\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?::[^=\n]+)?=\s*document\.getElementById\("app"\)/g,
  )) {
    names.add(m[1]);
  }
  // 形状 ③：const parent = this.barEl.parentElement;   （`#tab-bar` 的父节点 == `#app`）
  for (const m of src.matchAll(
    /\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?::[^=\n]+)?=\s*[\w.]*\bbarEl\b\.parentElement/g,
  )) {
    names.add(m[1]);
  }
  return names;
}

/**
 * 去掉 TS 的注释再扫。
 *
 * ⚠ 这一步是**踩出来的**：`tabs.ts` 的注释里逐字引用了它自己改之前的写法
 * （`?.insertBefore(...)`），不去注释的话尺子会把那段**说明文字**当成一个插入点，
 * 报出一个根本不存在的成员。注释里写代码在这个仓里很常见，所以这条得留着。
 */
function stripTsComments(src: string): string {
  let out = "";
  let quote: string | null = null;
  for (let i = 0; i < src.length; i++) {
    const c = src[i];
    if (quote) {
      out += c;
      if (c === "\\") {
        out += src[i + 1] ?? "";
        i += 1;
        continue;
      }
      if (c === quote) quote = null;
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      quote = c;
      out += c;
      continue;
    }
    if (c === "/" && src[i + 1] === "/") {
      const nl = src.indexOf("\n", i);
      i = nl === -1 ? src.length : nl - 1;
      continue;
    }
    if (c === "/" && src[i + 1] === "*") {
      const end = src.indexOf("*/", i + 2);
      i = end === -1 ? src.length : end + 1;
      continue;
    }
    out += c;
  }
  return out;
}

/** 扫一份源码，返回它往 `#app` 里插的所有节点表达式。 */
export function censusAppInserts(raw: string, file: string): Site[] {
  const src = stripTsComments(raw);
  const out: Site[] = [];
  const push = (expr: string): void => {
    const e = expr.trim();
    // 跨行的「表达式」一定是尺子咬错了（真的插入表达式都是一个标识符或一段属性链）
    if (e.includes("\n")) return;
    out.push({ file, expr: e });
  };
  // 形状 ①
  for (const m of src.matchAll(
    /document\.getElementById\("app"\)\??\.(?:appendChild|append|prepend)\(\s*([^,)]+?)\s*\)/g,
  )) {
    push(m[1]);
  }
  for (const m of src.matchAll(
    /document\.getElementById\("app"\)\??\.insertBefore\(\s*([^,]+?)\s*,/g,
  )) {
    push(m[1]);
  }
  // 形状 ② / ③
  for (const name of appBindings(src)) {
    const esc = name.replace(/\$/g, "\\$");
    for (const m of src.matchAll(
      new RegExp(`\\b${esc}\\??\\.(?:appendChild|append|prepend)\\(\\s*([^,)]+?)\\s*\\)`, "g"),
    )) {
      push(m[1]);
    }
    for (const m of src.matchAll(
      new RegExp(`\\b${esc}\\??\\.insertBefore\\(\\s*([^,]+?)\\s*,`, "g"),
    )) {
      push(m[1]);
    }
  }
  // 形状 ④：插成 barEl 的兄弟
  for (const m of src.matchAll(
    /\.insertBefore\(\s*([^,]+?)\s*,\s*[\w.]*\bbarEl\b\.nextSibling\s*\)/g,
  )) {
    push(m[1]);
  }
  return out;
}

/**
 * `src/**\/*.ts` 的全文，键是仓根相对路径。
 *
 * ⚠ **刻意不做目录递归遍历**：`tests/scanning-guard-registry.vitest.ts` 有一条只降不升的
 * 棘轮「测试里做目录遍历的文件只许变少」（今天上限 11，已满），而抬那个上限是
 * **另一个文件的决定**，不该由一条新判据顺手替它做。
 * `import.meta.glob` 是构建期展开的，既拿到整棵树，又不新增一个遍历者。
 *
 * ⚠⚠ 连**在注释里写出那几个 API 的名字**都不行 —— 那条棘轮是按**子串**数的，
 * 所以「解释我为什么不用它」这句话本身就会把自己算成一个遍历者（实测：改成
 * `import.meta.glob` 之后仍然红，红在这行注释上）。这正是那个仓治过的
 * 「判据在自己的登记表里找到自己」，只是高了一层。
 *
 * 顺带还白捡一条：glob 只圈 `src/`，**结构上读不到本文件**
 * ⇒ 不会出现「判据在自己的登记表里找到自己」那种恒绿。
 */
const SRC_FILES: Record<string, string> = Object.fromEntries(
  Object.entries(
    import.meta.glob("../src/**/*.ts", { query: "?raw", import: "default", eager: true }) as Record<
      string,
      string
    >,
  ).map(([p, text]) => [p.replace(/^\.\.\//, ""), text]),
);

// ── 2. CSS 尺子：一个只认「规则 → 声明」的小解析器 ───────────────────────────

interface CssRule {
  readonly selectors: string[];
  readonly decls: Map<string, string>;
  /** 外层 at-rule 的 prelude 链（`@media …` / `@layer …`），空 = 无条件生效。 */
  readonly atRules: string[];
  /** 源序（越大越晚）。 */
  readonly order: number;
}

/** 去注释（尊重字符串，`url("…/*…")` 不会被误伤）。 */
function stripComments(css: string): string {
  let out = "";
  let i = 0;
  let quote: string | null = null;
  while (i < css.length) {
    const c = css[i];
    if (quote) {
      out += c;
      if (c === "\\") {
        out += css[i + 1] ?? "";
        i += 2;
        continue;
      }
      if (c === quote) quote = null;
      i += 1;
      continue;
    }
    if (c === '"' || c === "'") {
      quote = c;
      out += c;
      i += 1;
      continue;
    }
    if (c === "/" && css[i + 1] === "*") {
      const end = css.indexOf("*/", i + 2);
      i = end === -1 ? css.length : end + 2;
      out += " ";
      continue;
    }
    out += c;
    i += 1;
  }
  return out;
}

export function parseCss(rawCss: string): CssRule[] {
  const css = stripComments(rawCss);
  const rules: CssRule[] = [];
  const stack: string[] = [];
  let buf = "";
  let quote: string | null = null;
  let order = 0;
  for (let i = 0; i < css.length; i++) {
    const c = css[i];
    if (quote) {
      buf += c;
      if (c === "\\") {
        buf += css[i + 1] ?? "";
        i += 1;
        continue;
      }
      if (c === quote) quote = null;
      continue;
    }
    if (c === '"' || c === "'") {
      quote = c;
      buf += c;
      continue;
    }
    if (c === "{") {
      const prelude = buf.trim();
      buf = "";
      if (prelude.startsWith("@")) {
        stack.push(prelude);
        continue;
      }
      // 普通规则：读到配对的 `}` 为止（内部不再嵌套 —— 本仓没有原生 CSS 嵌套）
      const end = css.indexOf("}", i);
      const body = css.slice(i + 1, end === -1 ? css.length : end);
      rules.push({
        selectors: prelude
          .split(",")
          .map((s) => s.replace(/\s+/g, " ").trim())
          .filter(Boolean),
        decls: parseDecls(body),
        atRules: [...stack],
        order: order++,
      });
      i = end === -1 ? css.length : end;
      continue;
    }
    if (c === "}") {
      stack.pop();
      buf = "";
      continue;
    }
    buf += c;
  }
  return rules;
}

function parseDecls(body: string): Map<string, string> {
  const out = new Map<string, string>();
  let depth = 0;
  let quote: string | null = null;
  let cur = "";
  const flush = (): void => {
    const idx = cur.indexOf(":");
    if (idx > 0) {
      out.set(cur.slice(0, idx).trim().toLowerCase(), cur.slice(idx + 1).trim());
    }
    cur = "";
  };
  for (let i = 0; i < body.length; i++) {
    const c = body[i];
    if (quote) {
      cur += c;
      if (c === "\\") {
        cur += body[i + 1] ?? "";
        i += 1;
        continue;
      }
      if (c === quote) quote = null;
      continue;
    }
    if (c === '"' || c === "'") {
      quote = c;
      cur += c;
      continue;
    }
    if (c === "(") depth++;
    if (c === ")") depth--;
    if (c === ";" && depth === 0) {
      flush();
      continue;
    }
    cur += c;
  }
  flush();
  return out;
}

/** 把 `grid-template-areas` 的值拆成行（每行一串具名区域）。 */
function areaRows(value: string): string[][] {
  return [...value.matchAll(/"([^"]*)"/g)].map((m) => m[1].trim().split(/\s+/).filter(Boolean));
}

/** 数轨道数（括号感知：`minmax(0, 1fr)` 算一条）。 */
function trackCount(value: string): number {
  let depth = 0;
  let n = 0;
  let inTok = false;
  for (const c of value) {
    if (c === "(") depth++;
    else if (c === ")") depth--;
    if (depth === 0 && /\s/.test(c)) {
      inTok = false;
      continue;
    }
    if (!inTok) {
      n++;
      inTok = true;
    }
  }
  return n;
}

/** 某个选择器在某模式下拿到的一条属性（后来者赢；`body.viewer-mode X` 压过 `X`）。 */
function resolve(rules: CssRule[], selector: string, mode: Mode, prop: string): string | undefined {
  let best: { rank: number; order: number; value: string } | undefined;
  for (const r of rules) {
    // 带条件的 at-rule（媒体查询）不参与「无条件认领」的判定
    if (r.atRules.some((a) => /^@(?:media|supports|container)\b/.test(a))) continue;
    const v = r.decls.get(prop);
    if (v === undefined) continue;
    for (const sel of r.selectors) {
      let rank = -1;
      if (sel === selector) rank = 0;
      else if (mode === "viewer" && sel === `body.viewer-mode ${selector}`) rank = 1;
      if (rank < 0) continue;
      if (!best || rank > best.rank || (rank === best.rank && r.order > best.order)) {
        best = { rank, order: r.order, value: v };
      }
    }
  }
  return best?.value;
}

interface Template {
  readonly areas: Set<string>;
  readonly rows: number;
  readonly cols: number;
  readonly declaredRows: number;
  readonly declaredCols: number;
}

function templateOf(rules: CssRule[], mode: Mode): Template {
  const areasRaw = resolve(rules, "#app", mode, "grid-template-areas");
  const rowsRaw = resolve(rules, "#app", mode, "grid-template-rows");
  const colsRaw = resolve(rules, "#app", mode, "grid-template-columns");
  if (!areasRaw || !rowsRaw || !colsRaw) {
    throw new Error(`#app 在 ${mode} 模式下缺 grid 模板（areas/rows/columns 三条必须都在）`);
  }
  const grid = areaRows(areasRaw);
  return {
    areas: new Set(grid.flat().filter((a) => a !== ".")),
    rows: grid.length,
    cols: grid[0]?.length ?? 0,
    declaredRows: trackCount(rowsRaw),
    declaredCols: trackCount(colsRaw),
  };
}

type Claim =
  | { kind: "area"; detail: string }
  | { kind: "out-of-flow"; detail: string }
  | { kind: "not-an-item"; detail: string }
  | { kind: "none"; detail: string };

export function claimOf(rules: CssRule[], selector: string, mode: Mode, tpl: Template): Claim {
  const display = resolve(rules, selector, mode, "display");
  if (display === "none") return { kind: "not-an-item", detail: "display: none" };
  const position = resolve(rules, selector, mode, "position");
  if (position === "absolute" || position === "fixed") {
    return { kind: "out-of-flow", detail: `position: ${position}` };
  }
  const area = resolve(rules, selector, mode, "grid-area");
  if (area && tpl.areas.has(area)) return { kind: "area", detail: `grid-area: ${area}` };
  if (area) return { kind: "none", detail: `grid-area: ${area} —— 模板里没有这个具名区域` };
  return { kind: "none", detail: "既没有 grid-area、也没有 position、也没被 display:none 摘掉" };
}

// ── 3. 判据 ────────────────────────────────────────────────────────────────

const CSS = readFileSync(join(REPO_ROOT, "src/styles.css"), "utf8");
const RULES = parseCss(CSS);

describe("S24 · #app 的每个直接子元素都认领了格子", () => {
  it("尺 A 自检：合成一个新插入点，它必须被看见", () => {
    const fake = [
      'const appEl = document.getElementById("app");',
      "appEl.appendChild(ghostPanel);",
      'document.getElementById("app")?.appendChild(anotherGhost);',
      "const parent = this.barEl.parentElement;",
      "parent.insertBefore(thirdGhost, this.barEl.nextSibling);",
    ].join("\n");
    const found = censusAppInserts(fake, "fake.ts").map((s) => s.expr);
    expect(found).toContain("ghostPanel");
    expect(found).toContain("anotherGhost");
    expect(found).toContain("thirdGhost");
    // 反向：往别的容器里插的，不许被算进来
    expect(censusAppInserts("streamRoot.appendChild(card);", "fake.ts")).toHaveLength(0);
    expect(APP_INSERT_SHAPES).toHaveLength(4);
  });

  it("尺 A：源码里往 #app 插节点的调用点，与人群表逐条对得上", () => {
    const found: Site[] = [];
    for (const [rel, text] of Object.entries(SRC_FILES)) {
      found.push(...censusAppInserts(text, rel));
    }
    expect(Object.keys(SRC_FILES).length, "glob 没扫到 src 树 —— 下面那条会零命中地绿").toBeGreaterThan(80);
    const key = (s: Site): string => `${s.file} :: ${s.expr}`;
    const got = [...new Set(found.map(key))].sort();
    const want = [...new Set(INSERTED_CHILDREN.map(key))].sort();
    // 尺子得先证明自己不是空的
    expect(got.length).toBeGreaterThan(0);
    expect(got).toEqual(want);
  });

  it("尺 B：人群表里的每个 expr，在源码里确实被赋成了表里写的那个 class/id", () => {
    const sources = new Map(Object.entries(SRC_FILES));
    const all = [...sources.values()].join("\n");
    for (const c of INSERTED_CHILDREN) {
      // `a.b` 这种属性表达式，认的是那个属性名在哪被赋了 className
      const leaf = c.expr.includes(".") ? c.expr.slice(c.expr.lastIndexOf(".") + 1) : c.expr;
      const name = c.selector.slice(1);
      const hay = c.expr.includes(".") ? all : (sources.get(c.file) ?? "");
      const isId = c.selector.startsWith("#");
      const pat = isId
        ? new RegExp(`\\b${leaf}\\.id\\s*=\\s*"${name}"`)
        : new RegExp(`\\b${leaf}\\.className\\s*=\\s*"${name}(?:[ "])`);
      expect(pat.test(hay), `${c.file} 的 \`${c.expr}\` 应该被赋成 \`${c.selector}\``).toBe(true);
    }
    // index.html 里那三个静态的
    const html = readFileSync(join(REPO_ROOT, "index.html"), "utf8");
    for (const c of STATIC_CHILDREN) expect(html).toContain(c.expr);
  });

  it("尺 C 自检：把 .tab-archive 的 grid-area 摘掉，这条判据必须当场红", () => {
    const mutated = parseCss(CSS.replace(/(\.tab-archive\s*\{[^}]*?)\n\s*grid-area:\s*archive;/, "$1"));
    const tpl = templateOf(mutated, "default");
    expect(claimOf(mutated, ".tab-archive", "default", tpl).kind).toBe("none");
    // 未变异的那份必须是认领了的 —— 否则上面那条是「两边都红」的假证明
    expect(claimOf(RULES, ".tab-archive", "default", templateOf(RULES, "default")).kind).toBe("area");
  });

  for (const mode of BOTH) {
    describe(`模式 ${mode}`, () => {
      it("模板自洽：具名区域的行/列数 == 声明的轨道数（隐式行数因此被钉在 0）", () => {
        const tpl = templateOf(RULES, mode);
        expect(tpl.rows, "grid-template-areas 的行数 != grid-template-rows 的轨道数").toBe(
          tpl.declaredRows,
        );
        expect(tpl.cols, "grid-template-areas 的列数 != grid-template-columns 的轨道数").toBe(
          tpl.declaredCols,
        );
        expect(tpl.areas.size).toBeGreaterThan(0);
      });

      it("每个 in-flow 直接子元素都认领了一个本模板里声明过的格子", () => {
        const tpl = templateOf(RULES, mode);
        const unclaimed: string[] = [];
        for (const c of ALL_CHILDREN) {
          if (!c.modes.includes(mode)) continue;
          const claim = claimOf(RULES, c.selector, mode, tpl);
          if (claim.kind === "none") {
            unclaimed.push(`${c.selector}（插入点 ${c.file} :: ${c.expr}）—— ${claim.detail}`);
          }
        }
        expect(
          unclaimed,
          `${mode} 模式下 #app 有没认领格子的 in-flow 子元素 —— 浏览器会给它开隐式行，` +
            "把高度从别的格子里偷走（病史见本文件抬头）",
        ).toEqual([]);
      });

      it("模板里声明的每个具名区域都有人认领（没有空转的行）", () => {
        const tpl = templateOf(RULES, mode);
        const claimed = new Set<string>();
        for (const c of ALL_CHILDREN) {
          if (!c.modes.includes(mode)) continue;
          const claim = claimOf(RULES, c.selector, mode, tpl);
          if (claim.kind === "area") claimed.add(claim.detail.replace("grid-area: ", ""));
        }
        expect([...tpl.areas].filter((a) => !claimed.has(a))).toEqual([]);
      });
    });
  }
});
