// 要求住址：主会话 09-28 裁 MIG-3b ①（`tool_registry` ＋ `build_rows` 进后端）· `设计/05 §14.3`「成品的两侧对拍：界面按形状严格收 · 跨语言金样」。
//
// 后端 `footprint-report` 的应答 == `tests/__fixtures__/footprint-report.golden.json`（后端判据产出的那一份）；
// 界面解码器读同一份：逐格收下来与原文相等（一格不丢、一格不改），多一格 / 少一格 / 档不在闭集 ⇒ 抛「形状不对」。
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { decodeFootprint } from "../../src/settings/footprint-reads";

const golden = JSON.parse(
  readFileSync(resolve(dirname(fileURLToPath(import.meta.url)), "../__fixtures__/footprint-report.golden.json"), "utf8"),
) as { report: { rows: Record<string, unknown>[] }; clientAsks: string[] };

describe("〔MIG-3b 续〕足迹成品的跨语言金样", () => {
  it("★ 金样逐格收下来 == 原文；本机第一趟（report: null）也收得下", () => {
    expect(golden.report.rows.length).toBeGreaterThan(10);
    expect(decodeFootprint(golden)).toEqual(golden);
    expect(decodeFootprint({ report: null, clientAsks: ["/m/x"] })).toEqual({ report: null, clientAsks: ["/m/x"] });
  });

  it("★ 多一格 / 少一格 / 档不在闭集 / 状态种类不认 ⇒ 抛，不猜", () => {
    const clone = (): typeof golden => JSON.parse(JSON.stringify(golden)) as typeof golden;
    const extra = clone();
    extra.report.rows[0].surplus = 1;
    const missing = clone();
    delete missing.report.rows[0].note;
    const tier = clone();
    tier.report.rows[0].tier = "AppAssumesPresent";
    const state = clone();
    state.report.rows[0].state = { kind: "maybe" };
    for (const bad of [extra, missing, tier, state, { report: null }, { ...golden, origin: "<local>" }]) {
      expect(() => decodeFootprint(bad)).toThrow(/形状不对/);
    }
  });
});
