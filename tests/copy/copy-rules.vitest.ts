/**
 * CP2a · 文案规范（`调研/设计/91 §5.2` 集中分析的产物）的判据。规范住 `src/shared/copy/rules.json`。
 *
 * # 它治的病
 *
 * `91 §5.2` 逐字：「规范里每一条都要标能不能机检；不能机检的如实标『主观项』。
 * **一份不标这个的规范会和散文一起腐**，而且腐了没人会发现」。所以本文件判三件事：
 *
 * 1. ★ **规矩 ↔ 实现两向相等**：`rules.json` 里标「机检」的规矩号 ==
 *    本文件 `CHECKS` 的键 ∪ `copy-table.vitest.ts` 里以 `[C-xx]` 开头的测试标题。
 *    标了机检却没实现 ⇒ 红（规范在吹牛）；实现了却标主观 / 没登记 ⇒ 红（规范漏记）。
 *    两侧异源：一侧是 JSON 里人写的标签，一侧是代码里真存在的检查。
 * 2. ★ **文案表逐条过全部机检规矩**：违反的要么改，要么在该条 `waive` 里登记理由；
 *    不可豁免的规矩没有例外；**豁免了却没违反 ⇒ 红（死豁免）**。
 * 3. **规范里的正反例与实现一致**：每条机检规矩的 `bad` 必须被它自己的检查逮住、`good` 必须放过
 *    —— 否则规范写的是一回事、检查查的是另一回事（`91 §4` R5 第一版就栽在这里：
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
import { censusList, loadTable, loadTerms, r1Hits, speech, type Entry, type Term } from "./copy-support.ts";

const RULES_PATH = resolve(REPO_ROOT, "src", "shared", "copy", "rules.json");
const TABLE_TESTS = resolve(REPO_ROOT, "tests", "copy", "copy-table.vitest.ts");
const CENSUS = resolve(REPO_ROOT, "tests", "evidence", "K-T68-A1-outward-copy-census.py");

interface Rule {
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
}

function loadRules(): Rule[] {
  return (JSON.parse(readFileSync(RULES_PATH, "utf8")) as { rules?: Rule[] }).rules ?? [];
}

interface Ctx {
  terms: Term[];
  colloquial: string[];
  imperative: string[];
}

const NARROW = new Set(["title", "control", "action"]);
const DASH = /——|—/g;
const PAREN = /（([^（）]*)）|\(([^()]*)\)/g;
const HAN = /[一-鿿]/g;
const hanCount = (s: string): number => (s.match(HAN) ?? []).length;

/** 一条机检规矩 = 对一条文案给出「违反了什么」（`null` = 没违反）。 */
type Check = (e: Entry, ctx: Ctx) => string | null;

export const CHECKS: Record<string, Check> = {
  "C-Y2": (e) => (/\*\*|`|⇒|🔴/.test(e.zh) ? "有 markdown / 论证符号" : null),
  "C-Y3": (e, ctx) => {
    if (e.kind !== "title") return null;
    const s = speech(e.zh);
    if (/[？?]/.test(s)) return "title 档里有问号";
    const c = ctx.colloquial.find((w) => s.indexOf(w) >= 0);
    if (c) return `title 档里有口语词「${c}」`;
    const v = ctx.imperative.find((w) => s.trimStart().startsWith(w));
    return v ? `title 档以祈使动词「${v}」开头` : null;
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
      const g = groups.find((x) => hanCount(x) >= 2);
      return g !== undefined ? `${e.kind} 档的括号里装了说明「${g}」` : null;
    }
    const n = groups.filter((x) => x.trim().length >= 2).length;
    return n > 1 ? `括号补充 ${n} 处 > 1` : null;
  },
  "C-L4": (e) => (/^\s*(?:（[^（）]*）|\([^()]*\))\s*$/.test(e.zh) ? "整条被括号包住" : null),
};

/** 文案表逐条过全部机检规矩，带豁免：违反未豁免 · 豁免不可豁免的 · 死豁免 · 豁免了不存在的规矩，都是问题。 */
export function tableViolations(table: Record<string, Entry>, rules: Rule[], ctx: Ctx): string[] {
  const out: string[] = [];
  const byId = new Map(rules.map((r) => [r.id, r]));
  for (const [key, e] of Object.entries(table)) {
    const waive = e.waive ?? {};
    for (const id of Object.keys(waive)) {
      const r = byId.get(id);
      if (!r || r.check !== "机检") out.push(`${key}：豁免了 ${id}，它不是一条机检规矩`);
      else if (!r.waivable) out.push(`${key}：${id} 不可豁免`);
      if (!waive[id]?.trim()) out.push(`${key}：豁免 ${id} 没写理由`);
    }
    for (const [id, check] of Object.entries(CHECKS)) {
      const v = check(e, ctx);
      if (v && !(id in waive)) out.push(`${key}：违反 ${id}（${v}）`);
      if (!v && id in waive) out.push(`${key}：${id} 的豁免是死的 —— 它没违反这一条，删掉豁免`);
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
  const y3 = rules.find((r) => r.id === "C-Y3");
  const ctx: Ctx = { terms, colloquial: y3?.colloquial ?? [], imperative: y3?.imperative ?? [] };

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
    expect(tableViolations(table, rules, ctx)).toEqual([]);
  });

  it("规范里的正反例与实现一致：bad 被逮住、good 被放过", () => {
    const p: string[] = [];
    for (const r of rules) {
      const check = CHECKS[r.id];
      if (!check) continue;
      const kind = r.probeKind ?? "body";
      for (const zh of r.bad)
        if (!check({ kind, zh, args: [] }, ctx)) p.push(`${r.id} 放过了自己的反例「${zh}」`);
      for (const zh of r.good)
        if (check({ kind, zh, args: [] }, ctx)) p.push(`${r.id} 逮住了自己的正例「${zh}」`);
    }
    expect(p).toEqual([]);
  });
});

describe("CP2a · 文案规范判据自己会不会死（正控）", () => {
  const rules = loadRules();
  const y3 = rules.find((r) => r.id === "C-Y3");
  const ctx: Ctx = { terms: loadTerms(), colloquial: y3?.colloquial ?? [], imperative: y3?.imperative ?? [] };

  it("违反未豁免 · 死豁免 · 豁免不可豁免的规矩 · 豁免不存在的规矩 —— 各红一次", () => {
    const run = (e: Entry): string => tableViolations({ "a.b.c": e }, rules, ctx).join();
    expect(run({ kind: "body", zh: "daemon 挂了", args: [] })).toMatch(/违反 C-T1/);
    expect(run({ kind: "body", zh: "好了", args: [], waive: { "C-T1": "理由" } })).toMatch(/死的/);
    expect(run({ kind: "body", zh: "**好**", args: [], waive: { "C-Y2": "理由" } })).toMatch(/不可豁免/);
    expect(run({ kind: "body", zh: "好了", args: [], waive: { "C-R3": "理由" } })).toMatch(/不是一条机检规矩/);
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
