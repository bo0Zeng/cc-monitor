/**
 * 轮换那一组词：「抢回」与「兜底」是两件事，说法分开；同一件事全产品一个说法。
 *
 * # 判什么
 *
 * 1. 两件事不共用一个动词：轮换这一组键（规则说明 · 摘要 · 换法 · 兜底开关 · 时间轴短码 · 换号记录）里不出现「切回」。
 *    抢回从「前面的号」说（它有额度就换回它）；兜底从「这个号」说 —— 说明里点它的名（`{list}`），不说「其余号」。
 * 2. 同一件事一个说法：抢回在换法卡 · 摘要 · 时间轴短码三处同字；离开兜底的时间轴短码就是换号记录那一句的头一段；
 *    先等的时间轴短码与换号记录那一句开头同一个词。
 * 3. 「别的号」与「其余号」只留「别的号」：文案表里不出现「其余号」（术语表禁档同时扫）。
 * 4. quota-warm 的 SKILL.md（AI 自己管轮换时读的说明）用同一组词：写「抢回」「兜底」，不写「切回」。
 *
 * # 不判什么
 *
 * - 句子通不通顺、读者能不能分清：只逮词面，读法靠审截图。
 * - 时长那一格是不是成品字（「40 分钟」不是「40m」）：在 Rust 侧 `rule_text_tests.rs`（值由 `format_duration` 出）。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { REPO_ROOT } from "../test-support/repo-root.ts";
import { TABLE_PATH } from "./copy-support.ts";

const entries = JSON.parse(readFileSync(TABLE_PATH, "utf8")).entries as Record<string, { zh: string; args?: string[] }>;
const zh = (k: string): string => {
  const e = entries[k];
  if (e === undefined) throw new Error(`文案表里没有 ${k}`);
  return e.zh;
};
const SEP = zh("kit.text.sep");

/** 轮换这一组键：规则说明 · 摘要 · 换法 · 兜底 · 等待 · 时间轴短码 · 换号记录里与换法有关的几种原因。 */
const GROUP = Object.keys(entries).filter(
  (k) =>
    /^(beRotation\.(explain|sum)\.|rot\.(how|fallback|wait|why)\.)/.test(k) ||
    ["acct.hist.preempt", "acct.hist.leaveFallback", "acct.hist.wait"].includes(k),
);

describe("轮换：抢回与兜底说法分开", () => {
  it("这一组里有键（正控：分组的正则没写空）", () => {
    expect(GROUP).toContain("beRotation.explain.preempt");
    expect(GROUP).toContain("rot.fallback.hint");
    expect(GROUP.length).toBeGreaterThan(20);
  });

  it("★ 两件事不共用「切回」", () => {
    const hits = GROUP.filter((k) => zh(k).includes("切回")).map((k) => `${k}: ${zh(k)}`);
    expect(hits).toEqual([]);
  });

  it("★ 抢回从「前面的号」说；规则说明与换法卡那一句同字", () => {
    for (const k of ["beRotation.explain.preempt", "rot.how.preemptHint"]) {
      expect(zh(k).startsWith("前面的号"), `${k}: ${zh(k)}`).toBe(true);
    }
    expect(zh("rot.how.preemptHint")).toBe(zh("beRotation.explain.preempt"));
  });

  it("★ 兜底从「这个号」说：规则说明里点兜底号的名，不说「其余号」", () => {
    expect(entries["beRotation.explain.leave"]?.args).toEqual(["list"]);
    expect(zh("beRotation.explain.leave")).toContain("{list}");
  });

  it("★ 同一件事一个说法：抢回三处同字；离开兜底 · 先等 的时间轴短码与换号记录那一句开头同词", () => {
    expect(zh("rot.why.preempt")).toBe(zh("rot.how.preempt"));
    expect(zh("beRotation.sum.preempt")).toBe(zh("rot.how.preempt"));
    expect(zh("acct.hist.leaveFallback").split(SEP)[0]).toBe(zh("rot.why.leave"));
    expect(zh("rot.why.leave")).toContain(zh("rot.fallback.tag"));
    const waitWord = (s: string): string => s.split(" ")[0] ?? "";
    expect(waitWord(zh("rot.why.wait"))).toBe(waitWord(zh("acct.hist.wait")));
  });

  it("★ 文案表里不出现「其余号」（只留「别的号」）", () => {
    const hits = Object.entries(entries)
      .filter(([, e]) => e.zh.includes("其余号"))
      .map(([k]) => k);
    expect(hits).toEqual([]);
  });

  it("★ quota-warm 的 SKILL.md 用同一组词", () => {
    const skill = readFileSync(resolve(REPO_ROOT, "src", "shared", "quota-warm", "SKILL.md"), "utf8");
    expect(skill).toContain(zh("rot.how.preempt"));
    expect(skill).toContain(zh("rot.fallback.tag"));
    expect(skill).toContain(zh("rot.why.leave"));
    expect(skill).not.toContain("切回");
    expect(skill).not.toContain("其余号");
  });
});
