/**
 * 界面自己出的事的钟面（`clock-face.ts`）与核心写「几点」那一处（后端 `common/time.rs::hm`）对同一份金样
 * `tests/__fixtures__/clock-face.golden.json`；Rust 那一侧 `tests/backend/common/time_tests.rs::t13_*` 读同一份。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { clockFace } from "../../src/frontend/ui/clock-face.ts";

const golden = JSON.parse(readFileSync(resolve(__dirname, "../__fixtures__/clock-face.golden.json"), "utf8")) as {
  cases: { ms: number; offsetMin: number; want: string }[];
};

describe("钟面对拍（金样 clock-face.golden.json）", () => {
  it("每条：clockFace 出来的 == 手写的 want", () => {
    expect(golden.cases.length).toBeGreaterThanOrEqual(6);
    const wrong = golden.cases.filter((c) => clockFace(c.ms, c.offsetMin) !== c.want).map((c) => `${JSON.stringify(c)} ⇒ ${clockFace(c.ms, c.offsetMin)}`);
    expect(wrong).toEqual([]);
  });
});
