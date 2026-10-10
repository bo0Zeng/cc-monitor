// `quota-read` 线上形状的金样（后端真出口写的那一份，`tests/__fixtures__/quota-read.golden.json`）：界面的解码器每个案例都收，
// 每号的 rows 是 [[{text, tone}]]、语气在闭集里；号名 / 位名的表收进来之后号名照它出。
import { describe, it, expect } from "vitest";
import GOLDEN from "../../__fixtures__/quota-read.golden.json";
import { decodeQuotaRead } from "../../../src/frontend/ui/quota-reads";
import { accountLabel, fiveHourCell } from "../../../src/frontend/ui/acct-words";

const TONES = new Set(["plain", "fail", "warn", "need", "now", "busy"]);

describe("quota-read 线上金样", () => {
  it("案例不少", () => expect(GOLDEN.cases.length).toBeGreaterThanOrEqual(10));
  for (const c of GOLDEN.cases) {
    it(c.name, () => {
      const r = decodeQuotaRead(c.reply);
      for (const a of [...r.accounts, ...r.unseen]) {
        expect(a.rows.length).toBeGreaterThan(0);
        for (const row of a.rows) for (const cell of row) expect(TONES.has(cell.tone), JSON.stringify(cell)).toBe(true);
        expect(a.warm.text).not.toBe("");
      }
      for (const a of r.accounts) expect(fiveHourCell(r, a.agent, a.account)).toBe(r.fiveHour ?? a.fiveHour);
      expect(accountLabel("_")).toBe(c.reply.names.accounts._);
    });
  }
});
