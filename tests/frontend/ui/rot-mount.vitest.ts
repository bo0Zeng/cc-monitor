/**
 * 轮换四块的接线（`rot-mount.ts`）：手势翻成哪条 typed 命令、发给谁。会话面板（目标 `{sid}`）与设置窗（目标 `{strategy, settings}`）
 * 只差这几条。读写都经 `rot-reads.ts`（这里桩住）。期望手写。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const calls: { name: string; req: unknown }[] = [];
const opened: { origin: string; at: unknown }[] = [];
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
    return name === "rotation-settings" ? { dirty: true, settings: { 夹具: "改完的那份", n: calls.filter((c) => c.name === name).length } } : {};
  }),
}));

import { mountRotation } from "../../../src/frontend/ui/rot-mount";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { RotationStrategiesReply } from "../../../src/frontend/ui/generated/RotationStrategiesReply";
import type { RotationViewReply } from "../../../src/frontend/ui/generated/RotationViewReply";

const LIST: RotationStrategiesReply = { strategies: [{ id: "按权重轮换", description: "说明", summary: "说明", rev: "h:1", inUse: [], forNew: true, wordsBad: false, tzUnknown: false }], forNew: "按权重轮换", oldScripts: 0 };
const VIEW: RotationViewReply = {
  now: 1,
  nowText: "00:00", title: null,
  strategy: LIST.strategies[0],
  settings: { accounts: [{ id: "work" }], close: 25 },
  board: { all: [{ key: "close", kind: "percent", label: "夹具·容差", v: 25, num: 25, from: "own", min: 0, max: 100 }], rows: [{ account: "work", line: "", hover: "", cells: [] }] },
  dirty: true,
  start: "work",
  current: "team",
  accounts: [{ account: "work", plate: { color: 0, label: "work", text: "wo" }, kind: "sub", w5h: { open: true, used: 99 }, w7d: { open: true, used: 1 }, login: "ok", blocked: { why: "used", text: "后端写的那一句" } }],
  listed: 1,
  decision: null,
  timeline: null,
  past: null,
  baseRev: "h:1",
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
  refuse.clear();
  shown.view = VIEW;
});

describe("会话面板（目标 {sid}）", () => {
  it("★ 改一格 ⇒ rotation-settings（那个意图 ＋ 读到的版本，不带整份）；存 ⇒ rotation-save；还原 ⇒ rotation-revert", async () => {
    const m = mountRotation({ origin: "<local>", sid: "s1" });
    await m.refresh();
    editClose(m.root, "30");
    await settle();
    expect(calls.find((c) => c.name === "rotation-settings")?.req).toEqual({ target: { sid: "s1" }, edit: { set: { key: "close", v: 30 } }, ifRev: "h:1" });
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
  it("★ 改一格 ⇒ 后端照手上的草稿改、回改完的那份；界面原样存着、照它重推（rotation-view 带 settings）；存 ⇒ rotation-save 带它", async () => {
    const m = mountRotation({ origin: "<local>", sid: null, machine: "夹具机" });
    await m.refresh();
    expect(calls.at(-1)).toEqual({ name: "rotation-view", req: { strategy: "按权重轮换" } });
    editClose(m.root, "40");
    await settle();
    expect(calls.at(-2)).toEqual({ name: "rotation-settings", req: { target: { strategy: "按权重轮换" }, edit: { set: { key: "close", v: 40 } } } });
    const first = { 夹具: "改完的那份", n: 1 };
    expect(calls.at(-1)).toEqual({ name: "rotation-view", req: { strategy: "按权重轮换", settings: first, rev: "h:1" } });
    editClose(m.root, "41");
    await settle();
    expect(calls.at(-2)?.req, "下一笔照上一次回的那份改").toEqual({ target: { strategy: "按权重轮换" }, settings: first, edit: { set: { key: "close", v: 41 } } });
    btn(m.root, copyText("rot.top.save")).click();
    await settle();
    expect(calls.find((c) => c.name === "rotation-save")?.req).toEqual({ target: { strategy: "按权重轮换" }, settings: { 夹具: "改完的那份", n: 2 }, ifRev: "h:1" });
  });
});

describe("提示里的按钮", () => {
  const withHints = (over: Partial<RotationViewReply> = {}): RotationViewReply => ({
    ...VIEW,
    strategy: { ...LIST.strategies[0], rev: "h:2" },
    baseRev: "h:1",
    hints: [
      { hint: { k: "newer" }, tone: "plain", text: "夹具·别处改了那一句" },
      { hint: { k: "tzUnknown", zone: "Asia/Shanghai" }, tone: "need", text: "时区不明" },
      { hint: { k: "strategyError", error: { msg: "坏了" }, path: "/夹具/strategies/按权重轮换.rhai" }, tone: "fail", text: "夹具·出错那一句" },
    ],
    ...over,
  });

  it("★ 会话：写的时候 ifRev 照 baseRev（草稿基于的）；留着这边的 ⇒ rotation-settings 交 keep、ifRev 跟上文件此刻的；载入新的 ⇒ rotation-revert", async () => {
    shown.view = withHints();
    const m = mountRotation({ origin: "<local>", sid: "s1" });
    await m.refresh();
    editClose(m.root, "30");
    await settle();
    expect((calls.find((c) => c.name === "rotation-settings")?.req as { ifRev: string }).ifRev).toBe("h:1");
    calls.length = 0;
    btn(m.root, copyText("rot.hint.keepMine")).click();
    await settle();
    expect(calls.find((c) => c.name === "rotation-settings")?.req).toEqual({ target: { sid: "s1" }, edit: "keep", ifRev: "h:2" });
    btn(m.root, copyText("rot.hint.loadNew")).click();
    await settle();
    expect(calls.find((c) => c.name === "rotation-revert")?.req).toEqual({ sid: "s1" });
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
    const m = mountRotation({ origin: "<local>", sid: "s1" });
    await m.refresh();
    calls.length = 0;
    refuse.set("rotation-settings", "conflict");
    editClose(m.root, "30");
    await settle();
    expect(calls.map((c) => c.name)).toEqual(["rotation-settings", "rotation-view"]);
    expect(m.root.querySelector('[data-rot-hint="newer"]')).toBeNull();
  });

  it("★ 设置页：留着这边的 ⇒ 草稿照旧、随读交的版本换成文件此刻的；载入新的 ⇒ 丢草稿", async () => {
    const m = mountRotation({ origin: "<local>", sid: null, machine: "夹具机" });
    await m.refresh();
    editClose(m.root, "40");
    await settle();
    expect(calls.at(-1)?.req, "草稿照读到的那个版本改出来").toMatchObject({ rev: "h:1" });
    // 文件在别处被改了（推送来了 ⇒ 重读）：后端说别处改了。
    shown.view = withHints();
    await m.refresh();
    btn(m.root, copyText("rot.hint.keepMine")).click();
    await settle();
    expect(calls.at(-1)?.req).toMatchObject({ settings: { 夹具: "改完的那份" }, rev: "h:2" });
    btn(m.root, copyText("rot.hint.loadNew")).click();
    await settle();
    expect(calls.at(-1)?.req).toEqual({ strategy: "按权重轮换" });
  });
});
