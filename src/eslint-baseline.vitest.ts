/**
 * V7-3〔audit-0805 · 2026-08-09 `/full-audit`〕：**eslint 的基线数与它的作用面必须有人数着**。
 *
 * ## 这条为什么存在（病史，别当背景故事读）
 *
 * E83（07-31 `a02f340`）把 `npm run lint` 从 `eslint src` 放开到 `eslint .`，当场实测
 * 「全仓 7 个，与 `eslint src` 的基线一致」，并把这句话**同时写进** `eslint.config.js`
 * 与 `.github/workflows/ci.yml`。六天后（08-06 `cab8a75`）新建的
 * `scripts/assert-coverage-floors.mjs` 一进来就带 7 条 `no-undef`（`console`/`process`，
 * 纯缺一段 globals）⇒ 基线**从 7 静默变成 14**，而那两句散文一句都没人回来改。
 * `/full-audit` 是**第一次有人真跑 `npx eslint .`**，才把这个数量出来。
 *
 * ⇒ 病根不在「谁忘了改注释」，在于 **eslint 这两个基线数是全仓仅有的没被机检的**：
 * shellcheck 的文件数被 `shell_lint_registry` 钉成**等号**、e2e 套数被 `e2e_gate_registry`
 * 四份副本对拍、覆盖率有地板与递减棘轮 —— 唯独 eslint/stylelint 靠散文。
 * 而 `ci.yml` 那步是 `npm run lint || true`（已登记为**结构上不会红**）⇒ CI 也接不住。
 *
 * ## 两条腿，各堵一半（**刻意不合成一条**）
 *
 * ① **数**：真跑一次 `eslint .` 数错误总数，钉成等号。它接的是「值漂了」。
 * ② **面**：从 `git ls-files` 的**全集**派生人群 —— 每个被 lint 到的 `.mjs` 所在目录
 *    都必须在 `eslint.config.js` 里有一个 `files:` 块认领它。它接的是「**新目录进来时没人管**」，
 *    也就是本条真正的复发机制：`ignores` 是仓级的（全集），而 globals 是**按目录枚举**的。
 *
 * ⚠ ② 的人群来自 `git ls-files` 而不是来自配置里已经写着的那些目录 ——
 * 这是本区反复吃亏的那一族（「按已经有名字的那批取样」）的**对治**，
 * 也是 `/full-audit` 三路独立收敛的那句话在本条上的落地：
 * 判据的人群要从**文件系统全集**来，不从「配置里已经承认的那批」来。
 *
 * ## 诚实边界
 *
 * - 本条**不管 stylelint**（`ci.yml` 那个 50 同样没被机检）。理由是 stylelint 的作用面
 *   逐字写死在 `package.json` 的 `stylelint "src/**\/*.css"`，没有 ② 那个「新目录悄悄进来」
 *   的机制 ⇒ 只缺 ① 那半。**登记在此，不假装覆盖了。**
 * - ① 跑的是**真 eslint**，约数秒。这是本仓少数几条 spawn 外部进程的判据之一
 *   （先例：`node-suite-registry-guard.vitest.ts`）。慢的代价换的是「散文数字第一次有东西读它」。
 */
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, it, expect } from "vitest";
import { REPO_ROOT } from "./test-support/repo-root.ts";

/**
 * 全仓 `npx eslint .` 的既有错误数。**基线/顾问式**（同 clippy 不强制）——
 * 这个数字的意义不是「零告警」，是「**它变了必须有人知道**」。
 *
 * ⚠ 改这个数之前先问：是修好了一条（往下调，欢迎），还是**又有一批没被 globals 认领的文件
 * 溜进了作用面**（那是 V7-3 的复发，去看第二条判据说了什么）。
 */
const ESLINT_ERROR_BASELINE = 7;

/** `eslint.config.js` 与 `ci.yml` 里那两句散文声称的数 —— 它们必须与上面这个常量同一个值。 */
const PROSE_CLAIM = /全仓(?:实测仍是)?\s*\*{0,2}(\d+)\s*(?:个|项)/g;

type EslintJsonResult = { filePath: string; errorCount: number; warningCount: number };

/**
 * 跑一次真 eslint，返回它的 JSON 报告。非零退出是常态（有既有告警），不能当失败。
 *
 * ⚠ 结果**在本文件内缓存一次** —— 全仓 eslint 约 10 秒，两条判据各跑一次就是 20 秒。
 * 缓存不影响判据强度：同一次进程内源码不会变。
 * ⚠ 每个 `it` 都显式带 `TIMEOUT_MS`：vitest 默认 5 秒，**首版就是因为这个红的**
 * （红在超时上，不是红在被测性质上 —— 那种红没有信息量，必须消掉）。
 */
let cached: EslintJsonResult[] | null = null;
const TIMEOUT_MS = 120_000;

function runEslintCached(): EslintJsonResult[] {
  cached ??= runEslint();
  return cached;
}

function runEslint(): EslintJsonResult[] {
  let out: string;
  try {
    out = execFileSync("npx", ["eslint", ".", "-f", "json"], {
      cwd: REPO_ROOT,
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
    });
  } catch (e) {
    // eslint 有发现时以非零退出，stdout 仍是完整 JSON。
    const err = e as { stdout?: string };
    if (!err.stdout) throw e;
    out = err.stdout;
  }
  return JSON.parse(out) as EslintJsonResult[];
}

function read(file: string): string {
  return readFileSync(resolve(REPO_ROOT, file), "utf8");
}

describe("V7-3：eslint 基线与作用面", () => {
  it("① 全仓错误数就是基线那个数（散文声称的那个）", () => {
    const results = runEslintCached();

    // 抽取器自检：eslint 必须真的扫到了东西。**没有这一格，删光 `files:` 块本条会零命中地绿。**
    expect(
      results.length,
      "eslint 一个文件都没扫到 —— 要么 `ignores` 被改成吃掉全仓，要么本条跑歪了（会零命中地绿）",
    ).toBeGreaterThan(100);

    const errors = results.reduce((n, r) => n + r.errorCount, 0);
    const offenders = results
      .filter((r) => r.errorCount > 0)
      .map((r) => `${r.filePath.replace(`${REPO_ROOT}/`, "")}: ${r.errorCount}`)
      .sort();

    expect(
      errors,
      `eslint 全仓错误数变了：实测 ${errors}，基线 ${ESLINT_ERROR_BASELINE}。\n` +
        `逐文件：\n  ${offenders.join("\n  ")}\n` +
        `⚠ 变大时先看第二条判据 —— V7-3 那次就是新目录的 .mjs 没被任何 globals 块认领，\n` +
        `  7 条 no-undef 一次性涌进来，而两处散文写着 7 一直没人改。`,
    ).toBe(ESLINT_ERROR_BASELINE);
  }, TIMEOUT_MS);

  it("① b 两处散文声称的数与基线常量一致（E12：散文要有一条会红的判据读它）", () => {
    for (const file of ["eslint.config.js", ".github/workflows/ci.yml"]) {
      const src = read(file);
      const claims = [...src.matchAll(PROSE_CLAIM)].map((m) => Number(m[1]));
      // 抽取器自检：那句话还在不在。措辞改了就该在这里红，而不是静静地零命中。
      expect(
        claims.length,
        `${file} 里找不到「全仓 N 个/项」那句散文 —— 措辞改了？改了就把本条的正则一起改，\n` +
          `别让它零命中地绿（这正是 V7-3 那次腐坏能活六天的机制）`,
      ).toBeGreaterThan(0);
      for (const n of claims) {
        expect(
          n,
          `${file} 声称全仓 ${n} 个，而基线常量是 ${ESLINT_ERROR_BASELINE} —— 两处副本又漂了`,
        ).toBe(ESLINT_ERROR_BASELINE);
      }
    }
  }, TIMEOUT_MS);

  it("② 每个被 lint 到的 .mjs 目录都有 files: 块认领（人群从 git ls-files 全集派生）", () => {
    // 人群 = 版本控制里的全部 .mjs，**不是**配置里已经写着的那几个目录。
    const tracked = execFileSync("git", ["ls-files", "*.mjs"], {
      cwd: REPO_ROOT,
      encoding: "utf8",
    })
      .split("\n")
      .filter(Boolean);

    // 抽取器自检：全集不许是空的。
    expect(tracked.length, "git ls-files '*.mjs' 零命中 —— 抽取器坏了，本条会零命中地绿").toBeGreaterThan(0);

    const cfg = read("eslint.config.js");
    // eslint 实际扫到了哪些 .mjs（`ignores` 已经生效过一遍）—— 只对这批要求认领。
    const linted = new Set(
      runEslintCached()
        .map((r) => r.filePath.replace(`${REPO_ROOT}/`, ""))
        .filter((p) => p.endsWith(".mjs")),
    );

    const unclaimed = [...linted].filter((p) => {
      const topDir = p.split("/")[0];
      // 认领 = 配置里存在一个覆盖该顶层目录 .mjs 的 `files:` 模式。
      return !new RegExp(`"${topDir}/\\*\\*/\\*\\.mjs"`).test(cfg);
    });

    expect(
      unclaimed,
      `这些 .mjs 在 eslint 的作用面里，但 eslint.config.js 没有任何 files: 块给它们配 globals：\n` +
        `  ${unclaimed.join("\n  ")}\n` +
        `⇒ 它们会整批报 no-undef（console/process），把基线数顶上去 —— V7-3 的复发形态。\n` +
        `修法：照 e2e/scripts 那两块的样子加一段 { files: ["<目录>/**/*.mjs"], languageOptions: { globals } }。`,
    ).toEqual([]);
  }, TIMEOUT_MS);
});
