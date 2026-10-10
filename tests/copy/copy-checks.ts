/**
 * 文案机检：九条检法管 `src/shared/copy/rules.json` 里全部标「机检」的规矩（`copy-table.vitest.ts` 里以 `[C-xx]` 开头的那几条除外）。
 *
 * 规矩（rules.json 的一条，C-W1 · C-L2 …）是写给人看的规范，也是豁免的单位（文案表一条的 `waive` 按规矩号写）；
 * 检法是机器怎么判 —— 好几条规矩共用一张表、一个检查：
 *
 * | 检法 | 管哪几条规矩 |
 * |---|---|
 * | J1 字符 × 档 | C-Y2 · C-W1 · C-W2 · C-W10 · C-W12 · C-W16 · C-L2 · C-Y4 |
 * | J2 词 × 档 | C-P2 · C-T1 · C-Y3 · C-W5 · C-W7（禁词那半）· C-W9 · C-W17 · C-W19 · C-W20 |
 * | J3 状态 / 失败句 | C-W3 · C-W7（「失败」一行至多一次那半）· C-W8 · C-W14 |
 * | J4 按档限长 | C-L1 · C-W4 · C-W15 |
 * | J5 时刻与时长 | C-W6 |
 * | J6 量词 | C-W11 |
 * | J7 点名 | C-W13 |
 * | J8 原话与码不上句 | C-Y5 · C-W18 |
 * | J9 括号 | C-L3 · C-L4 |
 *
 * 每条检法对一条文案给出一串命中，每个命中带它违反的规矩号 ⇒ 豁免、正反例都还按规矩号对；欠账名单按检法号记。
 * 词表 / 闭集都读 rules.json 那一条的 `words` · `cells` · `limits` · `families` · `symbols` 格，这里不另抄一份。
 *
 * 量长度的尺子（J4）：汉字与全角符号记 1、ASCII 记 0.5、占位符记 2（「≤60 字」按汉字宽度算）。
 */
import { NAMED_PH, r1Hits, speech, type Entry, type Term } from "./copy-support.ts";

/** rules.json 里一条规矩里检法要读的那几格。 */
export interface WRule {
  id: string;
  words?: string[];
  cells?: string[];
  /** C-W3：可连同主语 / 宾语嵌进句里的闭集格（该远端未启用多账号 · 未连接远端）。 */
  embeddable?: string[];
  limits?: Record<string, number>;
  /** C-W8：原因格的形状（整格匹配，如「不支持仅密码登录」）。 */
  shapes?: string[];
  /** roleTemplates：该角色的条目匹配它也算（如原因格「<对象> 无法解析」）。 */
  /** lead：族句前可接一格事实（结果未知 · 无应答 · 后生效）—— 那一格不再触发别的族。 */
  families?: { trigger: string; template: string; roleTemplates?: Record<string, string>; lead?: boolean }[];
  symbols?: string;
}

export interface CheckCtx {
  terms: Term[];
  /** 规矩号 → 那一条（检法从这里读词表）。 */
  byId: Map<string, WRule>;
  /** 全表：J7（点名）拿「」里的字对表里的名字。 */
  table: Record<string, Entry>;
  /** C-Y3：口语词 · 祈使动词两张表与各自的射程（rules.json 的 C-Y3）。 */
  colloquial: string[];
  imperative: string[];
  labelKinds: string[];
  imperativeKinds: string[];
  /** C-W18：生产代码里值是下层原话的那几个占位（按值认）：key → 占位名。 */
  rawFed?: Map<string, Set<string>>;
}

/** 一处命中：违反了哪条规矩 · 怎么违反的。 */
export interface Hit {
  rule: string;
  msg: string;
}

export interface Check {
  id: string;
  name: string;
  /** 这条检法管的规矩号（rules.json）。一条规矩可以分在两条检法里各判一半（C-W7）。 */
  covers: string[];
  run: (e: Entry, ctx: CheckCtx, key?: string) => Hit[];
}

const words = (ctx: CheckCtx, id: string): string[] => ctx.byId.get(id)?.words ?? [];

/** 一行的宽度：占位符 2 · ASCII 0.5 · 其余 1。 */
export function width(line: string): number {
  let n = 0;
  for (const ch of line.replace(NAMED_PH, "\u0000\u0000")) n += ch === "\u0000" ? 1 : ch.charCodeAt(0) < 128 ? 0.5 : 1;
  return n;
}
const widest = (zh: string): number => Math.max(...zh.split("\n").map(width));

/** 命令行（ccm 的帮助与 stderr）豁免长度、时长写法与句号（规范 §2.14）。 */
const isCli = (e: Entry): boolean => e.role === "命令行";

const HAN = /[㐀-䶿一-鿿]/;
/** 常规全角标点（不算符号）。 */
const CJK_PUNCT = "，。、；：？！“”‘’（）《》「」【】『』［］～";
const NARROW = ["title", "control", "action", "aria"];

// ── 字符 × 档、词 × 档：一张表一行一格 ─────────────────────────────────────────

/**
 * 表的一行：哪条规矩 · 管哪几档（缺省全管）· 命令行豁不豁免 · 对原文还是对去掉占位符的话（`speech`）判 · 怎么找。
 * `find` 回命中的说法（`null` = 没命中）；同一条规矩的几行按表序判，头一个命中的算数。
 */
interface Row {
  rule: string;
  kinds?: (ctx: CheckCtx) => string[];
  role?: string;
  cliExempt?: boolean;
  on: "zh" | "speech";
  find: (text: string, e: Entry, ctx: CheckCtx) => string | null;
}

const kinds =
  (...ks: string[]) =>
  (): string[] =>
    ks;
const re = (rx: RegExp, msg: string | ((e: Entry) => string)) => (t: string, e: Entry): string | null =>
  rx.test(t) ? (typeof msg === "string" ? msg : msg(e)) : null;

function runRows(rows: Row[], e: Entry, ctx: CheckCtx): Hit[] {
  const out: Hit[] = [];
  const done = new Set<string>();
  for (const r of rows) {
    if (done.has(r.rule)) continue;
    if (r.kinds && !r.kinds(ctx).includes(e.kind)) continue;
    if (r.role !== undefined && e.role !== r.role) continue;
    if (r.cliExempt && isCli(e)) continue;
    const msg = r.find(r.on === "zh" ? e.zh : speech(e.zh), e, ctx);
    if (msg) {
      out.push({ rule: r.rule, msg });
      done.add(r.rule);
    }
  }
  return out;
}

/** J1：每一档许不许出现哪些字符。 */
const CHAR_ROWS: Row[] = [
  { rule: "C-Y2", on: "zh", find: re(/\*\*|`|⇒|🔴/, "有 markdown / 论证符号") },
  {
    rule: "C-W1",
    on: "speech",
    find: (t, _e, ctx) => {
      const ok = ctx.byId.get("C-W1")?.symbols ?? "";
      const bad = [...new Set([...t].filter((c) => c.charCodeAt(0) > 127 && !HAN.test(c) && !CJK_PUNCT.includes(c) && !ok.includes(c)))];
      return bad.length ? `符号表外的字符 ${bad.join(" ")}` : null;
    },
  },
  { rule: "C-W2", on: "zh", find: re(/\S·|·\S/, "「·」两边没各空一格") },
  { rule: "C-W10", on: "speech", cliExempt: true, find: re(/。/, "有句号") },
  { rule: "C-W10", on: "speech", kinds: kinds(...NARROW, "error"), find: re(/：/, (e) => `${e.kind} 档有冒号`) },
  { rule: "C-W10", on: "speech", find: re(/[！!]/, "有感叹号") },
  { rule: "C-W12", on: "zh", find: re(/Ctrl\+|Alt\+|Cmd\+|⌘|Shift\+(?!Enter)|\([A-Z,`]\)/, "写死了键位") },
  {
    rule: "C-W16",
    on: "zh",
    kinds: kinds("aria"),
    find: (t, _e, ctx) => {
      const sym = [...(ctx.byId.get("C-W16")?.symbols ?? "")].find((c) => t.includes(c));
      return sym ? `读屏名里有符号「${sym}」` : null;
    },
  },
  // 破折号只许单独成格（两边是空白或行首尾）；窄档整条不是「—」就不许有。
  { rule: "C-L2", on: "zh", kinds: kinds(...NARROW), find: (t, e) => (t.includes("—") && t.trim() !== "—" ? `${e.kind} 档里有破折号` : null) },
  { rule: "C-L2", on: "zh", find: (t) => (t.replace(/(^|\s)—(?=\s|$)/g, "").includes("—") ? "破折号没有单独成格" : null) },
  { rule: "C-Y4", on: "speech", find: re(/[？?]/, "有问号") },
];

/** 命中词表里的哪几个（`all` = 全列，否则头一个）。 */
const listed = (id: string, msg: (hits: string[]) => string, all = true) => (t: string, _e: Entry, ctx: CheckCtx): string | null => {
  const h = words(ctx, id).filter((w) => t.includes(w));
  return h.length ? msg(all ? h : h.slice(0, 1)) : null;
};

/** J2：每一档许不许出现哪些词。 */
const WORD_ROWS: Row[] = [
  { rule: "C-P2", on: "zh", find: re(/我们/, "产品自称「我们」") },
  {
    rule: "C-T1",
    on: "speech",
    find: (t, _e, ctx) => {
      const hits = r1Hits(t, ctx.terms);
      return hits.length ? `命中术语表禁档：${hits.join(" · ")}` : null;
    },
  },
  // C-Y3：title 与 aria 档（`kinds`）不许问号、口语词；title 档（`imperativeKinds`）不许以祈使动词开头。
  { rule: "C-Y3", on: "speech", kinds: (c) => c.labelKinds, find: re(/[？?]/, (e) => `${e.kind} 档里有问号`) },
  {
    rule: "C-Y3",
    on: "speech",
    kinds: (c) => c.labelKinds,
    find: (t, e, ctx) => {
      const c = ctx.colloquial.find((w) => t.indexOf(w) >= 0);
      return c ? `${e.kind} 档里有口语词「${c}」` : null;
    },
  },
  {
    rule: "C-Y3",
    on: "speech",
    kinds: (c) => c.imperativeKinds.filter((k) => c.labelKinds.includes(k)),
    find: (t, e, ctx) => {
      const v = ctx.imperative.find((w) => t.trimStart().startsWith(w));
      return v ? `${e.kind} 档以祈使动词「${v}」开头` : null;
    },
  },
  { rule: "C-W5", on: "speech", find: listed("C-W5", (h) => `有论证 / 猜测词「${h.join("」「")}」`) },
  { rule: "C-W7", on: "speech", kinds: kinds("error"), find: listed("C-W7", (h) => `报错里有「${h[0]}」`, false) },
  {
    rule: "C-W9",
    on: "zh",
    kinds: kinds("action"),
    find: (t, _e, ctx) => (words(ctx, "C-W9").includes(t.trim()) ? `按钮写成「${t.trim()}」` : null),
  },
  // 口语词（「了」不在「了解 · 了结」里，「请」不在「请求」里）。
  {
    rule: "C-W17",
    on: "speech",
    find: (t, _e, ctx) => {
      const h = words(ctx, "C-W17").filter((w) => t.includes(w));
      if (/了(?![解结])/.test(t)) h.push("了");
      if (/请(?!求)/.test(t)) h.push("请");
      return h.length ? `口语词「${h.join("」「")}」` : null;
    },
  },
  { rule: "C-W19", on: "speech", find: listed("C-W19", (h) => `第二人称「${h.join("」「")}」`) },
  // 术语表「会话」：「会话」前紧跟的技术前缀只许 words 里那几个（tmux）。
  {
    rule: "C-W20",
    on: "speech",
    find: (t, _e, ctx) => {
      const ok = new Set(words(ctx, "C-W20"));
      const h = [...t.matchAll(/([A-Za-z][A-Za-z0-9_-]*) ?会话/g)].map((m) => m[1]).filter((w) => !ok.has(w));
      return h.length ? `「会话」前接了技术前缀「${[...new Set(h)].join("」「")}」` : null;
    },
  },
];

// ── J3：状态 / 失败句的语法 ───────────────────────────────────────────────────

/**
 * 一句状态 / 结果句按语法读一遍：
 * ① 状态句（role = 状态）的头一格是状态闭集里的一格（或闭集格嵌在句里 · chip 形「名词 值」）；头一格被状态读走，后面接的是事实；
 * ② 「失败 · X」的 X 是原因词（「<写入 / 读取 / 删除>失败 · <对象> · <原因>」那一形原因在第三格）；
 * ③ 报错一行至多一个「失败」；
 * ④ 族触发词命中的，整句长成该族的样子。
 */
function statusHead(e: Entry, ctx: CheckCtx): { hit: Hit | null; consumed: number } {
  if (e.role !== "状态") return { hit: null, consumed: 0 };
  const cells = ctx.byId.get("C-W3")?.cells ?? [];
  // 正在进行的那一格可带省略号（读取中…）。
  const head = speech(e.zh).trim().split(" · ")[0].trim().replace(/…$/, "");
  // 状态句打头的「失败」是状态格（运行结局），由状态读走 —— 后面那一格是事实，不归原因词管。
  const consumed = e.zh.startsWith("失败 · ") ? "失败".length : 0;
  if (cells.includes(head)) return { hit: null, consumed };
  // 只有 embeddable 里那几格可连同主语 / 宾语嵌在句里（该远端未启用多账号 · 未连接远端）；别的格（失败 · 完成 …）要打头。
  const emb = ctx.byId.get("C-W3")?.embeddable ?? [];
  if (emb.some((c) => cells.includes(c) && head.includes(c))) return { hit: null, consumed };
  // chip 形「名词 值」：值里有占位符（上下文 {pct}% · 未读 {n} · 无权限目录 ×{n} · 任务 {done}/{all}）。
  const first = e.zh.trim().split(" · ")[0];
  if (/^[^{}\s]+ [×+]?\{[A-Za-z][A-Za-z0-9]*\}(%|\/\{[A-Za-z][A-Za-z0-9]*\})?$/.test(first)) return { hit: null, consumed };
  return { hit: { rule: "C-W3", msg: `状态「${head}」不在闭集里` }, consumed };
}

function reasonCell(e: Entry, ctx: CheckCtx, from: number): Hit | null {
  const reasons = words(ctx, "C-W8");
  const shapes = (ctx.byId.get("C-W8")?.shapes ?? []).map((s) => new RegExp(s));
  const ok = (x: string): boolean => reasons.includes(x) || shapes.some((r) => r.test(x)) || /^(\{[A-Za-z][A-Za-z0-9]*\}\s*)+$/.test(x);
  const bad = (msg: string): Hit => ({ rule: "C-W8", msg });
  for (const m of e.zh.matchAll(/失败 · ([^\n]*?)(?= · |\n|$)/g)) {
    if (m.index < from) continue;
    const x = m[1].trim();
    if (ok(x)) continue;
    // 写入 / 读取 / 删除失败 · <对象> · <原因>：一行以它开头时第一格是对象，原因在下一格。
    const lineStart = e.zh.lastIndexOf("\n", m.index) + 1;
    if (m.index === lineStart + 2 && /^(写入|读取|删除)失败 · /.test(e.zh.slice(lineStart))) {
      const next = e.zh.slice(m.index + m[0].length).match(/^ · ([^\n]*?)(?= · |\n|$)/);
      if (next && ok(next[1].trim())) continue;
      return bad(next ? `「失败 · ${x} · ${next[1].trim()}」的原因格不是原因词` : `「失败 · ${x}」后面缺原因格`);
    }
    return bad(`「失败 · ${x}」不是原因词`);
  }
  return null;
}

function oneFailurePerLine(e: Entry): Hit | null {
  if (e.kind !== "error") return null;
  // 一行一件事：「失败」每行至多一次（同一行里两次 ＝ 两层主语）；分行写的是两件先后的事。
  const n = Math.max(...speech(e.zh).split("\n").map((l) => (l.match(/失败/g) ?? []).length));
  return n > 1 ? { rule: "C-W7", msg: `一行里「失败」${n} 次（两层主语）` } : null;
}

function family(e: Entry, ctx: CheckCtx): Hit | null {
  const fams = ctx.byId.get("C-W14")?.families ?? [];
  // 主族（lead）句式成立时，它前面接的那一格事实（到头一个「 · 」）不再拿去触发别的族。
  let rest = e.zh;
  const cut = e.zh.indexOf(" · ");
  if (cut >= 0 && !e.zh.slice(0, cut).includes("\n")) {
    const tail = e.zh.slice(cut + 3);
    if (fams.some((f) => f.lead && new RegExp(f.trigger).test(tail) && new RegExp(f.template).test(tail))) rest = tail;
  }
  for (const f of fams) {
    const zh = f.lead ? e.zh : rest;
    if (!new RegExp(f.trigger).test(zh) || new RegExp(f.template).test(zh)) continue;
    const byRole = e.role ? f.roleTemplates?.[e.role] : undefined;
    if (byRole !== undefined && new RegExp(byRole).test(zh)) continue;
    return { rule: "C-W14", msg: `命中族「${f.trigger}」却不是族里的句式` };
  }
  return null;
}

function sentence(e: Entry, ctx: CheckCtx): Hit[] {
  const head = statusHead(e, ctx);
  return [head.hit, reasonCell(e, ctx, head.consumed), oneFailurePerLine(e), family(e, ctx)].filter((h): h is Hit => h !== null);
}

// ── J4：按档限长 ──────────────────────────────────────────────────────────────

const hanCount = (s: string): number => (s.match(/[一-鿿]/g) ?? []).length;

function lengths(e: Entry, ctx: CheckCtx): Hit[] {
  const out: Hit[] = [];
  // 首句（到第一个 。！？；或换行为止，插值点不算）≤24 个汉字。
  const n = hanCount(speech(e.zh).split(/[。！？!?；;\n]/)[0]);
  if (n > 24) out.push({ rule: "C-L1", msg: `首句 ${n} 个汉字 > 24` });
  // 说明每行 ≤60（命令行豁免）。
  if (e.kind === "body" && !isCli(e)) {
    const w = widest(e.zh);
    if (w > 60) out.push({ rule: "C-W4", msg: `一行宽 ${w} > 60` });
  }
  // 按角色限长（确认框只量正文那一档）。
  const lim = e.role ? ctx.byId.get("C-W15")?.limits?.[e.role] : undefined;
  if (lim !== undefined && !(e.role === "确认框" && e.kind !== "body")) {
    const w = widest(e.zh);
    if (w > lim) out.push({ rule: "C-W15", msg: `${e.role} 宽 ${w} > ${lim}` });
  }
  return out;
}

// ── 其余各成一条 ────────────────────────────────────────────────────────────

const one = (rule: string, msg: string | null): Hit[] => (msg ? [{ rule, msg }] : []);

function durations(e: Entry): Hit[] {
  if (isCli(e)) return [];
  const z = e.zh;
  if (/(\d|\})\s*(毫秒|秒钟?|分钟|小时|天)(前|后|内)?/.test(z)) return one("C-W6", "写死了时长单位（时长走 {dur}）");
  if (/\d+\s*月\s*\d+\s*日/.test(z)) return one("C-W6", "写了「X 月 X 日」");
  if (/今天|昨天|前天/.test(z)) return one("C-W6", "写了今天 / 昨天 / 前天");
  return [];
}

function counters(e: Entry): Hit[] {
  // 名词在前、数字在后（「处理 · 种类」这类词里的字不算量词）；「失败」计数写「失败 ×{n}」。
  if (/\{\w+\}\s*(个|台|次|件|处(?!理)|份|种(?!类))/.test(e.zh)) return one("C-W11", "数字后跟量词");
  if (/第\s*\{\w+\}\s*(行|个|项|条)/.test(e.zh)) return one("C-W11", "写了「第 N 行」");
  if (/失败 \{\w+\}/.test(e.zh)) return one("C-W11", "「失败」后的计数没写 ×");
  return [];
}

/** 表里能被「」点名的名字（title / action / control）与标题 —— 每张表只算一次。 */
const NAMES = new WeakMap<object, { names: Set<string>; titles: Set<string> }>();
function namesOf(table: Record<string, Entry>): { names: Set<string>; titles: Set<string> } {
  let got = NAMES.get(table);
  if (!got) {
    got = { names: new Set(), titles: new Set() };
    for (const x of Object.values(table)) {
      if (["title", "action", "control"].includes(x.kind)) got.names.add(x.zh);
      if (x.kind === "title") got.titles.add(x.zh);
    }
    NAMES.set(table, got);
  }
  return got;
}

function pointers(e: Entry, ctx: CheckCtx): Hit[] {
  // 「」里点名的按钮 / 页必须是表里的名字（或「A → B」链，每段是标题）。
  const { names, titles } = namesOf(ctx.table);
  for (const m of e.zh.matchAll(/「([^」]*)」/g)) {
    const x = m[1];
    if (/\{[A-Za-z]/.test(x) || names.has(x)) continue;
    if (x.includes(" → ") && x.split(" → ").every((p) => titles.has(p))) continue;
    return one("C-W13", `「${x}」不是表里的名字`);
  }
  return [];
}

function rawWords(e: Entry, ctx: CheckCtx, key?: string): Hit[] {
  const out: Hit[] = [];
  // 错误码不上屏：占位名是 code（或以 code / Code 结尾）的一律不许 —— 码留在日志与诊断里。
  const code = /\{([A-Za-z_]*(?:code|Code))\}/.exec(e.zh);
  if (code) out.push({ rule: "C-Y5", msg: `把错误码插进了话里（{${code[1]}}）` });
  // 原话 · 退出码 · 错误码不上句子，进「复制详情」（条带 §5.2）。按占位符的语义判（名字在 words 闭集里的就是原话型），
  //   另按值认：生产代码喂进去的是 `e.to_string()` 一类（`rust-refs.ts::isRawValue`）⇒ 占位叫什么都算原话型。命令行豁免。
  if (e.role !== "命令行") {
    const raw = new Set(words(ctx, "C-W18"));
    const fed = (key !== undefined ? ctx.rawFed?.get(key) : undefined) ?? new Set<string>();
    const h = [...new Set([...e.zh.matchAll(/\{([A-Za-z][A-Za-z0-9]*)\}/g)].map((m) => m[1]))].filter((a) => raw.has(a) || fed.has(a));
    if (h.length) out.push({ rule: "C-W18", msg: `句子里接了原话 {${h.join("} {")}}` });
  }
  return out;
}

const PAREN = /（([^（）]*)）|\(([^()]*)\)/g;

function parens(e: Entry): Hit[] {
  const out: Hit[] = [];
  const groups = [...speech(e.zh).matchAll(PAREN)].map((m) => m[1] ?? m[2] ?? "");
  if (NARROW.includes(e.kind)) {
    // 「（可选）」标的是选填，不是说明。
    const g = groups.find((x) => hanCount(x) >= 2 && x.trim() !== "可选");
    if (g !== undefined) out.push({ rule: "C-L3", msg: `${e.kind} 档的括号里装了说明「${g}」` });
  } else {
    const n = groups.filter((x) => x.trim().length >= 2).length;
    if (n > 1) out.push({ rule: "C-L3", msg: `括号补充 ${n} 处 > 1` });
  }
  if (/^\s*(?:（[^（）]*）|\([^()]*\))\s*$/.test(e.zh)) out.push({ rule: "C-L4", msg: "整条被括号包住" });
  return out;
}

export const CHECKS: Check[] = [
  { id: "J1", name: "字符 × 档", covers: [...new Set(CHAR_ROWS.map((r) => r.rule))], run: (e, ctx) => runRows(CHAR_ROWS, e, ctx) },
  { id: "J2", name: "词 × 档", covers: [...new Set(WORD_ROWS.map((r) => r.rule))], run: (e, ctx) => runRows(WORD_ROWS, e, ctx) },
  { id: "J3", name: "状态 / 失败句", covers: ["C-W3", "C-W7", "C-W8", "C-W14"], run: sentence },
  { id: "J4", name: "按档限长", covers: ["C-L1", "C-W4", "C-W15"], run: lengths },
  { id: "J5", name: "时刻与时长", covers: ["C-W6"], run: durations },
  { id: "J6", name: "量词", covers: ["C-W11"], run: counters },
  { id: "J7", name: "点名", covers: ["C-W13"], run: pointers },
  { id: "J8", name: "原话与码不上句", covers: ["C-Y5", "C-W18"], run: rawWords },
  { id: "J9", name: "括号", covers: ["C-L3", "C-L4"], run: parens },
];

/** 一条文案的全部命中（带检法号）。 */
export function hitsOf(e: Entry, ctx: CheckCtx, key?: string): (Hit & { check: string })[] {
  return CHECKS.flatMap((c) => c.run(e, ctx, key).map((h) => ({ ...h, check: c.id })));
}

/** 一条规矩对一条文案的说法（`null` = 没违反这一条）：正反例与单条正控用它。 */
export function ruleHit(rule: string, e: Entry, ctx: CheckCtx, key?: string): string | null {
  for (const c of CHECKS) {
    if (!c.covers.includes(rule)) continue;
    const h = c.run(e, ctx, key).find((x) => x.rule === rule);
    if (h) return h.msg;
  }
  return null;
}
