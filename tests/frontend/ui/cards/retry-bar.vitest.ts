/**
 * 报错卡与重试细条只摆后端写好的那几句：卡头（`error.title`）· 细条那一行（`retry.text`）· 一串的整句（会话事实 `retries[].text`）·
 * 来话条抬头（`peerSession.title`）。夹具里的字都是占位：界面若还在自己映原因、自己拼次数与钟面，这几条就红。
 */
import { describe, it, expect } from "vitest";
import { applyRetries, buildApiErrorCard, buildApiRetryCard } from "../../../../src/frontend/ui/cards/api-error";
import { renderMessage } from "../../../../src/frontend/ui/cards/index";
import { LOCAL } from "../../../../src/frontend/ui/ipc/origin";
import type { LineRecord } from "../../../../src/frontend/ui/generated/LineRecord";
import type { RetryRun } from "../../../../src/frontend/ui/generated/RetryRun";

const stream = (...els: HTMLElement[]) => {
  const root = document.createElement("div");
  root.append(...els);
  return root;
};
const runs = (...rs: RetryRun[]) => new Map(rs.map((r) => [r.id, r]));
const ctx = () => ({ parentPath: "/p/s.jsonl", origin: LOCAL, toolUseNames: new Map(), toolUseElements: new Map(), pendingToolResults: new Map(), lazy: false });

describe("重试细条：字照后端那一行 · 一串照会话事实的整句", () => {
  it("★ 建卡照记录那一行；事实到了照那一串的整句与结局（按首条的 id 读）；结局变了照新的", () => {
    const bar = buildApiRetryCard({ text: "REC-LINE", id: "r1" });
    expect([bar.textContent, bar.dataset.state]).toEqual(["REC-LINE", "retrying"]);
    const root = stream(bar, document.createElement("div"));
    applyRetries(root, runs({ id: "r2", outcome: "recovered", text: "OTHER" }));
    expect([bar.textContent, bar.dataset.state], "别的那一串不动它").toEqual(["REC-LINE", "retrying"]);
    applyRetries(root, runs({ id: "r1", outcome: "retrying", text: "RUN-NOW" }));
    expect([bar.textContent, bar.dataset.state]).toEqual(["RUN-NOW", "retrying"]);
    applyRetries(root, runs({ id: "r1", outcome: "recovered", text: "RUN-OK" }));
    expect([bar.textContent, bar.dataset.state]).toEqual(["RUN-OK", "recovered"]);
    applyRetries(root, runs({ id: "r1", outcome: "interrupted", text: "RUN-STOP" }));
    expect([bar.textContent, bar.dataset.state]).toEqual(["RUN-STOP", "interrupted"]);
  });

  it("建卡时事实已经到了 ⇒ 照那一串画", () => {
    const bar = buildApiRetryCard({ text: "REC-LINE", id: "r1", run: { id: "r1", outcome: "recovered", text: "RUN-OK" } });
    expect([bar.textContent, bar.dataset.state]).toEqual(["RUN-OK", "recovered"]);
  });

  it("★ 没接上 ⇒ 细条收起、那一串的整句进紧跟着的报错卡（那两颗终端按钮不被冲掉）；卡头照后端那一句、原文进折叠", () => {
    const bar = buildApiRetryCard({ text: "REC-LINE", id: "r1" });
    const card = buildApiErrorCard({ timeLabel: "02:12", title: "CARD-HEAD", text: "API Error: Overloaded", status: 529 });
    applyRetries(stream(bar, card), runs({ id: "r1", outcome: "failed", text: "RUN-NEXT" }));
    expect(bar.dataset.state).toBe("failed");
    expect(card.querySelector(".api-error-label")?.textContent).toBe("CARD-HEAD");
    expect(card.querySelector(".api-error-next-text")?.textContent).toBe("RUN-NEXT");
    expect(card.querySelectorAll(".api-error-acts [data-act]").length).toBe(2);
    expect(card.querySelector(".api-error-body")?.textContent).toBe("529 · API Error: Overloaded");
  });
});

describe("记录 ⇒ 卡：只摆后端那一格", () => {
  const at = "2026-01-01T14:02:00.000Z";
  it("★ 报错回复的卡头 · 重试那一条的那一行 · 来话条抬头都照记录里写好的", () => {
    const err = renderMessage({ agent: "claude", t: "reply", id: "e", at, timeText: "14:02", blocks: [{ type: "text", text: "boom" }], autoReply: false, draw: "card", human: false, error: { reason: "quota", status: 429, title: "CARD-HEAD" } } as never as LineRecord, ctx());
    if (err.kind !== "card") throw new Error(err.kind);
    expect(err.element.querySelector(".api-error-label")?.textContent).toBe("CARD-HEAD");

    const retry = renderMessage({ agent: "claude", t: "retry", id: "r", at, timeText: "14:02", reason: "overloaded", attempt: 2, max: 10, text: "REC-LINE", draw: "card", human: false } as never as LineRecord, ctx());
    if (retry.kind !== "card") throw new Error(retry.kind);
    expect(retry.element.textContent).toBe("REC-LINE");

    const peer = renderMessage({ agent: "claude", t: "said", id: "p", at, timeText: "14:02", blocks: [], who: { speaker: { kind: "peerSession", from: "s2", title: "PEER-HEAD" }, text: "" }, draw: "card", human: false } as never as LineRecord, ctx());
    if (peer.kind !== "card") throw new Error(peer.kind);
    expect(peer.element.textContent).toContain("PEER-HEAD");
    expect(peer.element.textContent).not.toContain("s2");
  });
});
