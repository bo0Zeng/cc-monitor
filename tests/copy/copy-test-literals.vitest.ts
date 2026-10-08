/**
 * **测试按文案键断言，不按中文原文**：测试源码里零处与文案表某条 `zh` 逐字相同的中文字面量，
 * 例外逐条写进 `test-literal-exempt.json`（按 文件 · 那句 · 用途 认）。
 *
 * # 为什么
 *
 * 文案表改一句话，不该撞测试：断言写成 `copyText("键", {…})` / `copyPattern("键")`（TS）、
 * `copy_core::copy_text` / `copy_static!` / `copy_matches`（Rust），改表值时它们跟着变。
 * 测试里照抄的原文不会跟着变 ⇒ 改一句就红一片，还会诱人去改测试迁就文案。
 *
 * # 判什么
 *
 * 1. 人群：测试源码里的字符串字面量，去掉首尾空白后至少 2 个汉字、且与表里某条 `zh`（去掉首尾空白）逐字相同。
 *    两种位置按形状不算（按构造不和产品输出比）：Rust 的 `.expect("…")` / `.expect_err("…")` 报错语，
 *    `it` / `test` / `describe` 的用例名。
 * 2. 人群里每一处的（文件 · 字面量）都要在豁免表里、用途取自闭集 `USES`；否则红 —— 改成按键断言。
 * 3. 豁免表里每一条都要在那份文件里真有一处命中（死豁免红）；登记的文件要存在。
 * 4. 正控：同一个扫描器对着样本数得出表里的原句，跳过上面两种位置与注释，对着不在表里的中文、多一字的句子数出 0。
 *
 * # 不判什么（诚实段）
 *
 * - 只认整条相等：`toContain("某句的一截")` 这种半句断言、带占位符的模板串不在人群里（按半句认，噪声大过信号）。
 * - 截图场景（`tests/shots/scenes/` 与同类的组件总览 `tests/shots/kit-gallery.ts`：合成数据画组件）、
 *   文案表自己的判据（`tests/copy/`）、取文口自测（`tests/common/copy-core/`）不进人群。
 * - 按行用正则抠字面量，不是编译器；整行注释跳过，行尾注释里的引号会被当成字面量（注释里引原文用「」）。
 */
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { SCAN_TIMEOUT_MS } from "../test-support/production-sources.ts";
import { REPO_ROOT } from "../test-support/repo-root.ts";
import { TABLE_PATH } from "./copy-support.ts";

const EXEMPT_PATH = resolve(REPO_ROOT, "tests", "copy", "test-literal-exempt.json");
const OUT_OF_SCOPE = [/^tests\/shots\/scenes\//, /^tests\/shots\/kit-gallery\.ts$/, /^tests\/copy\//, /^tests\/common\/copy-core\//];

/** 豁免的用途闭集：这几类字面量本来就不该换成键。 */
export const USES = {
  输入: "喂给被测函数的值（标签 · 机器名 · 标题 · 搜索词），恰好与表里某条同字",
  回显: "夹具里替后端或上游说的话，断言原样带回",
  用例名: "表驱动用例的名字、等待点的标签",
  非文案: "登记表 · 守卫词表 · 说明格里的字，与表里某条同字异义",
  词义: "断言某个词在或不在（性质），不是钉某一句",
} as const;
type Use = keyof typeof USES;

interface Exempt {
  text: string;
  use: Use;
}

const HAN = /[㐀-鿿]/g;
const hanCount = (s: string): number => (s.match(HAN) ?? []).length;

function tableValues(): Set<string> {
  const entries = JSON.parse(readFileSync(TABLE_PATH, "utf8")).entries as Record<string, { zh: string }>;
  const out = new Set<string>();
  for (const e of Object.values(entries)) {
    const t = e.zh.trim();
    if (hanCount(t) >= 2) out.add(t);
  }
  return out;
}

const LITERAL = /"((?:\\.|[^"\\])*)"|'((?:\\.|[^'\\])*)'|`((?:\\.|[^`\\])*)`/g;
const escapeRe = (s: string): string => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** 一份源码里与表值逐字相同的中文字面量（按出现顺序，含重复）。 */
export function copyLiterals(src: string, values: ReadonlySet<string>): string[] {
  const out: string[] = [];
  for (const line of src.split("\n")) {
    if (/^\s*(\/\/|\*|\/\*)/.test(line)) continue;
    for (const m of line.matchAll(LITERAL)) {
      const lit = (m[1] ?? m[2] ?? m[3] ?? "").trim();
      if (hanCount(lit) < 2 || !values.has(lit)) continue;
      const q = escapeRe(m[0]);
      if (new RegExp(`\\.expect(?:_err)?\\(\\s*${q}\\s*\\)`).test(line)) continue;
      if (new RegExp(`\\b(?:it|test|describe)(?:\\.\\w+)*\\(\\s*${q}`).test(line)) continue;
      out.push(lit);
    }
  }
  return out;
}

function testSources(): string[] {
  const out = execFileSync("git", ["ls-files", "--", "tests"], { cwd: REPO_ROOT, encoding: "utf8" });
  return out
    .split("\n")
    .filter((f) => /\.(ts|mts|rs)$/.test(f) && !f.endsWith(".d.ts"))
    .filter((f) => !OUT_OF_SCOPE.some((re) => re.test(f)));
}

describe("测试按文案键断言（与表值逐字相同的中文字面量）", () => {
  const values = tableValues();

  it("正控：表里的原句数得出来；报错语 · 用例名 · 注释 · 不在表里的中文 · 多一字的都不算", () => {
    const one = [...values].filter((v) => !/[\n"\\]/.test(v)).sort((a, b) => b.length - a.length)[0];
    const q = JSON.stringify(one);
    expect(copyLiterals(`expect(x).toBe(${q});`, values)).toEqual([one]);
    expect(copyLiterals(`assert_eq!(s, ${q});`, values)).toEqual([one]);
    expect(copyLiterals(`let v = parse(&a).expect(${q});`, values)).toEqual([]);
    expect(copyLiterals(`it(${q}, () => {});`, values)).toEqual([]);
    expect(copyLiterals(`// expect(x).toBe(${q})`, values)).toEqual([]);
    expect(copyLiterals('const t = "蒹葭苍苍白露为霜";', values)).toEqual([]);
    expect(copyLiterals(`expect(x).toBe(${JSON.stringify(one + "啊")});`, values)).toEqual([]);
  });

  it("★ 测试里零处与表值逐字相同的中文字面量（豁免表里的除外）· 豁免表无死条", () => {
    const exempt = JSON.parse(readFileSync(EXEMPT_PATH, "utf8")).files as Record<string, Exempt[]>;
    const files = testSources();
    expect(files.length, "一份测试源码都没找到 —— git ls-files 没接上").toBeGreaterThan(100);
    const bad = new Set<string>();
    const used = new Set<string>();
    let population = 0;
    for (const f of files) {
      const lits = copyLiterals(readFileSync(resolve(REPO_ROOT, f), "utf8"), values);
      population += lits.length;
      const mine = new Set((exempt[f] ?? []).map((e) => e.text));
      for (const lit of lits) {
        if (mine.has(lit)) used.add(`${f}\u0000${lit}`);
        else bad.add(`${f} · ${lit}`);
      }
    }
    expect(population, "人群为 0 —— 扫描器没接上（豁免表里明明有登记）").toBeGreaterThan(0);
    expect(
      [...bad],
      "这几处按中文原文写 —— 断言改成按文案键（copyText / copyPattern / copy_text / copy_matches）；喂进去的输入 · 夹具回显 · 用例名才进 test-literal-exempt.json",
    ).toEqual([]);
    const dead: string[] = [];
    const badUse: string[] = [];
    for (const [f, list] of Object.entries(exempt)) {
      if (!existsSync(resolve(REPO_ROOT, f))) dead.push(`${f}（文件不在）`);
      for (const e of list) {
        if (!(e.use in USES)) badUse.push(`${f} · ${e.text} · ${e.use}`);
        if (!used.has(`${f}\u0000${e.text}`)) dead.push(`${f} · ${e.text}`);
      }
    }
    expect(badUse, `用途不在闭集里（${Object.keys(USES).join(" · ")}）`).toEqual([]);
    expect(dead, "死豁免：那份文件里已没有这句（或表值变了）—— 从豁免表删掉").toEqual([]);
  }, SCAN_TIMEOUT_MS);
});
