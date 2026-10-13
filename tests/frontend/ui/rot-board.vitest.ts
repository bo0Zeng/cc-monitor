/**
 * 轮换四块（`rot-board.ts`）：只照 `rotation-view` · `rotation-strategies` 的回包画，手势翻成 `BoardActs`。
 * 号名 · 文件名 · 时刻全是合成的；格的样子照 `我们的轮换.rhai` 那几格（这里的 label / help 是夹具里的字，不是界面写的）。期望手写。
 */
import { describe, expect, it, vi } from "vitest";
import { board, textOf, type BoardActs, type BoardState } from "../../../src/frontend/ui/rot-board";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { Fact } from "../../../src/frontend/ui/generated/Fact";
import type { RotFormCell } from "../../../src/frontend/ui/generated/RotFormCell";
import type { RotationStrategiesReply } from "../../../src/frontend/ui/generated/RotationStrategiesReply";
import type { RotationViewReply } from "../../../src/frontend/ui/generated/RotationViewReply";

const NOW = 1_800_000_000;
const fact = (account: string, u5: number, u7: number, blocked = false): Fact => ({
  account,
  kind: "sub",
  w5h: { open: true, used: u5, resets: NOW + 3600, resetsText: "18:30" },
  w7d: { open: true, used: u7 },
  login: "ok",
  ...(blocked ? { blocked: { why: "used", text: "5h 用满 · 18:30 重置" } } : {}),
});

const FORM: RotFormCell[] = [
  { key: "caps", kind: "daySegments", scope: "each", inherit: true, windows: ["5h", "7d"], min: 0, max: 100, unit: "percent", default: 90, label: { zh: "夹具·上限" }, help: { zh: "夹具·上限的定义" } },
  { key: "close", kind: "percent", scope: "all", min: 0, max: 100, default: 25, label: { zh: "夹具·容差" } },
  { key: "force", kind: "percent", scope: "all", min: 0, max: 100, default: 25, nullable: { zh: "夹具·空值" }, label: { zh: "夹具·阈值" } },
  { key: "fallback", kind: "toggle", scope: "each", default: false, label: { zh: "夹具·开关" } },
  { key: "delay", kind: "minutes", scope: "each", min: 0, max: 1440, default: 0, label: { zh: "夹具·延迟" } },
];

const SETTINGS = {
  accounts: [{ id: "work" }, { id: "team" }, { id: "lab", fallback: true }],
  caps: { "5h": [{ from: 0, to: 1440, v: 90 }], "7d": [{ from: 0, to: 1440, v: 90 }] },
  close: 25,
  force: 25,
};

function view(over: Partial<RotationViewReply> = {}): RotationViewReply {
  return {
    now: NOW,
    nowText: "14:30",
    strategy: { id: "按权重轮换", description: "第一句说明\n第二句说明\n\n第二段", rev: "h:1", inUse: [], forNew: true, wordsBad: false, tzUnknown: false },
    form: FORM,
    settings: SETTINGS,
    dirty: false,
    start: "work",
    current: "team",
    accounts: [fact("work", 94, 40, true), fact("team", 58, 30), fact("lab", 35, 20), fact("api", 0, 0)],
    listed: 3,
    decision: { pick: "team", order: ["team", "lab"], why: [], fromTemp: false, skipped: [], temp: [] },
    timeline: {
      switches: [{ at: NOW + 5400, atText: "16:00", account: "lab" }],
      uses: [
        { from: NOW, to: NOW + 5400, account: "team", fromTemp: false, capUnknown: false, skipped: [] },
        { from: NOW + 5400, to: NOW + 9000, account: "lab", fromTemp: true, capUnknown: false, skipped: ["work"] },
        { from: NOW + 9000, to: NOW + 43_200, fromTemp: false, capUnknown: false, skipped: [] },
      ],
      lanes: [
        { account: "team", segs: [{ from: NOW, to: NOW + 3600, sendable: true, used: [58, 30], reset: false, fromText: "14:30", left: 42 }, { from: NOW + 3600, to: NOW + 43_200, sendable: true, used: [0, 30], reset: true, fromText: "15:30", left: 70 }] },
        { account: "work", segs: [{ from: NOW, to: NOW + 43_200, sendable: false, used: [100, 40], reset: false, fromText: "14:30", left: 0 }] },
      ],
      truncated: false,
      ticks: [{ at: NOW + 1800, text: "15:00" }, { at: NOW + 9000, text: "17:00" }],
    },
    log: [],
    hints: [],
    ...over,
  };
}

const LIST: RotationStrategiesReply = {
  strategies: [
    { id: "按权重轮换", description: "第一句说明\n第二句说明", rev: "h:1", inUse: ["s1", "s2"], forNew: true, wordsBad: false, tzUnknown: false },
    { id: "夜里省着用", description: "夜里那一句", rev: "h:2", inUse: [], forNew: false, wordsBad: false, tzUnknown: false },
  ],
  forNew: "按权重轮换",
  oldScripts: 0,
};

function acts() {
  return {
    pick: vi.fn(),
    copy: vi.fn(),
    rename: vi.fn(),
    forNew: vi.fn(),
    remove: vi.fn(),
    add: vi.fn(),
    settings: vi.fn(),
    save: vi.fn(),
    revert: vi.fn(),
    dayPaint: vi.fn(async () => []),
    useNow: vi.fn(),
    temp: vi.fn(),
    hours: vi.fn(),
    open: vi.fn(),
  };
}

const state = (over: Partial<BoardState> = {}): BoardState => ({ hours: 12, open: null, refused: null, ...over });

function draw(v: RotationViewReply | null, s: BoardState, a: BoardActs, where: "session" | "settings" = "session"): HTMLElement {
  const root = board({ view: v, list: LIST, state: s, acts: a, origin: "<local>", where, machine: "夹具机" });
  document.body.replaceChildren(root);
  return root;
}

const texts = (root: HTMLElement, sel: string): string[] => [...root.querySelectorAll<HTMLElement>(sel)].map((e) => e.textContent ?? "");
const button = (root: HTMLElement, text: string): HTMLButtonElement | undefined => [...root.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === text);

describe("第一行与说明", () => {
  it("★ 说明照文件开头的注释（每段一块），标题就是文件名；不画任何讲实现的字", () => {
    const root = draw(view(), state(), acts());
    expect(texts(root, "p")).toEqual(["第一句说明\n第二句说明", "第二段"]);
    expect(root.textContent).toContain("按权重轮换");
  });

  it("★ 会话改过设置 ⇒ 没存 · 还原 · 存成这份策略；点了翻成 revert · save", () => {
    const a = acts();
    const root = draw(view({ dirty: true }), state(), a);
    expect(root.textContent).toContain(copyText("rot.top.dirty"));
    button(root, copyText("rot.top.revert"))!.click();
    button(root, copyText("rot.top.save"))!.click();
    expect(a.revert).toHaveBeenCalledOnce();
    expect(a.save).toHaveBeenCalledOnce();
  });

  it("★ 不轮换的会话：第一行 ＋ 一行「只用开会话的号 work」，号牌 · 设置 · 模拟都不出", () => {
    const root = draw(view({ strategy: null, form: [], decision: null, timeline: null, accounts: [], listed: 0 }), state(), acts());
    expect(root.textContent).toContain(copyText("rot.none.desc"));
    expect(root.textContent).toContain("work");
    expect(root.querySelector("[data-account]")).toBeNull();
    expect(root.textContent).not.toContain(copyText("rot.h.sim"));
  });

  it("★ 提示照后端写好的那一句；停发那一句里的「还有多久」在画的那一刻照 now · until 填", () => {
    const root = draw(
      view({ hints: [{ hint: { k: "hold", until: NOW + 4 * 3600, untilText: "18:30", account: "team" }, text: "停发了 · 18:30 team · 还剩 {dur}" }, { hint: { k: "wordsBad" }, text: "说明写法不合规那一句" }] }),
      state(),
      acts(),
    );
    const said = texts(root, "[data-rot-hint]");
    expect(said[0]).toContain("18:30 team");
    expect(said[0]).not.toContain("{dur}");
    expect(said[1]).toContain("说明写法不合规那一句");
  });
});

describe("号牌与点开一个号", () => {
  it("★ 号牌照号列表的先后（列表外的不出），在用的号带绿点、用不了的变淡；点一下翻成 open", () => {
    const a = acts();
    const root = draw(view(), state(), a);
    const cards = [...root.querySelectorAll<HTMLElement>("[data-account]")];
    expect(cards.map((c) => c.dataset.account)).toEqual(["work", "team", "lab"]);
    expect(cards[1].dataset.inUse).toBe("true");
    expect(cards[0].dataset.blocked).toBe("true");
    cards[2].dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, clientX: 10, clientY: 10 }));
    document.dispatchEvent(new MouseEvent("pointerup", { bubbles: true, clientX: 10, clientY: 10 }));
    expect(a.open).toHaveBeenCalledWith("lab");
  });

  it("★ 拖第 1 张松在第 3 张上 ⇒ 整份设置里号列表排成 team · work · lab（拖到它前头），别的格原样", () => {
    const a = acts();
    const root = draw(view(), state(), a);
    const cards = [...root.querySelectorAll<HTMLElement>("[data-account]")];
    cards.forEach((c, k) => (c.getBoundingClientRect = () => ({ left: k * 100, right: k * 100 + 90, top: 0, bottom: 30, width: 90, height: 30, x: k * 100, y: 0, toJSON: () => ({}) })));
    cards[0].dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, clientX: 40, clientY: 10 }));
    document.dispatchEvent(new MouseEvent("pointermove", { bubbles: true, clientX: 140, clientY: 10 }));
    document.dispatchEvent(new MouseEvent("pointerup", { bubbles: true, clientX: 240, clientY: 10 }));
    expect(a.settings).toHaveBeenCalledOnce();
    const next = a.settings.mock.calls[0][0] as typeof SETTINGS;
    expect(next.accounts.map((r) => r.id)).toEqual(["team", "work", "lab"]);
    expect(next.close).toBe(25);
    expect(a.open).not.toHaveBeenCalled();
  });

  it("★ 点开 lab：每号那几格照声明（标签是策略写的字，跟总体的画虚线）；改开关 ⇒ 只改 lab 那一行", () => {
    const a = acts();
    const root = draw(view(), state({ open: "lab" }), a);
    const pane = root.querySelector<HTMLElement>('[data-account="lab"]:not(button)')!;
    const keys = [...new Set(texts(pane, "span").filter((t) => t.startsWith("夹具")))];
    expect(keys).toEqual([copyText("rot.cell.win", { label: "夹具·上限", win: copyText("rot.win.fiveHour") }), copyText("rot.cell.win", { label: "夹具·上限", win: copyText("rot.win.sevenDay") }), "夹具·开关", "夹具·延迟"]);
    expect(pane.querySelectorAll('[data-inherited="true"]').length).toBeGreaterThan(0);
    pane.querySelector<HTMLButtonElement>('[role="switch"]')!.click();
    expect(a.settings).toHaveBeenCalledOnce();
    const next = a.settings.mock.calls[0][0] as { accounts: Record<string, unknown>[] };
    expect(next.accounts[2]).toEqual({ id: "lab", fallback: false });
    expect(next.accounts[0]).toEqual({ id: "work" });
  });

  it("★ 「现在用」被拦下 ⇒ 按钮下直接写原因（后端那一句）；在设置页（没有会话）不出「现在用」", () => {
    const a = acts();
    const root = draw(view(), state({ open: "work", refused: { account: "work", text: "work 用不了那一句" } }), a);
    expect(root.textContent).toContain("work 用不了那一句");
    button(root, copyText("rot.acct.useNow", { account: "work" }))!.click();
    expect(a.useNow).toHaveBeenCalledWith("work");
    const set = draw(view(), state({ open: "work" }), { ...acts(), useNow: undefined }, "settings");
    expect(button(set, copyText("rot.acct.useNow", { account: "work" }))).toBeUndefined();
  });
});

describe("总体设置", () => {
  it("★ 照 `scope: all` 的先后画；可空的那一格点空值说法 ⇒ 交 null，再点 ⇒ 交回数", () => {
    const a = acts();
    const root = draw(view(), state(), a);
    expect(root.textContent).toContain("夹具·容差");
    expect(root.textContent).toContain("夹具·阈值");
    button(root, "夹具·空值")!.click();
    expect((a.settings.mock.calls[0][0] as typeof SETTINGS).force).toBeNull();
    const b = acts();
    const again = draw(view({ settings: { ...SETTINGS, force: null } }), state(), b);
    button(again, "夹具·空值")!.click();
    expect((b.settings.mock.calls[0][0] as typeof SETTINGS).force).toBe(25);
  });

  it("textOf：一句 · 分语言的取中文", () => {
    expect(textOf("一句")).toBe("一句");
    expect(textOf({ zh: "中", en: "en" })).toBe("中");
  });
});

describe("模拟", () => {
  it("★ 时刻的字全照回包：此刻 · 刻度 · 换号时刻；最上那条左边留空；每号行此刻那段写剩多少、重置那段写 ↻ 钟面、发不出去的不写字", () => {
    const root = draw(view(), state(), acts());
    expect(root.textContent).toContain("14:30");
    expect(root.textContent).toContain("17:00");
    expect(root.textContent, "挨着此刻的刻度让开").not.toContain("15:00");
    expect(root.textContent).toContain(copyText("rot.sim.switch", { at: "16:00", account: "lab" }));
    const segs = texts(root, '[data-state]');
    expect(segs).toEqual([copyText("rot.sim.left", { n: 42 }), copyText("rot.sim.reset", { at: "15:30" }), ""]);
  });

  it("★ 拖出来的那段白描边（data-temp）· 轮到时用不了的写「跳过」· 停发那段写「停下」；标题右边 手动 N 段 · 清掉手动顺序", () => {
    const a = acts();
    const root = draw(view({ decision: { pick: "team", order: [], why: [], fromTemp: false, skipped: [], temp: ["team", "lab"] } }), state(), a);
    expect(root.querySelectorAll('[data-temp="true"]').length).toBe(1);
    expect(root.textContent).toContain(copyText("rot.sim.skip", { account: "work" }));
    expect(root.querySelector('[data-hold="true"]')?.textContent).toBe(copyText("rot.sim.hold"));
    expect(root.textContent).toContain(copyText("rot.sim.manual", { n: 2 }));
    button(root, copyText("rot.sim.clear"))!.click();
    expect(a.temp).toHaveBeenCalledWith("clear");
  });

  it("★ 拖号名松在最上那条上 ⇒ 排进临时条目：落在第一次换号之后 ⇒ 第 2 位", () => {
    const a = acts();
    const root = draw(view(), state(), a);
    const top = root.querySelector<HTMLElement>('[data-hold="true"]')!.parentElement!;
    top.getBoundingClientRect = () => ({ left: 0, right: 1200, top: 100, bottom: 120, width: 1200, height: 20, x: 0, y: 100, toJSON: () => ({}) });
    const head = root.querySelector<HTMLElement>('[data-drag="true"]')!;
    head.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, clientX: 0, clientY: 200 }));
    // 12 小时 ⇒ 1200px：第一次换号在 90 分钟（150px），松在 200px。
    document.dispatchEvent(new MouseEvent("pointerup", { bubbles: true, clientX: 200, clientY: 110 }));
    expect(a.temp).toHaveBeenCalledWith({ place: { account: "team", at: 2 } });
  });
});
