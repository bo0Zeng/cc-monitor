/**
 * 结束会话只有一个确认框（A2-17-12-3）：右键 · 多选 · 终端名单 · 起会话占位交出去的都是 `endConfirm` 那一份；
 * 结局一种说法（一条点名那一条 · 几条「已结束 k · 失败 ×m」＋［查看］逐条）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn(), recentToasts: vi.fn(() => [{ title: "最新一条" }]) }));
vi.mock("../../../src/frontend/ui/status-messages", () => ({ showMessage: vi.fn() }));
vi.mock("../../../src/frontend/ui/terminal-reads", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/terminal-reads")>()),
  endTerminal: vi.fn(async () => undefined),
}));

import { endConfirm, sayEnded } from "../../../src/frontend/ui/end-confirm";
import { endRows } from "../../../src/frontend/ui/terminals-board-reads";
import { endTerminal } from "../../../src/frontend/ui/terminal-reads";
import { toast } from "../../../src/frontend/ui/kit/toast";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { BoardRow } from "../../../src/frontend/ui/generated/BoardRow";

const row = (key: string, title: string): BoardRow => ({ key, terminal: `h-${key}`, title }) as unknown as BoardRow;

beforeEach(() => vi.clearAllMocks());

describe("结束会话的确认框", () => {
  it("★ 标题 · 按钮写个数，列出要结束的那几条，写会中断 · 会保留什么", () => {
    expect(endConfirm(["甲", "乙"], "另 ×1 已结束 · 跳过")).toEqual({
      title: copyText("tabBatch.stop.title", { n: 2 }),
      action: copyText("tabBatch.stop.action", { n: 2 }),
      danger: true,
      rows: [
        { label: copyText("kit.interrupts.cut"), items: [copyText("tabBatch.stop.cuts")] },
        { label: copyText("kit.interrupts.keep"), items: [copyText("tabSessionActions.kill.keeps")] },
      ],
      list: ["甲", "乙"],
      note: "另 ×1 已结束 · 跳过",
    });
  });

  it("★ 终端名单「结束所选」开的就是这个框；结局照一批那一种说", async () => {
    const ask = vi.fn(async () => true);
    await endRows([{ origin: "devbox", row: row("a", "甲") }, { origin: "devbox", row: row("b", "乙") }], ask);
    expect(ask.mock.calls).toEqual([[endConfirm(["甲", "乙"])]]);
    expect(vi.mocked(endTerminal).mock.calls).toEqual([
      ["devbox", { terminal: "h-a" }, "甲"],
      ["devbox", { terminal: "h-b" }, "乙"],
    ]);
    expect(vi.mocked(toast).mock.calls.at(-1)?.[0]).toBe(copyText("sessionState.batch.done", { done: 2 }));
  });

  it("终端名单：一行没结束 ⇒ 汇总写失败几个，逐条写那台那一句；不确认 ⇒ 一行不动", async () => {
    vi.mocked(endTerminal).mockRejectedValueOnce(new Error("不在名单"));
    await endRows([{ origin: "devbox", row: row("a", "甲") }, { origin: "devbox", row: row("b", "乙") }], async () => true);
    const [head, , opts] = vi.mocked(toast).mock.calls.at(-1) as [string, string, { more?: string[] }];
    expect(head).toBe(copyText("sessionState.batch.someFailed", { done: 1, failed: 1 }));
    expect(opts.more).toEqual([copyText("tabBatch.result.failedLine", { title: "甲", why: "不在名单" })]);
    vi.mocked(endTerminal).mockClear();
    await endRows([{ origin: "devbox", row: row("a", "甲") }], async () => false);
    expect(endTerminal).not.toHaveBeenCalled();
  });

  it("只有一条 ⇒ 点名那一条（成 · 没成）", () => {
    sayEnded([{ title: "甲", outcome: "done", why: "" }]);
    expect(vi.mocked(toast).mock.calls.at(-1)?.[0]).toBe(copyText("sessionState.kill.done", { title: "甲" }));
    sayEnded([{ title: "甲", outcome: "failed", why: "那台说的", detail: "d" }]);
    expect(vi.mocked(toast).mock.calls.at(-1)).toEqual([copyText("tabSessionActions.kill.failed", { title: "甲" }), "那台说的", { detail: "d" }]);
  });
});
