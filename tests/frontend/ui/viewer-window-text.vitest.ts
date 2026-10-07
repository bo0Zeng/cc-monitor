/**
 * 独立查看窗的两句字：状态栏 `只读 · {n} 条 · 实时`（只在真订着流、在跑时）/ `只读 · {n} 条 · 已结束`；
 * 系统标题 `{会话标题} · {状态} · {项目}`，远端再加 `· {机器}`，状态说不清写「状态不明」。
 */
import { describe, it, expect } from "vitest";
import { windowStatus, windowTitle } from "../../../src/frontend/ui/viewer-window-text";
import type { HistoryRow } from "../../../src/frontend/ui/history-list-reads";
import { copyText } from "../../../src/frontend/ui/copy-table";

const row = (over: Partial<HistoryRow> = {}): HistoryRow =>
  ({ label: "支付回调验签", untitled: false, projectName: "orders", ...over }) as HistoryRow;

describe("独立查看窗的字", () => {
  it("状态栏：在跑且订着 ⇒ 实时；在跑但那台看不见 ⇒ 只说条数；结束 ⇒ 已结束", () => {
    expect(windowStatus({ n: 42, more: false, live: true, following: true })).toBe(copyText("viewerWindow.status.live", { n: "42" }));
    expect(windowStatus({ n: 42, more: true, live: true, following: false })).toBe(copyText("viewerWindow.status.plain", { n: "42" }));
    expect(windowStatus({ n: 88, more: false, live: false, following: true })).toBe(copyText("viewerWindow.status.ended", { n: "88", state: copyText("sessionState.ended.name") }));
  });

  it("系统标题：标题 · 状态 · 项目；远端加机器；没说过话的会话用兜底名", () => {
    expect(windowTitle(row(), true)).toBe(copyText("viewerWindow.title.local", { title: "支付回调验签", state: copyText("history.row.live"), project: "orders" }));
    expect(windowTitle(row({ origin: "devbox" }), false)).toBe(copyText("viewerWindow.title.remote", { title: "支付回调验签", state: copyText("sessionState.ended.name"), project: "orders", machine: "devbox" }));
    expect(windowTitle(row({ untitled: true }), null)).toBe(
      copyText("viewerWindow.title.local", { title: copyText("history.row.untitled"), state: copyText("sessionState.unseen.name"), project: "orders" }),
    );
  });
});
