/**
 * 「现在重启」：先问本机与各台会打断什么（`machine-interrupts` 带 `appExit`，各台照它自己的退出行为答）；
 * 什么都不断 ⇒ 不弹框、直接重启；有 ⇒ 列「中断 · 保留」、默认焦点在［取消］的那种框，取消就不重启。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { calls, answers, toasts } = vi.hoisted(() => ({
  calls: [] as Array<{ origin: string; op: string; args: Record<string, unknown> }>,
  answers: new Map<string, Record<string, unknown> | null>(),
  toasts: [] as string[],
}));
vi.mock("../../../../src/comms/inward/chan", () => ({
  ChanError: class extends Error {},
  chan: {
    call: (origin: string, op: string, body: Uint8Array) => {
      const args = JSON.parse(new TextDecoder().decode(body)) as Record<string, unknown>;
      calls.push({ origin, op, args });
      const a = answers.get(origin);
      if (!a) return Promise.reject(new Error(`${origin} 断开`));
      return Promise.resolve(new TextEncoder().encode(JSON.stringify(a)));
    },
  },
}));
const { filewin } = vi.hoisted(() => ({ filewin: { now: { windows: 1, unsaved: 1, running: 2 } as Record<string, number> | null } }));
vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: {
    filewin_interrupts: () => (filewin.now ? Promise.resolve(filewin.now) : Promise.reject(new Error("壳没答"))),
  },
}));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: (t: string) => toasts.push(t) }));

import { askAppExitInterruptRows } from "../../../../src/frontend/ui/settings/interrupts";
import { restartNow, restartNowButton } from "../../../../src/frontend/ui/settings/restart-now";
import { copyText } from "../../../../src/frontend/ui/copy-table";

/** 后端排好的两行（字由后端写；这里只核界面照摆、不另拼）。 */
const ROWS = [
  { label: "中断（后端写的）", items: ["会话请求 2（后端写的）"] },
  { label: "保留（后端写的）", items: ["会话照跑（后端写的）"] },
];
const reply = (rows: typeof ROWS) => ({ relayedSessions: 2, relayedMaybe: 0, liveStreams: 4, forwards: 2, rows });

beforeEach(() => {
  calls.length = 0;
  answers.clear();
  toasts.length = 0;
});

describe("现在重启 · 会打断什么", () => {
  it("★ 只问本机后端一条（appExit · fleet · act=restart · 文件窗口那两个数），画的就是回包里那两行；壳答不出文件窗口 ⇒ 不带那一格；本机后端问不到 ⇒ 只那一句", async () => {
    answers.set("<local>", reply(ROWS) as never);
    const got = await askAppExitInterruptRows();
    expect(calls).toEqual([
      { origin: "<local>", op: "machine-interrupts", args: { appExit: true, fleet: true, act: "restart", files: { unsaved: 1, running: 2 } } },
    ]);
    expect(got).toEqual(ROWS);
    filewin.now = null;
    calls.length = 0;
    await askAppExitInterruptRows();
    expect(calls[0]!.args).toEqual({ appExit: true, fleet: true, act: "restart" });
    filewin.now = { windows: 1, unsaved: 1, running: 2 };
    answers.delete("<local>");
    expect(await askAppExitInterruptRows()).toEqual([{ label: copyText("quitNow.dialog.unknown"), items: [] }]);
  });

  it("★ 什么都不断 ⇒ 不弹框，toast 一句、直接重启", async () => {
    const confirm = vi.fn(async () => true);
    const restart = vi.fn(async () => {});
    expect(await restartNow({ ask: async () => [], confirm, restart })).toBe(true);
    expect(confirm).not.toHaveBeenCalled();
    expect(restart).toHaveBeenCalledTimes(1);
    expect(toasts).toEqual([copyText("restartNow.run.toast")]);
  });

  it("★ 有要断的 ⇒ 弹框（不是红色危险键）、行就是后端那两行；取消 ⇒ 不重启", async () => {
    const confirm = vi.fn(async (_spec: unknown) => false);
    const restart = vi.fn(async () => {});
    const ok = await restartNow({ ask: async () => ROWS, confirm, restart });
    expect(ok).toBe(false);
    expect(restart).not.toHaveBeenCalled();
    const spec = confirm.mock.calls[0]![0] as unknown as { title: string; danger: boolean; rows: typeof ROWS };
    expect(spec.title).toBe(copyText("restartNow.dialog.title"));
    expect(spec.danger).toBe(false);
    expect(spec.rows).toEqual(ROWS);
  });

  it("按钮：点了走同一条，做完前不能连点", async () => {
    let release!: () => void;
    const restart = vi.fn(() => new Promise<void>((r) => (release = r)));
    const b = restartNowButton({ ask: async () => [], restart });
    expect(b.textContent).toBe(copyText("restartNow.bar.action"));
    b.click();
    expect(b.disabled).toBe(true);
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
    expect(restart).toHaveBeenCalledTimes(1);
    release();
    for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
    expect(b.disabled).toBe(false);
  });
});
