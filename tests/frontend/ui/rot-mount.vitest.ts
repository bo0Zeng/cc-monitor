/**
 * 轮换四块的接线（`rot-mount.ts`）：手势翻成哪条 typed 命令、发给谁。会话面板（目标 `{sid}`）与设置窗（目标 `{strategy, settings}`）
 * 只差这几条。读写都经 `rot-reads.ts`（这里桩住）。期望手写。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const calls: { name: string; req: unknown }[] = [];
const refuse = new Map<string, string>();
vi.mock("../../../src/frontend/ui/rot-reads", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/rot-reads")>()),
  readStrategies: vi.fn(async () => LIST),
  readView: vi.fn(async (_o: string, target: unknown) => {
    calls.push({ name: "rotation-view", req: target });
    return VIEW;
  }),
  rotCall: vi.fn(async (_o: string, name: string, req: unknown) => {
    calls.push({ name, req });
    const code = refuse.get(name);
    if (code) {
      refuse.delete(name);
      throw Object.assign(new Error(code), { code });
    }
    return {};
  }),
  refusedCode: (e: unknown) => ((e as { code?: string }).code ? { code: (e as { code: string }).code } : null),
}));

import { mountRotation } from "../../../src/frontend/ui/rot-mount";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { RotationStrategiesReply } from "../../../src/frontend/ui/generated/RotationStrategiesReply";
import type { RotationViewReply } from "../../../src/frontend/ui/generated/RotationViewReply";

const LIST: RotationStrategiesReply = { strategies: [{ id: "按权重轮换", description: "说明", rev: "h:1", inUse: [], forNew: true, wordsBad: false, tzUnknown: false }], forNew: "按权重轮换", oldScripts: 0 };
const VIEW: RotationViewReply = {
  now: 1,
  nowText: "00:00",
  strategy: LIST.strategies[0],
  form: [{ key: "close", kind: "percent", scope: "all", min: 0, max: 100, default: 25, label: "夹具·容差" }],
  settings: { accounts: [{ id: "work" }], close: 25 },
  dirty: true,
  start: "work",
  current: "team",
  accounts: [{ account: "work", kind: "sub", w5h: { open: true, used: 99 }, w7d: { open: true, used: 1 }, login: "ok", blocked: { why: "used", text: "后端写的那一句" } }],
  listed: 1,
  decision: null,
  timeline: null,
  log: [],
  hints: [],
};

const settle = async (): Promise<void> => {
  for (let i = 0; i < 6; i++) await Promise.resolve();
};
const btn = (root: HTMLElement, text: string): HTMLButtonElement => [...root.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === text)!;
const editClose = (root: HTMLElement, v: string): void => {
  const input = root.querySelector<HTMLInputElement>('input[aria-label="夹具·容差"]')!;
  input.value = v;
  input.dispatchEvent(new Event("change"));
};

beforeEach(() => {
  calls.length = 0;
  refuse.clear();
});

describe("会话面板（目标 {sid}）", () => {
  it("★ 改一格 ⇒ rotation-settings（整份设置 ＋ 读到的版本）；存 ⇒ rotation-save；还原 ⇒ rotation-revert", async () => {
    const m = mountRotation({ origin: "<local>", sid: "s1" });
    await m.refresh();
    editClose(m.root, "30");
    await settle();
    expect(calls.find((c) => c.name === "rotation-settings")?.req).toEqual({ target: { sid: "s1" }, settings: { accounts: [{ id: "work" }], close: 30 }, ifRev: "h:1" });
    btn(m.root, copyText("rot.top.save")).click();
    await settle();
    expect(calls.find((c) => c.name === "rotation-save")?.req).toEqual({ target: { sid: "s1" }, ifRev: "h:1" });
    btn(m.root, copyText("rot.top.revert")).click();
    await settle();
    expect(calls.find((c) => c.name === "rotation-revert")?.req).toEqual({ sid: "s1" });
  });

  it("★ 换一份策略碰上没存的改动（dirty）⇒ 问一句，答了才带 discard 再换", async () => {
    const confirm = vi.fn(async () => true);
    const m = mountRotation({ origin: "<local>", sid: "s1", confirm });
    await m.refresh();
    refuse.set("rotation-use", "dirty");
    // 下拉（kit 的 select：点开浮层、点一项）选「不轮换」。
    m.root.querySelector<HTMLButtonElement>(`button[aria-label="${copyText("rot.top.pick")}"]`)!.click();
    const item = [...document.querySelectorAll<HTMLElement>('[role="menuitem"], [role="menuitemradio"], [role="option"]')].find((e) => e.textContent?.includes(copyText("rot.none.label")))!;
    item.click();
    const uses = (): unknown[] => calls.filter((c) => c.name === "rotation-use").map((c) => c.req);
    await settle();
    expect(confirm).toHaveBeenCalledOnce();
    expect(uses()).toEqual([
      { sid: "s1", strategy: null, discard: false },
      { sid: "s1", strategy: null, discard: true },
    ]);
  });

  it("★ 「现在用」碰上用不了（unusable）⇒ 不换，号下面写「用不了」那一句（为什么照后端写的那一句）", async () => {
    const m = mountRotation({ origin: "<local>", sid: "s1" });
    await m.refresh();
    const card = m.root.querySelector<HTMLElement>('button[data-account="work"]')!;
    card.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
    document.dispatchEvent(new MouseEvent("pointerup", { bubbles: true }));
    refuse.set("rotation-use-now", "unusable");
    btn(m.root, copyText("rot.acct.useNow", { account: "work" })).click();
    await settle();
    expect(calls.find((c) => c.name === "rotation-use-now")?.req).toEqual({ sid: "s1", account: "work" });
    expect(m.root.textContent).toContain(copyText("rot.acct.refused", { account: "work", why: "后端写的那一句" }));
  });
});

describe("设置窗（目标 {strategy, settings}）", () => {
  it("★ 改一格只改手上的草稿、照它重推（rotation-view 带 settings），不写盘；存 ⇒ rotation-save 带草稿", async () => {
    const m = mountRotation({ origin: "<local>", sid: null, machine: "本机" });
    await m.refresh();
    expect(calls.at(-1)).toEqual({ name: "rotation-view", req: { strategy: "按权重轮换" } });
    editClose(m.root, "40");
    await settle();
    expect(calls.some((c) => c.name === "rotation-settings")).toBe(false);
    expect(calls.at(-1)).toEqual({ name: "rotation-view", req: { strategy: "按权重轮换", settings: { accounts: [{ id: "work" }], close: 40 } } });
    btn(m.root, copyText("rot.top.save")).click();
    await settle();
    expect(calls.find((c) => c.name === "rotation-save")?.req).toEqual({ target: { strategy: "按权重轮换" }, settings: { accounts: [{ id: "work" }], close: 40 }, ifRev: "h:1" });
  });
});
