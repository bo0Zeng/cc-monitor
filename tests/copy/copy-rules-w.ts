/**
 * CP2a · 新写法（`调研/…/文案/新写法/规范.md` §1 的 N 系）落成的机检：C-W1–C-W17 ＋ 收严的 C-L2 · C-P1 · C-Y4。
 *
 * 每条检法读的词表 / 闭集都住 `src/shared/copy/rules.json` 那一条的 `words` · `cells` · `limits` · `families` 格，
 * 检法里不另抄一份 —— 改规矩就是改那一格。
 *
 * 量长度的尺子（C-W4 · C-W15）：汉字与全角符号记 1、ASCII 记 0.5、占位符记 2（「≤60 字」按汉字宽度算）。
 */
import { NAMED_PH, speech, type Entry, type Term } from "./copy-support.ts";

export interface WRule {
  id: string;
  words?: string[];
  cells?: string[];
  limits?: Record<string, number>;
  families?: { trigger: string; template: string }[];
  symbols?: string;
}

export interface WCtx {
  terms: Term[];
  /** 规矩号 → 那一条（检法从这里读词表）。 */
  byId: Map<string, WRule>;
  /** 全表：C-W13（指路核对）要拿「」里的字对表里的名字。 */
  table: Record<string, Entry>;
}

const words = (ctx: WCtx, id: string): string[] => ctx.byId.get(id)?.words ?? [];

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

type Check = (e: Entry, ctx: WCtx) => string | null;

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

export const W_CHECKS: Record<string, Check> = {
  // N1a：表外符号。
  "C-W1": (e, ctx) => {
    const ok = ctx.byId.get("C-W1")?.symbols ?? "";
    const bad = [...new Set([...speech(e.zh)].filter((c) => c.charCodeAt(0) > 127 && !HAN.test(c) && !CJK_PUNCT.includes(c) && !ok.includes(c)))];
    return bad.length ? `符号表外的字符 ${bad.join(" ")}` : null;
  },
  // N1b：「·」两边各一个空格。
  "C-W2": (e) => (/\S·|·\S/.test(e.zh) ? "「·」两边没各空一格" : null),
  // N3：状态取自闭集（或闭集一格 ＋ · ＋ 时刻 / 计数 / 对象；或 chip 形「名词 值」）。
  "C-W3": (e, ctx) => {
    if (e.role !== "状态") return null;
    const cells = ctx.byId.get("C-W3")?.cells ?? [];
    const head = speech(e.zh).trim().split(" · ")[0].trim();
    if (cells.includes(head)) return null;
    // chip 形「名词 值」：值里有占位符（上下文 {pct}% · 未读 {n} · 无权限目录 ×{n} · 任务 {done}/{all}）。
    const first = e.zh.trim().split(" · ")[0];
    if (/^[^{}\s]+ [×+]?\{[A-Za-z][A-Za-z0-9]*\}(%|\/\{[A-Za-z][A-Za-z0-9]*\})?$/.test(first)) return null;
    return `状态「${head}」不在闭集里`;
  },
  // N4a：说明每行 ≤60（命令行豁免）。
  "C-W4": (e) => {
    if (e.kind !== "body" || isCli(e)) return null;
    const w = widest(e.zh);
    return w > 60 ? `一行宽 ${w} > 60` : null;
  },
  // N4c：猜测 / 论证 / 意图 / 开发过程词。
  "C-W5": (e, ctx) => {
    const s = speech(e.zh);
    const h = words(ctx, "C-W5").filter((w) => s.includes(w));
    return h.length ? `有论证 / 猜测词「${h.join("」「")}」` : null;
  },
  // N5：时刻与时长只一种写法（命令行豁免）。
  "C-W6": (e) => {
    if (isCli(e)) return null;
    const z = e.zh;
    if (/(\d|\})\s*(毫秒|秒钟?|分钟|小时|天)(前|后|内)?/.test(z)) return "写死了时长单位（时长走 {dur}）";
    if (/\d+\s*月\s*\d+\s*日/.test(z)) return "写了「X 月 X 日」";
    if (/今天|昨天|前天/.test(z)) return "写了今天 / 昨天 / 前天";
    return null;
  },
  // N6a ＋ N6c：报错不写「没能 · 不了」，「失败」至多一次。
  "C-W7": (e, ctx) => {
    if (e.kind !== "error") return null;
    const s = speech(e.zh);
    const h = words(ctx, "C-W7").find((w) => s.includes(w));
    if (h) return `报错里有「${h}」`;
    const n = (s.match(/失败/g) ?? []).length;
    return n > 1 ? `「失败」${n} 次（两层主语）` : null;
  },
  // N6b：「失败 · X」的 X 取自原因词闭集或占位符。
  "C-W8": (e, ctx) => {
    const reasons = words(ctx, "C-W8");
    for (const m of e.zh.matchAll(/失败 · ([^\n]*?)(?= · |\n|$)/g)) {
      const x = m[1].trim();
      if (reasons.includes(x) || /^(\{[A-Za-z][A-Za-z0-9]*\}\s*)+$/.test(x)) continue;
      return `「失败 · ${x}」不是原因词`;
    }
    return null;
  },
  // N7b：按钮不写「确定 / 好 / 是」。
  "C-W9": (e, ctx) => (e.kind === "action" && words(ctx, "C-W9").includes(e.zh.trim()) ? `按钮写成「${e.zh.trim()}」` : null),
  // N9a–c：句号（命令行豁免）· 窄档与报错的冒号 · 感叹号。
  "C-W10": (e) => {
    const s = speech(e.zh);
    if (!isCli(e) && s.includes("。")) return "有句号";
    if (["title", "control", "action", "aria", "error"].includes(e.kind) && s.includes("：")) return `${e.kind} 档有冒号`;
    if (/[！!]/.test(s)) return "有感叹号";
    return null;
  },
  // N10：名词在前、数字在后（「处理 · 种类」这类词里的字不算量词）。
  "C-W11": (e) => {
    if (/\{\w+\}\s*(个|台|次|件|处(?!理)|份|种(?!类))/.test(e.zh)) return "数字后跟量词";
    if (/第\s*\{\w+\}\s*(行|个|项|条)/.test(e.zh)) return "写了「第 N 行」";
    return null;
  },
  // N11a：键位不写死。
  "C-W12": (e) => (/Ctrl\+|Alt\+|Cmd\+|⌘|Shift\+(?!Enter)|\([A-Z,`]\)/.test(e.zh) ? "写死了键位" : null),
  // N11b：「」里点名的按钮 / 页必须是表里的名字（或「A → B」链，每段是标题）。
  "C-W13": (e, ctx) => {
    const { names, titles } = namesOf(ctx.table);
    for (const m of e.zh.matchAll(/「([^」]*)」/g)) {
      const x = m[1];
      if (/\{[A-Za-z]/.test(x) || names.has(x)) continue;
      if (x.includes(" → ") && x.split(" → ").every((p) => titles.has(p))) continue;
      return `「${x}」不是表里的名字`;
    }
    return null;
  },
  // N12：族触发词命中的，必须长成该族的样子。
  "C-W14": (e, ctx) => {
    for (const f of ctx.byId.get("C-W14")?.families ?? []) {
      if (new RegExp(f.trigger).test(e.zh) && !new RegExp(f.template).test(e.zh)) return `命中族「${f.trigger}」却不是族里的句式`;
    }
    return null;
  },
  // N13：按角色限长（确认框只量正文那一档）。
  "C-W15": (e, ctx) => {
    const lim = e.role ? ctx.byId.get("C-W15")?.limits?.[e.role] : undefined;
    if (lim === undefined || (e.role === "确认框" && e.kind !== "body")) return null;
    const w = widest(e.zh);
    return w > lim ? `${e.role} 宽 ${w} > ${lim}` : null;
  },
  // N15：读屏名不用符号。
  "C-W16": (e, ctx) => {
    if (e.kind !== "aria") return null;
    const sym = [...(ctx.byId.get("C-W16")?.symbols ?? "")].find((c) => e.zh.includes(c));
    return sym ? `读屏名里有符号「${sym}」` : null;
  },
  // N2：口语词（「了」不在「了解 · 了结」里，「请」不在「请求」里）。
  "C-W17": (e, ctx) => {
    const s = speech(e.zh);
    const h = words(ctx, "C-W17").filter((w) => s.includes(w));
    if (/了(?![解结])/.test(s)) h.push("了");
    if (/请(?!求)/.test(s)) h.push("请");
    return h.length ? `口语词「${h.join("」「")}」` : null;
  },
};

/** 收严的三条旧规矩（替掉 copy-rules.vitest.ts 里的旧实现）。 */
export const TIGHTENED: Record<string, Check> = {
  // N1c：破折号只许单独成格；窄档一个都不许。
  "C-L2": (e) => {
    if (!e.zh.includes("—")) return null;
    if (["title", "control", "action", "aria"].includes(e.kind) && e.zh.trim() !== "—") return `${e.kind} 档里有破折号`;
    return e.zh.replace(/(^|\s)—(?=\s|$)/g, "").includes("—") ? "破折号没有单独成格" : null;
  },
  // N4b：不以「你」开头；任何位置不许「您」。
  "C-P1": (e) => {
    const s = speech(e.zh).trimStart();
    if (/您/.test(s)) return "称用户用了「您」";
    return s.startsWith("你") ? "以「你」开头" : null;
  },
  // N7a：问号全表禁。
  "C-Y4": (e) => (/[？?]/.test(speech(e.zh)) ? "有问号" : null),
};
