/**
 * 额度行模型（界面那一侧）对金样 `tests/__fixtures__/quota-text.golden.json` 逐字：每号每行每格都相等。
 * 终端 `--text` 那一侧在 `tests/backend/control/quota_text_tests.rs` 对同一份金样（两边想不一致得先改金样）。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { seenBlock, unseenBlock, type QuotaRead } from "../../../src/frontend/ui/quota-lines.ts";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { copyText } from "../../../src/frontend/ui/copy-table";

interface Case {
  name: string;
  machine: string;
  tzOffsetMin: number;
  reply: QuotaRead;
  blocks: { account: string; rows: string[][] }[];
}

const golden = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/quota-text.golden.json"), "utf8")) as { cases: Case[] };

describe("额度行模型 ＝ 金样", () => {
  it("金样不空、各形都在（被拒 · 超额 · 按量 · 需登录 · 需 key · 无采样 · 已过 · 跨年）", () => {
    const all = JSON.stringify(golden.cases.map((c) => c.blocks));
    expect(golden.cases.length).toBeGreaterThanOrEqual(10);
    for (const w of ["✕", copyText("acct.val.over"), copyText("acct.val.overOn"), copyText("acct.kind.api"), copyText("acct.tag.login"), copyText("acct.tag.key"), copyText("acct.seen.none"), copyText("acct.reset.past", { at: "" }).replace("↻", "").trim(), "2027-", "2025-", "+3d", "+1h50m", "+45m", "+2h\""]) {
      expect(all, w).toContain(w);
    }
  });

  for (const c of golden.cases) {
    it(c.name, () => {
      const r = c.reply;
      // 每号一段：先出过数的、再没出过的，各按回包的次序（账号面板逐号取的就是这两个）；读不出 ⇒ 空。
      const blocks =
        r.state === "unreadable"
          ? []
          : [...r.accounts.map((a) => seenBlock(a, r.now, c.tzOffsetMin, c.machine)), ...r.unseen.map((u) => unseenBlock(u))];
      const got = blocks.map((b) => ({ account: b.account, rows: b.rows }));
      expect(got).toEqual(c.blocks);
    });
  }
});
