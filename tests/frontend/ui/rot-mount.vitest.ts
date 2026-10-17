/**
 * 轮换的接线（`rot-mount.ts`）：手势翻成哪条 typed 命令、发给谁。会话面板（目标 `{sid}`：载入一份 · 临时动作）与设置窗
 * （目标 `{strategy}`：改一格当场存成新版）。读写都经 `rot-reads.ts`（这里桩住）。期望手写。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const calls: { name: string; req: unknown }[] = [];
const opened: { origin: string; at: unknown }[] = [];
const settingsAt: unknown[] = [];
vi.mock("../../../src/frontend/ui/open-settings", () => ({
  openSettingsWindow: vi.fn(async (_t: unknown, at: unknown) => {
    settingsAt.push(at);
  }),
}));
vi.mock("../../../src/frontend/ui/file-window", () => ({
  openOnMachine: vi.fn(async (origin: string, at: unknown) => {
    opened.push({ origin, at });
  }),
}));
const refuse = new Map<string, string>();
vi.mock("../../../src/frontend/ui/rot-reads", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/rot-reads")>()),
  readStrategies: vi.fn(async () => LIST),
  readView: vi.fn(async (_o: string, target: unknown) => {
    calls.push({ name: "rotation-view", req: target });
    return shown.view;
  }),
  refusedCode: (e: unknown) => ((e as { code?: string }).code ? { code: (e as { code: string }).code } : null),
}));
vi.mock("../../../src/frontend/ui/ipc/chan-caller", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/ipc/chan-caller")>()),
  callTyped: vi.fn(async (_o: string, name: string, req: unknown) => {
    calls.push({ name, req });
    const code = refuse.get(name);
    if (code) {
      refuse.delete(name);
      throw Object.assign(new Error(code), { code });
    }
    return name === "rotation-settings" ? { rev: "h:9" } : name === "rotation-strategy-add" ? { id: "新的" } : {};
  }),
}));

import { mountRotation, parseRotAnchor, rotAnchor } from "../../../src/frontend/ui/rot-mount";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { RotationStrategiesReply } from "../../../src/frontend/ui/generated/RotationStrategiesReply";
import type { RotationViewReply } from "../../../src/frontend/ui/generated/RotationViewReply";

const LIST: RotationStrategiesReply = { strategies: [{ id: "按权重轮换", description: "说明", summary: "说明", rev: "h:1", inUse: [], forNew: true, wordsBad: false, tzUnknown: false, hand: false }, { id: "夜里", description: "", summary: "", rev: "h:3", inUse: [], forNew: false, wordsBad: false, tzUnknown: false, hand: true }], forNew: "按权重轮换", oldScripts: 0 };
const VIEW: RotationViewReply = {
  now: 1,
  nowText: "00:00", title: null,
  strategy: LIST.strategies[0],
  board: { all: [{ key: "close", kind: "percent", label: "夹具·容差", v: 25, num: 25, from: "own", min: 0, max: 100 }], rows: [{ account: "work", line: "", hover: "", cells: [], rules: [], marks: [] }] },
  start: "work",
  current: "team",
  lineup: ["team", "work"],
  off: [],
  accounts: [{ account: "work", plate: { color: 0, label: "work", text: "wo" }, kind: "sub", w5h: { open: true, used: 99 }, w7d: { open: true, used: 1 }, login: "ok", blocked: { why: "used", text: "后端写的那一句" } }],
  listed: 1,
  decision: null,
  timeline: null,
  past: null,
  log: [],
  hints: [],
};
const shown = { view: VIEW };

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
  opened.length = 0;
  settingsAt.length = 0;
  refuse.clear();
  shown.view = VIEW;
});

describe("会话面板（目标 {sid}）", () => {
  it("★ 下拉选一份 ⇒ rotation-use（载入它此刻的版本；选「不轮换」⇒ null）；编辑策略 ⇒ 开设置窗那台的「轮换」栏", async () => {
    const m = mountRotation({ origin: "<local>", sid: "s1" });
    await m.refresh();
    // 下拉（kit 的 select：点开浮层、点一项）选「不轮换」。
    m.root.querySelector<HTMLButtonElement>(`button[aria-label="${copyText("rot.top.pick")}"]`)!.click();
    const item = [...document.querySelectorAll<HTMLElement>('[role="menuitem"], [role="menuitemradio"], [role="option"]')].find((e) => e.textContent?.includes(copyText("rot.none.label")))!;
    item.click();
    await settle();
    expect(calls.filter((c) => c.name === "rotation-use").map((c) => c.req)).toEqual([{ sid: "s1", strategy: null }]);
    btn(m.root, copyText("rot.top.edit")).click();
    expect(settingsAt).toEqual([{ machine: "<local>", tab: "rot", anchor: rotAnchor("按权重轮换", null) }]);
    expect(parseRotAnchor(rotAnchor("按权重轮换", "work"))).toEqual({ strategy: "按权重轮换", account: "work" });
    expect(parseRotAnchor("keybindings")).toBeNull();
  });

  it("★ 勾 · 拖 ⇒ rotation-temp（这个会话的临时动作）；拖到第一格碰上用不了（unusable）⇒ 不换，顺序上方写那一句（为什么照后端写的）", async () => {
    const m = mountRotation({ origin: "<local>", sid: "s1" });
    document.body.replaceChildren(m.root);
    await m.refresh();
    let rows = [...m.root.querySelectorAll<HTMLElement>("[data-rot-row]")];
    rows[1].querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
    await settle();
    expect(calls.find((c) => c.name === "rotation-temp")?.req).toEqual({ sid: "s1", op: { off: { account: "work", off: true } } });
    rows = [...m.root.querySelectorAll<HTMLElement>("[data-rot-row]")];
    rows.forEach((r, k) => (r.getBoundingClientRect = () => ({ left: 0, right: 300, top: k * 40, bottom: k * 40 + 30, width: 300, height: 30, x: 0, y: k * 40, toJSON: () => ({}) })));
    refuse.set("rotation-use-now", "unusable");
    rows[1].dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, clientX: 10, clientY: 50 }));
    document.dispatchEvent(new MouseEvent("pointermove", { bubbles: true, clientX: 10, clientY: 30 }));
    document.dispatchEvent(new MouseEvent("pointerup", { bubbles: true, clientX: 10, clientY: 10 }));
    await settle();
    expect(calls.find((c) => c.name === "rotation-use-now")?.req).toEqual({ sid: "s1", account: "work" });
    expect(m.root.querySelector('[data-rot-order] [role="status"]')?.textContent).toBe(copyText("rot.acct.refused", { account: "work", why: "后端写的那一句" }));
  });
});

describe("设置窗（目标 {strategy}）", () => {
  it("★ 改一格 ⇒ rotation-settings（那个意图 ＋ 文件此刻的版本，不带整份）⇒ 重读文件此刻的", async () => {
    const m = mountRotation({ origin: "<local>", sid: null, machine: "夹具机" });
    await m.refresh();
    expect(calls.at(-1)).toEqual({ name: "rotation-view", req: { strategy: "按权重轮换" } });
    editClose(m.root, "40");
    await settle();
    expect(calls.at(-2)).toEqual({ name: "rotation-settings", req: { strategy: "按权重轮换", edit: { set: { key: "close", v: 40 } }, ifRev: "h:1" } });
    expect(calls.at(-1)).toEqual({ name: "rotation-view", req: { strategy: "按权重轮换" } });
  });
});

describe("设置窗：看哪份 · 新建", () => {
  const pick = (root: HTMLElement, label: string): void => {
    root.querySelector<HTMLButtonElement>(`button[aria-label="${copyText("rot.top.pick")}"]`)!.click();
    [...document.querySelectorAll<HTMLElement>('[role^="menuitem"]')].find((e) => e.textContent?.startsWith(label))!.click();
  };

  it("★ 下拉选一份 ⇒ 只换着看（不改新会话用哪份）；选「不轮换」⇒ rotation-for-new null", async () => {
    const m = mountRotation({ origin: "<local>", sid: null, machine: "夹具机" });
    document.body.replaceChildren(m.root);
    await m.refresh();
    pick(m.root, "夜里");
    await settle();
    expect(calls.at(-1)).toEqual({ name: "rotation-view", req: { strategy: "夜里" } });
    expect(calls.some((c) => c.name === "rotation-for-new")).toBe(false);
    pick(m.root, copyText("rot.none.label"));
    await settle();
    expect(calls.find((c) => c.name === "rotation-for-new")?.req).toEqual({ strategy: null });
  });

  it("★ 新建策略 ⇒ 问名字 ⇒ rotation-strategy-add 只带名字（照编辑器那一份建是后端的）⇒ 看新建的那份；跳过来的 focus ⇒ 看那份、开那个号", async () => {
    const asked: (string | undefined)[] = [];
    const m = mountRotation({ origin: "<local>", sid: null, machine: "夹具机", ask: async (_t, _i, note) => (asked.push(note), "新的") });
    document.body.replaceChildren(m.root);
    await m.refresh();
    m.root.querySelector<HTMLButtonElement>(`button[aria-label="${copyText("rot.more.label")}"]`)!.click();
    [...document.querySelectorAll<HTMLElement>('[role^="menuitem"]')].find((e) => e.textContent === copyText("rot.more.create"))!.click();
    await settle();
    await settle();
    expect(asked).toEqual([copyText("rot.ask.createNote")]);
    expect(calls.find((c) => c.name === "rotation-strategy-add")?.req).toEqual({ name: "新的" });
    m.focus("夜里", "work");
    await settle();
    expect(calls.at(-1)).toEqual({ name: "rotation-view", req: { strategy: "夜里" } });
    expect(m.root.querySelector("[data-rot-editor]")?.getAttribute("data-account")).toBe("work");
  });
});

describe("提示里的按钮", () => {
  const withHints = (over: Partial<RotationViewReply> = {}): RotationViewReply => ({
    ...VIEW,
    strategy: { ...LIST.strategies[0], rev: "h:2" },
    hints: [
      { hint: { k: "newer" }, tone: "plain", text: "夹具·改过了那一句" },
      { hint: { k: "older", n: 2 }, tone: "plain", text: "夹具·2 个会话那一句" },
      { hint: { k: "tzUnknown", zone: "Asia/Shanghai" }, tone: "need", text: "时区不明" },
      { hint: { k: "strategyError", error: { msg: "坏了" }, path: "/夹具/strategies/按权重轮换.rhai" }, tone: "fail", text: "夹具·出错那一句" },
    ],
    ...over,
  });

  it("★ 载入新版：会话 ⇒ rotation-use 同一份；设置页都载入新版 ⇒ rotation-reload", async () => {
    shown.view = withHints();
    const m = mountRotation({ origin: "<local>", sid: "s1" });
    await m.refresh();
    btn(m.root, copyText("rot.hint.loadNew")).click();
    await settle();
    expect(calls.find((c) => c.name === "rotation-use")?.req).toEqual({ sid: "s1", strategy: "按权重轮换" });
    const page = mountRotation({ origin: "<local>", sid: null, machine: "夹具机" });
    page.focus("按权重轮换", "work");
    await settle();
    btn(page.root, copyText("rot.hint.loadAll")).click();
    await settle();
    expect(calls.find((c) => c.name === "rotation-reload")?.req).toEqual({ strategy: "按权重轮换" });
  });

  it("★ 设时区 ⇒ rotation-strategy-tz（设成后端给的那个、带文件此刻的版本）；看代码 ⇒ 照「在文件窗口打开」那一路开那份文件", async () => {
    shown.view = withHints();
    const m = mountRotation({ origin: "<local>", sid: "s1" });
    await m.refresh();
    btn(m.root, copyText("rot.hint.setTz")).click();
    await settle();
    expect(calls.find((c) => c.name === "rotation-strategy-tz")?.req).toEqual({ id: "按权重轮换", tz: "Asia/Shanghai", ifRev: "h:2" });
    btn(m.root, copyText("rot.hint.code")).click();
    await settle();
    expect(opened).toEqual([{ origin: "<local>", at: { revealFile: "/夹具/strategies/按权重轮换.rhai" } }]);
  });

  it("★ 写撞上 conflict ⇒ 不自己造提示，重读（提示由后端出）", async () => {
    const m = mountRotation({ origin: "<local>", sid: null, machine: "夹具机" });
    await m.refresh();
    calls.length = 0;
    refuse.set("rotation-settings", "conflict");
    editClose(m.root, "30");
    await settle();
    expect(calls.map((c) => c.name)).toEqual(["rotation-settings", "rotation-view"]);
    expect(m.root.querySelector('[data-rot-hint="newer"]')).toBeNull();
  });
});
