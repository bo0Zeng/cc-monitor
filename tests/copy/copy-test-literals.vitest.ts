/**
 * **测试按文案键断言，不按中文原文**：测试源码里与文案表某句固定片段重合的中文字面量，逐份数清、登记成账，只许减不许长。
 *
 * # 为什么
 *
 * 文案表改一句话，不该撞测试：断言写成 `copyText("键", {…})` / `copyPattern("键")`（TS）、
 * `copy_core::copy_text` / `copy_static!` / `copy_matches`（Rust），改表值时它们跟着变。
 * 测试里照抄的原文不会跟着变 ⇒ 改一句就红一片，还会诱人去改测试迁就文案。
 *
 * # 判什么
 *
 * 1. 每份测试源码里「像文案的中文字面量」的处数 ≤ `test-literal-ledger.json` 里那份的登记数
 *    （多了 ⇒ 新长出按原文的断言，改成按键；少了不拦 —— 改表值会让片段变、数自然变小，不逼别的路去改账）。
 *    ⚠ 过渡形：终态换成「测试里零处与文案表值逐字相同的中文字面量 ＋ 按 文件 · 键或用途 认的明写豁免表」，那时本账删掉。
 *    「像文案」：字面量（去掉首尾空白）至少 3 个汉字、且是表里某条固定片段的子串；或至少 2 个汉字、且恰等于某条固定片段。
 *    固定片段 = 表里一条 `zh` 按占位符与换行切开的每一段。
 * 2. 正控：同一个计数器对着一段带表里原句的样本数得出来；对着不在表里的中文数出 0。
 *
 * # 不判什么（诚实段）
 *
 * - 剩下登记着的多是**夹具与输入**（喂给被测函数的会话标题、后端原话、命令行回显），本来就不该换成键；
 *   也有少数是同字异键、还没认清该挂哪个键的断言。账只保证不再长，不保证每一处都该留。
 * - 截图场景（`tests/shots/scenes/`）、文案表自己的判据（`tests/copy/`）、取文口自测（`tests/common/copy-core/`，
 *   它要对着写死的句子验插值）不进人群。
 * - 按行用正则抠字面量，不是编译器：模板串里的 `${…}` 之外的字照样算；注释行跳过。
 */
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { SCAN_TIMEOUT_MS } from "../test-support/production-sources.ts";
import { REPO_ROOT } from "../test-support/repo-root.ts";
import { TABLE_PATH } from "./copy-support.ts";

const LEDGER_PATH = resolve(REPO_ROOT, "tests", "copy", "test-literal-ledger.json");
const OUT_OF_SCOPE = [/^tests\/shots\/scenes\//, /^tests\/copy\//, /^tests\/common\/copy-core\//];

const HAN = /[㐀-鿿]/g;
const hanCount = (s: string): number => (s.match(HAN) ?? []).length;

function fragments(): string[] {
  const entries = JSON.parse(readFileSync(TABLE_PATH, "utf8")).entries as Record<string, { zh: string }>;
  const out = new Set<string>();
  for (const e of Object.values(entries)) {
    for (const p of e.zh.split(/\{[A-Za-z][A-Za-z0-9]*\}|\n/)) {
      const t = p.trim();
      if (hanCount(t) >= 2) out.add(t);
    }
  }
  return [...out];
}

/** 一份源码里「像文案」的中文字面量处数。 */
export function countCopyLiterals(src: string, frags: readonly string[], fragSet: ReadonlySet<string>): number {
  let n = 0;
  for (const line of src.split("\n")) {
    if (/^\s*(\/\/|\*|\/\*)/.test(line)) continue;
    for (const m of line.matchAll(/"((?:\\.|[^"\\])*)"|'((?:\\.|[^'\\])*)'|`((?:\\.|[^`\\])*)`/g)) {
      const lit = (m[1] ?? m[2] ?? m[3] ?? "").trim();
      const h = hanCount(lit);
      if (h < 2) continue;
      if (fragSet.has(lit) || (h >= 3 && frags.some((f) => f.includes(lit)))) n++;
    }
  }
  return n;
}

function testSources(): string[] {
  const out = execFileSync("git", ["ls-files", "--", "tests"], { cwd: REPO_ROOT, encoding: "utf8" });
  return out
    .split("\n")
    .filter((f) => /\.(ts|mts|rs)$/.test(f) && !f.endsWith(".d.ts"))
    .filter((f) => !OUT_OF_SCOPE.some((re) => re.test(f)));
}

describe("测试按文案键断言（中文原文字面量的账）", () => {
  const frags = fragments();
  const fragSet = new Set(frags);

  it("正控：表里的原句数得出来，不在表里的中文数出 0", () => {
    const longest = [...frags].sort((a, b) => b.length - a.length)[0];
    expect(countCopyLiterals(`expect(x).toBe(${JSON.stringify(longest)});`, frags, fragSet)).toBe(1);
    expect(countCopyLiterals(`assert!(s.contains("${longest.slice(0, 3)}"));`, frags, fragSet)).toBe(frags.some((f) => f.includes(longest.slice(0, 3))) ? 1 : 0);
    expect(countCopyLiterals('const t = "蒹葭苍苍白露为霜";\n// expect(x).toBe("' + longest + '")', frags, fragSet)).toBe(0);
  });

  it("★ 每份测试源码的处数不超过账上登记的数（过渡：只拦新长出来的）", () => {
    const ledger = JSON.parse(readFileSync(LEDGER_PATH, "utf8")).files as Record<string, number>;
    const got: Record<string, number> = {};
    const files = testSources();
    expect(files.length, "一份测试源码都没找到 —— git ls-files 没接上").toBeGreaterThan(100);
    for (const f of files) {
      const n = countCopyLiterals(readFileSync(resolve(REPO_ROOT, f), "utf8"), frags, fragSet);
      if (n > 0) got[f] = n;
    }
    const grew = Object.keys(got).filter((f) => got[f] > (ledger[f] ?? 0)).map((f) => `${f}: ${ledger[f] ?? 0} → ${got[f]}`);
    expect(grew, "这几份测试新长出了按中文原文的断言 —— 改成按文案键（copyText / copyPattern / copy_text / copy_matches）").toEqual([]);
  }, SCAN_TIMEOUT_MS);
});
