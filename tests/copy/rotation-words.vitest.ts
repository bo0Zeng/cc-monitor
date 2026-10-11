/**
 * 轮换第三版的词（`轮换-积木.md` §9 S13 的文案那一半）。
 *
 * # 判什么
 *
 * 1. 后端写的那几组键（`beRotation.lib.*` 共用小函数的字 · `beRotation.desc.*` 描述片段 · `beRotation.leave.*` 换号记录引擎那一半 ·
 *    `beRotation.strategy.*` 策略出错 · `beRotation.blocked.*` 卡住条）都在表里。
 * 2. 退役的旧词不出现在 `beRotation.*`：抢回 · 兜底 · 封顶 · 单段预算 · 触发 · 无号可换 · 规则 · 身份 · 只当备用 · 例外。
 * 3. 没有第二人称。
 *
 * # 不判什么
 *
 * - 句子通不通顺：读法靠审截图。
 * - 策略文件自己的字（`texts()`）：在 Rust 侧 `strategy_tests.rs`（两种语言都有、键都写得成字）。
 */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { TABLE_PATH } from "./copy-support.ts";

const entries = JSON.parse(readFileSync(TABLE_PATH, "utf8")).entries as Record<string, { zh: string }>;
const be = Object.keys(entries).filter((k) => k.startsWith("beRotation."));

const MUST = [
  "beRotation.lib.refused",
  "beRotation.lib.full",
  "beRotation.lib.overage",
  "beRotation.lib.login",
  "beRotation.lib.line",
  "beRotation.lib.off",
  "beRotation.lib.pending",
  "beRotation.lib.going",
  "beRotation.desc.n",
  "beRotation.desc.line",
  "beRotation.desc.full",
  "beRotation.desc.acctLine",
  "beRotation.desc.acctFull",
  "beRotation.desc.acctSlot",
  "beRotation.desc.acctOff",
  "beRotation.desc.delay",
  "beRotation.desc.stop",
  "beRotation.desc.go",
  "beRotation.desc.broken",
  "beRotation.leave.top",
  "beRotation.leave.unusable",
  "beRotation.leave.manual",
  "beRotation.leave.manualRestart",
  "beRotation.leave.refresh",
  "beRotation.leave.held",
  "beRotation.leave.overage",
  "beRotation.strategy.failed",
  "beRotation.strategy.timeout",
  "beRotation.strategy.missing",
  "beRotation.strategy.badShape",
  "beRotation.blocked.stop",
];

const RETIRED = ["抢回", "兜底", "封顶", "单段预算", "触发", "无号可换", "规则", "身份", "只当备用", "例外"];

describe("轮换第三版的词", () => {
  it("★ 后端写的那几组键都在表里", () => {
    expect(MUST.filter((k) => entries[k] === undefined)).toEqual([]);
  });

  it("★ 退役的旧词不出现在 beRotation.*", () => {
    const hits = be.flatMap((k) => RETIRED.filter((w) => entries[k].zh.includes(w)).map((w) => `${k}: ${w}`));
    expect(hits).toEqual([]);
  });

  it("★ 没有第二人称", () => {
    expect(be.filter((k) => /你|您/.test(entries[k].zh))).toEqual([]);
  });
});

describe("轮换的 skill", () => {
  it("★ SKILL.md 里没有退役的旧词、没有第二人称", async () => {
    const { REPO_ROOT } = await import("../test-support/repo-root.ts");
    const text = readFileSync(`${REPO_ROOT}/src/shared/rotation/SKILL.md`, "utf8");
    expect(RETIRED.filter((w) => text.includes(w))).toEqual([]);
    expect(/你|您/.test(text)).toBe(false);
  });
});
