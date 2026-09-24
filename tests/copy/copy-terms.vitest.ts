/**
 * CP2a · 术语对照表（`调研/设计/91 §5.2④`）的判据。表住 `src/shared/copy/terms.json`。
 *
 * # 它治的病
 *
 * `91 §5.2④` 逐字：R1 禁词表说**不许出现什么**，术语表说**该说什么**；「今天只有前者 ⇒
 * 判据红了之后人只能自己编一个词」。这张表要是只靠人记着维护，会和散文一起腐 ——
 * 所以它的人群**不是本文件自己定的**，而是从两份别人写的东西里借来、两向对拍：
 *
 * | 对拍 | 另一侧住哪 | 挡什么 |
 * |---|---|---|
 * | 表里 `census` 的集合 == 普查量具的 `R1_WORDS ∪ R1_CANDIDATES` | `tests/evidence/K-T68-A1-outward-copy-census.py` | 普查加了一个禁词 / 候选词，术语表没给换法 |
 * | 表里 `ledger` 的并集 == CP1 台账依据列里的全部 `新词:` 标记 | `tests/evidence/CP1-copy-verdicts.tsv` | 裁文案的人标出一个新内部词，术语表没收 |
 *
 * 两侧是**异源**的：普查那张表是照 `91 §4` 抄的，台账标记是 6 路裁文案时逐条手标的，
 * 都不是从本表生成的 ⇒ 相等不是恒真。
 *
 * # 它**不**判什么（诚实段）
 *
 * - 不判换法好不好（`say` 写得对不对）—— 那是人工 review。
 * - `scan: null` 的条目（单字、普通词的内部义）**机器判不了**，只登记 `noScanWhy`。
 * - 本文件只判表自己；拿表去扫文案的那一条住 `copy-table.vitest.ts`。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { REPO_ROOT } from "../test-support/repo-root.ts";
import { censusList, loadTerms, scannerOf, type Term } from "./copy-support.ts";

const CENSUS = resolve(REPO_ROOT, "tests", "evidence", "K-T68-A1-outward-copy-census.py");
const LEDGER = resolve(REPO_ROOT, "tests", "evidence", "CP1-copy-verdicts.tsv");

/** 台账依据列里的 `新词:` 标记。标记止于空白或中文标点 / 括号（与 CP1 头注第五段同口径）。 */
export function ledgerMarkers(tsv: string): Set<string> {
  const out = new Set<string>();
  for (const line of tsv.split("\n").slice(1)) {
    const basis = line.split("\t")[3];
    if (!basis) continue;
    for (const m of basis.matchAll(/新词:([^\s（(，。；、)）]+)/g)) out.add(m[1]);
  }
  return out;
}

function diff(a: Iterable<string>, b: Set<string>): string[] {
  return [...new Set(a)].filter((x) => !b.has(x)).sort();
}

const TIERS = new Set(["正名", "限用", "禁"]);

/** 表自己的形状问题（纯函数，死值验与正控都喂它）。 */
export function shapeProblems(terms: Term[]): string[] {
  const p: string[] = [];
  const seen = new Set<string>();
  const named = new Set(terms.filter((t) => t.tier !== "禁").map((t) => t.word));
  for (const t of terms) {
    const at = `「${t.word}」`;
    if (!t.word) p.push("有一条没有 word");
    if (seen.has(t.word)) p.push(`${at} 重复`);
    seen.add(t.word);
    if (!TIERS.has(t.tier)) p.push(`${at} 的 tier「${t.tier}」不在三档里`);
    if (!t.source) p.push(`${at} 没写出处`);
    if (t.tier === "正名" && !t.meaning) p.push(`${at} 是正名却没写 meaning`);
    if (t.tier === "限用" && !t.context) p.push(`${at} 是限用词却没写许说的语境（R1b 第 1 条）`);
    if (t.tier === "禁") {
      if (!t.say) p.push(`${at} 是禁词却没写换成什么（say）`);
      if (t.points !== undefined && !named.has(t.points))
        p.push(`${at} 的 points「${t.points}」不是表里的正名或限用词 —— 换法指向了一个没登记的词`);
      if (t.scan === undefined) p.push(`${at} 没写 scan（判不了就写 null 并给 noScanWhy）`);
      if (t.scan === null && !t.noScanWhy) p.push(`${at} scan 为 null 却没说为什么判不了`);
    } else {
      if (t.scan) p.push(`${at} 不是禁词却带了 scan —— R1 会把它扫红`);
      if (t.points !== undefined) p.push(`${at} 不是禁词却带了 points`);
    }
    const rx = (() => {
      try {
        return scannerOf(t);
      } catch (e) {
        p.push(`${at} 的 scan 编不过：${String(e)}`);
        return null;
      }
    })();
    if (rx) {
      for (const ex of t.examples ?? [t.word])
        if (!rx.test(ex)) p.push(`${at} 的 scan 扫不到样例「${ex}」—— 死正则`);
      for (const ce of t.counterExamples ?? [])
        if (rx.test(ce)) p.push(`${at} 的 scan 误中了反例「${ce}」`);
    }
  }
  return p;
}

describe("CP2a · 术语对照表", () => {
  const terms = loadTerms();
  const py = readFileSync(CENSUS, "utf8");
  const tsv = readFileSync(LEDGER, "utf8");

  it("表的形状：三档 · 禁词有换法且换法指向登记过的词 · 扫描正则不死、不误中反例", () => {
    expect(terms.length, "术语表一条都没读出来 —— 下面的对拍会零命中地绿").toBeGreaterThan(0);
    expect(shapeProblems(terms)).toEqual([]);
  });

  it("三档都有人（任何一档被清空，R1 / R1b 那一侧就空转）", () => {
    const tiers = new Set(terms.map((t) => t.tier));
    expect([...tiers].sort()).toEqual(["正名", "禁", "限用"].sort());
  });

  it("★ 表里 census 的集合 == 普查的 R1 禁词表 ∪ R1 候选（两向），且 R1 禁词表里的每一个都在禁档", () => {
    const r1 = censusList(py, "R1_WORDS");
    const cand = censusList(py, "R1_CANDIDATES");
    // 正控：抽取器真的读到了两张表（锚两个一定在的名字）。
    expect(r1, "没从普查里抽出 R1_WORDS —— 抽取器坏了，下面的相等会变成空集对空集").toContain("daemon");
    expect(cand, "没从普查里抽出 R1_CANDIDATES").toContain("tmux");
    const ours = terms.filter((t) => t.census !== undefined).map((t) => t.census as string);
    const theirs = new Set([...r1, ...cand]);
    expect(diff(theirs, new Set(ours)), "普查有、术语表没给处置").toEqual([]);
    expect(diff(ours, theirs), "术语表写了 census 名，普查里没有（改名了？）").toEqual([]);
    const byCensus = new Map(terms.map((t) => [t.census, t]));
    const notBanned = r1.filter((w) => byCensus.get(w)?.tier !== "禁");
    expect(notBanned, "R1 禁词表里的词在术语表里不是禁档").toEqual([]);
  });

  it("★ 表里 ledger 的并集 == CP1 台账的全部「新词:」标记（两向）", () => {
    const marks = ledgerMarkers(tsv);
    // 正控：锚两个裁文案时一定标过的词。
    // 〔第二波 T4 09-24〕第二个锚从「拉前」换成「围栏」：标「新词:拉前」的那四行随原文（E73 那段）一起删了，
    //   台账里已经没有它 —— 锚一个真没了的词，这条正控会把「删对了」读成「抽取器坏了」。
    expect(marks.has("backend") && marks.has("围栏"), "台账标记一个都没抽到 —— 抽取器坏了").toBe(true);
    const ours = terms.flatMap((t) => t.ledger ?? []);
    expect(diff(marks, new Set(ours)), "台账标了新词，术语表没收（去 terms.json 给它一个处置）").toEqual([]);
    expect(diff(ours, marks), "术语表认领了一个台账里已经没有的标记").toEqual([]);
    const dup = ours.filter((m, i) => ours.indexOf(m) !== i);
    expect(dup, "同一个台账标记被两条术语认领").toEqual([]);
  });

  it("CP1 判不了的两条都有了答案（ccm · in-place 都是限用，并写明语境）", () => {
    const ccm = terms.find((t) => t.word === "ccm");
    const inPlace = terms.find((t) => t.word === "in-place");
    expect(ccm?.tier).toBe("限用");
    expect(inPlace?.tier).toBe("限用");
    expect(ccm?.context && inPlace?.context).toBeTruthy();
  });
});

describe("CP2a · 术语表判据自己会不会死（正控）", () => {
  const base: Term = { word: "daemon", tier: "禁", say: "后端。", scan: { re: "daemon", flags: "i" }, source: "x" };

  it("禁词不给换法 / 换法指向没登记的词 / 死正则 / 误中反例 —— 各红一次", () => {
    expect(shapeProblems([{ ...base, say: undefined }]).join()).toMatch(/没写换成什么/);
    expect(shapeProblems([{ ...base, points: "不存在的词" }]).join()).toMatch(/不是表里的正名或限用词/);
    expect(shapeProblems([{ ...base, scan: { re: "daem0n", flags: "" } }]).join()).toMatch(/死正则/);
    expect(shapeProblems([{ ...base, counterExamples: ["daemon"] }]).join()).toMatch(/误中了反例/);
    expect(shapeProblems([{ word: "tmux", tier: "限用", source: "x" }]).join()).toMatch(/许说的语境/);
  });

  it("两个抽取器在合成语料上各抽得出东西", () => {
    const py = 'X = 1\nR1_WORDS = [\n    ("daemon", re.compile(r"daemon")),\n]\n';
    expect(censusList(py, "R1_WORDS")).toEqual(["daemon"]);
    const tsv = "住址\t原文\t裁词\t依据\na:1\tx\t保留\t[对外] 新词:拉前 新词:C8①（设计条目号\n";
    expect([...ledgerMarkers(tsv)].sort()).toEqual(["C8①", "拉前"]);
  });
});
