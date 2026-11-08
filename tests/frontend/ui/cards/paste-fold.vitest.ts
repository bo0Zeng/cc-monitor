// 人粘贴的块怎么切、折不折、折起那一行写什么都是后端切好的 `who.parts`：界面一段一段照摆，不认标记、不自己切、不自己数行、不自己拼字。
// 夹具的正文故意带着标记、段里的字与正文对不上：界面若还在照 `text` 自己切，这几条就红。
import { describe, it, expect } from "vitest";
import * as cards from "../../../../src/frontend/ui/cards/index";
import { renderMessage } from "../../../../src/frontend/ui/cards/index";
import { LOCAL } from "../../../../src/frontend/ui/ipc/origin";
import type { LineRecord } from "../../../../src/frontend/ui/generated/LineRecord";
import type { SaidPart } from "../../../../src/frontend/ui/generated/SaidPart";

const ctx = () => ({ parentPath: "/p/s.jsonl", origin: LOCAL, toolUseNames: new Map(), toolUseElements: new Map(), pendingToolResults: new Map(), lazy: false });
const said = (text: string, parts?: SaidPart[]): LineRecord =>
  ({ agent: "claude", t: "said", id: "p", at: "2026-01-01T14:02:00.000Z", blocks: [{ type: "text", text }], who: { speaker: { kind: "human" }, text, ...(parts ? { parts } : {}) }, draw: "card", human: true }) as never;

describe("粘贴块：照后端切好的段摆", () => {
  it("★ 折着的那段收成后端写好的那一行、点开是那段的字；照排的段照摆；正文里的标记不露", () => {
    const text = "看这段：<P1>甲\n乙\n丙</P1>再看<P2>丁</P2>完";
    const r = renderMessage(
      said(text, [
        { text: "SEG-A" },
        { text: "FOLDED-BODY", fold: { shown: 0, total: 3, more: "FOLD-LINE" } },
        { text: "SEG-B" },
      ]),
      ctx(),
    );
    if (r.kind !== "card") throw new Error(r.kind);
    const folds = r.element.querySelectorAll<HTMLDetailsElement>(".paste-fold");
    expect(folds.length).toBe(1);
    expect([folds[0].open, folds[0].querySelector("summary")?.textContent]).toEqual([false, "FOLD-LINE"]);
    expect(folds[0].querySelector(".paste-body")?.textContent).toBe("FOLDED-BODY");
    const all = r.element.querySelector(".card-body")?.textContent ?? "";
    expect(all).toContain("SEG-A");
    expect(all).toContain("SEG-B");
    expect(all, "照段摆、不再照正文切").not.toMatch(/<P|甲|丁/);
  });

  it("没有段 ⇒ 正文整段照排", () => {
    const r = renderMessage(said("就一句"), ctx());
    if (r.kind !== "card") throw new Error(r.kind);
    expect(r.element.querySelector(".paste-fold")).toBeNull();
    expect(r.element.querySelector(".card-body")?.textContent).toContain("就一句");
  });

  it("界面不留折叠阈值", () => {
    expect(Object.keys(cards)).not.toContain("PASTE_FOLD_LINES");
  });
});
