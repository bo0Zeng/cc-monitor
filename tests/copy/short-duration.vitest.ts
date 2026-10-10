/**
 * 短时长（桌面走字那一个读口）：共用金样 `tests/__fixtures__/short-duration.golden.json` 逐条喂给 `fmtDur`，
 * 期望是金样里手写的；Rust 那一侧 `tests/common/copy-core/lib_tests.rs::the_shared_short_duration_golden_agrees_with_this_reader` 读同一份。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { fmtDur } from "../../src/frontend/ui/quota-lines.ts";

const golden = JSON.parse(readFileSync(resolve(__dirname, "../__fixtures__/short-duration.golden.json"), "utf8")) as {
  cases: { ms: number; want: string }[];
};

describe("两个读口的短时长对拍（金样 short-duration.golden.json）", () => {
  it("金样读得出（反空真）", () => {
    expect(golden.cases.length).toBeGreaterThanOrEqual(10);
  });

  it("每条：fmtDur 出来的 == 手写的 want", () => {
    const wrong = golden.cases.filter((c) => fmtDur(c.ms / 1000) !== c.want).map((c) => `${c.ms}: ${JSON.stringify(fmtDur(c.ms / 1000))} ≠ ${c.want}`);
    expect(wrong).toEqual([]);
  });
});
