/**
 * 结束会话只有一个确认框（A2-17-12-3）：右键 · 多选 · 终端名单 · 起会话占位交出去的都是 `endConfirm` 那一份；
 * 结局一种说法（一条点名那一条 · 几条「已结束 k · 失败 ×m」＋［查看］逐条）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn(), undoToast: vi.fn(), recentToasts: vi.fn(() => [{ title: "最新一条" }]) }));
vi.mock("../../../src/frontend/ui/status-messages", () => ({ showMessage: vi.fn() }));
vi.mock("../../../src/frontend/ui/terminal-reads", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/terminal-reads")>()),
  endTerminal: vi.fn(async () => undefined),
}));

import { endConfirm, sayEnded, sayReply } from "../../../src/frontend/ui/end-confirm";
import { endRows } from "../../../src/frontend/ui/terminals-board-reads";
import { endTerminal } from "../../../src/frontend/ui/terminal-reads";
import { toast, undoToast } from "../../../src/frontend/ui/kit/toast";
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

describe("一批的回包带［撤销］（批量隐藏，A2-17-7-3-1）", () => {
  const reply = { results: [], said: "S-隐藏 2 项 · 完成 1 · 失败 ×1", failed: true, lines: ["L-乙"], copyDetail: "D-乙" };
  it("★ 给了撤销 ⇒ 同一条上［撤销］［查看］：标题是回包那一句，逐条进「消息」，不另出一条", () => {
    const run = vi.fn();
    const settle = vi.fn();
    sayReply(reply, { run, settle });
    expect(toast).not.toHaveBeenCalled();
    expect(undoToast).toHaveBeenCalledTimes(1);
    const [title, undo, commit, opts] = vi.mocked(undoToast).mock.calls[0];
    expect(title).toBe(reply.said);
    expect(undo).toBe(run);
    expect(commit).toBe(settle);
    expect(opts?.level).toBe("error");
    expect(opts?.more).toEqual(["L-乙"]);
    expect((opts?.action as { label: string } | undefined)?.label).toBe(copyText("tabBatch.result.view"));
    expect(opts?.detail).toBe("D-乙");
  });
  it("没给撤销 ⇒ 照旧那一条（不出［撤销］）", () => {
    sayReply(reply);
    expect(undoToast).not.toHaveBeenCalled();
    expect(toast).toHaveBeenCalledTimes(1);
  });
});

describe("一批会话的回包照摆（`sessions-batch`：那一句 · 逐条 · 复制详情由本机后端合好）", () => {
  const reply = { results: [], said: "S-合好", failed: true, lines: ["L-一", "L-二"], copyDetail: "D-详" };
  it("★ toast 标题就是 said、more 就是 lines、详情就是 copyDetail；有没成的 ⇒ 出错的样子、带［查看］", () => {
    sayReply(reply);
    expect(toast).toHaveBeenCalledTimes(1);
    const [title, body, opts] = vi.mocked(toast).mock.calls[0];
    expect(title).toBe("S-合好");
    expect(body).toBe("");
    expect(opts?.level).toBe("error");
    expect(opts?.more).toEqual(["L-一", "L-二"]);
    expect(opts?.detail).toBe("D-详");
    expect((opts?.action as { label: string } | undefined)?.label).toBe(copyText("tabBatch.result.view"));
  });
  it("都成了 ⇒ 成功的样子、没有［查看］", () => {
    sayReply({ ...reply, failed: false, lines: [], copyDetail: "" });
    const [, , opts] = vi.mocked(toast).mock.calls[0];
    expect(opts?.level).toBe("success");
    expect(opts?.action).toBeUndefined();
  });
});
