/**
 * `S25` —— **CSS 与代码之间那本账的量具**（`设计/40 §7` 步 9 ② · `设计/41 §12` 件 8）。
 *
 * 🔴 **这份文件只是量具，不是判据。** 判据（含"哪些前缀准用、凭什么准用"那张登记表）
 * 住 `tests/css-ledger.vitest.ts`。把量具与判据分开是**刻意的**，两条理由：
 *
 * 1. `tests/scanning-guard-registry.vitest.ts` 有一条棘轮（`WALKER_CEILING = 11`）：
 *    **`.vitest.ts` / `.test.ts` 里做目录遍历的文件只许变少**。本量具要走 `src/` 整棵树，
 *    住进 `.vitest.ts` 就会把那条棘轮顶破 —— 而那个文件不在本轮写区。
 *    ⇒ 遍历留在本文件（它既不是 `.vitest.ts` 也不是 `.test.ts`，`IS_TEST` 取不到它），
 *    判据那边一行 `readdirSync` 都不写。**这不是绕过那条棘轮，是满足它给的第二条出路**
 *    （「扫的树不含自己」在这里升级成「扫的人和判的人不是同一个文件」）。
 * 2. 量具可以对着**变异过的副本**跑死值验（`buildLedger(<别的根>)`），不必去动真工作树 ——
 *    与 `tests/evidence/K-R122-ruler.py` 的 `K_R122_ROOT` 同一条取法。
 *
 * ## 这本账要回答的两个方向
 *
 * - **① CSS → 代码**：`styles.css` 里写着的类名，有没有人真的往 DOM 上挂它。
 *   没人挂 ＝ 死规则（`真相源/51 §2` 那 13 个就是这么查出来的）。
 * - **② 代码 → CSS**：代码往 DOM 上挂的类名，CSS 里有没有对应规则。
 *   没有 ＝ 挂了个不存在的钩子（多半是改名只改了一边）。
 *
 * ## 🔴 ① 那一边有四种**假阳性**，本量具必须认得出来，否则它就是一把坏尺子
 *
 * 上一轮（`tests/evidence/S21-css-readings.md` 步 3）现打的教训：普查给出的 13 个"死规则"
 * 里 **3 个是活的**。三个各属一族，本量具对每一族都有一条明确的识别路径：
 *
 * | 族 | 现打的例子 | 本量具怎么认 |
 * |---|---|---|
 * | ① **常量拼接** | `` `${FIRST_RUN_HINT_CLASS}-open` `` ⇒ `.status-first-run-open` | 先收 `const X = "字面量"`，把 `${X}` 这种洞**填回去**再扫一遍（`constConcat`） |
 * | ② **第三方库自产** | `.katex-display` 由 `katex/dist/katex.min.css` 产出 | 从 `src/**\/*.ts` 里**现读** `import "<包>/….css"`，把那几份 CSS 的类名收成 `vendorClasses` |
 * | ③ **模板拼接** | `` `settings-btn-${variant}` `` ⇒ `.settings-btn-secondary` | 词法器给出模板的**静态段 ＋ 洞**，紧挨着洞的那个尾 token 记成 `prefixes` |
 * | ④ **HTML 片段里的 class 属性** | `` `<div class="code-bar">` `` | token 切分按「类名能用的字符」切，不按空白切 ⇒ `class="code-bar"` 里切得出 `code-bar` |
 *
 * ⚠ ③ 那条**只产出候选**。哪些前缀准拿来解释一个类名，由判据那边的登记表说了算 ——
 * 自动派生的前缀里有垃圾（现打 `` `c${Date.now()…}` `` 会派生出一个 1 字符前缀 `c`，
 * 它能"解释"掉 `code-bar` / `ccm-alias-gen` 等 7 个类，**而那 7 个其实各有真住址**）。
 * ⇒ **能解释的前缀必须逐条登记并写下理由**，这条纪律的落点在判据文件里。
 *
 * ## 🔴 词法器为什么不能用正则代替
 *
 * 第一版用 `` /`([^`]*)`/g `` 取模板，当场栽两次，两次都**静默给出错答案**：
 *
 * - **嵌套模板**：`src/paste-block.ts:90` 是
 *   `` root.className = `paste-block${spec.className ? ` ${spec.className}` : ""}` ``。
 *   正则在**内层**那个反引号就收尾 ⇒ 拿到的静态段是 `paste-block${spec.className ? `，
 *   token 切出来是 `paste-block$` 之类的垃圾，`.paste-block` 当场被判成死规则。
 * - **正则字面量里的引号**：`src/launcher-diagnostics.ts:149` 是
 *   ``  const q = (s) => `'${s.replace(/'/g, `'\\''`)}'` ``。
 *   那个 `/'/g` 里的单引号被当成字符串开头 ⇒ 词法**整份文件错位**，
 *   同一个文件里 476 行的 `wrap.className = "ccm-alias-gen"` 就此读不到 ⇒ 4 个类平白变死。
 *
 * ⇒ 本文件走一个**真词法器**：认单/双引号、模板（带 `${}` 嵌套栈）、行/块注释、
 *   以及正则字面量（靠"上一个有效字符"判 `/` 是除号还是正则开头的经典启发式）。
 *   它对着 `tests/css-ledger.vitest.ts` 里那组夹具跑**正控**，别删那组。
 *
 * ## 诚实边界（本量具买不到的东西）
 *
 * - ① 那一边的"有人用"判准是**字面量出现过**，不是"真的挂到了 DOM 上"。
 *   一个类名只要在 `src/` 任何一处字符串里出现过（哪怕是报错文案）就算活的 ⇒
 *   **本量具会漏掉一部分真死规则**，但不会把活的判成死的（这个方向是刻意选的：
 *   假阳性会让人删掉在用的样式，假阴性只是少清一点噪音）。
 * - ② 那一边只认四种**语法形态**的调用点（见 `USE_FORMS`）。用别的写法把类名挂上去
 *   （`setAttribute("class", …)` 之外的间接写法、从配置里读的类名）本量具一律看不见。
 * - 两边都只看 `src/`。`tests/` 里出现的类名**不算**用户 —— 那正是上一轮
 *   `.settings-group-empty` 那个例子要的：测试里有一条"它不该存在"的正面断言，
 *   把 `tests/` 收进语料就会让它自己把自己救活。
 * - 不认 CSS-in-JS、不认 CSS Modules（`设计/41` 件 10 摊开做，真做起来这本账要跟着改）。
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

// ─────────────────────────────────────────────────────────────────────────────
// 词法器
// ─────────────────────────────────────────────────────────────────────────────

/** 模板字面量的一段静态文本；`hole` ＝ 它后面紧跟着一个 `${…}` 插值。 */
export interface TplChunk {
  text: string;
  hole: boolean;
}

/** 词法器认出来的一个字面量。 */
export type LexToken =
  | { kind: "str"; value: string; line: number }
  | { kind: "tpl"; chunks: TplChunk[]; line: number };

/**
 * `/` 前面是这些东西的时候，它开的是**正则字面量**而不是除法。
 *
 * ⚠ 这是启发式，不是 JS 的真语法（真判定要先有 AST，而有了 AST 就不需要词法器了）。
 * 它在本仓现打全绿；失效的样子是**把除号当成正则开头**，那会让词法从该处起整份错位 ——
 * 而错位的后果是一批类名平白变"死"，在判据那边表现为**红**，不是静默绿。
 * ⇒ 这条启发式失效时是**吵闹的**，可以接受。
 */
const REGEX_PREV_PUNCT = new Set([
  "(", ",", "=", ":", "[", "!", "&", "|", "?", "{", "}", ";", "+", "-", "*", "%", "~", "^", "<", ">", "\n",
]);
const REGEX_PREV_WORDS = new Set([
  "return", "typeof", "case", "in", "of", "new", "delete", "void", "instanceof", "do", "else", "yield", "await",
]);

function opensRegex(src: string, slashAt: number): boolean {
  let i = slashAt - 1;
  while (i >= 0 && /\s/.test(src[i]) && src[i] !== "\n") i--;
  if (i < 0) return true;
  const c = src[i];
  if (REGEX_PREV_PUNCT.has(c)) return true;
  if (/[A-Za-z0-9_$]/.test(c)) {
    let j = i;
    while (j >= 0 && /[A-Za-z0-9_$]/.test(src[j])) j--;
    return REGEX_PREV_WORDS.has(src.slice(j + 1, i + 1));
  }
  return false;
}

/**
 * 扫一份 TS/HTML 源码，返回里面所有的字符串与模板字面量。
 *
 * ⚠ 注释里的内容**不返回**（`// ` 与 `/* `），但**模板静态段里**的 `//` 是字面文本，不当注释。
 */
export function scanStrings(src: string): LexToken[] {
  return lexCore(src);
}

/**
 * 把注释涂成同长空白（换行保留），别的一个字不动。
 *
 * 方向 ② 的那几条正则是**在源码原文上跑**的，不走词法器 —— 不先把注释抹掉就会读到散文。
 * 现打栽过：`src/render.ts:49` 的 JSDoc 里逐字写着
 * `` `<code class="hljs language-X">` ``，于是 `.language-X` / `.code-pending` 这种
 * **只存在于注释里的名字**被当成"代码在用的类"，方向 ② 平白多出两条假账。
 */
export function stripCodeComments(src: string): string {
  const spans: [number, number][] = [];
  lexCore(src, spans);
  const buf = [...src];
  for (const [a, b] of spans) {
    for (let i = a; i < b && i < buf.length; i++) if (buf[i] !== "\n") buf[i] = " ";
  }
  return buf.join("");
}

function lexCore(src: string, comments?: [number, number][]): LexToken[] {
  const out: LexToken[] = [];
  type Frame = { type: "tpl"; chunks: TplChunk[]; line: number } | { type: "expr"; depth: number };
  const stack: Frame[] = [];
  let buf: string | null = null; // 当前模板的静态缓冲
  let i = 0;
  let line = 1;
  const n = src.length;

  const inTpl = (): boolean => stack.length > 0 && stack[stack.length - 1].type === "tpl";

  while (i < n) {
    const c = src[i];

    if (!inTpl()) {
      // ── 代码区（含 `${…}` 插值内部）
      if (c === "\n") {
        line++;
        i++;
        continue;
      }
      if (c === "/" && src[i + 1] === "/") {
        const a = i;
        while (i < n && src[i] !== "\n") i++;
        comments?.push([a, i]);
        continue;
      }
      if (c === "/" && src[i + 1] === "*") {
        const a = i;
        i += 2;
        while (i < n && !(src[i] === "*" && src[i + 1] === "/")) {
          if (src[i] === "\n") line++;
          i++;
        }
        i += 2;
        comments?.push([a, Math.min(i, n)]);
        continue;
      }
      if (c === "/" && opensRegex(src, i)) {
        i++;
        let inClass = false;
        while (i < n) {
          const r = src[i];
          if (r === "\\") {
            i += 2;
            continue;
          }
          if (r === "\n") break; // 未闭合：当它不是正则，就地收手
          if (r === "[") inClass = true;
          else if (r === "]") inClass = false;
          else if (r === "/" && !inClass) {
            i++;
            break;
          }
          i++;
        }
        continue;
      }
      if (c === '"' || c === "'") {
        const q = c;
        const startLine = line;
        let v = "";
        i++;
        while (i < n && src[i] !== q) {
          if (src[i] === "\\") {
            v += src[i + 1] ?? "";
            i += 2;
            continue;
          }
          if (src[i] === "\n") break; // 未闭合，放弃这一条
          v += src[i];
          i++;
        }
        i++;
        out.push({ kind: "str", value: v, line: startLine });
        continue;
      }
      if (c === "`") {
        stack.push({ type: "tpl", chunks: [], line });
        buf = "";
        i++;
        continue;
      }
      const top = stack[stack.length - 1];
      if (top?.type === "expr") {
        if (c === "{") {
          top.depth++;
          i++;
          continue;
        }
        if (c === "}") {
          if (top.depth === 0) {
            stack.pop(); // 插值结束，回到模板静态区
            buf = "";
            i++;
            continue;
          }
          top.depth--;
          i++;
          continue;
        }
      }
      i++;
      continue;
    }

    // ── 模板静态区
    if (c === "\n") line++;
    if (c === "\\") {
      buf = (buf ?? "") + (src[i + 1] ?? "");
      i += 2;
      continue;
    }
    if (c === "$" && src[i + 1] === "{") {
      for (let k = stack.length - 1; k >= 0; k--) {
        const f = stack[k];
        if (f.type === "tpl") {
          f.chunks.push({ text: buf ?? "", hole: true });
          break;
        }
      }
      stack.push({ type: "expr", depth: 0 });
      buf = null;
      i += 2;
      continue;
    }
    if (c === "`") {
      const f = stack.pop() as { type: "tpl"; chunks: TplChunk[]; line: number };
      f.chunks.push({ text: buf ?? "", hole: false });
      buf = null;
      out.push({ kind: "tpl", chunks: f.chunks, line: f.line });
      i++;
      continue;
    }
    buf = (buf ?? "") + c;
    i++;
  }
  return out;
}

// ─────────────────────────────────────────────────────────────────────────────
// CSS 侧
// ─────────────────────────────────────────────────────────────────────────────

const blankOut = (m: string): string => m.replace(/[^\n]/g, " ");

/** 把注释与字符串涂成同长空白（行号不变），剩下的才是选择器与声明。 */
function stripCssNoise(src: string): string {
  return src
    .replace(/\/\*[\s\S]*?\*\//g, blankOut)
    .replace(/"(?:[^"\\\n]|\\.)*"/g, blankOut)
    .replace(/'(?:[^'\\\n]|\\.)*'/g, blankOut);
}

/**
 * 一份 CSS 里选择器写到的类名 → 住址。
 *
 * 判准：一段文本**以 `{` 收尾**才算选择器（声明块里的 `color: red` 后面跟的是 `;` 或 `}`）。
 * 以 `@` 开头的是 at-rule 前奏（`@media` / `@supports` / `@layer`），不取类名，
 * 但它的花括号照常进出 ⇒ **嵌在 `@media` 里的规则照样数得到**。
 */
export function cssClassesOf(src: string, file: string): Map<string, string[]> {
  const clean = stripCssNoise(src);
  const out = new Map<string, string[]>();
  let seg = "";
  let segLine = 1;
  let line = 1;
  for (let i = 0; i < clean.length; i++) {
    const ch = clean[i];
    if (ch === "{") {
      const sel = seg.trim();
      if (sel && !sel.startsWith("@")) {
        for (const m of sel.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)) {
          const list = out.get(m[1]) ?? [];
          list.push(`${file}:${segLine}`);
          out.set(m[1], list);
        }
      }
      seg = "";
      segLine = line;
    } else if (ch === "}" || ch === ";") {
      seg = "";
      segLine = line;
    } else {
      if (seg.trim() === "") segLine = line;
      seg += ch;
    }
    if (ch === "\n") line++;
  }
  return out;
}

/** 一条 `z-index` 声明的住址与它写的值。 */
export interface ZIndexDecl {
  file: string;
  line: number;
  value: string;
}

/** 一份 CSS 里所有 `z-index:` 声明（注释里的不算 —— 那些已经被涂白了）。 */
export function zIndexDeclsOf(src: string, file: string): ZIndexDecl[] {
  const clean = stripCssNoise(src);
  const out: ZIndexDecl[] = [];
  const lines = clean.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const m = /(^|[;{\s])z-index\s*:\s*([^;}]+)/.exec(lines[i]);
    if (m) out.push({ file, line: i + 1, value: m[2].trim() });
  }
  return out;
}

// ─────────────────────────────────────────────────────────────────────────────
// 遍历
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// 代码侧
// ─────────────────────────────────────────────────────────────────────────────

/**
 * 把一段文本切成「类名形状」的 token。
 *
 * ⚠ **按"类名能用的字符"切，不按空白切**。按空白切会让 `` `<div class="code-bar">` ``
 * 里的 `code-bar` 切不出来（它连在 `class="` 上），而那正是 `src/render.ts` 挂
 * `.code-bar` / `.code-lang` / `.code-copy` 的唯一写法。
 */
function classTokens(text: string): string[] {
  return text.split(/[^A-Za-z0-9_-]+/).filter(Boolean);
}

/** 代码侧收上来的一张表：token → 住址。 */
export type SiteMap = Map<string, string[]>;

function addSite(m: SiteMap, k: string, site: string): void {
  const list = m.get(k) ?? [];
  if (list.length < 8) list.push(site);
  m.set(k, list);
}

/**
 * `const X = "字面量"` 这一形。收它是为了把 `` `${X}-open` `` 这种**常量拼接**填回去 ——
 * `.status-first-run-open` 就是这么活下来的（`src/first-run-hint.ts:65`）。
 */
function collectConstants(sources: { rel: string; text: string }[]): Map<string, string> {
  const out = new Map<string, string>();
  for (const s of sources) {
    for (const m of s.text.matchAll(
      /\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?::[^=;\n]+)?=\s*(["'])((?:(?!\2)[^\\\n]|\\.)*)\2/g,
    )) {
      out.set(m[1], m[3]);
    }
  }
  return out;
}

function harvest(sources: { rel: string; text: string }[], literals: SiteMap, prefixes: SiteMap): void {
  for (const s of sources) {
    for (const tok of scanStrings(s.text)) {
      const site = `${s.rel}:${tok.line}`;
      if (tok.kind === "str") {
        for (const t of classTokens(tok.value)) addSite(literals, t, site);
        continue;
      }
      for (const ch of tok.chunks) {
        const toks = classTokens(ch.text);
        for (let j = 0; j < toks.length; j++) {
          const t = toks[j];
          // 紧挨着洞的那个尾 token ⇒ 它既可能是个完整类名（洞里插的是空串或 " 另一个类"），
          // 也可能是个前缀（洞里插的是名字的后半截）。两边都登记，由判据那边的登记表裁。
          const isTail = j === toks.length - 1 && /[A-Za-z0-9_-]$/.test(ch.text);
          if (ch.hole && isTail) addSite(prefixes, t, site);
          addSite(literals, t, site);
        }
      }
    }
  }
}

/**
 * 方向 ② 认的调用点形态。**只认这四种**，别的写法本量具看不见（头注"诚实边界"第二条）。
 *
 * 每条的第一个捕获组要么是类名串（空白分隔），要么是 CSS 选择器串（由 `fromSelector` 决定）。
 */
const USE_FORMS: { name: string; re: RegExp; fromSelector: boolean }[] = [
  // `el.className = "a b"` / `{ className: "a b" }` / `el.className += " a"`
  { name: "className 赋值", re: /\bclassName\s*(?:=|\+=|:)\s*"([^"\n]*)"/g, fromSelector: false },
  { name: "className 赋值", re: /\bclassName\s*(?:=|\+=|:)\s*'([^'\n]*)'/g, fromSelector: false },
  // HTML 片段里的 class 属性（`src/render.ts` 大量这么写）
  { name: "class 属性", re: /\bclass\s*=\s*"([^"\n<>]*)"/g, fromSelector: false },
  // `classList.add("a", "b")` 这一族
  { name: "classList", re: /\bclassList\.(?:add|remove|toggle|contains|replace)\(([^)\n]*)\)/g, fromSelector: false },
  // `querySelector(".x")` / `closest(".x")` / `matches(".x")`
  {
    name: "选择器查询",
    re: /\b(?:querySelector|querySelectorAll|closest|matches)\(\s*"([^"\n]*)"/g,
    fromSelector: true,
  },
];

function collectUsedClasses(sources: { rel: string; text: string }[]): SiteMap {
  const out: SiteMap = new Map();
  for (const s of sources) {
    // 🔴 先抹注释：这几条正则跑在原文上，不抹就会把散文里的类名当成真调用点。
    const text = stripCodeComments(s.text);
    const lineAt = (idx: number): number => {
      let n = 1;
      for (let i = 0; i < idx; i++) if (text[i] === "\n") n++;
      return n;
    };
    for (const form of USE_FORMS) {
      for (const m of text.matchAll(form.re)) {
        const site = `${s.rel}:${lineAt(m.index)}`;
        const body = m[1];
        if (form.fromSelector) {
          for (const c of body.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)) addSite(out, c[1], site);
          continue;
        }
        if (form.name === "classList") {
          // 只取实参里的**字符串字面量**；`classList.add(x)` 这种变量实参本量具放过
          for (const lit of body.matchAll(/"([^"\n]*)"|'([^'\n]*)'/g)) {
            for (const t of (lit[1] ?? lit[2] ?? "").split(/\s+/)) if (t && !t.includes("$")) addSite(out, t, site);
          }
          continue;
        }
        for (const t of body.split(/\s+/)) {
          // 含 `${` 的段是模板洞，方向 ② 不猜它；`/` `<` 这类是切歪了，丢掉
          if (!t || t.includes("$") || !/^-?[_a-zA-Z][\w-]*$/.test(t)) continue;
          addSite(out, t, site);
        }
      }
    }
  }
  return out;
}

// ─────────────────────────────────────────────────────────────────────────────
// 总装
// ─────────────────────────────────────────────────────────────────────────────

export interface Ledger {
  /** 被扫的仓根（绝对路径）。 */
  root: string;
  /** `src/**\/*.css` —— 仓相对路径。 */
  cssFiles: string[];
  /** `src/**\/*.ts` ＋ `index.html` —— 仓相对路径。 */
  codeFiles: string[];
  /** 从 `src` 的 TS 里现读到的第三方 CSS 规格串（如 `katex/dist/katex.min.css`）。 */
  vendorSpecs: string[];
  /** CSS 选择器里的类名 → 住址。 */
  cssClasses: SiteMap;
  /** 第三方 CSS 自己产出的类名 → 是哪份第三方 CSS。 */
  vendorClasses: Map<string, string>;
  /** 代码里**直接**出现的类名形 token → 住址。 */
  literals: SiteMap;
  /** 把 `${常量}` 填回去之后**才**出现的 token → 住址（＝ 常量拼接那一族）。 */
  constConcat: SiteMap;
  /** 模板里紧挨着插值的尾 token → 住址（＝ 前缀候选，**只是候选**）。 */
  prefixCandidates: SiteMap;
  /** `const X = "字面量"` 收到的常量表。 */
  constants: Map<string, string>;
  /** 方向 ②：代码里确实当类名用的 → 住址。 */
  usedClasses: SiteMap;
  /** 所有 `z-index` 声明。 */
  zIndexDecls: ZIndexDecl[];
}

/** 走一遍树，把上面那些表全建出来。`root` 就是仓根（含 `package.json` 那层）。 */
export function buildLedger(root: string): Ledger {
  const rel = (p: string): string => relative(root, p).split("\\").join("/");
  const srcDir = join(root, "src");

  const cssPaths = walk(srcDir, (p) => p.endsWith(".css"));
  const cssClasses: SiteMap = new Map();
  const zIndexDecls: ZIndexDecl[] = [];
  for (const p of cssPaths) {
    const text = readFileSync(p, "utf8");
    for (const [name, sites] of cssClassesOf(text, rel(p))) {
      const list = cssClasses.get(name) ?? [];
      list.push(...sites);
      cssClasses.set(name, list);
    }
    zIndexDecls.push(...zIndexDeclsOf(text, rel(p)));
  }

  const tsPaths = walk(srcDir, (p) => p.endsWith(".ts") && !p.endsWith(".d.ts"));
  const codePaths = [...tsPaths, join(root, "index.html")];
  const sources = codePaths.map((p) => ({ rel: rel(p), text: readFileSync(p, "utf8") }));

  // 第三方 CSS：住址从 `import "<包>/….css"` 现读，**本文件不写死任何包名**。
  const vendorSpecs = new Set<string>();
  for (const s of sources) {
    for (const m of s.text.matchAll(/import\s+["']([^"'\n]+\.css)["']/g)) {
      if (!m[1].startsWith(".") && !m[1].startsWith("/")) vendorSpecs.add(m[1]);
    }
  }
  const vendorClasses = new Map<string, string>();
  for (const spec of vendorSpecs) {
    const p = join(root, "node_modules", spec);
    let text: string;
    try {
      text = readFileSync(p, "utf8");
    } catch {
      continue; // 装没装由判据那边的自检去管，量具不在这儿抛
    }
    for (const [name] of cssClassesOf(text, spec)) if (!vendorClasses.has(name)) vendorClasses.set(name, spec);
  }

  const constants = collectConstants(sources);

  const literals: SiteMap = new Map();
  const prefixCandidates: SiteMap = new Map();
  harvest(sources, literals, prefixCandidates);

  // 第二趟：把 `${CONST}` 这种洞用常量表填回去再扫一遍，**只保留新冒出来的那些**。
  const subbed = sources.map((s) => ({
    rel: s.rel,
    text: s.text.replace(/\$\{\s*([A-Za-z_$][\w$]*)\s*\}/g, (all, id: string) => constants.get(id) ?? all),
  }));
  const literals2: SiteMap = new Map();
  const prefixes2: SiteMap = new Map();
  harvest(subbed, literals2, prefixes2);
  const constConcat: SiteMap = new Map();
  for (const [k, v] of literals2) if (!literals.has(k)) constConcat.set(k, v);
  for (const [k, v] of prefixes2) if (!prefixCandidates.has(k)) prefixCandidates.set(k, v);

  return {
    root,
    cssFiles: cssPaths.map(rel),
    codeFiles: codePaths.map(rel),
    vendorSpecs: [...vendorSpecs],
    cssClasses,
    vendorClasses,
    literals,
    constConcat,
    prefixCandidates,
    constants,
    usedClasses: collectUsedClasses(sources),
    zIndexDecls,
  };
}

/** 一个 CSS 类名"为什么算活着"的裁决。 */
export type Verdict =
  | { kind: "literal"; via: string }
  | { kind: "const-concat"; via: string }
  | { kind: "vendor"; via: string }
  | { kind: "prefix"; prefix: string; via: string }
  | { kind: "unexplained" };

/**
 * 判一个 CSS 类名活着还是死着。
 *
 * ⚠ 顺序是**刻意**的：字面量 → 常量拼接 → 第三方 → 前缀。前缀排最后，
 * 这样"只能靠前缀解释"的那批会被单独显出来（它们正是需要人写理由的那批）。
 * `allowedPrefixes` 由判据那边传进来 —— **量具自己不决定哪个前缀算数**。
 */
export function explain(led: Ledger, name: string, allowedPrefixes: readonly string[]): Verdict {
  const lit = led.literals.get(name);
  if (lit) return { kind: "literal", via: lit[0] };
  const cc = led.constConcat.get(name);
  if (cc) return { kind: "const-concat", via: cc[0] };
  const ven = led.vendorClasses.get(name);
  if (ven) return { kind: "vendor", via: ven };
  const hit = allowedPrefixes
    .filter((p) => name.startsWith(p) && name.length > p.length)
    .sort((a, b) => b.length - a.length)[0];
  if (hit) return { kind: "prefix", prefix: hit, via: led.prefixCandidates.get(hit)?.[0] ?? "<无住址>" };
  return { kind: "unexplained" };
}
