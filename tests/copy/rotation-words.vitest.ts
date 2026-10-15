/**
 * 轮换的词（产品里不出现「抢回」「兜底」「身份」这类说法）。
 *
 * # 判什么
 *
 * 1. 退役的旧词不出现在 `beRotation.*`，也不出现在轮换的 skill 里：抢回 · 兜底 · 封顶 · 单段预算 · 触发 · 无号可换 · 规则 · 身份 · 只当备用 · 例外。
 * 2. 没有第二人称。
 *
 * # 不判什么
 *
 * - 句子通不通顺：读法靠审截图。
 * - 策略文件自己的字（声明里的标签与说明）：后端照 `src/shared/rotation/form-words.json` 判，界面提示「说明写法不合规」。
 */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { TABLE_PATH } from "./copy-support.ts";

const entries = JSON.parse(readFileSync(TABLE_PATH, "utf8")).entries as Record<string, { zh: string }>;
const be = Object.keys(entries).filter((k) => k.startsWith("beRotation."));

const RETIRED = ["抢回", "兜底", "封顶", "单段预算", "触发", "无号可换", "规则", "身份", "只当备用", "例外"];

describe("轮换的词", () => {
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
