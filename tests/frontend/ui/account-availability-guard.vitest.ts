/**
 * **可选只由那台后端那一格答**：可用性一律读那台后端写好的 `selectable`（规则住 `acct_core::account_selectable`，
 * 跨三处金样 `tests/__fixtures__/account-default.golden.json`）。
 *
 * 「鉴权那几格（`loggedIn` · `authReady` · `authKind`）只在 `accounts.ts` 出现」由 eslint 守（`eslint.config.js` 的
 * `singleDoors`，家是 `src/frontend/ui/accounts.ts`；`single-home-lint.vitest.ts` 证会抓）。本文件钉另一半：
 * 可选列表确实只读成品那一格 —— 否则把可选判定挪回界面（`mode` · `exists` 拼一遍），那一条照样绿
 * （`mode` / `exists` 不在被禁的名字里）。
 *
 * 买不到：
 * - Rust 侧的同类绕过：由两条同名判据各守自己那份（`src/frontend/shell/src/local_accounts.rs` 与
 *   `src/backend/observe/accounts_query.rs` 的 `the_auth_dimension_has_exactly_one_computation_path`）。
 * - `accounts.ts` 里头再长一处读鉴权格：家里不数次数。
 * - 测试侧手抄的一份 `isSelectable`（`vi.mock` 替身可以与真身语义相反）：靠「测试侧不许手抄纯函数」那条纪律。
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, it, expect } from "vitest";

import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { stripComments } from "../../test-support/strip-comments.ts";

describe("账号可用性只有一个出口", () => {
  it("可选列表只照那台写好的 `selectable`（界面不另判）", () => {
    const acc = stripComments(readFileSync(join(REPO_ROOT, "src", "frontend", "ui", "accounts.ts"), "utf8"), "ts");
    const m = /export function selectableAccounts\(state: AccountsState\): Account\[\] \{\n([\s\S]*?)\n\}/.exec(acc);
    expect(m, "切不出 `selectableAccounts` 的函数体（签名或大括号形状变了）").not.toBeNull();
    const body = m?.[1] ?? "";
    expect(body, "切出来的「函数体」跨进了下一个函数 —— 窗口又变无界了").not.toContain("export function");
    expect(/\ba\.selectable\b/.test(body), "`selectableAccounts` 不再读成品的 `selectable`").toBe(true);
    for (const f of ["mode", "authReady", "exists", "loggedIn"]) {
      expect(new RegExp(`\\ba\\.${f}\\b`).test(body), `\`selectableAccounts\` 又自己读了 \`${f}\` —— 可选判定长回了界面`).toBe(false);
    }
  });
});
