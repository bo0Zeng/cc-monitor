/**
 * 大小的那一个读口（桌面 `format.ts::sizeText`）与核心 `copy_core::size_text` 对同一份金样 `tests/__fixtures__/size-text.golden.json`；
 * Rust 那一侧 `tests/common/copy-core/lib_tests.rs::the_shared_size_golden_agrees_with_this_reader` 读同一份。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { sizeText } from "../../src/frontend/ui/format.ts";

const golden = JSON.parse(readFileSync(resolve(__dirname, "../__fixtures__/size-text.golden.json"), "utf8")) as {
  cases: { bytes: number; want: string }[];
};

describe("大小读口对拍（金样 size-text.golden.json）", () => {
  it("每条：sizeText 出来的 == 手写的 want", () => {
    expect(golden.cases.length).toBeGreaterThanOrEqual(10);
    const wrong = golden.cases.filter((c) => sizeText(c.bytes) !== c.want).map((c) => `${c.bytes}: ${sizeText(c.bytes)} ≠ ${c.want}`);
    expect(wrong).toEqual([]);
  });
});
