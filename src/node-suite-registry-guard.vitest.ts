/**
 * U0（2026-08-01）：**给 16 个 tsx node 套件补上机检地板。**
 *
 * # 洞在哪
 *
 * `npm test`（`ci.yml:136` 前端 job 跑的就是它）把 16 个 `*.test.ts` 用 `&&` 串起来跑
 * （tsx，非 vitest）。它们各自是这个形状：
 *
 * ```
 * let failed = 0;
 * function test(name, fn) { try { fn(); } catch { failed++; console.error(`✗ …`); } }
 * …
 * if (failed > 0) { throw new Error(`… ${failed} failed`); }
 * ```
 *
 * 这个形状有**两条**都能导致「退出码 0 但什么都没验」的路：
 * ① **把测试删光** ⇒ `failed` 恒 0 ⇒ 静默绿。
 * ② **把收尾的 `if (failed > 0) … throw` 删掉** ⇒ 测试还在跑、还在打 `✗`，退出码照样 0。
 *
 * ②比①更重（表面上一切正常），而且**一行编辑就能重开**。Phase D 审计实测：
 * 删掉 `pricing.test.ts` 的收尾 + 让一条断言必然失败 ⇒ `npm run test:pricing` **RC=0**。
 * 15 套 e2e 早有 `assert-pass-floor.sh` 的运行期 PASS 数地板兜这一类，这 16 套 **242 条**一直没有。
 *
 * > 主计划把它记成「既无断言地板又被 `coverage.exclude` 排掉，双重不设防」——
 * > **「双重」那半不成立**：`coverage.exclude` 里的 `src/**\/*.test.ts` 排的是测试文件自身
 * > （标准做法），被测的生产代码仍在 `include` 里；且 `vitest.config.ts:21` 另有一条
 * > `src/**\/*.vitest.ts`，所以「放不放 `test-support/`」在覆盖率上没有差别。
 * > 真洞只有「无地板」这一条，本守卫只补这一条。
 *
 * # 判据（六条，互相咬）
 *
 * - **a 条数**：每个套件**行首** `test(` 数 == 登记数。删测试就红。
 * - **a2 总量**：**全仓** `src/**\/*.test.ts` 的行首 `test(` 总数 ≥ `TOTAL_FLOOR`。
 *   刻意**不从登记表求和** —— 那样它就成了 a 的副本。它挡的是 a 挡不住的两种：
 *   「删测试后顺手把登记数改小」（门禁一红最自然的那步）和「整套下线、登记与脚本一起删干净」。
 * - **b 集合**：登记集合 == `package.json` 里所有「跑 `.test.ts` 的 tsx 脚本」。
 *   **匹配 `tsx` 用的是词边界正则而不是 `startsWith("tsx ")`** —— Phase D 审计实测
 *   `"npx tsx …"` 能从 `startsWith` 版本里**静默逃逸**（加个不登记不进链的套件，守卫全绿）。
 * - **b2 路径**：登记的文件路径与 `package.json` 的命令逐字相符。
 * - **c 链路**：每个登记脚本都出现在 `npm test` 的链里，且链上不许有跑 `.test.ts` 却没登记的。
 *   **这条是关键** —— 套件文件还在、登记还在，但从链里被摘掉，a/b 都发现不了，
 *   而后果就是它再也不跑。另外**链的连接符必须是 `&&`**：换成 `;` 的话前面任何一套失败
 *   都会被最后一条的退出码盖掉，与「静默绿」同一类。
 * - **d 收尾**：每个套件都必须留着 `if (failed > 0)` + `throw` 的失败收尾。补上面那条②。
 *
 * # 它挡不住什么（诚实段；在一条专治安慰剂的守卫里，这段不完整本身就是缺陷）
 *
 * - **`test()` 还在但断言被掏空。** 运行期 PASS 计数同样挡不住（空 body 照样计一次 PASS），
 *   所以这不是选静态计数的损失。那一类归伪测试扫荡（R02）管。
 * - **非行首的 `test(` 不在计数里** —— 缩进的、`await test(`、循环里生成的，a 条看不见。
 *   Phase D 审计实测：给某套件加一条缩进两格的 `test(` ⇒ 守卫全绿。
 *   今天 16 个套件里**0 处**非行首写法（非行首命中全是 `function test(` 定义、
 *   `/re/.test(x)` 方法调用、模板串里的 `test(s) failed`），所以是**潜伏不是现患**。
 * - **不以 `test:` 开头的脚本名**（`unit:foo`）整个绕过 b。
 * - `d` 只看收尾**在不在**，不看它是否真的能被触达（比如被 `if (false)` 包住）。
 */
import { readdirSync, readFileSync } from "node:fs";
import { join, relative, resolve, sep } from "node:path";
import { describe, it, expect } from "vitest";
import { REPO_ROOT } from "./test-support/repo-root.ts";
import { stripComments } from "./test-support/strip-comments.ts";

/**
 * `(npm 脚本名, 文件, 行首 test() 条数)`。
 *
 * 数字**不要手打** —— 失败信息里带实测值，照着改。改之前先问：
 * 是真的删了那条测试，还是套件被掏空了？
 */
const NODE_SUITES: readonly (readonly [string, string, number])[] = [
  ["test:diff", "src/cards/diff.test.ts", 17],
  ["test:branching", "src/branching.test.ts", 23],
  ["test:api-error", "src/cards/api-error.test.ts", 5],
  ["test:bash", "src/cards/bash.test.ts", 20],
  ["test:remote-health", "src/remote-health.test.ts", 5],
  // F04b +1：`isValidNewTmuxName` 也禁 `=`（别创建一个主路杀不掉的名字）。
  ["test:remote-launch", "src/remote-launch.test.ts", 43],
  ["test:format", "src/format.test.ts", 10],
  ["test:history-cache", "src/views/history-cache.test.ts", 8],
  ["test:history-prefs", "src/views/history-prefs.test.ts", 18],
  ["test:history-actions", "src/views/history-actions.test.ts", 10],
  ["test:usage-pivot", "src/views/usage-pivot.test.ts", 14],
  ["test:pricing", "src/views/pricing.test.ts", 6],
  ["test:session-backend", "src/session-backend.test.ts", 9],
  ["test:panorama-session-files", "src/panorama/session-files.test.ts", 7],
  ["test:launch-dimensions", "src/launch-dimensions.test.ts", 28],
  ["test:launch-render-cli", "src/launch-render-cli.test.ts", 26],
];

/**
 * **全仓** `src/**\/*.test.ts` 的行首 `test(` 总数下限。实测基线 242（2026-08-01）。
 *
 * 与 `NODE_SUITES` 的登记数**无关**（不是求和）—— 它单独走一遍磁盘。
 * 这是它相对 a 条的全部价值：a 比的是「文件 vs 登记」，登记本身是可编辑的；
 * 这条比的是「磁盘 vs 一个常量」。
 */
const TOTAL_FLOOR = 244; // F13：242 → 244（棘轮往上拧 = 加强）

/** 判定一条 npm 命令是不是「用 tsx 跑某个 `.test.ts`」。`tsx …` 与 `npx tsx …` 都算。 */
const TSX_SUITE_CMD = /(^|\s)(npx\s+)?tsx\s+(--\S+\s+)*(\S+\.test\.ts)\s*$/;

function scripts(): Record<string, string> {
  const pkg = JSON.parse(readFileSync(resolve(REPO_ROOT, "package.json"), "utf8")) as {
    scripts: Record<string, string>;
  };
  return pkg.scripts;
}

/** 行首 `test(` 的条数。先剥注释，免得注释里写了 `test(` 也被数进去。 */
function lineStartTestCount(src: string): number {
  return (stripComments(src, "ts").match(/^test\(/gm) ?? []).length;
}

function readSuite(file: string): string {
  return readFileSync(resolve(REPO_ROOT, file), "utf8");
}

/** 全仓所有 `src/**\/*.test.ts`（仓库相对、`/` 分隔、已排序）。 */
function allTestTsFiles(): string[] {
  const root = resolve(REPO_ROOT, "src");
  const out: string[] = [];
  for (const e of readdirSync(root, { recursive: true, withFileTypes: true })) {
    if (!e.isFile() || !e.name.endsWith(".test.ts")) continue;
    out.push(relative(REPO_ROOT, join(e.parentPath, e.name)).split(sep).join("/"));
  }
  return out.sort();
}

describe("U0：tsx node 套件的机检地板", () => {
  it("a · 每个套件的行首 test() 条数与登记相符", () => {
    const drift = NODE_SUITES.flatMap(([script, file, want]) => {
      const got = lineStartTestCount(readSuite(file));
      return got === want ? [] : [`${script} (${file}): 登记 ${want}，实测 ${got}`];
    });
    expect(
      drift,
      `套件条数与登记表对不上：\n  ${drift.join("\n  ")}\n` +
        "先问「是真的删了测试，还是套件被掏空了」，再改登记表。",
    ).toEqual([]);
  });

  it("a2 · 全仓 *.test.ts 的行首 test() 总数不低于地板（独立于登记表）", () => {
    const files = allTestTsFiles();
    const total = files.reduce((n, f) => n + lineStartTestCount(readSuite(f)), 0);
    expect(
      total,
      `全仓 ${files.length} 个 *.test.ts 合计 ${total} 条 < 地板 ${TOTAL_FLOOR}。\n` +
        "**这条刻意不从登记表求和** —— 它挡的正是「门禁红了顺手把登记数改小」\n" +
        "和「整套下线、登记与脚本一起删干净」这两种 a/b/c 都发现不了的缩水。\n" +
        `扫到的文件：${files.join(", ")}`,
    ).toBeGreaterThanOrEqual(TOTAL_FLOOR);
  });

  it("b · 登记表覆盖 package.json 里全部 tsx 套件，不多不少", () => {
    const declared = Object.entries(scripts())
      .filter(([k, v]) => k.startsWith("test:") && TSX_SUITE_CMD.test(v))
      .map(([k]) => k)
      .sort();
    const registered = NODE_SUITES.map(([s]) => s).sort();
    expect(
      registered,
      "登记表与 package.json 的 tsx 套件集合不一致 —— 新加套件要登记，删套件要清登记。\n" +
        `package.json: ${declared.join(", ")}\n登记表: ${registered.join(", ")}`,
    ).toEqual(declared);
  });

  it("b2 · 登记的文件路径与 package.json 的命令逐字相符", () => {
    const sc = scripts();
    for (const [script, file] of NODE_SUITES) {
      expect(sc[script], `${script} 在 package.json 里不存在`).toBeTruthy();
      expect(sc[script].trim(), `${script} 的命令与登记的文件对不上（登记 ${file}）`).toBe(
        `tsx ${file}`,
      );
    }
  });

  it("c · 每个套件都真的挂在 `npm test` 链上，且链是 && 串的", () => {
    const chain = scripts()["test"];
    expect(chain, "package.json 里没有 `test` 脚本").toBeTruthy();
    expect(
      chain,
      "`npm test` 的链必须全用 `&&` 串：换成 `;` 或 `||` 之后，前面任何一套失败" +
        "都会被最后一条的退出码盖掉 —— 与「静默绿」同一类。",
    ).not.toMatch(/;|\|\|/);
    const missing = NODE_SUITES.map(([s]) => s).filter(
      // 用后随的分隔符钉住，免得 `test:history-cache` 被 `test:history-cache-extra` 误判成命中。
      (s) => !new RegExp(`npm run ${s}(\\s|$)`).test(chain),
    );
    expect(
      missing,
      `这些套件登记了、文件也在，但**没挂在 \`npm test\` 链上**⇒ 它们根本不跑：${missing.join(", ")}`,
    ).toEqual([]);
    // 反向：链上凡是「跑 .test.ts 的 tsx 脚本」都必须登记。
    // **按命令形态判，不按名字白名单** —— 白名单版本会让 `test:f40`（bash e2e）这类
    // 正当地挂进链时以「不在登记表里」误红，把人指向错误方向。
    const sc = scripts();
    for (const ref of chain.match(/npm run (test:[a-z0-9-]+)/g) ?? []) {
      const name = ref.replace("npm run ", "");
      if (!TSX_SUITE_CMD.test(sc[name] ?? "")) continue; // 非 tsx 套件（test:dom 等）不归本表管
      expect(
        NODE_SUITES.some(([s]) => s === name),
        `\`npm test\` 链上的 ${name} 跑的是 .test.ts 却不在登记表里`,
      ).toBe(true);
    }
  });

  it("d · 每个套件都留着 `if (failed > 0)` + throw 的失败收尾", () => {
    const broken = NODE_SUITES.flatMap(([script, file]) => {
      const src = stripComments(readSuite(file), "ts");
      const i = src.indexOf("if (failed > 0)");
      if (i < 0) return [`${script} (${file}): 找不到 \`if (failed > 0)\` 收尾`];
      // 收尾之后必须真的抛。窗口给宽些，容得下 `{ …console… throw … }` 的写法。
      return src.slice(i, i + 400).includes("throw")
        ? []
        : [`${script} (${file}): 有 \`if (failed > 0)\` 但其后 400 字符内没有 throw`];
    });
    expect(
      broken,
      "有套件丢了失败收尾：\n  " +
        broken.join("\n  ") +
        "\n没有它，测试照样跑、照样打 ✗，而**退出码是 0** —— `npm test` 全绿、CI 全绿。\n" +
        "这是本守卫要挡的第二条静默绿路（Phase D 审计实测复现过）。",
    ).toEqual([]);
  });
});

// ★★ **CI 里那两步的「有效性」压在两个 npm 脚本的内容上**
// 〔audit-0805 08-08，Phase G 第 67 件〕。
//
// `ci.yml` 有两步：`coverage floor (vitest jsdom)` → `npm run coverage`，
// 随后 `coverage per-file floors + zero-coverage ratchet` → 读覆盖率产物。
// `shared_crate_registry` 已经钉住「这两步在 CI 里还在」，
// 隔壁那条也钉住「每个套件都真的挂在 `npm test` 链上」。
//
// **但没人读 `coverage` 脚本本身**。08-08 实测：把它从 `vitest run --coverage`
// 改成 `vitest run`，**monitor 993 + vitest 1290 全绿** ——
// 而 `vitest.config.ts` 里那组 `thresholds` **只在 `--coverage` 时生效**，
// 下一步要读的覆盖率产物也不会生成。步骤名还在、CI 照旧绿，门槛整个不再执行。
//
// ⇒ 与 F58/F61 同族（散文/步骤名指着一件其实没在发生的事），这次载体是 npm 脚本。
describe("覆盖率那两步的有效性", () => {
  it("`npm run coverage` 必须真的带 --coverage", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve, dirname } = await import("node:path");
    const { fileURLToPath } = await import("node:url");
    const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
    const pkg = JSON.parse(readFileSync(resolve(ROOT, "package.json"), "utf8"));
    const script = pkg.scripts?.coverage;
    expect(script, "`package.json` 里没有 `coverage` 脚本 —— 抽取坏了或它被删了").toBeTypeOf(
      "string",
    );
    expect(
      script,
      `\`coverage\` 脚本是 ${JSON.stringify(script)}，没带 --coverage。\n` +
        "⚠ `vitest.config.ts` 里那组 thresholds **只在 --coverage 时生效**，\n" +
        "而 CI 下一步 `assert-coverage-floors.mjs` 要读的覆盖率产物也不会生成。\n" +
        "结果是：CI 那两步照跑、照绿，而覆盖率门槛整个不再执行。",
    ).toContain("--coverage");
  });

  it("阈值确实写在 vitest 配置里（否则上面那条守的是一件不存在的事）", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve, dirname } = await import("node:path");
    const { fileURLToPath } = await import("node:url");
    const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
    const cfg = readFileSync(resolve(ROOT, "vitest.config.ts"), "utf8");
    // ⚠ 要认的是**配置键**，不是「这个词出现过」：同一文件的注释里就写着
    //   「设地板阈值（下方 thresholds）」，只查词的话把 key 改名照样绿 ——
    //   变异当场证伪（F24 那一族，本会话第 N 次）。
    const keyed = cfg.split("\n").some((l) => /^\s*thresholds\s*:/.test(l));
    expect(
      keyed,
      "`vitest.config.ts` 里找不到 `thresholds:` 这个**配置键**（注释里提到不算）—— \n" +
        "那么 --coverage 也不会让谁红，上面那条就是在守一件不存在的事（本条此刻无效）。",
    ).toBe(true);
  });
});

// ★★ **覆盖率地板的「只许降」纪律本身是散文**〔audit-0805 08-08，Phase G 第 68 件〕。
//
// `scripts/assert-coverage-floors.mjs` 是一条递减棘轮：逐文件地板 + 0% 文件数上限，
// 头注逐字写着「**只许降**」「地板设在**当前值下方 ~5 点**」「实测值一起写下，
// 只改数字不写实测，下一个人看不出它过期没过期」。
//
// 那三句撑着整条棘轮，而**没人读它们**。08-08 实测：把 `src/tabs.ts` 的地板从
// `64/54` 调到 `10/5`（实测那两列原样不动），**monitor 993 + vitest 1292 全绿** ——
// 棘轮从此形同虚设，而它自己的诊断照旧说「地板通过」。
//
// ⇒ 钉的是**表自身的自洽**：既然每行都带着「写下时实测%」，就要求地板不许离它太远。
// 这不需要跑覆盖率，纯读表 —— 与那条真的跑覆盖率的门禁互补（一条在 CI 里跑、
// 一条在单测里读，失效模式不同）。
describe("覆盖率地板表的自洽", () => {
  it("每行地板都在「写下时实测」下方 ~5 点以内，且不高于实测", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve, dirname } = await import("node:path");
    const { fileURLToPath } = await import("node:url");
    const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
    const src = readFileSync(resolve(ROOT, "scripts/assert-coverage-floors.mjs"), "utf8");
    // 人群 = 表里每一行 `["<路径>", 语句地板, 实测, 分支地板, 实测],`
    const rows = [...src.matchAll(/\[\s*"([^"]+)"\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*\]/g)];
    expect(
      rows.length,
      "从 `PER_FILE_FLOORS` 抽不到足够的行（08-08 实测 6 行）—— 抽取坏了，本条此刻无效",
    ).toBeGreaterThanOrEqual(4);
    const SLACK = 8; // 纪律写的是 ~5 点，留一点余量给 v8 版本差
    for (const [, file, sFloor, sNow, bFloor, bNow] of rows) {
      const [sf, sn, bf, bn] = [sFloor, sNow, bFloor, bNow].map(Number);
      expect(
        sf,
        `${file}: 语句地板 ${sf} 高于写下时实测 ${sn} —— 那不是地板，是天花板`,
      ).toBeLessThanOrEqual(sn);
      expect(
        sn - sf,
        `${file}: 语句地板 ${sf} 离实测 ${sn} 差了 ${(sn - sf).toFixed(1)} 点（纪律是 ~5）。\n` +
          "⚠ 头注逐字写着「只许降」——余量一放大，棘轮就形同虚设而诊断照旧说「通过」。\n" +
          "真要放宽：先说清为什么，并把实测那一列一起更新（不然下一个人看不出它过期没过期）。",
      ).toBeLessThanOrEqual(SLACK);
      expect(bf, `${file}: 分支地板 ${bf} 高于写下时实测 ${bn}`).toBeLessThanOrEqual(bn);
      expect(
        bn - bf,
        `${file}: 分支地板 ${bf} 离实测 ${bn} 差了 ${(bn - bf).toFixed(1)} 点（纪律是 ~5）`,
      ).toBeLessThanOrEqual(SLACK);
    }
  });
});
