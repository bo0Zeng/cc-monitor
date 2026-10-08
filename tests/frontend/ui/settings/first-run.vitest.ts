/**
 * 首次运行「开始用」：那份数（`readiness`）严格收、问的时候带上机器表台数；设置窗那一块照它画（哪步做了 · 跳过过没有都是后端给的）；
 * 状态栏那一枚只数必做的步，跳过过就不出。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { calls, answer, disk } = vi.hoisted(() => ({
  calls: [] as Array<{ origin: string; op: string; args: Record<string, unknown> }>,
  answer: { value: null as unknown },
  disk: { cfg: {} as Record<string, unknown> },
}));
vi.mock("../../../../src/comms/inward/chan", () => ({
  ChanError: class extends Error {},
  chan: {
    call: (origin: string, op: string, body: Uint8Array) => {
      const args = JSON.parse(new TextDecoder().decode(body)) as Record<string, unknown>;
      calls.push({ origin, op, args });
      const v = op === "chores-mark" ? { declined: [], selfPaste: null, startSkipped: true } : answer.value;
      return Promise.resolve(new TextEncoder().encode(JSON.stringify(v)));
    },
  },
}));
vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({ commands: { load_config: () => Promise.resolve(structuredClone(disk.cfg)) } }));

import { decodeReadiness, readReadiness } from "../../../../src/frontend/ui/settings/readiness-reads";
import { FirstRun } from "../../../../src/frontend/ui/settings/first-run";
import { StatusStart } from "../../../../src/frontend/ui/status-start";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const settle = async () => {
  for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
};
const reply = (done: [boolean, boolean, boolean], skipped = false) => ({
  steps: [
    { id: "terminal", done: done[0], required: true },
    { id: "named", done: done[1], required: false },
    { id: "remote", done: done[2], required: false },
  ],
  left: done[0] ? 0 : 1,
  skipped,
});

beforeEach(() => {
  calls.length = 0;
  disk.cfg = { remote: { hosts: [{ label: "devbox", host: "d", user: "u", port: 22 }] } };
});

describe("那份数", () => {
  it("★ 问的时候带上机器表台数；严格收（多一格 / 少一格 / 步名不在闭集 ⇒ 收不下）", async () => {
    answer.value = reply([false, false, true]);
    expect((await readReadiness())?.left).toBe(1);
    expect(calls).toEqual([{ origin: "<local>", op: "readiness", args: { remotes: 1 } }]);
    expect(decodeReadiness({ ...reply([true, true, true]), extra: 1 })).toBeNull();
    const { skipped: _s, ...noSkip } = reply([true, true, true]);
    expect(decodeReadiness(noSkip)).toBeNull();
    expect(decodeReadiness({ ...reply([true, true, true]), steps: [{ id: "other", done: true, required: true }] })).toBeNull();
  });
});

describe("设置窗「开始用」那一块", () => {
  it("★ 照后端画：做了的打勾、不给按钮；没做的给修法；［接上］去本机别名页那一项；［添加机器］开添加框", async () => {
    answer.value = reply([false, true, false]);
    const went: unknown[] = [];
    const add = vi.fn();
    const fr = new FirstRun({ go: (t) => went.push(t), addMachine: add });
    await fr.loadNow();
    expect(fr.element.hidden).toBe(false);
    const rows = [...fr.element.querySelectorAll<HTMLElement>(".first-run-row")];
    expect(rows.map((r) => [r.dataset.step, r.dataset.done])).toEqual([
      ["terminal", "false"],
      ["named", "true"],
      ["remote", "false"],
    ]);
    expect(rows[1].querySelector("button")).toBeNull();
    rows[0].querySelector<HTMLButtonElement>('[data-action="terminal"]')!.click();
    rows[2].querySelector<HTMLButtonElement>('[data-action="remote"]')!.click();
    expect(went).toEqual([{ machine: "<local>", tab: "config", anchor: "connect-terminal" }]);
    expect(add).toHaveBeenCalledTimes(1);
  });

  it("★ 三步都做了 / 后端说跳过过 ⇒ 不出现；点［跳过］⇒ 交本机后端记下（chores-mark skipStart）再重读", async () => {
    answer.value = reply([true, true, true]);
    const fr = new FirstRun({ go: () => {}, addMachine: () => {} });
    await fr.loadNow();
    expect(fr.element.hidden).toBe(true);
    answer.value = reply([false, false, false]);
    await fr.loadNow();
    expect(fr.element.hidden).toBe(false);
    calls.length = 0;
    answer.value = reply([false, false, false], true);
    fr.element.querySelector<HTMLButtonElement>('[data-action="skip"]')!.click();
    await settle();
    expect(calls.map((c) => [c.origin, c.op, c.args])).toEqual([
      ["<local>", "chores-mark", { op: "skipStart" }],
      ["<local>", "readiness", { remotes: 1 }],
    ]);
    expect(fr.element.hidden).toBe(true);
  });
});

describe("状态栏那一枚", () => {
  it("★ 只数必做的步：必做的做完了（可选的没做）⇒ 不出；必做没做 ⇒「开始用 · 剩 N 步」；跳过过 ⇒ 不出；点了开设置窗那一块", async () => {
    const open = vi.fn();
    let got: { left: number; skipped: boolean } | null = { left: 0, skipped: false };
    const st = new StatusStart({ read: async () => got, open });
    await st.refresh();
    expect(st.el.textContent).toBe("");
    got = { left: 1, skipped: false };
    await st.refresh();
    expect(st.el.textContent).toBe(copyText("statusBar.start.label", { n: 1 }));
    st.el.querySelector<HTMLElement>("button, [role=button], .chip, *")!.click();
    expect(open).toHaveBeenCalled();
    got = { left: 1, skipped: true };
    await st.refresh();
    expect(st.el.textContent).toBe("");
    got = null;
    await st.refresh();
    expect(st.el.textContent).toBe("");
  });
});
