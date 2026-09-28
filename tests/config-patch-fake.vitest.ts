/**
 * 〔CFG1〕假盘（`tests/config-patch-fake.ts`）的补丁语义 == 金样 `tests/__fixtures__/config-patch.golden.json`。
 *
 * 守的要求：`设计/30 §4`「各自只写自己那个键」· `设计/70 §6.3` 红线 ④「读不懂的 `config.json` 不写」。
 * Rust 写口跑同一份金样（`tests/bridge/config_tests.rs::the_golden_cases_hold`）⇒ 前端测试里那块假盘不是自说自话。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { applyConfigEdits, ConfigRefused, type Edit } from "./config-patch-fake";

interface Case {
  name: string;
  before: string;
  edits: Edit[];
  after?: unknown;
  refused?: "bad_edit" | "unreadable" | "element_gone" | "element_exists";
}
const GOLDEN = JSON.parse(readFileSync("tests/__fixtures__/config-patch.golden.json", "utf8")) as {
  cases: Case[];
};

describe("CFG1 · 假盘补丁语义 == 金样", () => {
  it("金样不是空的（空集上的相等恒真）", () => {
    expect(GOLDEN.cases.length).toBe(16); // 〔FIX2 续〕12 → 16：insertin / removein 各两条
  });
  for (const c of GOLDEN.cases) {
    it(c.name, () => {
      if (c.refused) {
        let got: unknown;
        try {
          applyConfigEdits(c.before, c.edits);
        } catch (e) {
          got = e;
        }
        expect(got).toBeInstanceOf(ConfigRefused);
        expect((got as ConfigRefused).kind).toBe(c.refused);
      } else {
        expect(JSON.parse(applyConfigEdits(c.before, c.edits))).toEqual(c.after);
      }
    });
  }
});
