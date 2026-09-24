/**
 * `P21` —— **`INVARIANTS.md` 讲前端那五条的量具**（判与登记住
 * `tests/invariants-frontend-guard.vitest.ts`，分家的理由同 `S25`/`S30`：
 * `scanning-guard-registry.vitest.ts` 的 `WALKER_CEILING` 不许测试文件再多一个遍历者，
 * 而本文件不是 `*.vitest.ts`/`*.test.ts` ⇒ 不进那条棘轮的人群）。
 *
 * # 它量四件事实
 *
 * | 量具 | 服务哪条 | 取什么 |
 * |---|---|---|
 * | [`alertCallSites`] | 条 12（alert 不算错误反馈） | 生产 TS 里 `alert(` 的调用处 |
 * | [`fixedPortals`] | 条 13（portal 必须真挂 body） | CSS 里声明 `position: fixed` 的类 × 它的建元素/挂载处 |
 * | [`storageKeys`] | 条 14（key 必须前缀 `cc-monitor.`） | 生产 TS 里喂给存储访问器的 key 字面量 |
 * | [`cssDecl`] | 条 21.2（`.stream` 不许 `overflow-anchor: none`） | 某选择器声明了哪些属性 |
 *
 * # 🔴 三条刻意的取舍（都由调用侧的**等号/零命中**断言兜底，不是无声假设）
 *
 * 1. **一律先剥注释**（`stripComments`）。本仓为「拿注释当代码判据」栽过四次
 *    （住址：`tests/test-support/strip-comments.ts` 头注那四条）。本文件量的四件事实
 *    里有三件是**否定式**（零命中），而否定式断言吃到注释是**假绿**，永远不叫。
 * 2. **[`fixedPortals`] 只认「同一份文件里 `X.className = "cls"` ＋ 同一份文件里挂载 `X`」这一形。**
 *    今天 25 个 fixed 类全是这一形（现打）。有人把建元素与挂载拆到两份文件，
 *    本量具会把它报成「挂载处不明」—— 方向是**假红、会叫**，不是漏红。
 * 3. **[`storageKeys`] 只解一层 `const NAME = "字面量"`。**
 *    今天两处绕过接入层的直写 key（`main.ts` / `restart-notice.ts`）正是这一形。
 *    拐两层（`const A = B`）会被报成「解不出」而当场红，同样是假红方向。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { productionTsFiles } from "../test-support/production-sources.ts";
import { stripComments } from "../test-support/strip-comments.ts";
import { REPO_ROOT } from "../test-support/repo-root.ts";

/** 一处源码位置。 */
export interface Site {
  /** 相对仓根，正斜杠。 */
  readonly file: string;
  /** 1 起的行号。 */
  readonly line: number;
  /** 那一行剥注释后的正文（已 trim）。 */
  readonly text: string;
}

/** 生产 TS：`{ file, 剥注释后的代码 }`。**本量具的唯一人群来源。** */
export function productionCode(): ReadonlyArray<{ file: string; code: string }> {
  return productionTsFiles("src").map((s) => ({ file: s.file, code: stripComments(s.text, "ts") }));
}

function sitesOf(code: string, file: string, re: RegExp): Site[] {
  const out: Site[] = [];
  code.split("\n").forEach((line, i) => {
    // 每行独立跑：`re` 带 `g` 时 `lastIndex` 会跨行残留。
    if (new RegExp(re.source, re.flags.replace("g", "")).test(line)) {
      out.push({ file, line: i + 1, text: line.trim() });
    }
  });
  return out;
}

// ────────────────────────────── 条 12 ──────────────────────────────

/**
 * `alert(` 的调用处（整词；认 `window.alert(`，不认 `xx.alert(` / `showAlert(`）。
 *
 * ⚠ 射程**只有 `alert`** —— 条 12 的标题逐字就是「前端 alert 不算错误反馈」。
 * `confirm` / `prompt` 今天生产里有 25 处，它们问的是「要不要干这件事」而不是
 * 「出错了」⇒ **不在条 12 的射程里**，别顺手一起禁掉（那会把一条真断言变成误红源）。
 */
export const ALERT_CALL = /(?<![\w.$])(?:window\s*\.\s*)?alert\s*\(/;

export function alertCallSites(sources = productionCode()): Site[] {
  return sources.flatMap((s) => sitesOf(s.code, s.file, ALERT_CALL));
}

// ────────────────────────────── CSS 侧 ──────────────────────────────

/**
 * 应用的全部 CSS，剥注释后拼成一份。
 *
 * 〔三入口拆分 · 人群改定义〕原先人群就是 `src/styles.css` 这一份；拆开之后是
 * **三个 html 的 `<link rel="stylesheet">` 清单去重** —— 那就是三个窗口真正加载的全部样式表，
 * 没有第二份清单可漂。下游各条（规则数 > 500、fixed 选择器 == 25、包含块正控）一字未改。
 */
export function styleSheet(): string {
  const files = new Set<string>();
  for (const html of ["index.html", "settings.html", "viewer.html"]) {
    const text = readFileSync(resolve(REPO_ROOT, html), "utf8");
    for (const m of text.matchAll(/<link rel="stylesheet" href="\/([^"]+)"/g)) files.add(m[1]);
  }
  return [...files].map((f) => stripComments(readFileSync(resolve(REPO_ROOT, f), "utf8"), "ts")).join("\n");
}

/** 一条 CSS 规则：选择器串 ＋ 声明体。 */
export interface CssRule {
  readonly selector: string;
  readonly body: string;
}

/**
 * 全部规则，`{ 选择器, 声明体 }`。`@media`/`@supports` 里的规则**照收**（取其内层选择器）。
 *
 * 🔴 **不许退回 `matchAll(/(^|[};])([^{};]+)\{([^{}]*)\}/g)` 那一形** —— 相邻两条规则
 * 共用同一个 `}`，而 `matchAll` 不允许重叠 ⇒ **每隔一条漏一条**。这不是理论洞：
 * 本文件第一版就是那一形，`fixedSelectors()` 实测报 **16**（真值 **25**，与同一趟的
 * python 对拍脚本差 9 条）。而 16 那个数**看起来完全正常** —— 没有任何东西会告诉你它漏了谁。
 * ⇒ 改成按 `}` 切段、段内取最后一个 `{` 之前那一截当选择器（对嵌套 at-rule 天然正确）。
 * 调用侧那条「分母自检」是为这一刀留的常驻哨兵。
 */
export function cssRules(css = styleSheet()): CssRule[] {
  const out: CssRule[] = [];
  for (const chunk of css.split("}")) {
    const brace = chunk.lastIndexOf("{");
    if (brace < 0) continue;
    const before = chunk.slice(0, brace);
    const cut = Math.max(before.lastIndexOf("{"), before.lastIndexOf(";"));
    const selector = before
      .slice(cut + 1)
      .split(/\s+/)
      .join(" ")
      .trim();
    if (!selector || selector.startsWith("@")) continue;
    out.push({ selector, body: chunk.slice(brace + 1) });
  }
  return out;
}

/** 某选择器在 CSS 里声明的某个属性的值（多条则取最后一条；没有则 `null`）。 */
export function cssDecl(selector: string, prop: string, css = styleSheet()): string | null {
  let v: string | null = null;
  for (const r of cssRules(css)) {
    if (r.selector !== selector) continue;
    for (const d of r.body.matchAll(new RegExp(`(?:^|;)\\s*${prop}\\s*:([^;]*)`, "g"))) {
      v = d[1].trim();
    }
  }
  return v;
}

/**
 * 会给后代 `position: fixed` **改包含块**的属性（CSS spec；条 13 的「为什么」逐字点了前四个）。
 *
 * `contain` / `content-visibility` 是同一族的第二批（layout containment 同样生成包含块）——
 * 本仓 `.stream-content > *` 就带 `content-visibility: auto`，读数住
 * `tests/evidence/S21-css-readings.md`。
 */
export const CONTAINING_BLOCK_PROPS = [
  "transform",
  "filter",
  "perspective",
  "will-change",
  "contain",
  "content-visibility",
  "backdrop-filter",
] as const;

/** 某选择器身上声明了哪几个「会改包含块」的属性。 */
export function containingBlockProps(selector: string, css = styleSheet()): string[] {
  return CONTAINING_BLOCK_PROPS.filter((p) => cssDecl(selector, p, css) !== null);
}


// ────────────────────────────── 条 13 ──────────────────────────────

/**
 * 一个 `position: fixed` 的浮层，以及它在 TS 侧的**建元素文件**。
 *
 * 🔴 **判定单元是「文件」而不是「变量」**，这是本量具最重要的一个取舍，理由是实测：
 * 变量级追踪在本仓会**漏红也会假红** —— `.grid-monitor` / `.history-view` /
 * `.panorama-view` / `.cc-bus-view` / `.settings-panel` 的建元素处是局部 `view`/`root`，
 * 而挂载处写的是 `document.body.appendChild(this.root)`（中间经过 `this.root = view`
 * 或一个 `build…()` 返回值）；`.tab-context-menu` 更有**两个**建元素处
 * （真菜单 ＋ `"tab-context-menu tab-context-submenu"` 那个刻意挂在 `wrap` 里的二级飞出）。
 *
 * ⇒ 单元取「文件」：**建这个 fixed 类的那份文件里必须出现 `document.body.<挂载动词>(`**。
 * 这是条 13 的**必要条件**（一个从不碰 `document.body` 的模块不可能把浮层挂到 body 上），
 * 而它对上面那两族形状都判得对。不满足的那几个 == 登记表，且登记的宿主要**机检理由**。
 */
export interface FixedPortal {
  /** CSS 选择器末段（`.cls` 或 `#id`）。 */
  readonly sel: string;
  /** 建这个类/id 的生产文件（可能多份；按路径排序）。 */
  readonly bornIn: string[];
  /** 这些文件里**都**出现了 `document.body.<挂载动词>(` 吗。 */
  readonly mountsBody: boolean;
}

/** 把浮层挂进 DOM 的动词。 */
const MOUNT_VERB = "append|appendChild|prepend|insertBefore|replaceChildren|replaceWith";

const BODY_MOUNT = new RegExp(`document\\s*\\.\\s*body\\s*\\.\\s*(?:${MOUNT_VERB})\\(`);

/**
 * CSS 里所有声明 `position: fixed` 的选择器**末段**（`.cls` / `#id`，去重排序）。
 *
 * 取末段而不是整条：`body.settings-window-mode .settings-panel` 与 `.settings-panel`
 * 讲的是同一个元素，条 13 管的是**那个元素挂在哪**。
 */
export function fixedSelectors(css = styleSheet()): string[] {
  const out = new Set<string>();
  for (const r of cssRules(css)) {
    if (!/(?:^|;)\s*position\s*:\s*fixed\s*(?:;|$)/.test(r.body)) continue;
    for (const one of r.selector.split(",")) {
      const last = one.trim().split(/\s+/).pop() ?? "";
      const m = /^([.#][A-Za-z0-9_-]+)/.exec(last);
      if (m) out.add(m[1]);
    }
  }
  return [...out].sort();
}

/** 逐个 fixed 选择器算出「谁建的、那份文件碰不碰 `document.body`」。 */
export function fixedPortals(sources = productionCode(), css = styleSheet()): FixedPortal[] {
  return fixedSelectors(css).map((sel) => {
    const name = sel.slice(1);
    const esc = name.replace(/[-]/g, "\\$&");
    const born =
      sel[0] === "."
        ? new RegExp(
            `className\\s*=\\s*["\`][^"\`]*\\b${esc}\\b|classList\\s*\\.\\s*add\\(\\s*["\`]${esc}["\`]`,
          )
        : new RegExp(
            `\\.\\s*id\\s*=\\s*["\`]${esc}["\`]|getElementById\\(\\s*["\`]${esc}["\`]|=\\s*["\`]${esc}["\`]`,
          );
    const bornIn = sources.filter((s) => born.test(s.code)).map((s) => s.file);
    const mountsBody =
      bornIn.length > 0 &&
      bornIn.every((f) => BODY_MOUNT.test(sources.find((s) => s.file === f)?.code ?? ""));
    return { sel, bornIn, mountsBody };
  });
}

// ────────────────────────────── 条 14 ──────────────────────────────

/** 一个解出了静态前缀的存储 key。 */
export interface StorageKey {
  readonly file: string;
  readonly line: number;
  /** key 的**静态前缀**（模板串取 `${` 之前那一段）。 */
  readonly key: string;
  /** 它来自 `LS_KEYS` 那份集中登记（`false` = 绕过接入层的直写 key）。 */
  readonly registered: boolean;
}

/** 一处**解不出**静态 key 的访问器调用（不许静默跳过，由登记表对账）。 */
export interface OpaqueKeySite extends Site {
  /** 那个实参的原文。 */
  readonly arg: string;
}

/** 认哪些函数「第一个实参是存储 key」。 */
const ACCESSOR =
  "(?:localStorage|sessionStorage)\\s*\\.\\s*(?:getItem|setItem|removeItem)|" +
  "safeGet|safeSet|safeRemove|safeGetJson|safeSetJson";

/** 取一段字面量（`"…"` / `'…'` / `` `…` ``）的**静态前缀**。 */
function staticPrefix(lit: string): string {
  const inner = lit.slice(1, -1);
  const i = inner.indexOf("${");
  return i < 0 ? inner : inner.slice(0, i);
}

const LITERAL = /^(["'`])((?:\\.|(?!\1).)*)\1/;

/** `LS_KEYS` 对象字面量里登记的那一族 key。 */
function lsKeysBlock(sources: ReadonlyArray<{ file: string; code: string }>): StorageKey[] {
  const ls = sources.find((s) => s.file === "src/local-storage.ts");
  if (!ls) return [];
  const from = ls.code.indexOf("export const LS_KEYS");
  const to = ls.code.indexOf("\n} as const;", from);
  if (from < 0 || to <= from) return [];
  const base = ls.code.slice(0, from).split("\n").length;
  const out: StorageKey[] = [];
  ls.code
    .slice(from, to)
    .split("\n")
    .forEach((line, i) => {
      for (const m of line.matchAll(/:\s*(?:\([^)]*\)\s*=>\s*)?(["'`])((?:\\.|(?!\1).)*)\1/g)) {
        out.push({ file: ls.file, line: base + i, key: staticPrefix(`${m[1]}${m[2]}${m[1]}`), registered: true });
      }
    });
  return out;
}

/**
 * 全部存储 key：`LS_KEYS` 里登记的那一族 ∪ 访问器实参解出来的字面量。
 *
 * 解不出的那几处由 [`opaqueKeySites`] 单独报，**不混进来也不静默丢**。
 */
export function storageKeys(sources = productionCode()): StorageKey[] {
  const out = lsKeysBlock(sources);
  const argRe = new RegExp(`(?:${ACCESSOR})\\s*\\(\\s*([^,)]+)`, "g");
  for (const s of sources) {
    s.code.split("\n").forEach((line, i) => {
      if (/\bfunction\s/.test(line)) return; // 访问器自己的**声明行**不是调用点
      for (const m of line.matchAll(argRe)) {
        const arg = m[1].trim();
        if (LITERAL.test(arg)) {
          out.push({ file: s.file, line: i + 1, key: staticPrefix(LITERAL.exec(arg)![0]), registered: false });
          continue;
        }
        const r = resolveKeyArg(arg, s.code);
        if (r !== null && r !== "") out.push({ file: s.file, line: i + 1, key: r, registered: false });
      }
    });
  }
  return out;
}

/**
 * 把一个非字面量实参解成静态前缀。
 *
 * - `LS_KEYS.x` / `LS_KEYS.x(id)` ⇒ `""`（已由 [`lsKeysBlock`] 收过，别重复计）
 * - 接入层内部的形参 `key` ⇒ `""`（不是 key 字面量）
 * - `NAME` / `this.NAME` ⇒ 同文件解**一层**赋值：字面量则取其前缀，`LS_KEYS…` 则 `""`
 * - 其余 ⇒ `null`（解不出，由 [`opaqueKeySites`] 报出来）
 */
function resolveKeyArg(arg: string, code: string): string | null {
  if (/^LS_KEYS\b/.test(arg)) return "";
  if (/^(?:key|k)$/.test(arg)) return "";
  const id = /^(?:this\s*\.\s*)?([A-Za-z_$][\w$]*)$/.exec(arg);
  if (!id) return null;
  const isThis = arg.startsWith("this");
  const decl = isThis
    ? new RegExp(`this\\s*\\.\\s*${id[1]}\\s*=\\s*([^;\\n]+)`)
    : new RegExp(`\\b(?:const|let|var)\\s+${id[1]}\\s*=\\s*([^;\\n]+)`);
  const d = decl.exec(code);
  if (!d) return null;
  const rhs = d[1].trim();
  if (/^LS_KEYS\b/.test(rhs)) return "";
  if (LITERAL.test(rhs)) return staticPrefix(LITERAL.exec(rhs)![0]);
  return null;
}

/** 访问器调用里**解不出**静态 key 的那几处。 */
export function opaqueKeySites(sources = productionCode()): OpaqueKeySite[] {
  const out: OpaqueKeySite[] = [];
  const argRe = new RegExp(`(?:${ACCESSOR})\\s*\\(\\s*([^,)]+)`, "g");
  for (const s of sources) {
    s.code.split("\n").forEach((line, i) => {
      if (/\bfunction\s/.test(line)) return;
      for (const m of line.matchAll(argRe)) {
        const arg = m[1].trim();
        if (LITERAL.test(arg)) continue;
        if (resolveKeyArg(arg, s.code) === null) {
          out.push({ file: s.file, line: i + 1, text: line.trim(), arg });
        }
      }
    });
  }
  return out;
}

// ────────────────────────────── 条 22（TS 那半）──────────────────────────────

/**
 * 切出一个**顶层**函数的函数体（含签名行）。
 *
 * 判定「函数结束」用的是**列 0 的 `}`** —— 本仓 `src/main.ts` 的
 * `bootstrapViewer` / `bootstrapSettings` 都是顶层 `async function`，缩进是 prettier
 * 保证的。切不到时返回 `null`，由调用侧那条分母自检当场红（**不返回空串**：
 * 空串会让下面那几条「A 必须在 B 之前」全部零命中地绿）。
 */
export function topLevelFnBody(code: string, name: string): string | null {
  const m = new RegExp(`^(?:export\\s+)?(?:async\\s+)?function\\s+${name}\\s*\\(`, "m").exec(code);
  if (!m) return null;
  const rest = code.slice(m.index);
  const end = /\n\}/.exec(rest);
  return end ? rest.slice(0, end.index + 2) : null;
}

/** `src/main.ts` 剥注释后的代码（条 22 那几项的住址都在这一份里）。 */
export function mainCode(sources = productionCode()): string {
  return sources.find((s) => s.file === "src/main.ts")?.code ?? "";
}

// ────────────────────────── 条 21.2（JS 侧临时关闭的还原纪律）──────────────────────────

/** 一处 `overflow-anchor` 的临时关闭 / 还原。 */
export interface AnchorToggle {
  readonly file: string;
  /** 关闭处（`style.overflowAnchor = "none"`）的行号。 */
  readonly offLine: number;
  /** 还原处（`style.overflowAnchor = ""`）的行号，`null` = 全文件找不到还原。 */
  readonly onLine: number | null;
  /** 还原那一处**在 `finally {` 块里**吗。 */
  readonly restoredInFinally: boolean;
}

/**
 * 条 21.2 的唯一豁免类（补批期在**同一同步任务内**临时 `overflow-anchor: none`）
 * 今天在哪、还原得对不对。
 *
 * 「在 `finally` 里」的判法：从还原处往前回看 600 字符，必须出现 `} finally {` 或 `finally {`，
 * 且那个 `finally` **之后**不再出现 `try {`。这是个近似 —— 但方向是**假红**
 * （嵌套 try 会让它错判成「不在 finally 里」而当场叫），不是漏红。
 * 本仓今天两处（`tabs.fillAbove` / `session-viewer.maybeFillAbove`）都判得对。
 */
export function anchorToggles(sources = productionCode()): AnchorToggle[] {
  const out: AnchorToggle[] = [];
  for (const s of sources) {
    const lines = s.code.split("\n");
    const offs: number[] = [];
    const ons: number[] = [];
    lines.forEach((l, i) => {
      if (/style\s*\.\s*overflowAnchor\s*=\s*"none"/.test(l)) offs.push(i);
      if (/style\s*\.\s*overflowAnchor\s*=\s*""/.test(l)) ons.push(i);
    });
    for (let k = 0; k < offs.length; k++) {
      const on = ons[k];
      let inFinally = false;
      if (on !== undefined) {
        const at = lines.slice(0, on).join("\n").length;
        const look = s.code.slice(Math.max(0, at - 600), at);
        const fin = look.lastIndexOf("finally");
        inFinally = fin >= 0 && !/\btry\s*\{/.test(look.slice(fin));
      }
      out.push({
        file: s.file,
        offLine: offs[k] + 1,
        onLine: on === undefined ? null : on + 1,
        restoredInFinally: inFinally,
      });
    }
  }
  return out;
}
