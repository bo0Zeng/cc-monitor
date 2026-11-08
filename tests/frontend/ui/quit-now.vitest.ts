/**
 * 退出 cc-monitor（关主窗、而它是最后一个会吊住进程的窗口）：先问会打断什么（与「现在重启」同一问、同一种框），
 * 有才弹；取消 ⇒ 不退；还开着别的查看窗（进程不退）⇒ 不问、直接关。
 */
import { describe, it, expect, vi } from "vitest";
import { quitNow } from "../../../src/frontend/ui/quit-now";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { Interrupts } from "../../../src/frontend/ui/settings/interrupts";

const zero: Interrupts = { relayedSessions: 0, relayedMaybe: 0, liveStreams: 0, forwards: 0, unsaved: 0, filewinRunning: 0 };

describe("退出前问会打断什么", () => {
  it("★ 文件窗口有一份没存的改动 ⇒ 退出先出那一问；取消就不退", async () => {
    const confirm = vi.fn(async (_spec: unknown) => false);
    const ok = await quitNow({ ask: async () => ({ ...zero, unsaved: 1 }), confirm, exits: async () => true });
    expect(ok).toBe(false);
    const spec = confirm.mock.calls[0]![0] as { title: string; rows: Array<{ items: string[] }> };
    expect(spec.title).toBe(copyText("quitNow.dialog.title"));
    expect(spec.rows[0]!.items).toContain(copyText("interrupts.item.unsaved", { n: 1 }));
  });

  it("★ 什么都不断 ⇒ 不弹、直接退", async () => {
    const confirm = vi.fn(async () => true);
    expect(await quitNow({ ask: async () => ({ ...zero }), confirm, exits: async () => true })).toBe(true);
    expect(confirm).not.toHaveBeenCalled();
  });

  it("关的不是最后一个窗口（进程不退）⇒ 不问", async () => {
    const ask = vi.fn(async () => ({ ...zero, unsaved: 3 }));
    const confirm = vi.fn(async () => false);
    expect(await quitNow({ ask, confirm, exits: async () => false })).toBe(true);
    expect(ask).not.toHaveBeenCalled();
    expect(confirm).not.toHaveBeenCalled();
  });
});
