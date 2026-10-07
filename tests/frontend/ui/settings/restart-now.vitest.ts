/**
 * 「现在重启」：先问本机与各台会打断什么（`machine-interrupts` 带 `appExit`，各台照它自己的退出行为答）；
 * 什么都不断 ⇒ 不弹框、直接重启；有 ⇒ 列「中断 · 保留」、默认焦点在［取消］的那种框，取消就不重启。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { calls, answers, toasts } = vi.hoisted(() => ({
  calls: [] as Array<{ origin: string; op: string; args: Record<string, unknown> }>,
  answers: new Map<string, Record<string, number> | null>(),
  toasts: [] as string[],
}));
vi.mock("../../../../src/comms/inward/chan", () => ({
  ChanError: class extends Error {},
  chan: {
    call: (origin: string, op: string, body: Uint8Array) => {
      const args = JSON.parse(new TextDecoder().decode(body)) as Record<string, unknown>;
      calls.push({ origin, op, args });
      const a = answers.get(origin);
      if (!a) return Promise.reject(new Error(`${origin} 连不上`));
      return Promise.resolve(new TextEncoder().encode(JSON.stringify(a)));
    },
  },
}));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: (t: string) => toasts.push(t) }));

import { askAppExitInterrupts } from "../../../../src/frontend/ui/settings/interrupts";
import { restartNow, restartNowButton } from "../../../../src/frontend/ui/settings/restart-now";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const zero = { relayedSessions: 0, relayedMaybe: 0, liveStreams: 0, forwards: 0 };

beforeEach(() => {
  calls.length = 0;
  answers.clear();
  toasts.length = 0;
});

describe("现在重启 · 会打断什么", () => {
  it("★ 问本机与各台都带 appExit；加起来；远端问不到 ＝ 本来就没连着、不算；本机问不到 ⇒ 说数不出", async () => {
    answers.set("<local>", { ...zero, liveStreams: 1, forwards: 2 });
    answers.set("devbox", { ...zero, relayedSessions: 2, liveStreams: 3 });
    const got = await askAppExitInterrupts(["devbox", "gpu"]);
    expect(calls.every((c) => c.op === "machine-interrupts" && c.args.appExit === true)).toBe(true);
    expect(calls.map((c) => c.origin).sort()).toEqual(["<local>", "devbox", "gpu"]);
    expect(got).toEqual({ relayedSessions: 2, relayedMaybe: 0, liveStreams: 4, forwards: 2 });
    answers.delete("<local>");
    const blind = await askAppExitInterrupts([]);
    expect(blind.relayedSessions).toBeNull();
    expect(blind.forwards).toBeNull();
  });

  it("★ 什么都不断 ⇒ 不弹框，toast 一句、直接重启", async () => {
    const confirm = vi.fn(async () => true);
    const restart = vi.fn(async () => {});
    expect(await restartNow({ ask: async () => ({ ...zero }), confirm, restart, remotes: async () => [] })).toBe(true);
    expect(confirm).not.toHaveBeenCalled();
    expect(restart).toHaveBeenCalledTimes(1);
    expect(toasts).toEqual([copyText("restartNow.run.toast")]);
  });

  it("★ 有要断的 ⇒ 弹「中断 · 保留」框（不是红色危险键）；取消 ⇒ 不重启", async () => {
    const confirm = vi.fn(async (_spec: unknown) => false);
    const restart = vi.fn(async () => {});
    const ok = await restartNow({ ask: async () => ({ ...zero, relayedSessions: 1, liveStreams: 1 }), confirm, restart, remotes: async () => ["devbox"] });
    expect(ok).toBe(false);
    expect(restart).not.toHaveBeenCalled();
    const spec = confirm.mock.calls[0]![0] as unknown as { title: string; danger: boolean; rows: Array<{ label: string; items: string[] }> };
    expect(spec.title).toBe(copyText("restartNow.dialog.title"));
    expect(spec.danger).toBe(false);
    expect(spec.rows[0]!.label).toBe(copyText("interrupts.row.brief"));
    expect(spec.rows[0]!.items).toContain(copyText("interrupts.item.relayed", { n: 1 }));
  });

  it("按钮：点了走同一条，做完前不能连点", async () => {
    let release!: () => void;
    const restart = vi.fn(() => new Promise<void>((r) => (release = r)));
    const b = restartNowButton({ ask: async () => ({ ...zero }), restart, remotes: async () => [] });
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
