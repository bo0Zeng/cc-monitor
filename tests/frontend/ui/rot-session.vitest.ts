/**
 * 会话账号面板中间那一块（`rot-session.ts`，目标 `{sid}`）：提示 · 策略一行 · 说明 · 顺序；手势翻成哪条 typed 命令。
 * 与设置页只共用策略一行与提示：这里没有号牌 · 总体设置 · 模拟。读写都经 `rot-reads.ts` · `chan-caller`（这里桩住）。期望手写。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ChanError } from "../../../src/comms/inward/chan";

const calls: { name: string; req: unknown }[] = [];
const settingsAt: unknown[] = [];
vi.mock("../../../src/frontend/ui/open-settings", () => ({
  openSettingsWindow: vi.fn(async (_t: unknown, at: unknown) => {
    settingsAt.push(at);
  }),
}));
const refuse = new Map<string, string>();
vi.mock("../../../src/frontend/ui/rot-reads", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/rot-reads")>()),
  readStrategies: vi.fn(async () => LIST),
  readView: vi.fn(async (_o: string, target: unknown, hours: unknown) => {
    calls.push({ name: "rotation-view", req: hours === undefined ? target : { target, hours } });
    return shown.view;
  }),
}));
vi.mock("../../../src/frontend/ui/ipc/chan-caller", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/ipc/chan-caller")>()),
  callTyped: vi.fn(async (_o: string, name: string, req: unknown) => {
    calls.push({ name, req });
    const code = refuse.get(name);
    if (code) {
      refuse.delete(name);
      throw new ChanError({ layer: "peer", why: "refused", body: new TextEncoder().encode(JSON.stringify({ code, message: `那台说：${code}` })) }, `那台说：${code}`);
    }
    return {};
  }),
}));

import { mountSessionRotation } from "../../../src/frontend/ui/rot-session";
import { rotAnchor } from "../../../src/frontend/ui/rot-mount";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { RotationStrategiesReply } from "../../../src/frontend/ui/generated/RotationStrategiesReply";
import type { RotationViewReply } from "../../../src/frontend/ui/generated/RotationViewReply";

const LIST: RotationStrategiesReply = { strategies: [{ id: "按权重轮换", description: "第一句说明\n后面还有一段", summary: "第一句说明", rev: "h:1", inUse: [], forNew: true, wordsBad: false, tzUnknown: false, hand: false }], forNew: "按权重轮换", oldScripts: 0 };
const VIEW: RotationViewReply = {
  now: 1,
  nowText: "00:00",
  title: "夹具会话",
  strategy: LIST.strategies[0],
  board: { all: [{ key: "close", kind: "percent", label: "夹具·容差", v: 25, num: 25, from: "own", min: 0, max: 100 }], rows: [] },
  start: "work",
  current: "team",
  lineup: [
    { account: "team", group: "use", state: "夹具·在用", movable: false },
    { account: "work", group: "next", state: "夹具·下一个", movable: true },
  ],
  manual: false,
  off: [],
  accounts: [{ account: "work", plate: { color: 0, label: "work", text: "wo" }, full: [], kind: "sub", w5h: { open: true, used: 99 }, w7d: { open: true, used: 1 }, login: "ok", blocked: { why: "used", text: "后端写的那一句" } }],
  listed: 1,
  decision: null,
  log: [],
  hints: [],
  can: { reorder: true, off: true, edit: true },
};
const shown = { view: VIEW };

const settle = async (): Promise<void> => {
  for (let i = 0; i < 8; i++) await Promise.resolve();
};
const btn = (root: HTMLElement, text: string): HTMLButtonElement => [...root.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === text)!;
const rows = (root: HTMLElement): HTMLElement[] => [...root.querySelectorAll<HTMLElement>("[data-rot-row]")];
const boxes = (root: HTMLElement): void =>
  rows(root).forEach((r, k) => (r.getBoundingClientRect = () => ({ left: 0, right: 300, top: k * 48, bottom: k * 48 + 40, width: 300, height: 40, x: 0, y: k * 48, toJSON: () => ({}) })));

beforeEach(() => {
  calls.length = 0;
  settingsAt.length = 0;
  refuse.clear();
  shown.view = VIEW;
});

describe("整屏：提示 · 策略一行 · 顺序（与设置页拆开）", () => {
  it("★ 读的是会话那一形（不带推几小时）；只有提示 · 策略一行 · 说明 · 顺序：没有号牌 · 总体设置 · 模拟", async () => {
    const m = mountSessionRotation("<local>", "s1");
    await m.refresh();
    expect(calls.find((c) => c.name === "rotation-view")?.req).toEqual({ sid: "s1" });
    expect(m.root.querySelector("[data-rot-order]")).not.toBeNull();
    expect(m.root.querySelector("[data-rot-card]"), "没有号牌").toBeNull();
    expect(m.root.textContent).not.toContain(copyText("rot.h.sim"));
    expect(m.root.textContent).not.toContain(copyText("rot.h.all"));
    expect(m.root.textContent).not.toContain("夹具·容差");
  });

  it("★ 说明一行截断（第一行），点「说明」展开全文、再点收起（只归界面，不交后端）", async () => {
    const m = mountSessionRotation("<local>", "s1");
    await m.refresh();
    expect(m.root.textContent).toContain("第一句说明");
    expect(m.root.textContent).not.toContain("后面还有一段");
    const n = calls.length;
    btn(m.root, copyText("rot.top.about")).click();
    expect(m.root.textContent).toContain("后面还有一段");
    btn(m.root, copyText("rot.top.aboutClose")).click();
    expect(m.root.textContent).not.toContain("后面还有一段");
    expect(calls.length).toBe(n);
  });

  it("★ 不轮换 ⇒ 策略一行 ＋ 那一句 ＋ 号那几行（不可拖、没有勾）；点一行 ⇒［立即用］⇒ rotation-use-now，小窗里没有［编辑策略］", async () => {
    shown.view = {
      ...VIEW,
      strategy: null,
      decision: null,
      listed: 0,
      can: { reorder: false, off: false, edit: false },
      lineup: [
        { account: "team", group: "use", state: "夹具·在用", movable: false },
        { account: "work", group: "next", state: "夹具·备着", movable: false },
      ],
    };
    const m = mountSessionRotation("<local>", "s1");
    document.body.replaceChildren(m.root);
    await m.refresh();
    expect(m.root.textContent).toContain(copyText("rot.none.desc"));
    expect(m.root.querySelector("[data-rot-order]")).not.toBeNull();
    expect(m.root.querySelector("[data-rot-handle]"), "不轮换没有把手").toBeNull();
    expect(m.root.querySelector('input[type="checkbox"]'), "不轮换没有「先不用」的勾").toBeNull();
    expect(m.root.textContent).not.toContain(copyText("rot.h.orderHint"));
    boxes(m.root);
    rows(m.root)[1].dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, button: 0, clientX: 10, clientY: 60 }));
    document.dispatchEvent(new MouseEvent("pointerup", { bubbles: true, clientX: 10, clientY: 60 }));
    const pop = document.querySelector<HTMLElement>("[data-rot-pop]")!;
    expect([...pop.querySelectorAll("button")].map((b) => b.textContent)).toEqual([copyText("rot.order.useNow")]);
    [...pop.querySelectorAll("button")][0].click();
    await settle();
    expect(calls.filter((c) => c.name === "rotation-use-now").map((c) => c.req)).toEqual([{ sid: "s1", account: "work" }]);
  });

  it("★ 下拉选「不轮换」⇒ rotation-use null；［编辑策略］⇒ 开设置窗那台的「轮换」栏看这一份；策略改过了 ⇒［载入新版］⇒ rotation-use 同一份", async () => {
    shown.view = { ...VIEW, hints: [{ hint: { k: "newer" }, tone: "plain", text: "夹具·改过了那一句" }] };
    const m = mountSessionRotation("<local>", "s1");
    document.body.replaceChildren(m.root);
    await m.refresh();
    m.root.querySelector<HTMLButtonElement>(`button[aria-label="${copyText("rot.top.pick")}"]`)!.click();
    [...document.querySelectorAll<HTMLElement>('[role^="menuitem"]')].find((e) => e.textContent?.includes(copyText("rot.none.label")))!.click();
    await settle();
    btn(m.root, copyText("rot.hint.loadNew")).click();
    await settle();
    expect(calls.filter((c) => c.name === "rotation-use").map((c) => c.req)).toEqual([
      { sid: "s1", strategy: null },
      { sid: "s1", strategy: "按权重轮换" },
    ]);
    btn(m.root, copyText("rot.top.edit")).click();
    expect(settingsAt).toEqual([{ machine: "<local>", tab: "rot", anchor: rotAnchor("按权重轮换", null) }]);
  });
});

describe("顺序：命令与回包", () => {
  it("★ 勾 ⇒ rotation-temp off；拖到第一格 ⇒ move 到第 0 格，碰上用不了（unusable）⇒ 滑回，顺序上方写那一句（为什么照后端写的）", async () => {
    const m = mountSessionRotation("<local>", "s1");
    document.body.replaceChildren(m.root);
    await m.refresh();
    rows(m.root)[1].querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
    await settle();
    expect(calls.find((c) => c.name === "rotation-temp")?.req).toEqual({ sid: "s1", op: { off: { account: "work", off: true } } });
    boxes(m.root);
    refuse.set("rotation-temp", "unusable");
    rows(m.root)[1].dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, button: 0, clientX: 10, clientY: 60 }));
    document.dispatchEvent(new MouseEvent("pointermove", { bubbles: true, clientX: 10, clientY: 10 }));
    document.dispatchEvent(new MouseEvent("pointerup", { bubbles: true, clientX: 10, clientY: 10 }));
    await settle();
    expect(calls.filter((c) => c.name === "rotation-temp").at(-1)?.req).toEqual({ sid: "s1", op: { move: { account: "work", to: 0 } } });
    expect(m.root.querySelector('[data-rot-order] [role="status"]')?.textContent).toBe(copyText("rot.acct.refused", { account: "work", why: "后端写的那一句" }));
  });

  it("★ 拿着一行时来推送 ⇒ 先不读；放下回包成了 ⇒ 读一次、照后端画", async () => {
    const m = mountSessionRotation("<local>", "s1");
    document.body.replaceChildren(m.root);
    await m.refresh();
    boxes(m.root);
    rows(m.root)[1].dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, button: 0, clientX: 10, clientY: 60 }));
    document.dispatchEvent(new MouseEvent("pointermove", { bubbles: true, clientX: 10, clientY: 10 }));
    calls.length = 0;
    await m.refresh();
    expect(calls, "拿着时不读").toEqual([]);
    document.dispatchEvent(new MouseEvent("pointerup", { bubbles: true, clientX: 10, clientY: 10 }));
    await settle();
    expect(calls.map((c) => c.name)).toEqual(["rotation-temp", "rotation-view"]);
  });
});
