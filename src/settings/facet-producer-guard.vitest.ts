/**
 * Phase G 守卫：**`MACHINE_FACETS` 里的每一格都必须有人写**。
 *
 * # 它治的是一个真实发生过的洞
 *
 * S3 定了 5 格状态（`connection`/`daemon`/`ccm`/`acctIso`/`accounts`），S5/E56 拿它算
 * 「还差什么」。但收官核账时发现：全仓 `recordFacet` 的生产者**只覆盖 3 格**
 * —— `acctIso` 与 `accounts` 一个写点都没有。
 *
 * 后果不是「少两个格子」这么轻。`computeGaps` 把「账本里没这一格」判成 `unknown`，
 * 于是**每台机器恒定产出 ≥2 条**，`summarizeGaps` 恒非 null ⇒
 * `remote-section` 里「全绿就整块不出现」那一支成了**死代码**。
 * 用户拿到的是一张自称「还差什么、点哪里补齐」却**既补不齐也消不掉**的清单
 * —— 比不做这个功能更糟，因为它还占着落地页最上面那块地方。
 *
 * # 为什么用源码扫描，而不是运行时断言
 *
 * 「有没有人写过这一格」是**代码库的性质**，不是某次运行的性质：跑测试时没触发某个
 * 按钮，不等于那个按钮不存在。要钉的正是「加了第 6 格却忘了接线」这个**编辑期**失误
 * —— 与 S1 那个 `MissingField` 编译期穷尽检查是同一招，只是这一格没法用类型表达
 * （写点散在若干个 UI handler 里）。
 *
 * # 判据怎么定的（两个坑都绕开了）
 *
 * - **剥注释**：直接 grep 会被注释里提到的格名喂饱 —— 这个形状在本仓栽过四次
 *   （见 `test-support/strip-comments.ts` 头注）。所以先剥。
 * - **不只看格名，要看它出现在写入位置**：`readiness.ts` 里到处是格名，但它是**消费者**。
 *   所以判据是「格名出现在 `recordFacet(...)` 的实参里，或出现在 `facet:` 键上」
 *   （后者是 `machine-card.ts` 的 `runRemoteAction({ ledger: { facet, ok, fail } })` 那条间接路径）。
 */
import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { stripComments } from "../test-support/strip-comments";
import { MACHINE_FACETS } from "./machine-status";

const SETTINGS_DIR = dirname(fileURLToPath(import.meta.url));

/** `settings/` 下的生产源码（排掉测试与词汇表/消费者自身）。 */
function productionSources(): { file: string; code: string }[] {
  const skip = new Set([
    "machine-status.ts", // 词汇表本身
    "readiness.ts", // 消费者
  ]);
  return readdirSync(SETTINGS_DIR)
    .filter(
      (f) =>
        f.endsWith(".ts") &&
        !f.endsWith(".vitest.ts") &&
        !f.endsWith(".test.ts") &&
        !skip.has(f),
    )
    .map((f) => ({
      file: f,
      code: stripComments(readFileSync(resolve(SETTINGS_DIR, f), "utf8"), "ts"),
    }));
}

/**
 * 某一格在**写入位置**出现过的文件名清单。
 *
 * 两条写入形状：
 * - 直接：`recordFacet("ccm", …)` / `this.recordFacet("ccm", …)` / `this.note("accounts", …)`
 * - 间接：`{ facet: "daemon", ok: …, fail: … }`（machine-card 的 `runRemoteAction` 台账参数）
 *
 * ## 这里踩过一次，判据是被变异逼紧的
 *
 * 第一版 `viaLedger` 写的是 `facet\s*:\s*"X"`，**被类型标注喂饱了** —— 我自己给
 * `accounts-section.note()` 写的签名是 `facet: "acctIso" | "accounts"`，那串正好命中。
 * 变异实测（把 `acctIso` 的写点全删）**仍然绿**，是这条自检把它揪出来的：
 * *「类型里提到某格」不是「有人写过某格」*，同「注释里提到」一样不算数。
 *
 * ⇒ 现在要求它长得像**对象字面量里的一对键值**：`facet: "X",` 后面还得跟另一个键
 * （`ok:` / `fail:`）。类型标注（`"X" | "Y"`）与参数列表都不满足这个形状。
 */
function producersOf(facet: string): string[] {
  const lit = `["']${facet}["']`;
  const direct = new RegExp(`(?:recordFacet|note)\\s*\\(\\s*${lit}`);
  // `facet: "daemon", ok: …` —— 逗号后必须还有一个对象键，把类型标注排除在外
  const viaLedger = new RegExp(`facet\\s*:\\s*${lit}\\s*,\\s*\\w+\\s*:`);
  return productionSources()
    .filter(({ code }) => direct.test(code) || viaLedger.test(code))
    .map(({ file }) => file);
}

describe("每个 facet 都必须有生产者", () => {
  it.each([...MACHINE_FACETS])(
    "★ %s 至少有一个写点（没有 ⇒「还差什么」清单永远清不空）",
    (facet) => {
      const files = producersOf(facet);
      expect(
        files,
        `facet "${facet}" 在 src/settings/ 的生产源码里找不到任何 recordFacet 写点。\n` +
          `没有写点 ⇒ computeGaps 恒把它算成 unknown ⇒ 那张清单对任何用户都清不空。\n` +
          `加格子的同时必须加写点，或者在 readiness.notApplicable 里把它排掉。`,
      ).not.toEqual([]);
    },
  );

  it("反向自检：扫描器真的在读源码（编一个不存在的格名必须找不到）", () => {
    // 少了这条，`producersOf` 只要恒返回非空（比如正则写错成恒真）就能把上面全喂绿。
    expect(producersOf("thisFacetDoesNotExist")).toEqual([]);
    // 且确实扫到了文件（目录读错/过滤过头会静默变成「扫了 0 个文件」）
    expect(productionSources().length).toBeGreaterThan(10);
  });

  it("反向自检：只在注释里提到某格**不算**写点", () => {
    // `readiness.ts` 被 skip 了，但别的文件的注释里也会提到格名。
    // 这条钉的是「剥注释」这一步真的生效：构造一段只在注释里出现的代码。
    const commented = stripComments(
      `// recordFacet("acctIso", { kind: "ok" })\nconst x = 1;`,
      "ts",
    );
    expect(commented).not.toContain("acctIso");
  });
});
