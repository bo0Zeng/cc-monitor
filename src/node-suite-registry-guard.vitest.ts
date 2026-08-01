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
  ["test:branching", "src/branching.test.ts", 19],
  ["test:api-error", "src/cards/api-error.test.ts", 5],
  ["test:bash", "src/cards/bash.test.ts", 20],
  ["test:remote-health", "src/remote-health.test.ts", 5],
  ["test:remote-launch", "src/remote-launch.test.ts", 40],
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
const TOTAL_FLOOR = 242;

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
