/**
 * **TS 侧的扫描型判据卫生**（Rust 侧是 `scanning_guard_registry` · `needle_anchor_registry`，同一套词汇）。
 *
 * 两件事：
 * 1. **读目录只在 `tests/test-support/` 里**：测试文件一律经 `production-sources.ts` 的口读目录（那一处靠构造读不到判据自己：
 *    默认不收 `.vitest.` / `.test.`），也就只有一处决定跳哪些目录、认哪几种后缀。测试里直接调目录遍历 ⇒ 红，换成那个口。
 * 2. **磁盘语料上的裸 `.includes("…")`** 两向等于登记：存在性断言不危险，正向事实钉才危险
 *    （匹配单位比事实小 ⇒ 把事实撑大的改动从缝里溜过去而判据照样绿）。
 */
import { describe, it, expect } from "vitest";

import { sourceFiles, type ProductionSource } from "../../test-support/production-sources.ts";
import { stripComments } from "../../test-support/strip-comments.ts";

/** 判据的人群是测试文件；它们住 `tests/`。`src/` 一起走，好让下面的自检点得到一份生产入口。 */
const TREES = ["src", "tests"] as const;
/** 许读目录的那一处。 */
const WALK_HOME = "tests/test-support/";

const IS_TEST = (f: string): boolean => f.endsWith(".vitest.ts") || f.endsWith(".test.ts");

/** 剥过注释的 TS 里，直接调目录遍历的那几形（`readdirSync(` · `readdir(` · `opendirSync(` · `globSync(`）。字符串与注释里提到这几个词不算；vite 的 `import.meta.glob` 是构建期的 import，不在此列。 */
const WALK_CALL = /\b(?:readdirSync|readdir|opendirSync|globSync)\s*\(/;

/**
 * ★ **磁盘语料上的裸 `.includes("…")`**（两向相等：`文件 :: 接收者.includes("字面量"`，同一处可重复）。
 * 存在性断言不危险，正向事实钉才危险；新写一处就红（换成整行 / 有边界的比法），修掉一处也红（删那一行）。
 */
const BARE_INCLUDES: readonly string[] = [
  'tests/frontend/ui/generated-boundary-guard.vitest.ts :: attrs.includes("ts_rs::TS"',
  'tests/frontend/ui/generated-boundary-guard.vitest.ts :: attrs.includes("ts_rs::TS"',
  'tests/frontend/ui/generated-boundary-guard.vitest.ts :: own.includes("skip_serializing_if"',
  'tests/frontend/ui/gray-light-wiring.vitest.ts :: viewer.includes("onFace"',
  'tests/frontend/ui/gray-light-wiring.vitest.ts :: viewer.includes("follow: {"',
  'tests/frontend/ui/paste-block.vitest.ts :: block.includes("**"',
  'tests/frontend/ui/settings/accounts-section.vitest.ts :: code.includes("api_key"',
];

/** 一个 `const`/`let` 绑定的名字与右侧（右侧只取本行）。 */
function letBinding(line: string): [string, string] | null {
  const m = /\b(?:const|let)\s+(\w+)\s*(?::[^=]+)?=\s*(.*)/.exec(line);
  return m ? [m[1], m[2]] : null;
}

/**
 * RHS 是不是**从 `v` 这份文本直接切/借出来的**。
 *
 * ⚠ 用「以它开头」而不是「包含它」—— 后者会让传递闭包跑飞（一个文件能多认出上百个语料变量）。
 */
function isDirectDerivation(rhs: string, v: string): boolean {
  const b = rhs.replace(/^[\s(&*]+/, "");
  if (!b.startsWith(v)) return false;
  const after = b.slice(v.length, v.length + 1);
  return !/[A-Za-z0-9_]/.test(after);
}

/** 这份测试源码里，哪些变量装着**从磁盘读来的**语料。 */
function corpusVars(src: string): Set<string> {
  const vars = new Set<string>();
  for (let pass = 0; pass < 2; pass++) {
    const before = vars.size;
    for (const line of src.split("\n")) {
      const b = letBinding(line);
      if (!b) continue;
      const [name, rhs] = b;
      const seeded = /readFileSync|readdirSync/.test(rhs);
      const derived = [...vars].some((v) => isDirectDerivation(rhs, v));
      if (seeded || derived) vars.add(name);
    }
    if (vars.size === before) break;
  }
  return vars;
}

/** 一份源码里直接调目录遍历的那几行（`文件:行号`）。 */
function walkCalls(f: ProductionSource): string[] {
  return stripComments(f.text, "ts")
    .replace(/"(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*'|`(?:[^`\\]|\\[\s\S])*`/g, (s) => s.replace(/[^\n]/g, " "))
    .split("\n")
    .flatMap((line, k) => (WALK_CALL.test(line) ? [`${f.file}:${k + 1}`] : []));
}

describe("TS 侧扫描型判据的卫生", () => {
  const files = TREES.flatMap((t) => sourceFiles(t, { ext: ".ts", tests: true }));
  const tests = files.filter((f) => IS_TEST(f.file));

  it("抽取器自检：真的扫到了 TS 源码树", () => {
    expect(
      tests.some((f) => f.file === "tests/frontend/ui/scanning-guard-registry.vitest.ts"),
      "本文件不在扫描面里（或没被认成测试）—— 下面两条只看测试文件，那样它们什么也没量",
    ).toBe(true);
    expect(
      files.some((f) => f.file === "src/frontend/ui/main.ts"),
      `扫描面里没有 \`main.ts\`（扫到 ${files.length} 份）—— 遍历坏了`,
    ).toBe(true);
  });

  it("★ 测试里读目录只在 `tests/test-support/`：别处零处（带正控）", () => {
    // 正控：认得出 import 来的、带命名空间的两形；字符串与注释里提到不算。
    const probe = (text: string): string[] => walkCalls({ file: "x.ts", text });
    expect(probe(`for (const e of readdirSync(dir)) {}`)).toEqual(["x.ts:1"]);
    expect(probe(`await fs.readdir(dir);`)).toEqual(["x.ts:1"]);
    expect(probe(`const s = "readdirSync(dir)"; // readdirSync(dir)`)).toEqual([]);
    // 正控：许读的那一处真的在读（认法认得出今天真在的那一处）。
    expect(walkCalls(files.find((f) => f.file === `${WALK_HOME}production-sources.ts`)!), "认法认不出 production-sources.ts 那一处").not.toEqual([]);
    const stray = files.filter((f) => !f.file.startsWith(WALK_HOME) && !f.file.startsWith("src/")).flatMap(walkCalls);
    expect(stray, "这几处在测试里自己读目录 —— 换成 `tests/test-support/production-sources.ts` 的 `filePaths` / `sourceFiles`").toEqual([]);
  });

  it("★ 磁盘语料上的裸 `.includes(\"…\")` == 登记（两向）", () => {
    const got: string[] = [];
    for (const f of tests) {
      const vars = corpusVars(f.text);
      if (vars.size === 0) continue;
      for (const m of f.text.matchAll(/\b(\w+)\.includes\(\s*("(?:[^"\\\n]|\\.)*")/g)) {
        if (vars.has(m[1])) got.push(`${f.file} :: ${m[1]}.includes(${m[2]}`);
      }
    }
    // 抽取器自检：测试里 `.includes("` 一个都没找到 ⇒ 下面的相等是空转的。
    const allIncludes = tests.reduce((acc, f) => acc + (f.text.match(/\.includes\(\s*"/g)?.length ?? 0), 0);
    expect(allIncludes, "整棵树的测试里一个 `.includes(\"` 都没找到 —— 抽取器坏了").toBeGreaterThan(0);
    expect(
      got.sort(),
      "磁盘语料上的裸 `.includes(\"…\")` 与 `BARE_INCLUDES` 对不上。\n" +
        "★ 匹配单位（子串）比事实（一整行 / 一个完整的词）小时，把事实撑大的改动会从缝里溜过去而判据照样绿。\n" +
        "多出来的：换成整行 / 有边界的比法（不许抄进名单让它绿）；少了的：修掉了，删那一行。",
    ).toEqual([...BARE_INCLUDES].sort());
  });
});
