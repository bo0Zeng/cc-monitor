// 人粘贴的块折不折是后端写好的 `who.pasted[].fold`：界面照它摆，不自己数行。
// 夹具故意让行数与折不折对不上（3 行却要折 · 40 行却不折）：界面若还在拿行数比阈值，这几条就红。
import { describe, it, expect } from "vitest";
import * as cards from "../../../../src/frontend/ui/cards/index";
import { renderMessage } from "../../../../src/frontend/ui/cards/index";
import { LOCAL } from "../../../../src/frontend/ui/ipc/origin";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import type { LineRecord } from "../../../../src/frontend/ui/generated/LineRecord";

const ctx = () => ({ parentPath: "/p/s.jsonl", origin: LOCAL, toolUseNames: new Map(), toolUseElements: new Map(), pendingToolResults: new Map(), lazy: false });
const said = (text: string, pasted: object[]): LineRecord =>
  ({ agent: "claude", t: "said", id: "p", at: "2026-01-01T14:02:00.000Z", blocks: [{ type: "text", text }], who: { speaker: { kind: "human" }, text, pasted }, draw: "card", human: true }) as never;
const span = (text: string, open: string, body: string, close: string, lines: number, fold: boolean) => {
  const start = text.indexOf(open);
  const bodyStart = start + open.length;
  return { start, end: bodyStart + body.length + close.length, bodyStart, bodyEnd: bodyStart + body.length, lines, fold };
};

describe("粘贴块：折不折只读后端的 fold", () => {
  it("★ fold ⇒ 折成「粘贴的内容 · N 行」、点开是正文；不 fold 的照排、两头的标记不露", () => {
    const folded = "甲\n乙\n丙";
    const open = Array.from({ length: 40 }, (_, i) => `行${i}`).join("\n");
    const text = `看这段：<P1>${folded}</P1>再看<P2>${open}</P2>完`;
    const r = renderMessage(said(text, [span(text, "<P1>", folded, "</P1>", 3, true), span(text, "<P2>", open, "</P2>", 40, false)]), ctx());
    if (r.kind !== "card") throw new Error(r.kind);
    const folds = r.element.querySelectorAll<HTMLDetailsElement>(".paste-fold");
    expect(folds.length).toBe(1);
    expect([folds[0].open, folds[0].querySelector("summary")?.textContent]).toEqual([false, copyText("speaker.paste.fold", { n: "3" })]);
    expect(folds[0].querySelector(".paste-body")?.textContent).toBe("甲乙丙");
    const all = r.element.querySelector(".card-body")?.textContent ?? "";
    expect(all).not.toMatch(/<P|<\/P/);
    expect(all).toContain("行0行1");
  });

  it("界面不留折叠阈值", () => {
    expect(Object.keys(cards)).not.toContain("PASTE_FOLD_LINES");
  });
});
