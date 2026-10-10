/**
 * 时长与距今的那一个读口（桌面）：共用金样 `tests/__fixtures__/short-duration.golden.json` 逐条喂给 `fmtDur` · `clockNow`，
 * 期望是金样里手写的；Rust 那一侧 `tests/common/copy-core/lib_tests.rs` 读同一份（`short_duration` · `rel_duration`），手机端 `DurationFormatTest.kt` 也读它。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { clockNow, fmtDur } from "../../src/frontend/ui/duration-format.ts";

const golden = JSON.parse(readFileSync(resolve(__dirname, "../__fixtures__/short-duration.golden.json"), "utf8")) as {
  cases: { ms: number; want: string }[];
  clock: { text: string; from: number; to?: number; now: number; want: string }[];
};

describe("时长读口对拍（金样 short-duration.golden.json）", () => {
  it("金样读得出（反空真）", () => {
    expect(golden.cases.length).toBeGreaterThanOrEqual(10);
    expect(golden.clock.length).toBeGreaterThanOrEqual(3);
  });

  it("每条：fmtDur 出来的 == 手写的 want", () => {
    const wrong = golden.cases.filter((c) => fmtDur(c.ms / 1000) !== c.want).map((c) => `${c.ms}: ${JSON.stringify(fmtDur(c.ms / 1000))} ≠ ${c.want}`);
    expect(wrong).toEqual([]);
  });

  it("会走的钟：clockNow 按此刻（或 to）填 {dur}", () => {
    const wrong = golden.clock
      .filter((c) => clockNow({ text: c.text, from: c.from, to: c.to }, c.now) !== c.want)
      .map((c) => `${JSON.stringify(c)} ⇒ ${clockNow({ text: c.text, from: c.from, to: c.to }, c.now)}`);
    expect(wrong).toEqual([]);
  });
});
