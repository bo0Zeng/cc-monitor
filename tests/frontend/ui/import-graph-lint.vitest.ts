// @vitest-environment node
/**
 * import 图的两条规矩由 eslint 守（`eslint.config.js` 里 `import-x/no-cycle` · `import-x/no-restricted-paths`），
 * `eslint .` 基线 0 ⇒ 长一条就红（`eslint-baseline.vitest.ts` 与 CI 那一步接）。本文件只证这两条规则真的会抓人：
 * 用仓里那份真配置，往一份真文件的原文后面补一条 import（`lintText`，路径落在那份真文件上，不往盘上写）。
 *
 * - 环：`account-reads.ts` 运行期依赖 `last-seen.ts`；`last-seen.ts` 回头 import 它 ⇒ 成环。只算运行期边
 *   （`import type` 编译期擦除，`import-x/no-cycle` 不跟它走）；`export … from` 算。
 * - 主窗口的视图（`views/`）与设置窗（`settings/`）互不引用：两边共用的住在两边都往下引的那一层（`src/frontend/ui/`）。
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { ESLint } from "eslint";
import { describe, expect, it } from "vitest";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { WALL_CEILING_MS } from "../../test-support/wall-ceiling";

const UI = join(REPO_ROOT, "src", "frontend", "ui");
const eslint = new ESLint({ cwd: REPO_ROOT });

/** 那份真文件原文后面补 `extra`，照它的真路径 lint，回报到的规则名（去重排序）。 */
async function rulesHit(file: string, extra = ""): Promise<string[]> {
  const text = readFileSync(file, "utf8") + extra;
  const [result] = await eslint.lintText(text, { filePath: file });
  return [...new Set(result.messages.map((m) => m.ruleId ?? `(解析错) ${m.message}`))].sort();
}

const LEAF = join(UI, "last-seen.ts");
const VIEW = join(UI, "views", "command-bar.ts");

describe("import 图的规矩由 eslint 守", () => {
  it("前提：account-reads.ts 运行期 import last-seen.ts（不然下面那条成不了环）", () => {
    expect(readFileSync(join(UI, "account-reads.ts"), "utf8")).toMatch(/^import \{[^}]*\} from "\.\/last-seen";$/m);
  });

  it("原样零报（下面几条报的是补上去的那一行）", async () => {
    expect(await rulesHit(LEAF)).toEqual([]);
    expect(await rulesHit(VIEW)).toEqual([]);
  }, WALL_CEILING_MS);

  it("两份界面模块互相 import（值边）⇒ import-x/no-cycle", async () => {
    expect(await rulesHit(LEAF, '\nimport { _probe } from "./account-reads";\n')).toEqual(["import-x/no-cycle"]);
  }, WALL_CEILING_MS);

  it("回头那一条是 export … from（运行期再导出）⇒ 照样 import-x/no-cycle", async () => {
    expect(await rulesHit(LEAF, '\nexport * from "./account-reads";\n')).toEqual(["import-x/no-cycle"]);
  }, WALL_CEILING_MS);

  it("回头那一条带 .ts 后缀 ⇒ 照样 import-x/no-cycle（本仓两种写法都有）", async () => {
    expect(await rulesHit(LEAF, '\nimport { _probe } from "./account-reads.ts";\n')).toEqual(["import-x/no-cycle"]);
  }, WALL_CEILING_MS);

  it("视图 import 设置窗 ⇒ import-x/no-restricted-paths", async () => {
    expect(await rulesHit(VIEW, '\nimport "../settings/router";\n')).toContain("import-x/no-restricted-paths");
  }, WALL_CEILING_MS);
});
