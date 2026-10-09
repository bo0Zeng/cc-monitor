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
import { join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

// 〔src/test 分离〕本判据的**人群是"测试文件"**，而它们已经整体搬到 `tests/`；
// 同时下面 `files.length > 120` 那条自检量的是"整棵 TS 树扫到了东西"。
// ⇒ 两棵树都要走。分开列而不是写一个通配，是为了让"漏掉一棵"在这里肉眼可见。
const TREES = ["src", "tests"] as const;

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
 *
 * ⚠⚠ **同一个坑，高一层**〔08-14 实测〕：上面那句做到了「不写死**文件**名」，
 * 却写死了**目录**名 —— 原式是 `.replace(/^.*\/cc-monitor\//, "")`。
 * 于是在**任何一个 git worktree 里**（目录叫 `wt-backend-split` 之类），那个 `replace`
 * 不命中，`SELF` 停在绝对路径上、摘不掉自己 ⇒ 本文件把自己算进语料，两格当场红。
 * 多 agent 并发用 worktree 干活时 `npm test` **必红**，而红的原因与被测的性质无关。
 *
 * ⇒ 改成相对 `process.cwd()`（vitest 把 cwd 设在包根，与 `allTs` 从 `TREES` 起走同一个基准）。
 * ★ 它**不算白错**：下面那条 `files.includes(SELF)` 自检**当场把它逮住了** ——
 * 判据自己写着「路径推错了，那条摘除就成了死规则」，这次正是它兑现的那一刻。
 */
const SELF = relative(process.cwd(), fileURLToPath(import.meta.url)).split(sep).join("/");

/** 走一遍 `src/` 与 `tests/`，返回相对仓根的 `.ts` 路径。 */
function allTs(dir?: string, out: string[] = []): string[] {
  if (dir === undefined) {
    for (const t of TREES) allTs(t, out);
    return out;
  }
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
 * ★ **今天在测试里做目录遍历的文件**（两向相等：新增一个就红，那时要么让它摘掉自己、要么把它加进来并写明凭什么安全；
 * 不再遍历了也红，删那一行）。每一个都得说得清它靠什么读不到自己。
 */
const WALKERS: Record<string, string> = {
  "tests/frontend/ui/generated-boundary-guard.vitest.ts": "扫的是 Rust 源码（`.rs`），读不到自己",
  "tests/frontend/ui/import-cycle-guard.vitest.ts": "扫的是 `src/` 里的产品 TS，自己住 `tests/`",
  "tests/frontend/ui/ipc/commands.vitest.ts": "扫的是 `src/` 里的产品 TS，自己住 `tests/`",
  "tests/frontend/ui/node-suite-registry-guard.vitest.ts": "只列文件名对登记，不在文本里找字",
  "tests/frontend/ui/said-object-registry.vitest.ts": "扫的是 `src/frontend/ui` 里的产品 TS，自己住 `tests/`",
  "tests/frontend/ui/paste-block-guard.vitest.ts": "扫的是 `src/` 里的产品 TS，自己住 `tests/`",
  "tests/frontend/ui/scale3-one-screen-gate.vitest.ts": "遍历的是 `src/frontend/ui/cards/`（从卡片源码派生 `card-*` 类名），自己住 `tests/`",
  "tests/frontend/ui/settings/base-wording-guard.vitest.ts": "扫的是 `src/` 里的产品 TS，自己住 `tests/`",
  "tests/shots/browser-env.vitest.ts": "只扫 `tests/shots/` 下的 `.mjs` 脚本（起浏览器的那几份），自己是 `.vitest.ts`",
};

/**
 * ★ **磁盘语料上的裸 `.includes("…")`**（两向相等：`文件 :: 接收者.includes("字面量"`，同一处可重复）。
 * 存在性断言不危险，正向事实钉才危险；新写一处就红（换成整行 / 有边界的比法），修掉一处也红（删那一行）。
 */
const BARE_INCLUDES: readonly string[] = [
  'tests/frontend/ui/generated-boundary-guard.vitest.ts :: attrs.includes("ts_rs::TS"',
  'tests/frontend/ui/generated-boundary-guard.vitest.ts :: attrs.includes("ts_rs::TS"',
  'tests/frontend/ui/generated-boundary-guard.vitest.ts :: own.includes("skip_serializing_if"',
  'tests/frontend/ui/gray-light-wiring.vitest.ts :: viewer.includes("markTmuxIdle"',
  'tests/frontend/ui/gray-light-wiring.vitest.ts :: viewer.includes("follow: {"',
  'tests/frontend/ui/paste-block-guard.vitest.ts :: code.includes("writeText"',
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
    // 正控按名字点：本文件自己（一份测试）与主窗口入口（一份生产）都得在人群里。
    expect(
      files.some((f) => f.endsWith("scanning-guard-registry.vitest.ts") && IS_TEST(f)),
      "本文件不在扫描面里（或没被认成测试）—— 下面两条只看测试文件，那样它们什么也没量",
    ).toBe(true);
    expect(
      files.some((f) => f.endsWith("frontend/ui/main.ts")),
      `扫描面里没有 \`main.ts\`（扫到 ${files.length} 份）—— 遍历坏了`,
    ).toBe(true);
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

  it("★ 测试里做目录遍历的文件 == 登记（两向）", () => {
    const walkers = files
      .filter(IS_TEST)
      .filter((f) => f !== SELF)
      .filter((f) => WALK_FORMS.some((w) => readFileSync(f, "utf8").includes(w)))
      .sort();
    expect(
      walkers,
      "测试里做目录遍历的文件与 `WALKERS` 对不上。\n" +
        "★ 新增的那个**必须能说清它靠什么读不到自己**：扫的树排掉 `.vitest.ts`/`.test.ts`（TS 版的「剥生产段」），\n" +
        "或者干脆扫别的扩展名 / 别的目录 —— 写进 `WALKERS` 那一行的理由里。判据在自己的登记表 / 注释里找到自己 ⇒ **恒绿**。\n" +
        "不再遍历了 ⇒ 删那一行。",
    ).toEqual(Object.keys(WALKERS).sort());
  });

  it("★ 磁盘语料上的裸 `.includes(\"…\")` == 登记（两向）", () => {
    const got: string[] = [];
    for (const f of files.filter(IS_TEST)) {
      const src = readFileSync(f, "utf8");
      const vars = corpusVars(src);
      if (vars.size === 0) continue;
      for (const m of src.matchAll(/\b(\w+)\.includes\(\s*("(?:[^"\\\n]|\\.)*")/g)) {
        if (vars.has(m[1])) got.push(`${f} :: ${m[1]}.includes(${m[2]}`);
      }
    }
    // 抽取器自检：测试里 `.includes("` 一个都没找到 ⇒ 下面的相等是空转的。
    const allIncludes = files
      .filter(IS_TEST)
      .reduce((acc, f) => acc + (readFileSync(f, "utf8").match(/\.includes\(\s*"/g)?.length ?? 0), 0);
    expect(allIncludes, "整棵树的测试里一个 `.includes(\"` 都没找到 —— 抽取器坏了").toBeGreaterThan(0);
    expect(
      got.sort(),
      "磁盘语料上的裸 `.includes(\"…\")` 与 `BARE_INCLUDES` 对不上。\n" +
        "★ 匹配单位（子串）比事实（一整行 / 一个完整的词）小时，把事实撑大的改动会从缝里溜过去而判据照样绿。\n" +
        "多出来的：换成整行 / 有边界的比法（不许抄进名单让它绿）；少了的：修掉了，删那一行。",
    ).toEqual([...BARE_INCLUDES].sort());
  });
});
