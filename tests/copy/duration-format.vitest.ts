/**
 * 时长格式化（TS 这一侧）：共用金样 `tests/__fixtures__/duration-format.golden.json` 逐条喂给 `formatDuration`，
 * 期望是金样里手写的；Rust 那一侧 `tests/common/copy-core/lib_tests.rs::the_shared_duration_golden_agrees_with_this_reader` 读同一份。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { formatDuration } from "../../src/frontend/ui/duration-format.ts";

const golden = JSON.parse(readFileSync(resolve(__dirname, "../__fixtures__/duration-format.golden.json"), "utf8")) as {
  cases: { ms: number; want: string }[];
};

describe("两个读口的时长格式化对拍（金样 duration-format.golden.json）", () => {
  it("金样读得出（反空真）", () => {
    expect(golden.cases.length).toBeGreaterThanOrEqual(10);
  });

  it("每条：formatDuration 出来的 == 手写的 want", () => {
    const wrong = golden.cases.filter((c) => formatDuration(c.ms) !== c.want).map((c) => `${c.ms}: ${JSON.stringify(formatDuration(c.ms))} ≠ ${c.want}`);
    expect(wrong).toEqual([]);
  });
});
