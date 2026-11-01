/**
 * 设置窗「轮换」栏的接线（`rot-mount.ts`）：手势翻成哪条 typed 命令、发给谁（目标 `{strategy}`：改一格当场存成新版）。
 * 读写都经 `rot-reads.ts`（这里桩住）。会话面板那一处的判据在 `rot-session.vitest.ts`。期望手写。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ChanError } from "../../../src/comms/inward/chan";

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
}));
vi.mock("../../../src/frontend/ui/ipc/chan-caller", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/ipc/chan-caller")>()),
  callTyped: vi.fn(async (_o: string, name: string, req: unknown) => {
    calls.push({ name, req });
    const code = refuse.get(name);
    if (code) {
      refuse.delete(name);
      throw new ChanError({ layer: "peer", why: "refused", body: new TextEncoder().encode(JSON.stringify({ code, said: `那台说：${code}` })) }, `那台说：${code}`);
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
  board: { all: [{ key: "close", kind: "percent", label: "夹具·容差", v: 25, num: 25, from: "own", min: 0, max: 100 }], rows: [{ account: "work", row: 0, line: "", hover: "", cells: [{ key: "caps", kind: "daySegments", window: "5h", label: "夹具·上限 5h", v: null, from: "own", segs: [{ from: 0, to: 1440, fromText: "00:00", toText: "24:00", v: 90, text: "夹具·90", tip: "夹具·全天 90" }] }], rules: [], marks: [] }] },
  start: "",
  current: "",
  lineup: [],
  manual: false,
  off: [],
  accounts: [{ account: "work", plate: { color: 0, label: "work", text: "wo" }, full: [], kind: "sub", w5h: { open: true, used: 99 }, w7d: { open: true, used: 1 }, login: "ok", blocked: { why: "used", text: "后端写的那一句" } }],
  listed: 1,
  decision: null,
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

describe("设置窗（目标 {strategy}）", () => {
  it("★ 跳过来的那一格（看哪份 · 开哪个号）写得出也认得回", () => {
    expect(parseRotAnchor(rotAnchor("按权重轮换", "work"))).toEqual({ strategy: "按权重轮换", account: "work" });
    expect(parseRotAnchor("keybindings")).toBeNull();
  });

  it("★ 改一格 ⇒ rotation-settings（那个意图 ＋ 文件此刻的版本，不带整份）⇒ 重读文件此刻的", async () => {
    const m = mountRotation({ origin: "<local>", machine: "夹具机" });
    await m.refresh();
    expect(calls.at(-1)).toEqual({ name: "rotation-view", req: { strategy: "按权重轮换" } });
    editClose(m.root, "40");
    await settle();
    expect(calls.at(-2)).toEqual({ name: "rotation-settings", req: { strategy: "按权重轮换", edit: { set: { key: "close", v: 40 } }, ifRev: "h:1" } });
    expect(calls.at(-1)).toEqual({ name: "rotation-view", req: { strategy: "按权重轮换" } });
  });
});

describe("设置窗：加时段那一行（草稿）", () => {
  /** 设置窗、开 work 的编辑小窗、点「加时段」、「从」填 20:00（交不出：还缺两格）。 */
  const started = async (): Promise<ReturnType<typeof mountRotation>> => {
    const m = mountRotation({ origin: "<local>", machine: "夹具机" });
    document.body.replaceChildren(m.root);
    m.focus("按权重轮换", "work");
    await settle();
    btn(m.root, copyText("rot.seg.add")).click();
    const from = m.root.querySelector<HTMLInputElement>('[data-rot-seg="new"] input')!;
    from.value = "20:00";
    from.dispatchEvent(new Event("input"));
    from.dispatchEvent(new Event("change"));
    return m;
  };
  const fresh = (root: HTMLElement): HTMLInputElement[] => [...root.querySelectorAll<HTMLInputElement>('[data-rot-seg="new"] input')];
  const fill = (root: HTMLElement): void => {
    const [, to, v] = fresh(root);
    to.value = "23:00";
    to.dispatchEvent(new Event("change"));
    v.value = "0";
    v.dispatchEvent(new Event("change"));
  };

  it("★ 没填完时来一次重读（推送）⇒ 那一行 · 已打的字 · 焦点都还在、没交；填完交出的是那一段", async () => {
    const m = await started();
    await m.refresh();
    const line = fresh(m.root);
    expect(line.length, "草稿行还在").toBe(3);
    expect(line[0].value).toBe("20:00");
    expect(document.activeElement).toBe(line[0]);
    expect(calls.some((c) => c.name === "rotation-settings")).toBe(false);
    fill(m.root);
    await settle();
    expect(calls.filter((c) => c.name === "rotation-settings").map((c) => c.req)).toEqual([
      { strategy: "按权重轮换", edit: { seg: { key: "caps", window: "5h", row: 0, seg: { from: "20:00", to: "23:00", v: 0 } } }, ifRev: "h:1" },
    ]);
  });

  it("★ 已存的那一段改了一半（没交）来一次重读 ⇒ 那一格字还在、焦点还在", async () => {
    const m = mountRotation({ origin: "<local>", machine: "夹具机" });
    document.body.replaceChildren(m.root);
    m.focus("按权重轮换", "work");
    await settle();
    const at = m.root.querySelector<HTMLInputElement>('[data-rot-seg="0"] input')!;
    at.focus();
    at.value = "06:";
    await m.refresh();
    const after = m.root.querySelector<HTMLInputElement>('[data-rot-seg="0"] input')!;
    expect(after.value).toBe("06:");
    expect(after.value).toBe("06:");
    expect(document.activeElement).toBe(after);
  });

  it("★ 交成功 ⇒ 草稿清掉（再重读不再有那一行）", async () => {
    const m = await started();
    fill(m.root);
    await settle();
    await m.refresh();
    expect(fresh(m.root)).toEqual([]);
  });

  it("★ 交出去撞上 stale ⇒ 草稿与已打的字留着、照后端那一句说（不静默丢）", async () => {
    const m = await started();
    refuse.set("rotation-settings", "stale");
    fill(m.root);
    await settle();
    expect(fresh(m.root).map((x) => x.value)).toEqual(["20:00", "23:00", "0"]);
    expect(document.body.textContent).toContain("那台说：stale");
  });
});

describe("设置窗：看哪份 · 新建", () => {
  const pick = (root: HTMLElement, label: string): void => {
    root.querySelector<HTMLButtonElement>(`button[aria-label="${copyText("rot.top.pick")}"]`)!.click();
    [...document.querySelectorAll<HTMLElement>('[role^="menuitem"]')].find((e) => e.textContent?.startsWith(label))!.click();
  };

  it("★ 下拉选一份 ⇒ 只换着看（不改新会话用哪份）；选「不轮换」⇒ rotation-for-new null", async () => {
    const m = mountRotation({ origin: "<local>", machine: "夹具机" });
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
    const m = mountRotation({ origin: "<local>", machine: "夹具机", ask: async (_t, _i, note) => (asked.push(note), "新的") });
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

  it("★ 都载入新版 ⇒ rotation-reload", async () => {
    shown.view = withHints();
    const page = mountRotation({ origin: "<local>", machine: "夹具机" });
    page.focus("按权重轮换", "work");
    await settle();
    btn(page.root, copyText("rot.hint.loadAll")).click();
    await settle();
    expect(calls.find((c) => c.name === "rotation-reload")?.req).toEqual({ strategy: "按权重轮换" });
  });

  it("★ 设时区 ⇒ rotation-strategy-tz（设成后端给的那个、带文件此刻的版本）；看代码 ⇒ 照「在文件窗口打开」那一路开那份文件", async () => {
    shown.view = withHints();
    const m = mountRotation({ origin: "<local>", machine: "夹具机" });
    await m.refresh();
    btn(m.root, copyText("rot.hint.setTz")).click();
    await settle();
    expect(calls.find((c) => c.name === "rotation-strategy-tz")?.req).toEqual({ id: "按权重轮换", tz: "Asia/Shanghai", ifRev: "h:2" });
    btn(m.root, copyText("rot.hint.code")).click();
    await settle();
    expect(opened).toEqual([{ origin: "<local>", at: { revealFile: "/夹具/strategies/按权重轮换.rhai" } }]);
  });

  it("★ 写撞上 stale ⇒ 不自己造提示（照后端那一句出失败 toast，见草稿那组），重读此刻的", async () => {
    const m = mountRotation({ origin: "<local>", machine: "夹具机" });
    await m.refresh();
    calls.length = 0;
    refuse.set("rotation-settings", "stale");
    editClose(m.root, "30");
    await settle();
    expect(calls.map((c) => c.name)).toEqual(["rotation-settings", "rotation-view"]);
    expect(m.root.querySelector('[data-rot-hint="newer"]')).toBeNull();
  });
});
