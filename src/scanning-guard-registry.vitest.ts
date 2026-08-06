/**
 * **TS 侧的扫描型判据卫生**〔audit-0805 F23/F24 的 TS 那半，两者都登记为「未做」〕。
 *
 * Rust 侧有两条棘轮：`scanning_guard_registry`（判据不许读到自己）与
 * `needle_anchor_registry`（匹配单位不许比事实小）。两者的头注都逐字写着
 * 「**TS 侧未做**」。本文件是那半。
 *
 * # 先说清它**不是**什么
 *
 * 它**不是**在修 bug。B′ 逐条量过今天的 TS 侧：
 *
 * - **9 个遍历者全都摘掉了自己** —— 靠 `!p.endsWith(".vitest.ts")` 这类过滤
 *   （TS 版的「剥生产段」：判据自己就住在 `.vitest.ts` 里，扫生产 `.ts` 就读不到自己），
 *   或者干脆扫别的扩展名（`.rs` / `.test.ts`）。
 * - **磁盘语料上的裸 `.includes("…")` 只有 8 处**，逐条看过都是存在性断言/抽取器自检
 *   （判准见 Rust 侧 `needle_anchor_registry` 头注那张表：存在性不危险，正向事实钉才危险）。
 *
 * ⇒ 本文件的价值**全在挡新增**。这一点是 F23/F24 第二刀刚学到的：
 * 棘轮那个数**不等于「还欠多少 bug」**，它只是「今天有多少这种写法」。
 * 把它写在这里，免得下一个人看到 `9` 和 `8` 以为这里有 17 个待修项。
 *
 * # 判准（与 Rust 侧同一套词汇，别造第二套）
 *
 * | 问题 | 安全的答法 |
 * |---|---|
 * | 遍历者靠什么读不到自己？ | 扫的树不含自己（排掉 `.vitest.ts`/`.test.ts`，或扫别的扩展名） |
 * | 匹配单位够不够大？ | **存在性断言**够（只需要「有」）；**正向事实钉**不够（声称「就是这个」，撑大后照样绿） |
 */
import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, sep } from "node:path";
import { fileURLToPath } from "node:url";

const SRC = "src";

/**
 * ★ **本文件自己那一份要摘出去** —— 它就是自己这条规矩的第一个违反者。
 *
 * 下面 `WALK_FORMS` 里逐字写着 `readdirSync`，而本文件也真的在遍历目录 ⇒
 * 不摘的话它会把自己算进「遍历者」，而且**是靠自己的登记表字面量算进去的**
 * （F23 那一族：判据在自己的表里找到自己）。写这条时它当场咬了我一口：第一次跑
 * 得到 10 而不是 9。
 *
 * ⚠ 用 `import.meta.url` 推自己的路径，**不写死文件名** —— 写死的摘除**改名即静默失效**，
 * 而失效之后看起来和没失效一模一样（F23 第二刀刚在 `parity_ledger` 上处置过同一形态）。
 */
const SELF = fileURLToPath(import.meta.url)
  .split(sep)
  .join("/")
  .replace(/^.*\/cc-monitor\//, "");

/** 走一遍 `src/`，返回相对仓根的 `.ts` 路径。 */
function allTs(dir: string = SRC, out: string[] = []): string[] {
  for (const e of readdirSync(dir)) {
    const p = join(dir, e);
    if (statSync(p).isDirectory()) allTs(p, out);
    else if (p.endsWith(".ts")) out.push(p.split(sep).join("/"));
  }
  return out;
}

const IS_TEST = (f: string): boolean => f.endsWith(".vitest.ts") || f.endsWith(".test.ts");

/** 目录遍历的形态（与 Rust 侧 `RAW_WALKS` 同一套词汇）。 */
const WALK_FORMS = ["readdirSync", "globSync", "readdir("];

/**
 * ★ **今天在测试里做目录遍历的文件**（08-06 实测 9 个）。
 *
 * 只许降 —— 新增一个就红，那时要么让它摘掉自己、要么把它加进来并写明凭什么安全。
 */
const WALKER_CEILING = 9;

/**
 * ★ **磁盘语料上的裸 `.includes("…")`**（08-06 实测 8 处）。
 *
 * 只许降。判准同 Rust 侧：存在性断言不危险，正向事实钉才危险。
 */
const BARE_INCLUDES_CEILING = 8;

/** 一个 `const`/`let` 绑定的名字与右侧（右侧只取本行）。 */
function letBinding(line: string): [string, string] | null {
  const m = /\b(?:const|let)\s+(\w+)\s*(?::[^=]+)?=\s*(.*)/.exec(line);
  return m ? [m[1], m[2]] : null;
}

/**
 * RHS 是不是**从 `v` 这份文本直接切/借出来的**。
 *
 * ⚠ 用「以它开头」而不是「包含它」—— 后者会让传递闭包跑飞
 * （Rust 侧实测：一个文件 0 → 166 个语料变量，计数从 63 虚涨到 71）。
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

describe("TS 侧扫描型判据的卫生（F23/F24 的 TS 那半）", () => {
  const files = allTs();

  it("抽取器自检：真的扫到了 TS 源码树", () => {
    expect(
      files.length,
      `只扫到 ${files.length} 个 .ts（08-06 实测 180+）—— 遍历坏了，下面两条都会零命中地绿`,
    ).toBeGreaterThan(120);
    expect(
      files.filter(IS_TEST).length,
      "一个测试文件都没扫到 —— 下面两条只看测试文件，那样它们什么也没量",
    ).toBeGreaterThan(50);
  });

  it("对照组：摘除不是死规则 —— 不摘的话本文件会把自己算进去", () => {
    expect(
      files.includes(SELF),
      `\`SELF\`（${SELF}）不在扫到的清单里 —— 路径推错了，那条摘除就成了死规则，` +
        "而死规则和「摘对了」看起来一模一样",
    ).toBe(true);
    expect(
      WALK_FORMS.some((w) => readFileSync(SELF, "utf8").includes(w)),
      "本文件不再匹配任何遍历形态 —— 那上面那条摘除已经没有意义，删掉它并把这条一起删",
    ).toBe(true);
  });

  it("★ 测试里做目录遍历的文件只许变少", () => {
    const walkers = files
      .filter(IS_TEST)
      .filter((f) => f !== SELF)
      .filter((f) => WALK_FORMS.some((w) => readFileSync(f, "utf8").includes(w)));
    expect(
      walkers.length,
      `测试里做目录遍历的文件有 ${walkers.length} 个 > 上限 ${WALKER_CEILING}（08-06 实测 9）。\n` +
        "★ 新增的那个**必须能说清它靠什么读不到自己**：扫的树排掉 `.vitest.ts`/`.test.ts`\n" +
        "（TS 版的「剥生产段」），或者干脆扫别的扩展名。\n" +
        "判据在自己的登记表/注释里找到自己 ⇒ **恒绿**，而恒绿看起来和真绿一模一样。\n" +
        `当前清单：\n${walkers.map((w) => `  ${w}`).join("\n")}`,
    ).toBeLessThanOrEqual(WALKER_CEILING);
  });

  it("★ 磁盘语料上的裸 `.includes(\"…\")` 只许变少", () => {
    let total = 0;
    const byFile: string[] = [];
    for (const f of files.filter(IS_TEST)) {
      const src = readFileSync(f, "utf8");
      const vars = corpusVars(src);
      if (vars.size === 0) continue;
      let n = 0;
      for (const m of src.matchAll(/\b(\w+)\.includes\(\s*"/g)) {
        if (vars.has(m[1])) n++;
      }
      if (n > 0) {
        total += n;
        byFile.push(`  ${n}  ${f}`);
      }
    }
    // 抽取器自检：与被棘轮的那个数**无关**的一个量 —— 测试里 `.includes("` 的总数。
    const allIncludes = files
      .filter(IS_TEST)
      .reduce((acc, f) => acc + (readFileSync(f, "utf8").match(/\.includes\(\s*"/g)?.length ?? 0), 0);
    expect(
      allIncludes,
      `整棵树的测试里只找到 ${allIncludes} 个 \`.includes("\`（08-06 实测 100+）—— 抽取器坏了`,
    ).toBeGreaterThan(60);

    expect(
      total,
      `磁盘语料上的裸 \`.includes("…")\` 有 ${total} 处 > 上限 ${BARE_INCLUDES_CEILING}（08-06 实测 8）。\n` +
        "★ 匹配单位（子串）比事实（一整行 / 一个完整的词）小时，把事实撑大的改动会从缝里\n" +
        "溜过去而判据照样绿。**存在性断言不危险**（只需要「有」）；**正向事实钉危险**。\n" +
        "⚠ 不许把上限调上去让今天好过 —— 这是递减棘轮。\n" +
        `当前分布：\n${byFile.join("\n")}`,
    ).toBeLessThanOrEqual(BARE_INCLUDES_CEILING);
  });
});
