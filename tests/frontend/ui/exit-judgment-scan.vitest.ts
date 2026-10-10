/**
 * **出口零判定零格式化**的扫描判据（架构复审三第 0 批 · `src/doc/ARCHITECTURE.md` §2.9）。
 *
 * 扫：桌面界面 `src/frontend/ui`（TS）与壳 `src/frontend/shell/src`（Rust）的生产代码，剥过注释，按 [`exit-rules.ts`] 每条写法指纹数命中。
 * 今天的存量登在欠账名单 `tests/frontend/ui/exit-debt.json`，**只许删、不许加**（同 `UNSHAPED` 的规矩）：
 * 1. 每一格（文件 × 规则）实得 == 名单登的数：多一处 ⇒ 红（新长出来的自己判 / 自己写字）；少了 ⇒ 也红（删了就把那一格改小或删掉，
 *    不改，下一个人就能悄悄加回去）。名单外的文件命中一处 ⇒ 红。
 * 2. 合计 == `ceiling`；`ceiling` 只许往下改（本文件钉着上一批收完时的数 [`CEILING_AT_MOST`]，比它大 ⇒ 红）。
 * 3. 名单自身：每格 `n > 0`、`why` 非空、`kind` 在闭集里、没有重复格；`roots` 里每个目录都扫到了文件（反空真）。
 * 4. 阳性 / 阴性对照：每条规则对自己的样例各恰好命中一次 / 零次（探测器坏了 ⇒ 先红）。
 *
 * 手机端（`src/mobile`）并进来以后：在名单 `roots` 里加它那一（几）个目录与语言，跑一次本文件看红出来的那几格，
 * 逐格登进 `entries`（`kind` 照实填），`ceiling` 加上那几格的和、[`CEILING_AT_MOST`] 同改 —— 那一拍是唯一一次许「加」，之后同样只许删。
 *
 * 买不到：指纹只认登记过的写法（换个写法重写一遍看不见）；命中的也不全是违例（界面自己的骨架词表会撞 `caseWord`），
 * 名单里 `kind: "留"` 的格写明凭什么留。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { productionRsFiles, productionTsFiles, SCAN_TIMEOUT_MS } from "../../test-support/production-sources.ts";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { stripComments } from "../../test-support/strip-comments.ts";
import { RS_RULES, TS_RULES, type ExitRule } from "./exit-rules.ts";

/** 上一批收完时的合计（`ceiling` 只许往下改；改小了这里一起改小）。 */
const CEILING_AT_MOST = 279;

interface Debt {
  roots: { dir: string; lang: "ts" | "rs" }[];
  ceiling: number;
  entries: { file: string; rule: string; n: number; kind: string; why: string }[];
}

const KINDS = new Set(["第 1 批", "第 3 批", "第 7 批", "待分", "留"]);

const debt = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/frontend/ui/exit-debt.json"), "utf8")) as Debt;

function rulesOf(lang: "ts" | "rs"): ExitRule[] {
  return lang === "ts" ? TS_RULES : RS_RULES;
}

/** 盘上实得：`文件 · 规则` ⇒ 命中数（零的不进来）。 */
function scan(): { got: Map<string, number>; files: Map<string, number> } {
  const got = new Map<string, number>();
  const files = new Map<string, number>();
  for (const { dir, lang } of debt.roots) {
    const srcs = lang === "ts" ? productionTsFiles(dir) : productionRsFiles(dir);
    files.set(dir, srcs.length);
    for (const s of srcs) {
      const code = stripComments(s.text, lang === "rs" ? "rust" : "ts");
      for (const r of rulesOf(lang)) {
        const n = code.match(r.re)?.length ?? 0;
        if (n > 0) got.set(`${s.file} · ${r.id}`, n);
      }
    }
  }
  return { got, files };
}

describe("出口零判定零格式化：扫描 ＋ 只减不增的欠账名单", () => {
  it(
    "★ 每一格实得 == 名单登的数（多一处红 · 删了不改名单也红 · 名单外的命中红）",
    () => {
      const { got, files } = scan();
      for (const { dir } of debt.roots) expect(files.get(dir) ?? 0, `${dir} 一份生产文件都没扫到 —— 遍历坏了，下面恒绿`).toBeGreaterThan(5);
      const want = new Map(debt.entries.map((e) => [`${e.file} · ${e.rule}`, e.n]));
      const wrong: string[] = [];
      for (const [k, n] of got) {
        const w = want.get(k);
        if (w === undefined) wrong.push(`${k}：命中 ${n} 处，名单里没有 —— 出口新长出了自己判 / 自己写字（交核心写好那一格，界面照抄）`);
        else if (w !== n) wrong.push(`${k}：命中 ${n} 处，名单登 ${w}${n < w ? " —— 收掉了就把名单那一格改成实数（不改，下一个人就能悄悄加回去）" : " —— 多出来的那几处交核心写"}`);
      }
      for (const [k, w] of want) if (!got.has(k)) wrong.push(`${k}：一处都没有了，名单还登 ${w} —— 删掉那一格`);
      expect(wrong, wrong.join("\n")).toEqual([]);
    },
    SCAN_TIMEOUT_MS,
  );

  it("★ 合计 == ceiling，ceiling 只许往下改", () => {
    const sum = debt.entries.reduce((a, e) => a + e.n, 0);
    expect(sum, "名单各格之和与 ceiling 不一致 —— 收掉几处就把 ceiling 降到现数").toBe(debt.ceiling);
    expect(debt.ceiling, `ceiling ${debt.ceiling} 比上一批收完时的 ${CEILING_AT_MOST} 大 —— 名单只许删`).toBeLessThanOrEqual(CEILING_AT_MOST);
    expect(debt.ceiling, `ceiling 降到了 ${debt.ceiling} ⇒ 本文件的 CEILING_AT_MOST 一起改小`).toBe(CEILING_AT_MOST);
  });

  it("名单自身：每格 n > 0 · kind 在闭集里 · why 非空 · 没有重复格 · 规则都认得", () => {
    const seen = new Set<string>();
    const bad: string[] = [];
    for (const e of debt.entries) {
      const k = `${e.file} · ${e.rule}`;
      const lang = debt.roots.find((r) => e.file.startsWith(`${r.dir}/`))?.lang;
      if (!lang) bad.push(`${k}：不在任何一个 roots 目录下`);
      else if (!rulesOf(lang).some((r) => r.id === e.rule)) bad.push(`${k}：没有这条规则`);
      if (seen.has(k)) bad.push(`${k}：重复`);
      seen.add(k);
      if (!(Number.isInteger(e.n) && e.n > 0)) bad.push(`${k}：n 要是正整数`);
      if (!KINDS.has(e.kind)) bad.push(`${k}：kind「${e.kind}」不在闭集 ${[...KINDS].join(" / ")}`);
      if (e.why.trim() === "") bad.push(`${k}：why 空着`);
    }
    expect(bad, bad.join("\n")).toEqual([]);
  });

  it("阳性 / 阴性对照：每条规则对自己的样例各恰好命中一次 / 零次", () => {
    const bad: string[] = [];
    for (const r of [...TS_RULES, ...RS_RULES]) {
      expect(r.hits.length, `${r.id} 没有阳性样例`).toBeGreaterThan(0);
      for (const h of r.hits) if ((h.match(r.re)?.length ?? 0) !== 1) bad.push(`${r.id} 阳性「${h}」没恰好命中一次`);
      for (const m of r.misses) if ((m.match(r.re)?.length ?? 0) !== 0) bad.push(`${r.id} 阴性「${m}」命中了`);
    }
    expect(bad, bad.join("\n")).toEqual([]);
  });
});
