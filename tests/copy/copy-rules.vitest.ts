/**
 * CP2a · 文案规范（集中分析的产物）的判据。规范住 `src/shared/copy/rules.json`。
 *
 * # 它治的病
 *
 * 「规范里每一条都要标能不能机检；不能机检的如实标『主观项』。
 * **一份不标这个的规范会和散文一起腐**，而且腐了没人会发现」。所以本文件判三件事：
 *
 * 1. ★ **规矩 ↔ 实现两向相等**：`rules.json` 里标「机检」的规矩号 ==
 *    本文件 `CHECKS` 的键 ∪ `copy-table.vitest.ts` 里以 `[C-xx]` 开头的测试标题。
 *    标了机检却没实现 ⇒ 红（规范在吹牛）；实现了却标主观 / 没登记 ⇒ 红（规范漏记）。
 *    两侧异源：一侧是 JSON 里人写的标签，一侧是代码里真存在的检查。
 * 2. ★ **文案表逐条过全部机检规矩**：违反的要么改，要么在该条 `waive` 里登记理由；
 *    不可豁免的规矩没有例外；**豁免了却没违反 ⇒ 红（死豁免）**。
 * 3. **规范里的正反例与实现一致**：每条机检规矩的 `bad` 必须被它自己的检查逮住、`good` 必须放过
 *    —— 否则规范写的是一回事、检查查的是另一回事（第一版就栽在这里：
 *    「规矩禁的东西检法看不见」）。
 *
 * 另有一条异源对拍：C-Y3（R5）的口语词表与祈使动词表 == 普查 K-T68 的 `R5_COLLOQUIAL` / `R5_IMPERATIVE`。
 *
 * # 不判什么（诚实段）
 *
 * - 标「主观」的六条（C-Y1 · C-K3 · C-T2 · C-R2 · C-R3 · C-R4）**机器一个字都没判**。
 * - 机检规矩今天只对文案表里的条目生效（本波只有样板区 6 条）；没抽进表的文案不归本文件管。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { REPO_ROOT } from "../test-support/repo-root.ts";
import { censusList, loadTable, loadTerms, r1Hits, speech, type Entry, type Table, type Term } from "./copy-support.ts";
import { TIGHTENED, W_CHECKS, type WRule } from "./copy-rules-w.ts";
import { rawFedArgs, rustRefsIn, rustRefsOfTree } from "./rust-refs.ts";

const RULES_PATH = resolve(REPO_ROOT, "src", "shared", "copy", "rules.json");
const TABLE_TESTS = resolve(REPO_ROOT, "tests", "copy", "copy-table.vitest.ts");
const DEBT_PATH = resolve(REPO_ROOT, "tests", "copy", "copy-rules-debt.json");
const CENSUS = resolve(REPO_ROOT, "tests", "evidence", "K-T68-A1-outward-copy-census.py");

interface Rule extends WRule {
  id: string;
  part: string;
  rule: string;
  good: string[];
  bad: string[];
  check: "机检" | "主观";
  waivable?: boolean;
  why?: string;
  probeKind?: string;
  colloquial?: string[];
  imperative?: string[];
  /** C-Y3：问号与口语词那一条管哪几档（title ＋ aria）。 */
  kinds?: string[];
  /** C-Y3：祈使动词那一条管哪几档（control 档许以动词开头）。 */
  imperativeKinds?: string[];
  /** C-Y3：control 档的正例 —— 以动词开头、按 control 档查必须放过。 */
  controlGood?: string[];
  /** 探针用哪个 role 跑正反例（按角色判的那几条）。 */
  probeRole?: string;
  /** 这几档不许豁免（C-W17：窄档的口语词没有例外）。 */
  unwaivableKinds?: string[];
}

/**
 * 欠账名单：命中多的规矩不大批进豁免，而是把「今天还违反着的键」一次列全 ——
 * 名单 == 现打（两向）：改好一条就得删一行（否则死账红），新长出一条越界当场红。
 */
export type Debt = Record<string, string[]>;
export function loadDebt(): Debt {
  return (JSON.parse(readFileSync(DEBT_PATH, "utf8")) as { debt?: Debt }).debt ?? {};
}

export function loadRules(): Rule[] {
  return (JSON.parse(readFileSync(RULES_PATH, "utf8")) as { rules?: Rule[] }).rules ?? [];
}

interface Ctx {
  terms: Term[];
  colloquial: string[];
  imperative: string[];
  /** 问号与口语词只查这几档 —— 从 `rules.json` 的 `C-Y3.kinds` 读。 */
  labelKinds: string[];
  /** 祈使动词那一条只查这几档 —— 从 `rules.json` 的 `C-Y3.imperativeKinds` 读，检法不写死。 */
  imperativeKinds: string[];
  /** 规矩号 → 那一条（W 系检法从这里读词表 / 闭集）。 */
  byId: Map<string, WRule>;
  /** 全表（C-W13 指路核对拿「」里的字对表里的名字）。 */
  table: Table;
  /** C-W18：生产 Rust 里值是下层原话的那几个占位（按值认，`rust-refs.ts::isRawValue`）：key → 占位名。 */
  rawFed: Map<string, Set<string>>;
}

/** 生产 Rust 里「哪条的哪几个占位喂的是原话」（扫一遍全树，本文件只扫一次）。 */
let rawFedByKey: Map<string, Set<string>> | null = null;
function rawFedOf(): Map<string, Set<string>> {
  rawFedByKey ??= rawFedArgs(rustRefsOfTree().refs);
  return rawFedByKey;
}

/** 从规范里取 C-Y3 的三张表 —— 本文件三处共用这一个入口，不各抄一份。 */
export function y3Ctx(rules: Rule[], terms: Term[], table: Table = loadTable()): Ctx {
  const y3 = rules.find((r) => r.id === "C-Y3");
  return {
    byId: new Map(rules.map((r) => [r.id, r])),
    table,
    terms,
    colloquial: y3?.colloquial ?? [],
    imperative: y3?.imperative ?? [],
    labelKinds: y3?.kinds ?? [],
    imperativeKinds: y3?.imperativeKinds ?? [],
    rawFed: rawFedOf(),
  };
}

const NARROW = new Set(["title", "control", "action", "aria"]);
const DASH = /——|—/g;
const PAREN = /（([^（）]*)）|\(([^()]*)\)/g;
const HAN = /[一-鿿]/g;
const hanCount = (s: string): number => (s.match(HAN) ?? []).length;

/** 一条机检规矩 = 对一条文案给出「违反了什么」（`null` = 没违反）。`key` 给了 ⇒ 按生产代码怎么用它判的那几条（C-W18）也判。 */
type Check = (e: Entry, ctx: Ctx, key?: string) => string | null;

export const CHECKS: Record<string, Check> = {
  "C-Y2": (e) => (/\*\*|`|⇒|🔴/.test(e.zh) ? "有 markdown / 论证符号" : null),
  "C-Y3": (e, ctx) => {
    // 问号与口语词的射程读 `C-Y3.kinds`（title ＋ aria）。
    if (!ctx.labelKinds.includes(e.kind)) return null;
    const s = speech(e.zh);
    if (/[？?]/.test(s)) return `${e.kind} 档里有问号`;
    const c = ctx.colloquial.find((w) => s.indexOf(w) >= 0);
    if (c) return `${e.kind} 档里有口语词「${c}」`;
    // 祈使词只查 `imperativeKinds` 那几档（今天只有 title）。
    //   ⚠ 上面那一句 `labelKinds` 管的是问号与口语词，这一句管的是动词开头 —— 两件事，
    //   后者的射程住 `rules.json`，改规矩就是改那一格，检法跟着走。
    if (!ctx.imperativeKinds.includes(e.kind)) return null;
    const v = ctx.imperative.find((w) => s.trimStart().startsWith(w));
    return v ? `${e.kind} 档以祈使动词「${v}」开头` : null;
  },
  // 允许动词开头（问句仍禁）：control 档不许问句。
  "C-Y4": (e) => (e.kind === "control" && /[？?]/.test(speech(e.zh)) ? "control 档是问句" : null),
  // 错误码不上屏：占位名是 code（或以 code / Code 结尾）的一律不许 —— 码留在日志与诊断里。
  "C-Y5": (e) => {
    const m = /\{([A-Za-z_]*(?:code|Code))\}/.exec(e.zh);
    return m ? `把错误码插进了话里（{${m[1]}}）` : null;
  },
  "C-P1": (e) => (/您/.test(e.zh) ? "称用户用了「您」" : null),
  "C-P2": (e) => (/我们/.test(e.zh) ? "产品自称「我们」" : null),
  "C-T1": (e, ctx) => {
    const hits = r1Hits(speech(e.zh), ctx.terms);
    return hits.length ? `命中术语表禁档：${hits.join(" · ")}` : null;
  },
  "C-L1": (e) => {
    const first = speech(e.zh).split(/[。！？!?；;\n]/)[0];
    const n = hanCount(first);
    return n > 24 ? `首句 ${n} 个汉字 > 24` : null;
  },
  "C-L2": (e) => {
    const n = (e.zh.match(DASH) ?? []).length;
    if (NARROW.has(e.kind) && n > 0) return `${e.kind} 档里有破折号`;
    return n > 1 ? `破折号 ${n} 个 > 1` : null;
  },
  "C-L3": (e) => {
    const groups = [...speech(e.zh).matchAll(PAREN)].map((m) => m[1] ?? m[2] ?? "");
    if (NARROW.has(e.kind)) {
      // 「（可选）」标的是选填，不是说明（rules.json C-L3 的规矩文字同拍改）。
      const g = groups.find((x) => hanCount(x) >= 2 && x.trim() !== "可选");
      return g !== undefined ? `${e.kind} 档的括号里装了说明「${g}」` : null;
    }
    const n = groups.filter((x) => x.trim().length >= 2).length;
    return n > 1 ? `括号补充 ${n} 处 > 1` : null;
  },
  "C-L4": (e) => (/^\s*(?:（[^（）]*）|\([^()]*\))\s*$/.test(e.zh) ? "整条被括号包住" : null),
  // 收严的三条（C-L2 · C-P1 · C-Y4）替掉上面的旧实现；W 系（新写法 N 系）跟在后面。
  ...TIGHTENED,
  ...W_CHECKS,
};

/** 文案表逐条过全部机检规矩，带豁免：违反未豁免 · 豁免不可豁免的 · 死豁免 · 豁免了不存在的规矩，都是问题。 */
export function tableViolations(table: Record<string, Entry>, rules: Rule[], ctx: Ctx, debt: Debt = {}): string[] {
  const out: string[] = [];
  const byId = new Map(rules.map((r) => [r.id, r]));
  const owed = new Map(Object.entries(debt).map(([id, keys]) => [id, new Set(keys)]));
  for (const [id, keys] of Object.entries(debt)) {
    if (!CHECKS[id]) out.push(`欠账名单里的 ${id} 不是一条有检法的规矩`);
    if (keys.join("\n") !== [...new Set(keys)].sort().join("\n")) out.push(`欠账名单 ${id} 没排序或有重复`);
    for (const k of keys) if (!(k in table)) out.push(`欠账名单 ${id} 的 ${k} 不在表里 —— 删掉这一行`);
  }
  for (const [key, e] of Object.entries(table)) {
    const waive = e.waive ?? {};
    for (const id of Object.keys(waive)) {
      const r = byId.get(id);
      if (!r || r.check !== "机检") out.push(`${key}：豁免了 ${id}，它不是一条机检规矩`);
      else if (!r.waivable) out.push(`${key}：${id} 不可豁免`);
      else if (r.unwaivableKinds?.includes(e.kind)) out.push(`${key}：${id} 在 ${e.kind} 档不可豁免`);
      if (!waive[id]?.trim()) out.push(`${key}：豁免 ${id} 没写理由`);
      if (owed.get(id)?.has(key)) out.push(`${key}：${id} 既在欠账名单又登记了豁免 —— 二选一`);
    }
    for (const [id, check] of Object.entries(CHECKS)) {
      const v = check(e, ctx, key);
      const inDebt = owed.get(id)?.has(key) ?? false;
      if (v && !(id in waive) && !inDebt) out.push(`${key}：违反 ${id}（${v}）`);
      if (!v && id in waive) out.push(`${key}：${id} 的豁免是死的 —— 它没违反这一条，删掉豁免`);
      if (!v && inDebt) out.push(`${key}：${id} 已经不违反了 —— 从欠账名单里删掉这一行`);
    }
  }
  return out;
}

/** `copy-table.vitest.ts` 里以 `[C-xx]` 开头的测试标题带的规矩号。 */
export function titledIds(src: string): string[] {
  const out: string[] = [];
  for (const m of src.matchAll(/\bit\(\s*"((?:\[C-[A-Z0-9]+\])+)/g))
    for (const id of m[1].matchAll(/\[(C-[A-Z0-9]+)\]/g)) out.push(id[1]);
  return out;
}

/** 普查里 R5 祈使动词那条正则的词。 */
function censusImperative(py: string): string[] {
  const m = /R5_IMPERATIVE = re\.compile\(r"\^\\s\*\(([^)]+)\)"\)/.exec(py);
  return m ? m[1].split("|") : [];
}

describe("CP2a · 文案规范", () => {
  const rules = loadRules();
  const terms = loadTerms();
  const py = readFileSync(CENSUS, "utf8");
  const ctx = y3Ctx(rules, terms);

  it("规范的形状：规矩号唯一 · 每条标机检或主观 · 主观写明为什么判不了 · 机检写明可不可豁免", () => {
    expect(rules.length, "规范一条都没读出来").toBeGreaterThan(0);
    const p: string[] = [];
    const seen = new Set<string>();
    for (const r of rules) {
      if (seen.has(r.id)) p.push(`${r.id} 重复`);
      seen.add(r.id);
      if (!r.part || !r.rule) p.push(`${r.id} 缺 part / rule`);
      if (r.check !== "机检" && r.check !== "主观") p.push(`${r.id} 的 check「${r.check}」不是机检 / 主观`);
      if (r.check === "主观" && !r.why) p.push(`${r.id} 是主观项却没写为什么判不了`);
      if (r.check === "机检" && typeof r.waivable !== "boolean") p.push(`${r.id} 是机检却没写 waivable`);
    }
    expect(p).toEqual([]);
    // 两档都得有人：全标机检是在吹牛，全标主观是规范没有抓手。
    expect(new Set(rules.map((r) => r.check))).toEqual(new Set(["机检", "主观"]));
  });

  it("★ 规矩 ↔ 实现两向相等：标机检的规矩号 == CHECKS ∪ copy-table.vitest.ts 的 [C-xx] 标题", () => {
    const titled = titledIds(readFileSync(TABLE_TESTS, "utf8"));
    expect(titled.length, "copy-table.vitest.ts 里一条带规矩号的标题都没抽到 —— 抽取器坏了").toBeGreaterThan(0);
    const implemented = new Set([...Object.keys(CHECKS), ...titled]);
    const declared = new Set(rules.filter((r) => r.check === "机检").map((r) => r.id));
    expect([...declared].filter((x) => !implemented.has(x)), "标了机检、却没有实现").toEqual([]);
    expect([...implemented].filter((x) => !declared.has(x)), "有实现、规范里却没登记成机检").toEqual([]);
  });

  it("★ C-Y3 的两张词表 == 普查的 R5_COLLOQUIAL / R5_IMPERATIVE（两向）", () => {
    const col = censusList(py, "R5_COLLOQUIAL");
    const imp = censusImperative(py);
    expect(col, "没从普查里抽出 R5_COLLOQUIAL").toContain("还差");
    expect(imp, "没从普查里抽出 R5_IMPERATIVE").toContain("启用");
    expect([...ctx.colloquial].sort()).toEqual([...col].sort());
    expect([...ctx.imperative].sort()).toEqual([...imp].sort());
  });

  it("★ 文案表逐条过全部机检规矩（豁免登记的除外，且没有死豁免）", () => {
    const table = loadTable();
    expect(Object.keys(table).length, "文案表是空的 —— 这一条会零命中地绿").toBeGreaterThan(0);
    expect(tableViolations(table, rules, ctx, loadDebt())).toEqual([]);
  });

  it("规范里的正反例与实现一致：bad 被逮住、good 被放过", () => {
    const p: string[] = [];
    for (const r of rules) {
      const check = CHECKS[r.id];
      if (!check) continue;
      const kind = r.probeKind ?? "body";
      const role = r.probeRole;
      for (const zh of r.bad)
        if (!check({ kind, role, zh, args: [] }, ctx)) p.push(`${r.id} 放过了自己的反例「${zh}」`);
      for (const zh of r.good)
        if (check({ kind, role, zh, args: [] }, ctx)) p.push(`${r.id} 逮住了自己的正例「${zh}」`);
    }
    expect(p).toEqual([]);
  });

  it("★ 新写法那几条（C-W*）每条都带正控：至少一条反例、一条正例", () => {
    const w = rules.filter((r) => r.id.startsWith("C-W"));
    expect(w.length, "rules.json 里一条 C-W 都没有").toBe(Object.keys(W_CHECKS).length);
    expect(w.filter((r) => r.bad.length === 0 || r.good.length === 0).map((r) => r.id)).toEqual([]);
  });

  it("C-W14 的 roleTemplates：原因格「<对象> 无法解析」放过，同一串换个角色照逮；对象里夹「 · 」的原因格照逮", () => {
    const fam = rules.find((r) => r.id === "C-W14")?.families?.find((f) => f.roleTemplates);
    expect(fam?.roleTemplates?.["原因格"], "C-W14 没有原因格的 roleTemplates —— 下面零命中地绿").toBeDefined();
    const run = (role: string, zh: string) => CHECKS["C-W14"]({ kind: "error", role, zh, args: [] }, ctx);
    for (const zh of ["登录信息无法解析", "行 {line} 无法解析", "对端回的目录无法解析 · {e}", "内容无法解析 · 已跳过"]) {
      expect(run("原因格", zh), `原因格逮住了「${zh}」`).toBeNull();
      expect(run("报错", zh), `报错放过了「${zh}」`).not.toBeNull();
    }
    expect(run("原因格", "回的内容读不懂 · 无法解析")).not.toBeNull();
  });

  it("C-W8：状态句打头的「失败」是状态格（运行结局），不是「<动作>失败 · <原因>」；别的角色、或「失败」不打头，照逮", () => {
    const run = (role: string, zh: string) => CHECKS["C-W8"]({ kind: "body", role, zh, args: [] }, ctx);
    expect(run("状态", "失败 · 报错已交回")).toBeNull();
    expect(run("说明", "失败 · 报错已交回"), "不是状态句：缺主语照逮").not.toBeNull();
    expect(run("状态", "重启失败 · 报错已交回"), "「<动作>失败」照判原因格").not.toBeNull();
    expect(run("状态", "失败 · 磁盘满 · 写入失败 · 盘写满"), "后面的「失败 · X」照判").not.toBeNull();
  });
});

describe("复选框 / 开关标签允许动词开头 —— 规矩与检法同拍", () => {
  const rules = loadRules();
  const ctx = y3Ctx(rules, loadTerms());
  const y3 = rules.find((r) => r.id === "C-Y3")!;

  it("★ 规矩里写着射程，检法读的就是那一格：control 不在 imperativeKinds 里、title 在", () => {
    expect(y3.imperativeKinds, "C-Y3 没有 imperativeKinds —— 检法会退回写死的那一份").toBeDefined();
    expect(ctx.imperativeKinds).toEqual(y3.imperativeKinds);
    expect(ctx.imperativeKinds).toContain("title");
    expect(ctx.imperativeKinds, "control 档许以动词开头，射程里却还有它").not.toContain("control");
    expect(y3.rule, "规矩文字没写这条裁决 —— 规范与检法又会各说各的").toMatch(/control 档.*许以动词开头/);
  });

  it("★ 同一串按档各跑一遍：control 档放过、title 档逮住（判别在档，不在串）", () => {
    const good = y3.controlGood ?? [];
    expect(good.length, "controlGood 是空的 —— 下面的「放过」零命中地绿").toBeGreaterThan(0);
    const p: string[] = [];
    for (const zh of good) {
      if (CHECKS["C-Y3"]({ kind: "control", zh, args: [] }, ctx)) p.push(`control 档逮住了「${zh}」`);
    }
    expect(p).toEqual([]);
    // 正控：以祈使词开头的那几条换成 title 档必须红（否则「放过」可能是检法整个瞎了）。
    const verbStart = good.filter((zh) => ctx.imperative.some((w) => zh.startsWith(w)));
    expect(verbStart.length, "controlGood 里没有一条以祈使词开头 —— 这组正例证不了裁决").toBeGreaterThan(0);
    for (const zh of verbStart) {
      expect(CHECKS["C-Y3"]({ kind: "title", zh, args: [] }, ctx), `title 档放过了「${zh}」`).toMatch(/祈使动词/);
    }
  });
});

// 「aria-* 单立一个 kind、按标签面规则管」。
describe("〔FIX2〕aria 档（只进 aria-label 的无障碍名）按标签面规则管", () => {
  const rules = loadRules();
  const ctx = y3Ctx(rules, loadTerms());
  const aria = (zh: string) => ({ kind: "aria", zh, args: [] });

  it("★ 问号与口语词管它、祈使动词不管它、破折号 / 括号说明按窄档管", () => {
    expect(ctx.labelKinds).toEqual(["title", "aria"]);
    expect(ctx.imperativeKinds).not.toContain("aria");
    expect(CHECKS["C-Y3"](aria("关闭"), ctx), "按钮的无障碍名以动词开头是标准形").toBeNull();
    expect(CHECKS["C-Y3"](aria("要关掉吗？"), ctx)).toMatch(/aria 档里有问号/);
    expect(CHECKS["C-Y3"](aria("还差什么"), ctx)).toMatch(/aria 档里有口语词/);
    expect(CHECKS["C-L2"](aria("关闭 — 提示"), ctx)).toMatch(/aria 档里有破折号/);
  });
});

describe("CP2a · 文案规范判据自己会不会死（正控）", () => {
  const rules = loadRules();
  const ctx = y3Ctx(rules, loadTerms());

  it("违反未豁免 · 死豁免 · 豁免不可豁免的规矩 · 豁免不存在的规矩 —— 各红一次", () => {
    const run = (e: Entry): string => tableViolations({ "a.b.c": e }, rules, ctx).join();
    expect(run({ kind: "body", zh: "daemon 挂了", args: [] })).toMatch(/违反 C-T1/);
    expect(run({ kind: "body", zh: "好了", args: [], waive: { "C-T1": "理由" } })).toMatch(/死的/);
    expect(run({ kind: "body", zh: "**好**", args: [], waive: { "C-Y2": "理由" } })).toMatch(/不可豁免/);
    expect(run({ kind: "body", zh: "好了", args: [], waive: { "C-R3": "理由" } })).toMatch(/不是一条机检规矩/);
  });

  it("欠账名单：在名单里的违反放过 · 名单里已不违反的红 · 名单里的键不在表里红 · 没排序红 · 名单与豁免二选一", () => {
    const bad = { kind: "body", zh: "这台已坏", args: [] };
    const run = (t: Record<string, Entry>, d: Debt): string => tableViolations(t, rules, ctx, d).join("\n");
    expect(run({ "a.b.c": bad }, {})).toMatch(/违反 C-W17/);
    expect(run({ "a.b.c": bad }, { "C-W17": ["a.b.c"] })).toBe("");
    expect(run({ "a.b.c": { kind: "body", zh: "已坏", args: [] } }, { "C-W17": ["a.b.c"] })).toMatch(/从欠账名单里删掉/);
    expect(run({ "a.b.c": bad }, { "C-W17": ["a.b.c", "x.y.z"] })).toMatch(/x\.y\.z 不在表里/);
    expect(run({ "a.b.c": bad, "a.b.d": bad }, { "C-W17": ["a.b.d", "a.b.c"] })).toMatch(/没排序/);
    expect(run({ "a.b.c": { ...bad, waive: { "C-W17": "理由" } } }, { "C-W17": ["a.b.c"] })).toMatch(/二选一/);
    expect(run({ "a.b.c": { kind: "title", zh: "这台", args: [], waive: { "C-W17": "理由" } } }, {})).toMatch(/title 档不可豁免/);
  });

  it("C-W18 句子里不许有原话 / 码：按占位符的语义判（原话型占位红，原因词 · 对象型不红；命令行豁免）", () => {
    const run = (e: Entry): string | null => CHECKS["C-W18"](e, ctx);
    expect(run({ kind: "error", zh: "结束 {target} 失败 · tmux 报 {e}", args: ["target", "e"] })).toMatch(/\{e\}/);
    expect(run({ kind: "error", zh: "应答无法解析 · 「{reply}」", args: ["reply"] })).toMatch(/\{reply\}/);
    expect(run({ kind: "error", zh: "进程退出 {st}", args: ["st"] })).toMatch(/\{st\}/);
    expect(run({ kind: "body", zh: "后端报 {message}", args: ["message"] })).toMatch(/\{message\}/);
    expect(run({ kind: "error", zh: "结束 {target} 失败 · {why}", args: ["target", "why"] })).toBeNull();
    expect(run({ kind: "error", zh: "{e}", args: ["e"], role: "命令行" })).toBeNull();
    // 按值认：占位名不在原话型名单里（`{kind}`），生产代码喂的却是 `e.to_string()` ⇒ 一样红（审计二第 2 条那一族）。
    const fedRaw: Entry = { kind: "error", zh: "目录打不开 · {kind}", args: ["kind"] };
    const fed = (e: Entry, phs: string[]): string | null => CHECKS["C-W18"](e, { ...ctx, rawFed: new Map([["a.b.c", new Set(phs)]]) }, "a.b.c");
    expect(fed(fedRaw, ["kind"])).toMatch(/\{kind\}/);
    expect(fed(fedRaw, [])).toBeNull();
    expect(run(fedRaw), "没喂原话的同形那一条不红").toBeNull();
    // 值的认法：错误值直接变成字串的才算（e / err / error 的 to_string · format!），别的值不算。
    const raw = (v: string): string[] => rustRefsIn("x.rs", `copy_text("a.b.c", &[("x", ${v})]);`).refs.flatMap((r) => r.raw ?? []);
    for (const v of ["&e.to_string()", "&err.to_string()", "&error.to_string()", '&format!("{e}")', '&format!("{e:?}")']) expect(raw(v), v).toEqual(["x"]);
    for (const v of ["&o.to_string()", "&io_reason(e.kind())", "n", '&format!("{n}")', "&e.kind().to_string()"]) expect(raw(v), v).toEqual([]);
  });

  it("占位符名不算文字：{origin} 不会被术语表的 origin 禁词扫红", () => {
    expect(CHECKS["C-T1"]({ kind: "title", zh: "[{origin}] 预览", args: ["origin"] }, ctx)).toBeNull();
    expect(CHECKS["C-T1"]({ kind: "title", zh: "origin 预览", args: [] }, ctx)).not.toBeNull();
  });

  it("两个抽取器在合成语料上各抽得出东西", () => {
    expect(titledIds('it("[C-K1][C-K2] x", () => {});\nit("y", () => {});')).toEqual(["C-K1", "C-K2"]);
    expect(censusImperative('R5_IMPERATIVE = re.compile(r"^\\s*(启用|停用)")')).toEqual(["启用", "停用"]);
  });
});
