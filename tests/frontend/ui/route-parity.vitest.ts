/**
 * `routeMeta` 路由表单测（「收集路由 parity」）：收纳（不建卡）与渲染两条路共用这一份。
 * 标题记录（`t: "title"`）⇒ consumed（不建卡；叫会话什么是核心的账）；其余各类 ⇒ content（排队那一句的打字时刻、主线外清单都是后端给的，这里不配、不喂）。
 */
import { describe, it, expect } from "vitest";
import { routeMeta } from "../../../src/frontend/ui/render-stream-record";
import { renderMessage } from "../../../src/frontend/ui/cards";
import type { JsonlLinePayload } from "../../../src/frontend/ui/generated/JsonlLinePayload";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";

function mk(record: Record<string, unknown>): JsonlLinePayload {
  return { session_id: "s", cwd: null, path: "/p", seq: 1, record: { agent: "claude", id: "r1", ...record } } as unknown as JsonlLinePayload;
}

describe("routeMeta 路由表", () => {
  it("标题（代理起的 · 人改的）→ consumed", () => {
    expect(routeMeta(mk({ t: "title", text: "甲", by: "agent" }))).toBe("consumed");
    expect(routeMeta(mk({ t: "title", text: "乙", by: "user" }))).toBe("consumed");
  });

  it("said / reply / retry / queued → content", () => {
    for (const t of ["said", "reply", "retry", "queued"]) expect(routeMeta(mk({ t })), t).toBe("content");
  });
});

// 排队那一句（插进正在跑那一轮的一句）：后端只给人说的那一支、时刻已是打字时刻 ⇒ 界面照画，不配对、不改时刻。
describe("排队那一句照后端给的画", () => {
  const ctx = () => ({ parentPath: "/p", origin: LOCAL_ORIGIN, toolUseNames: new Map(), toolUseElements: new Map(), pendingToolResults: new Map() });
  const queued = (kind: string, text: string) =>
    ({ agent: "claude", t: "queued", id: "@9", at: "2026-08-12T09:51:06.664Z", timeText: "09:51", who: { speaker: { kind }, text } }) as never;

  it("人说的 ⇒ 排队卡，卡头时刻就是记录的 timeText", () => {
    const r = renderMessage(queued("human", "现在的计划还是围绕 Windows 前端对吧?"), ctx());
    if (r.kind !== "card") throw new Error(r.kind);
    expect(r.element.classList.contains("card-user-queued")).toBe(true);
    expect(r.element.querySelector(".card-header .ts")?.textContent).toBe("09:51");
  });

  it("不是人说的 / 没有正文 ⇒ 不建卡", () => {
    expect(renderMessage(queued("system", "x"), ctx()).kind).toBe("skip");
    expect(renderMessage(queued("human", ""), ctx()).kind).toBe("skip");
  });
});
