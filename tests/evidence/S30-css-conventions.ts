/**
 * `S30` —— **CSS 三条约定的量具**（`设计/40 §7` 步 9 ① · `设计/41 §7` · `设计/41 §10`）。
 *
 * 🔴 **这份文件只是量具，不是判据。** 登记表与判定住 `tests/css-conventions.vitest.ts`。
 * 分家的理由与 `tests/evidence/S25-class-ledger.ts` 逐字相同，这里不复述一份：
 * `tests/scanning-guard-registry.vitest.ts` 的 `WALKER_CEILING` 只数
 * `.vitest.ts` / `.test.ts`（`IS_TEST`）里的遍历者，量具住在 `tests/evidence/` 下的
 * 普通 `.ts` 里既满足那条棘轮，又让死值验能对着**镜像树**跑（`cssFacts(<别的根>)`）。
 *
 * ## 为什么不是扩 `tests/css-ledger.vitest.ts`
 *
 * 那本账（`S25`）管的是**类名与 z-index 的两个方向**，它的读数
 * `tests/evidence/S25-css-ledger-readings.md` 逐字写着「装了 4 格 / 13 条断言」。
 * 本轮加的是**另外三件事实**（令牌对账 · 过渡属性 · 状态载体）。
 * 把新格塞进那个文件会让一份**已冻结的读数**当场变成假话，而那份 `.md` 是历史记录。
 * ⇒ 新事实新住址；两边在头注里互指，**同一件事实不会有两本账**。
 *
 * ## 它量三件事
 *
 * | | 量什么 | 给哪一格用 |
 * |---|---|---|
 * | `cssFacts` | CSS 里**定义过**的自定义属性 · **`var()` 到**的自定义属性 · `transition` 动的属性 · 规则表 | ⑤ ⑥ ⑦ |
 * | `setPropertyVars` | TS 里 `style.setProperty("--x", …)` 设的名字 | ⑤ |
 * | `themeTokens` | `src/theme.ts` 的 `TOKEN_MAP` 暴露给用户的那几个 `cssVar` | ⑤ |
 * | `hiddenSites` | TS 里 `<元素>.hidden = …` 的每一处，以及那个元素挂的类名 | ⑦ |
 *
 * ## 🔴 三个会让尺子静默给出错答案的地方（都踩过，逐条钉住）
 *
 * 1. **CSS 注释里的散文会被当成代码。** `src/styles/tokens.css` 的头注逐字写着
 *    「z-index 只许写 `var(--z-*)`」—— 不剥注释就会多出一个叫 `--z-` 的「未定义变量」，
 *    而它根本不存在。⇒ `stripCssComments` 是承重的，判据那边有一条自检盯着它真的剥掉了东西。
 * 2. **同名局部变量跨函数串味。** `src/tabs.ts` 里 `wrap` / `list` 这两个名字在
 *    `ensureArchiveUi()`（`.tab-archive` / `.tab-archive-list`）与建分组那段
 *    （`.tab-group` / `.tab-group-list`）各用了一次。按「整份文件里这个名字挂过哪些类」
 *    去收，一个 `.hidden =` 会同时认领四个类名 —— **四个里有两个是错的**。
 *    ⇒ 取的是**离它最近的那一次上游赋值**（`nearest`），不是全文件并集。
 * 3. **接收者要整段比，不能只比最后一个标识符。** `src/settings/panel.ts:606` 是
 *    `b.el.hidden = …`，而同一份文件 `:353` 有 `this.el.classList.add("open")`。
 *    只比 `el` 会把设置面板自己的 `.open` 安到别人头上（第一版实测就这么错的）。
 *    ⇒ `RE_HIDDEN` / `RE_CLASSNAME` 抓的都是**完整的点号链**，逐字相等才算同一个元素。
 *
 * ## 它买不到什么（诚实边界）
 *
 * - **不是 JS 真语法。** 词法靠正则，`nearest` 靠行号而不是作用域。失效的样子是
 *   「解析不出类名」⇒ 落进 `classes === null`，而那一族在判据那边是**逐处登记的恒等**，
 *   多一处少一处都红 —— 失效是吵闹的，不是静默绿。
 * - **只看 `src/`。** 与 `S25` 同一条取法（理由见那份文件：把 `tests/` 收进语料，
 *   一条「断言它不该存在」的测试会把死规则救活）。
 * - **CSS 侧的「定义过」是集合式的**，不看层叠作用域：`.foo { --x: 1 }` 定义的 `--x`
 *   在本量具眼里就算「CSS 里定义过」，哪怕用它的是另一棵子树。`设计/40 §7` 步 9 ①
 *   要的就是集合式的那条（逐字：「CSS 里 `var(--x)` 的集合 ⊆ (CSS 定义 ∪ …)」）。
 * - **`transition` 的属性名只认静态字面量**：值里写 `var(--something)` 当属性名的话看不见。
 *   现打 0 处，属潜伏不是现患。
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const SKIP_DIRS = new Set(["node_modules", ".git", ".build", "coverage", "dist", "target"]);

function walk(dir: string, keep: (p: string) => boolean, out: string[] = []): string[] {
  for (const e of readdirSync(dir)) {
    if (SKIP_DIRS.has(e)) continue;
    const p = join(dir, e);
    if (statSync(p).isDirectory()) walk(p, keep, out);
    else if (keep(p)) out.push(p);
  }
  return out;
}

const relOf = (root: string, p: string): string => relative(root, p).split("\\").join("/");

/** 把 `/* … *\/` 换成等长的空白（行号不许漂，否则所有住址都错位）。 */
export function stripCssComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "));
}

const lineAt = (src: string, index: number): number => {
  let n = 1;
  for (let i = 0; i < index; i++) if (src.charCodeAt(i) === 10) n++;
  return n;
};

/** 一处「这个属性被过渡了」。`decl` 是那条声明的原文，方便红的时候把住址印全。 */
export interface TransitionUse {
  file: string;
  line: number;
  prop: string;
  decl: string;
}

/** 一条 CSS 规则。`sel` 未拆逗号，`body` 是花括号里的原文。 */
export interface CssRule {
  file: string;
  line: number;
  sel: string;
  body: string;
}

export interface CssFacts {
  root: string;
  /** `src/**\/*.css` —— 仓相对路径。 */
  cssFiles: string[];
  /** 剥注释**之前**的总字符数与之后的差（自检用：注释真的被剥掉了吗）。 */
  strippedChars: number;
  /** 自定义属性的**定义**（`--x: …`）→ 住址。 */
  defined: Map<string, string[]>;
  /** 自定义属性的**使用**（`var(--x…)`）→ 住址。 */
  used: Map<string, string[]>;
  /**
   * 同上，但**不剥注释**。只给判据那边的正控用：`usedRaw − used` 必须恰好是
   * 注释散文里那几个假名字。两边一样 ⇒ 剥注释这一步没在承重，正控当场红。
   */
  usedRaw: Map<string, string[]>;
  /** 每一处被 `transition` / `transition-property` 动到的属性名。 */
  transitions: TransitionUse[];
  /** `transition` / `transition-property` 声明的条数（分母，不是属性个数）。 */
  transitionDecls: number;
  /** 规则表（判 ⑦ 用）。 */
  rules: CssRule[];
}

const pushSite = (m: Map<string, string[]>, k: string, site: string): void => {
  const list = m.get(k);
  if (list) list.push(site);
  else m.set(k, [site]);
};

/** 走一遍 `src/` 的 CSS，把 ⑤⑥⑦ 要的四张表建出来。`root` 是仓根。 */
export function cssFacts(root: string): CssFacts {
  const paths = walk(join(root, "src"), (p) => p.endsWith(".css"));
  const facts: CssFacts = {
    root,
    cssFiles: paths.map((p) => relOf(root, p)),
    strippedChars: 0,
    defined: new Map(),
    used: new Map(),
    usedRaw: new Map(),
    transitions: [],
    transitionDecls: 0,
    rules: [],
  };
  for (const p of paths) {
    const rel = relOf(root, p);
    const raw = readFileSync(p, "utf8");
    const src = stripCssComments(raw);
    facts.strippedChars += raw.replace(/\s/g, "").length - src.replace(/\s/g, "").length;

    for (const m of src.matchAll(/(?:^|[;{\s])(--[A-Za-z0-9_-]+)\s*:/g)) {
      pushSite(facts.defined, m[1], `${rel}:${lineAt(src, m.index)}`);
    }
    for (const m of src.matchAll(/var\(\s*(--[A-Za-z0-9_-]+)/g)) {
      pushSite(facts.used, m[1], `${rel}:${lineAt(src, m.index)}`);
    }
    for (const m of raw.matchAll(/var\(\s*(--[A-Za-z0-9_-]+)/g)) {
      pushSite(facts.usedRaw, m[1], `${rel}:${lineAt(raw, m.index)}`);
    }
    for (const m of src.matchAll(/(?:^|[;{}\s])(transition|transition-property)\s*:\s*([^;}]+)/g)) {
      facts.transitionDecls++;
      const line = lineAt(src, m.index);
      const decl = `${m[1]}: ${m[2].trim().replace(/\s+/g, " ")}`;
      for (const part of m[2].split(",")) {
        // `transition` 简写里属性名恒是第一个 token；`transition-property` 整段就是属性名。
        const prop = part.trim().split(/\s+/)[0];
        if (prop) facts.transitions.push({ file: rel, line, prop, decl });
      }
    }
    for (const m of src.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      const sel = m[1].trim();
      if (sel === "" || sel.startsWith("@")) continue; // at-rule 的头不是选择器
      facts.rules.push({ file: rel, line: lineAt(src, m.index), sel, body: m[2] });
    }
  }
  return facts;
}

const tsSources = (root: string): { rel: string; text: string }[] =>
  walk(
    join(root, "src"),
    (p) => p.endsWith(".ts") && !p.endsWith(".d.ts") && !p.endsWith(".vitest.ts"),
  ).map((p) => ({ rel: relOf(root, p), text: readFileSync(p, "utf8") }));

/** TS 里 `…style.setProperty("--x", …)` 设过的自定义属性 → 住址。 */
export function setPropertyVars(root: string): Map<string, string[]> {
  const out = new Map<string, string[]>();
  for (const s of tsSources(root)) {
    for (const m of s.text.matchAll(/setProperty\(\s*["'`](--[A-Za-z0-9_-]+)["'`]/g)) {
      pushSite(out, m[1], `${s.rel}:${lineAt(s.text, m.index)}`);
    }
  }
  return out;
}

/** `src/theme.ts` 的 `TOKEN_MAP` 暴露给用户调的那几个 `cssVar`（按出现顺序）。 */
export function themeTokens(root: string): string[] {
  const text = readFileSync(join(root, "src", "theme.ts"), "utf8");
  return [...text.matchAll(/cssVar:\s*["'`](--[A-Za-z0-9_-]+)["'`]/g)].map((m) => m[1]);
}

/** 一处 `<元素>.hidden = …`。`classes === null` ＝ 静态解析不出它挂的是哪个类。 */
export interface HiddenSite {
  file: string;
  line: number;
  /** 完整的接收者表达式，如 `this.panel` / `b.el`。 */
  recv: string;
  /** 解析到的类名（可能多个：`className = "a b"`）；解析不出就是 `null`。 */
  classes: string[] | null;
  /** 类名是从哪一行的上游赋值取到的。 */
  from: number | null;
  /** 该行原文（去两端空白），红的时候印出来。 */
  text: string;
}

const EXPR = "(?:this\\.)?[A-Za-z_$][\\w$]*(?:\\.[A-Za-z_$][\\w$]*)*";
const RE_CLASSNAME = new RegExp(`(${EXPR})\\.className\\s*=\\s*["'\`]([^"'\`$]*)`);
const RE_CLASSADD = new RegExp(`(${EXPR})\\.classList\\.add\\(\\s*["'\`]([^"'\`$]*)["'\`]`);
const RE_ALIAS = new RegExp(`^\\s*(this\\.[A-Za-z_$][\\w$]*)\\s*=\\s*([A-Za-z_$][\\w$]*)\\s*;`);
const RE_HIDDEN = new RegExp(`(${EXPR})!?\\.hidden\\s*=`);

/**
 * 全仓 `src/**\/*.ts` 里每一处 `<元素>.hidden = …`，连同那个元素挂的类名。
 *
 * 解析只有两跳，**刻意不做更多**（多跳会把「解析不出」变成「解析错」，而后者是静默的）：
 * ① 接收者自己的 `className` / `classList.add`（取**行号在它之前、离它最近**的那一次）；
 * ② `this.<字段> = <局部变量>;` 这一种转手，再按 ① 去找那个局部变量。
 */
export function hiddenSites(root: string): HiddenSite[] {
  const out: HiddenSite[] = [];
  for (const s of tsSources(root)) {
    const lines = s.text.split("\n");
    const cls: { line: number; v: string; classes: string[] }[] = [];
    const alias: { line: number; field: string; src: string }[] = [];
    lines.forEach((l, i) => {
      const m = RE_CLASSNAME.exec(l) ?? RE_CLASSADD.exec(l);
      if (m) cls.push({ line: i + 1, v: m[1], classes: m[2].trim().split(/\s+/).filter(Boolean) });
      const a = RE_ALIAS.exec(l);
      if (a) alias.push({ line: i + 1, field: a[1], src: a[2] });
    });
    const nearest = (v: string, before: number): { line: number; classes: string[] } | null => {
      let best: { line: number; classes: string[] } | null = null;
      for (const c of cls) {
        if (c.v !== v || c.line > before) continue;
        if (!best || c.line > best.line) best = { line: c.line, classes: c.classes };
      }
      return best;
    };
    lines.forEach((l, i) => {
      const m = RE_HIDDEN.exec(l);
      if (!m) return;
      const recv = m[1];
      const line = i + 1;
      let hit = nearest(recv, line);
      if (!hit) {
        let a: { line: number; field: string; src: string } | null = null;
        for (const x of alias) if (x.field === recv && (!a || x.line > a.line)) a = x;
        if (a) hit = nearest(a.src, a.line);
      }
      out.push({
        file: s.rel,
        line,
        recv,
        classes: hit ? hit.classes : null,
        from: hit ? hit.line : null,
        text: l.trim(),
      });
    });
  }
  return out;
}

/** 一个「会被 `hidden` 切换」的类名在 CSS 里的处境。 */
export interface DisplayVerdict {
  /** 主语是这个类、且写了 `display` 的规则（不含 `[hidden]` 那条）。 */
  bare: string[];
  /** 有没有配套的 `…[hidden] { display: none }`。 */
  guarded: boolean;
}

/**
 * 判一个类名：CSS 有没有在**它自己身上**写 `display`，以及有没有 `[hidden]` 兜底。
 *
 * 只看**主语**（选择器最后那一个复合），因为 `hidden` 属性只被元素自己的 `display` 压过；
 * 祖先选择器里出现这个类（`.foo .bar { display: … }`）压的是别人，不是它。
 */
export function displayVerdict(facts: CssFacts, cls: string): DisplayVerdict {
  const subj = new RegExp(`\\.${cls}(?![\\w-])`);
  const bare: string[] = [];
  let guarded = false;
  for (const r of facts.rules) {
    for (const one of r.sel.split(",").map((x) => x.trim())) {
      const last = one.split(/[\s>+~]+/).filter(Boolean).pop() ?? "";
      if (!subj.test(last)) continue;
      if (/\[hidden\]/.test(last)) {
        if (/display\s*:\s*none/.test(r.body)) guarded = true;
        continue;
      }
      if (/(?:^|[;\s])display\s*:/.test(r.body)) bare.push(`${r.file}:${r.line} ${one}`);
    }
  }
  return { bare, guarded };
}

// ─────────────── ⑧ 同一个状态名不许同现于两种载体（〔W5-AUX〕`设计/41 §7` 一般形式）───────────────

/**
 * 〔W5-AUX · 41 #19〕状态的两种**有名字的**载体：类名 · `data-*`。
 *
 * `设计/41 §7` 逐字：「状态只用**一种**载体承载，不许同一个状态在类名、`data-*`、内联 `style` 三处各写一遍」·
 * 「**没判的**：一般形式『同一个状态名不得同时出现在两种载体里』今天没有判据」。
 *
 * 两种形都量（缺一种就有一族看不见，现打各逮到一例）：
 * - **名字形**：一个状态名既当类名用（`classList` 切的字面量 · 模板拼的类名族 `<名>-${值}` · CSS 复合选择器里挂在后面的修饰类），
 *   又当 `data-<名>` 用。类名去掉 `is-` / `has-` 前缀再比（`.is-open` 与 `data-open` 是同一个名字）。
 * - **同一处写两遍形**：同一个接收者（逐字整段比，同 `hiddenSites` 那条纪律），同一个值表达式，
 *   既拼进了类名模板的洞（或 `classList.toggle` 的第二个实参），又写进了 `data-*`（`dataset.x = …` / `setAttribute("data-x", …)`）。
 *   这一形名字对不上（类名族叫 `remote-gap-…`、属性叫 `data-kind`），只有按值认得出来。
 */
export interface StateCarriers {
  /** 当类名用的状态名（已去 `is-` / `has-`）→ 住址（`文件 · 原类名`）。 */
  classNames: Map<string, string[]>;
  /** `data-*` 的名字（去掉 `data-`，驼峰已转回连字符）→ 住址。 */
  dataNames: Map<string, string[]>;
  /** `classList.*(…)` 的实参里一个字符串字面量都没有的调用点 —— 静态认不出切的是哪个类（`文件 · 调用原文`）。 */
  unresolved: string[];
  /** 同一处写两遍：同一接收者、同一值表达式进了两种载体。 */
  sameWrite: string[];
  /** 分母：扫过的 TS / CSS / HTML 份数。 */
  scanned: { ts: number; css: number; html: number };
}

/** 类名 → 状态名：去掉 `is-` / `has-` 前缀（`.is-active` 与 `data-active` 是同一个名字）。 */
export const stateNameOfClass = (c: string): string => c.replace(/^(?:is|has)-/, "");
/** `dataset.fooBar` 的键 → `foo-bar`（与 `data-foo-bar` 同名）。 */
export const kebabOfDatasetKey = (k: string): string => k.replace(/[A-Z]/g, (x) => `-${x.toLowerCase()}`);

/** 名字形的判定（纯）：两种载体都出现过的状态名 → 两边的住址。 */
export function stateNameCollisions(c: Pick<StateCarriers, "classNames" | "dataNames">): Map<string, string[]> {
  const out = new Map<string, string[]>();
  for (const [name, sites] of c.classNames) {
    const d = c.dataNames.get(name);
    if (d) out.set(name, [...new Set(sites.map((s) => `类名 ${s}`))].concat([...new Set(d.map((s) => `data-${name} ${s}`))]));
  }
  return out;
}

const RECV = "(?:this\\.)?[A-Za-z_$][\\w$]*(?:\\.[A-Za-z_$][\\w$]*)*";

/** 从一份去过注释的 TS 里收两种载体 ＋ 同一处写两遍（纯；`rel` 只用来写住址）。 */
export function stateCarriersOfTs(
  rel: string,
  text: string,
  into: Pick<StateCarriers, "classNames" | "dataNames" | "unresolved" | "sameWrite">,
): void {
  const push = (m: Map<string, string[]>, k: string, site: string): void => pushSite(m, k, site);
  // ① `classList.add/remove/toggle/replace(…)` 的字符串字面量（`toggle` 只有第一个实参是类名）。
  for (const m of text.matchAll(/\bclassList\.(add|remove|toggle|replace)\(([^)\n]*)\)/g)) {
    const lits = [...m[2].matchAll(/"([^"\n$]*)"|'([^'\n$]*)'|`([^`\n$]*)`/g)].map((x) => x[1] ?? x[2] ?? x[3] ?? "");
    if (lits.length === 0) {
      into.unresolved.push(`${rel} · ${m[0]}`);
      continue;
    }
    for (const l of m[1] === "toggle" ? lits.slice(0, 1) : lits) {
      for (const t of l.split(/\s+/).filter(Boolean)) push(into.classNames, stateNameOfClass(t), `${rel} · ${t}`);
    }
  }
  // ② 类名模板里紧挨着洞的 `<名>-${…}` ⇒ 那一族类名的状态名就是 `<名>`。
  const classTpl = new RegExp(`(${RECV})\\.className\\s*(?:=|\\+=)\\s*\`([^\`]*)\``, "g");
  const holes: { recv: string; expr: string }[] = [];
  for (const m of text.matchAll(classTpl)) {
    for (const h of m[2].matchAll(/(?:^|\s)([A-Za-z][\w-]*-)?\$\{([^}]*)\}/g)) {
      if (h[1]) push(into.classNames, stateNameOfClass(h[1].slice(0, -1)), `${rel} · ${h[1]}\${…}`);
      holes.push({ recv: m[1], expr: h[2].trim() });
    }
  }
  for (const m of text.matchAll(new RegExp(`(${RECV})\\.classList\\.toggle\\(\\s*["'\`][^"'\`]*["'\`]\\s*,\\s*([^)\\n]+)\\)`, "g"))) {
    holes.push({ recv: m[1], expr: m[2].trim() });
  }
  // ③ `data-*`：`dataset.x` · `*Attribute("data-x"` · 字符串里的 `[data-x` / `data-x=`（选择器与 HTML 片段）。
  const dataWrites: { recv: string; name: string; expr: string }[] = [];
  for (const m of text.matchAll(/\bdataset\.([A-Za-z_$][\w$]*)/g)) push(into.dataNames, kebabOfDatasetKey(m[1]), rel);
  for (const m of text.matchAll(/\b(?:set|get|remove|toggle|has)Attribute\(\s*["'`]data-([\w-]+)["'`]/g)) push(into.dataNames, m[1], rel);
  for (const m of text.matchAll(/\[data-([\w-]+)/g)) push(into.dataNames, m[1], rel);
  for (const m of text.matchAll(/\bdata-([\w-]+)=/g)) push(into.dataNames, m[1], rel);
  // 属性表对象的键（`svgEl("g", { "data-conf": … })` 那一形）。
  for (const m of text.matchAll(/["'`]data-([\w-]+)["'`]\s*:/g)) push(into.dataNames, m[1], rel);
  for (const m of text.matchAll(new RegExp(`(${RECV})\\.dataset\\.([A-Za-z_$][\\w$]*)\\s*=(?!=)\\s*([^;\\n]+)`, "g"))) {
    dataWrites.push({ recv: m[1], name: kebabOfDatasetKey(m[2]), expr: m[3].trim() });
  }
  for (const m of text.matchAll(new RegExp(`(${RECV})\\.setAttribute\\(\\s*["'\`]data-([\\w-]+)["'\`]\\s*,\\s*([^)\\n]+)\\)`, "g"))) {
    dataWrites.push({ recv: m[1], name: m[2], expr: m[3].trim() });
  }
  // ④ 同一处写两遍：同一接收者 × 同一值表达式。
  for (const h of holes) {
    for (const d of dataWrites) {
      if (d.recv === h.recv && d.expr === h.expr) into.sameWrite.push(`${rel} · ${h.recv}：类名模板里的 \${${h.expr}} 与 data-${d.name} 写的是同一个值`);
    }
  }
}

/** CSS 复合选择器里挂在后面的修饰类（`.tab.ended` 里的 `ended`）；`[data-x` 属性选择器的名字。纯。 */
export function stateCarriersOfCss(rel: string, src: string, into: Pick<StateCarriers, "classNames" | "dataNames">): void {
  const clean = stripCssComments(src);
  for (const m of clean.matchAll(/([^{};]+)\{/g)) {
    const sel = m[1].trim();
    if (sel === "" || sel.startsWith("@")) continue;
    for (const compound of sel.split(/[\s,>+~]+/)) {
      const cls = [...compound.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)].map((x) => x[1]);
      for (const c of cls.slice(1)) pushSite(into.classNames, stateNameOfClass(c), `${rel} · .${c}`);
    }
    for (const a of sel.matchAll(/\[data-([\w-]+)/g)) pushSite(into.dataNames, a[1], rel);
  }
}

/** 走 `src/` 的 TS（去注释）与 CSS、仓根三份 HTML，把两种载体收齐。 */
export function stateCarriers(root: string, stripTs: (src: string) => string): StateCarriers {
  const out: StateCarriers = {
    classNames: new Map(),
    dataNames: new Map(),
    unresolved: [],
    sameWrite: [],
    scanned: { ts: 0, css: 0, html: 0 },
  };
  for (const s of tsSources(root)) {
    stateCarriersOfTs(s.rel, stripTs(s.text), out);
    out.scanned.ts++;
  }
  for (const p of walk(join(root, "src"), (x) => x.endsWith(".css"))) {
    stateCarriersOfCss(relOf(root, p), readFileSync(p, "utf8"), out);
    out.scanned.css++;
  }
  for (const h of ["index.html", "settings.html", "viewer.html"]) {
    const text = readFileSync(join(root, h), "utf8");
    for (const m of text.matchAll(/\bdata-([\w-]+)=/g)) pushSite(out.dataNames, m[1], h);
    out.scanned.html++;
  }
  return out;
}
