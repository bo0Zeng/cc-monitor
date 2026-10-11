/**
 * 计数的那一个读口（桌面 `format.ts::countText`）与核心 `copy_core::count_text` 对同一份金样 `tests/__fixtures__/count-text.golden.json`；
 * Rust 那一侧 `tests/common/copy-core/lib_tests.rs::the_shared_count_golden_agrees_with_this_reader` 读同一份。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { countText } from "../../src/frontend/ui/format.ts";

const golden = JSON.parse(readFileSync(resolve(__dirname, "../__fixtures__/count-text.golden.json"), "utf8")) as {
  cases: { n: number; want: string }[];
};

describe("计数读口对拍（金样 count-text.golden.json）", () => {
  it("每条：countText 出来的 == 手写的 want", () => {
    expect(golden.cases.length).toBeGreaterThanOrEqual(8);
    const wrong = golden.cases.filter((c) => countText(c.n) !== c.want).map((c) => `${c.n}: ${countText(c.n)} ≠ ${c.want}`);
    expect(wrong).toEqual([]);
  });
});
