/**
 * 设置窗「轮换」栏（`rot-board.ts`）：只照 `rotation-view {strategy}` · `rotation-strategies` 的回包画，手势翻成 `BoardActs`。
 * 号名 · 文件名 · 时刻全是合成的；格（`board`）是后端解好的那一形（label / help / 段上的字都是夹具里的字，不是界面写的）。期望手写。
 * 会话面板那一块（提示 · 策略一行 · 顺序）的判据在 `rot-session.vitest.ts` · `rot-order.vitest.ts`。
 */
import { describe, expect, it, vi } from "vitest";
import { board, draftKey, type BoardActs, type BoardState } from "../../../src/frontend/ui/rot-board";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { RotAccount } from "../../../src/frontend/ui/generated/RotAccount";
import type { RotCell } from "../../../src/frontend/ui/generated/RotCell";
import type { RotRow } from "../../../src/frontend/ui/generated/RotRow";
import type { RotationStrategiesReply } from "../../../src/frontend/ui/generated/RotationStrategiesReply";
import type { RotationViewReply } from "../../../src/frontend/ui/generated/RotationViewReply";

const NOW = 1_800_000_000;
const fact = (account: string, u5: number, u7: number, blocked = false): RotAccount => ({
  account,
  plate: { color: 5, label: account, text: account.slice(0, 2) },
  full: u5 >= 100 ? ["5h"] : [],
  kind: "sub",
  w5h: { open: true, used: u5, resets: NOW + 3600, resetsText: "18:30" },
  w7d: { open: true, used: u7 },
  login: "ok",
  ...(blocked ? { blocked: { why: "used", text: "5h 用满 · 18:30 重置" } } : {}),
});

const seg = { from: 0, to: 1440, fromText: "00:00", toText: "24:00", v: 90, text: "夹具·90", tip: "夹具·全天 90" };
const caps = (window: string, from: RotCell["from"]): RotCell => ({ key: "caps", kind: "daySegments", window, label: `夹具·上限 ${window}`, help: "夹具·上限的定义", v: null, from, segs: [seg] });
const each = (fallback: boolean, from: RotCell["from"]): RotCell[] => [
  caps("5h", "all"),
  caps("7d", "all"),
  { key: "fallback", kind: "toggle", label: "夹具·开关", v: fallback, from, ...(from === "own" ? { reset: true } : {}) },
  { key: "delay", kind: "minutes", label: "夹具·延迟", v: 0, num: 0, from: "default", min: 0, max: 1440 },
];
const row = (account: string, cells: RotCell[], marks: RotRow["marks"] = [], at = ["work", "team", "lab"].indexOf(account)): RotRow => ({ account, row: at, line: `夹具·${account} 那一行`, hover: `夹具·${account} 悬停`, cells, rules: [`夹具·${account} 的规则`], marks });
const BOARD = {
  all: [
    { key: "close", kind: "percent", label: "夹具·容差", v: 25, num: 25, from: "own", min: 0, max: 100, reset: true },
    { key: "force", kind: "percent", label: "夹具·阈值", v: 25, num: 25, from: "own", nullable: "夹具·空值", min: 0, max: 100, reset: true },
  ] satisfies RotCell[],
  rows: [row("work", each(false, "default")), row("team", each(false, "default")), row("lab", each(true, "own"))],
};

function view(over: Partial<RotationViewReply> = {}): RotationViewReply {
  return {
    now: NOW,
    nowText: "14:30", title: null,
    strategy: { id: "按权重轮换", description: "第一句说明\n第二句说明\n\n第二段", summary: "第一句说明", rev: "h:1", inUse: [], forNew: true, wordsBad: false, tzUnknown: false, hand: false },
    board: BOARD,
    start: "",
    current: "",
    lineup: [],
    manual: false,
    off: [],
    accounts: [fact("work", 94, 40, true), fact("team", 58, 30), fact("lab", 35, 20), fact("api", 0, 0)],
    listed: 3,
    decision: { pick: "team", order: ["team", "lab"], why: [{ account: "lab", why: "夹具·lab 的理由" }], fromTemp: false, skipped: [], temp: [] },
    timeline: {
      switches: [{ at: NOW + 5400, atText: "16:00", account: "lab" }],
      uses: [
        { from: NOW, to: NOW + 5400, account: "team", fromTemp: false, capUnknown: false, skipped: [], text: "team" },
        { from: NOW + 5400, to: NOW + 9000, account: "lab", fromTemp: true, capUnknown: false, skipped: ["work"], text: "lab" },
        { from: NOW + 9000, to: NOW + 43_200, fromTemp: false, capUnknown: false, skipped: [], text: copyText("rot.sim.hold") },
      ],
      lanes: [
        { account: "team", segs: [{ from: NOW, to: NOW + 3600, sendable: true, reset: false, text: "夹具·剩 42" }, { from: NOW + 3600, to: NOW + 43_200, sendable: true, reset: true, text: "夹具·↻15:30" }] },
        { account: "work", segs: [{ from: NOW, to: NOW + 43_200, sendable: false, reset: false, text: "" }] },
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
    { id: "按权重轮换", description: "第一句说明\n第二句说明", summary: "第一句说明", rev: "h:1", inUse: ["s1", "s2"], forNew: true, wordsBad: false, tzUnknown: false, hand: false },
    { id: "夜里省着用", description: "夜里那一句", summary: "夜里那一句", rev: "h:2", inUse: [], forNew: false, wordsBad: false, tzUnknown: false, hand: true },
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
    create: vi.fn(),
    add: vi.fn(),
    edit: vi.fn(),
    hours: vi.fn(),
    open: vi.fn(),
    showFile: vi.fn(),
    setTz: vi.fn(),
    loadNew: vi.fn(),
    toggleOld: vi.fn(),
  } satisfies BoardActs;
}

const state = (over: Partial<BoardState> = {}): BoardState => ({ hours: 12, open: null, ...over });

function draw(v: RotationViewReply | null, s: BoardState, a: BoardActs): HTMLElement {
  const root = board({ view: v, list: LIST, state: s, acts: a, origin: "<local>", machine: "夹具机" });
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

  it("★ 第一行右边写「设置 · 那台」，没有［编辑策略］（策略就在这里改）", () => {
    const root = draw(view(), state(), acts());
    expect(button(root, copyText("rot.top.edit"))).toBeUndefined();
    expect(root.textContent).toContain(copyText("rot.top.here", { machine: "夹具机" }));
  });

  it("★ 新会话不轮换：第一行 ＋「只用开会话的号」＋［新建策略…］，号牌 · 设置 · 模拟都不出", () => {
    const a = acts();
    const root = draw(view({ strategy: null, board: { all: [], rows: [] }, decision: null, timeline: undefined, accounts: [], listed: 0 }), state(), a);
    expect(root.textContent).toContain(copyText("rot.none.desc"));
    expect(root.querySelector("[data-account]")).toBeNull();
    expect(root.textContent).not.toContain(copyText("rot.h.sim"));
    button(root, copyText("rot.more.create"))!.click();
    expect(a.create).toHaveBeenCalledOnce();
  });

  it("★ 提示照后端写好的那一句；停发那一句里的「还有多久」在画的那一刻照 now · until 填", () => {
    const root = draw(
      view({ hints: [{ hint: { k: "hold", until: NOW + 4 * 3600, untilText: "18:30", account: "team" }, tone: "warn", text: "停发了 · 18:30 team · 还剩 {dur}" }, { hint: { k: "wordsBad" }, tone: "plain", text: "说明写法不合规那一句" }] }),
      state(),
      acts(),
    );
    const said = texts(root, "[data-rot-hint]");
    expect(said[0]).toContain("18:30 team");
    expect(said[0]).not.toContain("{dur}");
    expect(said[1]).toContain("说明写法不合规那一句");
  });
});

describe("第一行：下拉与「⋯」", () => {
  const items = (): HTMLButtonElement[] => [...document.querySelectorAll<HTMLButtonElement>('[role^="menuitem"]')];
  const menu = (): string[] => items().map((e) => e.textContent ?? "");
  it("★ 「⋯」：复制 · 改名 · 设为新会话用这份（已是 ⇒ 灰）· 删除（有会话在用 ⇒ 灰）· 新建策略 · 从文件添加；选中的是新会话用的 ⇒ 小标", () => {
    const a = acts();
    const root = draw(view(), state(), a);
    expect(root.textContent).toContain(copyText("rot.tag.forNewHere"));
    root.querySelector<HTMLButtonElement>(`button[aria-label="${copyText("rot.more.label")}"]`)!.click();
    const said = menu();
    for (const k of ["render.codeBlock.copy", "rot.more.rename", "rot.more.forNew", "rsFilewinWriteops.label.delete", "rot.more.create", "rot.more.add"] as const)
      expect(said.some((t) => t.startsWith(copyText(k))), k).toBe(true);
    const item = (k: Parameters<typeof copyText>[0]) => items().find((e) => e.textContent?.startsWith(copyText(k)))!;
    expect(item("rot.more.forNew").disabled).toBe(true);
    expect(item("rsFilewinWriteops.label.delete").disabled).toBe(true);
    item("rot.more.create").click();
    expect(a.create).toHaveBeenCalledOnce();
  });

  it("★ 下拉：每份带「新会话用」「手写」小标（后端给的）；末尾「新建策略…」", () => {
    const a = acts();
    const root = draw(view(), state(), a);
    root.querySelector<HTMLButtonElement>(`button[aria-label="${copyText("rot.top.pick")}"]`)!.click();
    const said = menu();
    expect(said.find((t) => t.startsWith("夜里省着用"))).toContain(copyText("rot.tag.hand"));
    expect(said.find((t) => t.startsWith("按权重轮换"))).not.toContain(copyText("rot.tag.hand"));
    items().find((e) => e.textContent === copyText("rot.more.create"))!.click();
    expect(a.create).toHaveBeenCalledOnce();
  });

  it("★ 总体的开关：说明写在旁边", () => {
    const smart: RotCell = { key: "smart", kind: "toggle", label: "夹具·智能", help: "夹具·值得才换", v: true, from: "own" };
    const root = draw(view({ board: { ...BOARD, all: [smart] } }), state(), acts());
    expect(root.textContent).toContain("夹具·值得才换");
  });
});

describe("号色 · 号名照视图里那一格", () => {
  it("★ 号牌的号色 · 号名 · 号牌字都照 rotation-view 每个号身上那一枚（不另问账号清单去对）", () => {
    const work = { ...fact("work", 94, 40, true), plate: { color: 6, label: "夹具·号名", text: "夹" } };
    const v = view({ accounts: [work, fact("team", 58, 30), fact("lab", 35, 20), fact("api", 0, 0)] });
    const root = draw(v, state(), acts());
    const card = [...root.querySelectorAll<HTMLElement>(".acct-avatar")].find((e) => e.parentElement?.textContent?.includes("夹具·号名"));
    expect(card, "画了视图里的号名").toBeDefined();
    expect(card!.classList.contains("acct-c6")).toBe(true);
    expect(card!.textContent).toBe("夹");
  });
});

describe("提示里的按钮", () => {
  it("★ 出错 ⇒ 看代码（开后端给的那份文件）；时区不明 ⇒ 设时区（设成后端给的那个，悬停写设成哪个）；没给 ⇒ 不出按钮", () => {
    const a = acts();
    const root = draw(
      view({
        hints: [
          { hint: { k: "strategyError", error: { line: 12, msg: "Division by zero: 1 / 0" }, path: "/夹具/strategies/夜里省着用.rhai" }, tone: "fail", text: "出错那一句" },
          { hint: { k: "tzUnknown", zone: "Asia/Shanghai" }, tone: "need", text: "时区那一句" },
        ],
      }),
      state(),
      a,
    );
    button(root, copyText("rot.hint.code"))!.click();
    expect(a.showFile).toHaveBeenCalledWith("/夹具/strategies/夜里省着用.rhai");
    const tz = button(root, copyText("rot.hint.setTz"))!;
    expect(tz.title).toBe(copyText("rot.hint.setTzTip", { zone: "Asia/Shanghai" }));
    tz.click();
    expect(a.setTz).toHaveBeenCalledWith("Asia/Shanghai");
    const bare = draw(view({ hints: [{ hint: { k: "strategyError", error: { msg: "文件没了" } }, tone: "fail", text: "夹具·出错那一句" }] }), state(), acts());
    expect(button(bare, copyText("rot.hint.code"))).toBeUndefined();
  });

  it("★ 出错那一条带复制详情（原话，后端给的）；别的提示没有", () => {
    const root = draw(
      view({
        hints: [
          { hint: { k: "strategyError", error: { line: 12, msg: "Division by zero: 1 / 0" } }, tone: "fail", text: "夹具·出错那一句", detail: "Division by zero: 1 / 0" },
          { hint: { k: "tzUnknown", zone: "UTC" }, tone: "need", text: "时区那一句" },
        ],
      }),
      state(),
      acts(),
    );
    const err = root.querySelector<HTMLElement>('[data-rot-hint="strategyError"]')!;
    expect(err.querySelector('[data-part="copy-detail"]')).not.toBeNull();
    expect(err.textContent).not.toContain("Division");
    const tz = root.querySelector<HTMLElement>('[data-rot-hint="tzUnknown"]')!;
    expect(tz.querySelector('[data-part="copy-detail"]')).toBeNull();
  });

  it("★ 旧格式 ⇒ 看是哪几份：展开后列出后端给的文件名，点名字开那一份", () => {
    const a = acts();
    const old = { hint: { k: "oldScripts" as const, n: 2, files: [{ name: "旧的一份.rhai", path: "/夹具/v3/旧的一份.rhai" }, { name: "旧的两份.rhai", path: "/夹具/v3/旧的两份.rhai" }] }, tone: "need" as const, text: "2 份旧格式" };
    const shut = draw(view({ hints: [old] }), state(), a);
    expect(button(shut, "旧的一份.rhai")).toBeUndefined();
    button(shut, copyText("rot.hint.which"))!.click();
    expect(a.toggleOld).toHaveBeenCalledOnce();
    const open = draw(view({ hints: [old] }), state({ oldOpen: true }), a);
    button(open, "旧的两份.rhai")!.click();
    expect(a.showFile).toHaveBeenCalledWith("/夹具/v3/旧的两份.rhai");
  });

  it("★ 「N 个会话还在用旧版」不在顶上出条，写在编辑小窗的底行［都载入新版］⇒ loadNew", () => {
    const a = acts();
    const older = view({ hints: [{ hint: { k: "older", n: 2 }, tone: "plain", text: "夹具·2 个会话那一句" }] });
    const set = draw(older, state(), a);
    expect(set.querySelector("[data-rot-hint]"), "不在顶上出条").toBeNull();
    const pane = draw(older, state({ open: "lab" }), a).querySelector<HTMLElement>("[data-rot-editor]")!;
    expect(pane.textContent).toContain(copyText("rot.pane.foot", { name: "按权重轮换" }));
    expect(pane.textContent).toContain("夹具·2 个会话那一句");
    button(pane, copyText("rot.hint.loadAll"))!.click();
    expect(a.loadNew).toHaveBeenCalledOnce();
  });
});

describe("号牌与点开一个号", () => {
  it("★ 号牌照号列表的先后（列表外的不出），用不了的变淡；点一下翻成 open", () => {
    const a = acts();
    const root = draw(view(), state(), a);
    const cards = [...root.querySelectorAll<HTMLElement>("[data-account]")];
    expect(cards.map((c) => c.dataset.account)).toEqual(["work", "team", "lab"]);
    expect(cards[0].dataset.blocked).toBe("true");
    cards[2].dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, clientX: 10, clientY: 10 }));
    document.dispatchEvent(new MouseEvent("pointerup", { bubbles: true, clientX: 10, clientY: 10 }));
    expect(a.open).toHaveBeenCalledWith("lab");
  });

  it("★ 号牌上那条小计量画不画满照后端说的用满了的那几窗（不照用量自己比）", () => {
    const v = view({ accounts: [{ ...fact("work", 94, 40), full: ["7d"] }, { ...fact("team", 100, 30), full: [] }, fact("lab", 35, 20)] });
    const root = draw(v, state(), acts());
    const fills = (account: string) => [...root.querySelector<HTMLElement>(`[data-account="${account}"]`)!.querySelectorAll<HTMLElement>("i")].map((e) => e.dataset.full ?? "");
    expect(fills("work")).toEqual(["", "true"]);
    expect(fills("team")).toEqual(["", ""]);
  });

  it("★ 拖第 1 张松在第 3 张上 ⇒ 交「第 0 行挪到第 2 行前头」那个意图（怎么排是后端的）", () => {
    const a = acts();
    const root = draw(view(), state(), a);
    const cards = [...root.querySelectorAll<HTMLElement>("[data-account]")];
    cards.forEach((c, k) => (c.getBoundingClientRect = () => ({ left: k * 100, right: k * 100 + 90, top: 0, bottom: 30, width: 90, height: 30, x: k * 100, y: 0, toJSON: () => ({}) })));
    cards[0].dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, clientX: 40, clientY: 10 }));
    document.dispatchEvent(new MouseEvent("pointermove", { bubbles: true, clientX: 140, clientY: 10 }));
    document.dispatchEvent(new MouseEvent("pointerup", { bubbles: true, clientX: 240, clientY: 10 }));
    expect(a.edit.mock.calls).toEqual([[{ move: { from: 0, to: 2 } }]]);
    expect(a.open).not.toHaveBeenCalled();
  });

  it("★ 点开 lab：那一行的字与格照回包（标签是后端挑好的字，跟总体的画虚线）；改开关 ⇒ 交「lab 那一行设这一格」", () => {
    const a = acts();
    const root = draw(view(), state({ open: "lab" }), a);
    const pane = root.querySelector<HTMLElement>('[data-account="lab"]:not(button)')!;
    const keys = [...new Set(texts(pane, "span").filter((t) => t.startsWith("夹具")))];
    expect(keys).toEqual(["夹具·lab 那一行", "夹具·上限 5h", "夹具·90", "夹具·上限 7d", "夹具·开关", "夹具·延迟"]);
    expect([...pane.querySelectorAll<HTMLElement>('[data-inherited="true"]')].map((e) => e.textContent), "只有 from = all 的画虚线").toEqual(["夹具·上限 5h", "夹具·90", "夹具·上限 7d", "夹具·90"]);
    pane.querySelector<HTMLButtonElement>('[role="switch"]')!.click();
    expect(a.edit.mock.calls).toEqual([[{ set: { key: "fallback", v: false, row: 2 } }]]);
  });

  it("★ 分段一段一行：改一格 ⇒ 交「改第几段」整段；✕ ⇒ 删那一段；＋ 加一段 ⇒ 三格填好才交；上面那条只画（照回包的字）", () => {
    const a = acts();
    const root = draw(view(), state({ open: "lab" }), a);
    const pane = root.querySelector<HTMLElement>("[data-rot-editor]")!;
    expect(pane.querySelector("[data-day-bar]")!.textContent).toBe("夹具·90");
    const line = pane.querySelector<HTMLElement>('[data-rot-seg="0"]')!;
    const [from, to, v] = [...line.querySelectorAll<HTMLInputElement>("input")];
    expect([from.value, to.value, v.value], "两格钟点照回包写好的字").toEqual(["00:00", "24:00", "90"]);
    v.value = "70";
    v.dispatchEvent(new Event("change"));
    to.value = "20:00";
    to.dispatchEvent(new Event("change"));
    line.querySelector<HTMLButtonElement>(`button[aria-label="${copyText("rot.seg.del")}"]`)!.click();
    const add = button(pane, copyText("rot.seg.add"))!;
    add.click();
    const fresh = pane.querySelector<HTMLElement>('[data-rot-seg="new"]')!;
    const [nf, nt, nv] = [...fresh.querySelectorAll<HTMLInputElement>("input")];
    nf.value = "20:00";
    nf.dispatchEvent(new Event("change"));
    nt.value = "23:00";
    nv.value = "0";
    nv.dispatchEvent(new Event("change"));
    expect(a.edit.mock.calls).toEqual([
      [{ seg: { key: "caps", window: "5h", row: 2, at: 0, seg: { from: "00:00", to: "24:00", v: 70 } } }],
      [{ seg: { key: "caps", window: "5h", row: 2, at: 0, seg: { from: "00:00", to: "20:00", v: 70 } } }],
      [{ seg: { key: "caps", window: "5h", row: 2, at: 0 } }],
      [{ seg: { key: "caps", window: "5h", row: 2, seg: { from: "20:00", to: "23:00", v: 0 } } }],
    ]);
  });

  it("★ 加时段的草稿住 state：打的字记进去、重画照它补画（一格一窗只一行）；草稿的 ✕ 只删草稿不发；这一次没画到的（切回同第一窗）画完删掉", () => {
    const a = acts();
    const st = state({ open: "lab" });
    let pane = draw(view(), st, a).querySelector<HTMLElement>("[data-rot-editor]")!;
    button(pane, copyText("rot.seg.add"))!.click();
    const nf = pane.querySelector<HTMLInputElement>('[data-rot-seg="new"] input')!;
    nf.value = "20:00";
    nf.dispatchEvent(new Event("input"));
    const k5 = draftKey("lab", "caps", "5h");
    expect(st.drafts).toEqual({ [k5]: { from: "20:00", to: "", v: "" } });
    pane = draw(view(), st, a).querySelector<HTMLElement>("[data-rot-editor]")!;
    const lines = () => [...pane.querySelectorAll<HTMLElement>('[data-rot-seg="new"]')];
    expect(lines().map((r) => r.querySelector("input")!.value)).toEqual(["20:00"]);
    button(pane, copyText("rot.seg.add"))!.click();
    expect(lines().length, "再点加时段 ⇒ 聚焦已有那一行，不再开一行").toBe(1);
    expect(document.activeElement).toBe(lines()[0].querySelector("input"));
    lines()[0].querySelector<HTMLButtonElement>(`button[aria-label="${copyText("rot.seg.del")}"]`)!.click();
    expect(lines()).toEqual([]);
    expect(st.drafts).toEqual({});
    expect(a.edit).not.toHaveBeenCalled();
    st.drafts = { [k5]: { from: "1:00", to: "", v: "" } };
    const rows = [row("work", each(false, "default")), row("team", each(false, "default")), row("lab", [{ ...caps("5h", "own"), same: "夹具·同第一窗", from: "same" }])];
    draw(view({ board: { ...BOARD, rows } }), st, a);
    expect(st.drafts).toEqual({});
  });

  it("★ 后面那一窗：［同 5 小时 · 另设］照回包（字是后端给的），同着时不出段；挂在别格下的缩进；前面那半句 · 「或 ［一有额度］」照声明", () => {
    const a = acts();
    const w7: RotCell = { ...caps("7d", "same"), same: "夹具·同第一窗" };
    const total: RotCell = { key: "outTotal", kind: "percent", label: "夹具·切出条件", v: null, num: 400, from: "own", nullable: "夹具·一有额度", lead: "夹具·前半句", under: "out", min: 0, max: 1000 };
    const rows = [row("work", each(false, "default")), row("team", each(false, "default")), row("lab", [caps("5h", "own"), w7, total])];
    const root = draw(view({ board: { ...BOARD, rows } }), state({ open: "lab" }), a);
    const pane = root.querySelector<HTMLElement>("[data-rot-editor]")!;
    expect(pane.querySelectorAll("[data-rot-seg]").length, "同着的那一窗不出段").toBe(1);
    expect(button(pane, "夹具·同第一窗")!.getAttribute("aria-checked")).toBe("true");
    button(pane, copyText("rot.cell.own"))!.click();
    const key = [...pane.querySelectorAll<HTMLElement>("[data-under]")].map((e) => [e.dataset.under, e.textContent]);
    expect(key).toEqual([["out", "夹具·切出条件"]]);
    expect(pane.textContent).toContain("夹具·前半句");
    button(pane, "夹具·一有额度")!.click();
    expect(a.edit.mock.calls).toEqual([[{ same: { key: "caps", window: "7d", row: 2, same: false } }], [{ set: { key: "outTotal", v: 400, row: 2 } }]]);
  });

  it("★ 号名旁的记号照后端给的（字形 · 悬停；虚线框那一种画在号牌上）", () => {
    const marks: RotRow["marks"] = [{ glyph: "↺", dashed: false, tip: "夹具·切入 · 强制切入" }, { dashed: true, tip: "夹具·延迟切入" }];
    const rows = [row("work", each(false, "default")), row("team", each(false, "default")), row("lab", each(true, "own"), marks)];
    const root = draw(view({ board: { ...BOARD, rows } }), state(), acts());
    const lab = root.querySelector<HTMLElement>('[data-account="lab"]')!;
    expect(texts(lab, "[data-rot-mark]")).toEqual(["↺"]);
    expect(lab.dataset.dashed).toBe("true");
    expect(root.querySelector<HTMLElement>('[data-account="team"]')!.dataset.dashed).toBeUndefined();
  });

  it("★ 行号照那一行带的（不照第几张号牌）：号列表第 1 行这台没出 ⇒ lab 是第二张号牌、第 2 行；改 · 移出 · 拖都交第 2 行", () => {
    const v = view({
      board: { all: BOARD.all, rows: [row("work", each(false, "default"), [], 0), row("lab", each(true, "own"), [], 2)] },
      accounts: [fact("work", 94, 40), fact("lab", 35, 20), fact("api", 0, 0)],
      listed: 2,
    });
    const a = acts();
    const root = draw(v, state({ open: "lab" }), a);
    const pane = root.querySelector<HTMLElement>("[data-rot-editor]")!;
    pane.querySelector<HTMLButtonElement>('[role="switch"]')!.click();
    button(pane, copyText("rot.acct.remove"))!.click();
    const cards = [...root.querySelectorAll<HTMLElement>("button[data-rot-card]")];
    cards.forEach((c, k) => (c.getBoundingClientRect = () => ({ left: k * 100, right: k * 100 + 90, top: 0, bottom: 30, width: 90, height: 30, x: k * 100, y: 0, toJSON: () => ({}) })));
    cards[1].dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, clientX: 140, clientY: 10 }));
    document.dispatchEvent(new MouseEvent("pointermove", { bubbles: true, clientX: 40, clientY: 10 }));
    document.dispatchEvent(new MouseEvent("pointerup", { bubbles: true, clientX: 40, clientY: 10 }));
    expect(a.edit.mock.calls).toEqual([[{ set: { key: "fallback", v: false, row: 2 } }], [{ remove: { row: 2 } }], [{ move: { from: 2, to: 0 } }]]);
  });

  it("★ 小窗的 ✕ ⇒ 收起；［移出这份策略］⇒ 交「移出那一行」", () => {
    const a = acts();
    const pane = draw(view(), state({ open: "lab" }), a).querySelector<HTMLElement>("[data-rot-editor]")!;
    button(pane, copyText("rot.acct.remove"))!.click();
    pane.querySelector<HTMLButtonElement>(`button[aria-label="${copyText("kit.drawer.close")}"]`)!.click();
    expect(a.edit.mock.calls).toEqual([[{ remove: { row: 2 } }]]);
    expect(a.open).toHaveBeenCalledWith(null);
  });
});

describe("总体设置", () => {
  it("★ 照 board.all 的先后画；可空的那一格点空值说法 ⇒ 交 null，再点 ⇒ 交回框里那个数（后端给的 num）", () => {
    const a = acts();
    const root = draw(view(), state(), a);
    expect(root.textContent).toContain("夹具·容差");
    expect(root.textContent).toContain("夹具·阈值");
    button(root, "夹具·空值")!.click();
    expect(a.edit.mock.calls).toEqual([[{ set: { key: "force", v: null } }]]);
    const b = acts();
    const force: RotCell = { ...BOARD.all[1], v: null, num: 30 };
    const again = draw(view({ board: { ...BOARD, all: [BOARD.all[0], force] } }), state(), b);
    button(again, "夹具·空值")!.click();
    expect(b.edit.mock.calls).toEqual([[{ set: { key: "force", v: 30 } }]]);
  });
});

describe("模拟（设置页：改一格就看得到接下来怎么换）", () => {
  it("★ 时刻的字全照回包：此刻 · 刻度 · 换号时刻；每号行此刻那段写剩多少、重置那段写 ↻ 钟面、发不出去的不写字", () => {
    const root = draw(view(), state(), acts());
    expect(root.textContent).toContain("14:30");
    expect(root.textContent).toContain("17:00");
    expect(root.textContent, "挨着此刻的刻度让开").not.toContain("15:00");
    expect(root.textContent).toContain(copyText("rot.sim.switch", { at: "16:00", account: "lab" }));
    const segs = texts(root, '[data-state]');
    expect(segs, "每段的字照回包").toEqual(["夹具·剩 42", "夹具·↻15:30", ""]);
  });

  it("★ 拖出来的那段白描边（data-temp）· 轮到时用不了的写「跳过」· 停发那段写「停下」；没有拖号名的落点（时间轴上不拖）", () => {
    const root = draw(view(), state(), acts());
    expect(root.querySelectorAll('[data-temp="true"]').length).toBe(1);
    expect(root.textContent).toContain(copyText("rot.sim.skip", { account: "work" }));
    expect(root.querySelector('[data-hold="true"]')?.textContent).toBe(copyText("rot.sim.hold"));
    expect(root.querySelector('[data-drag="true"]')).toBeNull();
  });
});

describe("恢复默认 · 清掉库里没有的号", () => {
  const resets = (root: HTMLElement): string[] => [...root.querySelectorAll<HTMLElement>("[data-reset]")].map((e) => e.dataset.reset ?? "");

  it("★ 总体那一格后端标了 reset ⇒ 旁边有［恢复默认］，点了交 {reset: {key}}；没标的（哪怕 from: own）没有", () => {
    const a = acts();
    const { reset: _, ...bare } = BOARD.all[0];
    const unmarked: RotCell = { ...bare, key: "spare", label: "夹具·没标的" };
    const root = draw(view({ board: { ...BOARD, all: [BOARD.all[0], unmarked] } }), state(), a);
    expect(resets(root)).toEqual(["close"]);
    const b = root.querySelector<HTMLButtonElement>('[data-reset="close"]')!;
    expect(b.textContent?.trim()).toBe(copyText("rot.cell.reset"));
    b.click();
    expect(a.edit.mock.calls).toEqual([[{ reset: { key: "close" } }]]);
  });

  it("★ 号那一行：标了 reset 的那一格 ⇒ 交 {reset: {key, row}}（行号照那一行带的）；没标的那几格没有", () => {
    const a = acts();
    const pane = draw(view(), state({ open: "lab" }), a).querySelector<HTMLElement>("[data-rot-editor]")!;
    expect(resets(pane)).toEqual(["fallback"]);
    pane.querySelector<HTMLButtonElement>('[data-reset="fallback"]')!.click();
    expect(a.edit.mock.calls).toEqual([[{ reset: { key: "fallback", row: 2 } }]]);
    const work = draw(view(), state({ open: "work" }), acts()).querySelector<HTMLElement>("[data-rot-editor]")!;
    expect(resets(work)).toEqual([]);
  });

  it("★ 分段：照每窗那一格的 reset 出（清的是整格）；后面那一窗另设的后端不标，不另出", () => {
    const own5: RotCell = { ...each(false, "default")[0], from: "own", reset: true };
    const own7: RotCell = { ...each(false, "default")[1], from: "own", same: "夹具·同 5h" };
    const v = view({ board: { ...BOARD, rows: [BOARD.rows[0], BOARD.rows[1], row("lab", [own5, own7])] } });
    const a = acts();
    const pane = draw(v, state({ open: "lab" }), a).querySelector<HTMLElement>("[data-rot-editor]")!;
    expect(resets(pane)).toEqual(["caps"]);
    pane.querySelector<HTMLButtonElement>('[data-reset="caps"]')!.click();
    expect(a.edit.mock.calls).toEqual([[{ reset: { key: "caps", row: 2 } }]]);
  });

  it("★ 号列表里有库里没有的号（missing）⇒ 号牌那一节有［清掉库里没有的号］，点了交 {prune: {}}；都在库里 ⇒ 没有", () => {
    const a = acts();
    const gone = { ...row("team", each(false, "default")), missing: true };
    const root = draw(view({ board: { ...BOARD, rows: [BOARD.rows[0], gone, BOARD.rows[2]] } }), state(), a);
    button(root, copyText("rot.acct.prune"))!.click();
    expect(a.edit.mock.calls).toEqual([[{ prune: {} }]]);
    expect(button(draw(view(), state(), acts()), copyText("rot.acct.prune"))).toBeUndefined();
  });
});
