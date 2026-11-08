// `quota-read` 线上形状的金样（后端真出口写的那一份，`tests/__fixtures__/quota-read.golden.json`；形状由 `cmd-wire` 解它的判据钉）：界面每个案例都收，
// 每号的 rows 是 [[{text, tone}]]、语气在闭集里；每格的语义位都在同一份的位名表里。
import { describe, it, expect } from "vitest";
import GOLDEN from "../../__fixtures__/quota-read.golden.json";
import { tookQuota, type QuotaRead } from "../../../src/frontend/ui/quota-reads";
import { LOCAL } from "../../../src/frontend/ui/ipc/origin";
import { nonEmpty } from "../../test-support/non-empty";

const TONES = new Set(["plain", "fail", "warn", "need", "now", "busy"]);

describe("quota-read 线上金样", () => {
  it("案例不少", () => expect(GOLDEN.cases.length).toBeGreaterThanOrEqual(10));
  for (const c of GOLDEN.cases) {
    it(c.name, () => {
      const r = tookQuota(LOCAL, c.reply as unknown as QuotaRead);
      for (const a of [...r.accounts, ...r.unseen]) {
        nonEmpty(a.rows);
        for (const row of a.rows) for (const cell of row) expect(TONES.has(cell.tone), JSON.stringify(cell)).toBe(true);
        expect(a.warm.text).not.toBe("");
      }
    });
  }
});
