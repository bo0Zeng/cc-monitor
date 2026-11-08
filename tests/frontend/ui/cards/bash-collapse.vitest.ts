// **bash 输出卡与派活卡折不折、露几行、展开那一句都照后端写好的折法（`TextFold`）**：阈值只住 `face_core::record`，界面不数行。
// 夹具故意让折法与行数对不上（5 行却要折 · 200 行却不折）、展开那一句是个占位：界面若还在自己比阈值、自己拼「展开 N 行」，这几条就红。
import { describe, it, expect } from "vitest";
import * as bash from "../../../../src/frontend/ui/cards/bash";
import * as brief from "../../../../src/frontend/ui/cards/brief";
import { buildBashOutputCard } from "../../../../src/frontend/ui/cards/bash";
import { buildBriefCard } from "../../../../src/frontend/ui/cards/brief";

const time = "00:00";
const linesOf = (n: number) => Array.from({ length: n }, (_, i) => `line-${i + 1}`).join("\n");
const WRAP = ".block-body-truncated-wrap";
const EXPAND = ".block-body-show-full";

describe("bash 输出卡：照后端的折法", () => {
  it("★ 有折法 ⇒ 露它说的那几行、展开那一颗写它那一句；点开露全文", () => {
    const card = buildBashOutputCard({ stdout: linesOf(5), stderr: "", stdoutFold: { shown: 2, total: 5, more: "MORE-LINE" } }, time);
    const pre = card.querySelector(`${WRAP} pre`);
    expect(pre?.textContent).toBe("line-1\nline-2");
    const expand = card.querySelector<HTMLButtonElement>(EXPAND);
    expect(expand?.textContent).toBe("MORE-LINE");
    expand?.click();
    expect(pre?.textContent).toBe(linesOf(5));
  });

  it("★ 没有折法 ⇒ 整份照排（200 行也不折）", () => {
    const card = buildBashOutputCard({ stdout: linesOf(200), stderr: "" }, time);
    expect(card.querySelector(WRAP)).toBeNull();
    expect(card.textContent).toContain("line-200");
  });

  it("stderr 照它自己那一份折法", () => {
    const card = buildBashOutputCard({ stdout: "", stderr: linesOf(3), stderrFold: { shown: 1, total: 3, more: "ERR-MORE" } }, time);
    expect(card.querySelector(`.bash-stderr-body`)?.textContent).toBe("line-1");
    expect(card.querySelector(EXPAND)?.textContent).toBe("ERR-MORE");
  });

  it("界面不留阈值", () => {
    expect(Object.keys(bash)).not.toContain("OUTPUT_HEAD_LINES");
    expect(Object.keys(brief)).not.toContain("BRIEF_LINES");
  });
});

describe("派活卡：照后端的折法", () => {
  it("★ 有折法 ⇒ 收着、展开那一颗写它那一句；没有 ⇒ 不收（9 行也不收）", () => {
    const folded = buildBriefCard(linesOf(2), { shown: 1, total: 2, more: "BRIEF-MORE" }, time, null);
    expect(folded.querySelector<HTMLElement>("[data-clamped]")).not.toBeNull();
    expect(folded.querySelector("button")?.textContent).toBe("BRIEF-MORE");
    const open = buildBriefCard(linesOf(9), undefined, time, null);
    expect(open.querySelector("[data-clamped]")).toBeNull();
    expect(open.querySelector("button")).toBeNull();
  });
});
