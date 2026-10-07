/**
 * 会话内查找面板（搜索 / 大纲两个模式）的判据。
 *
 * # 量什么
 *
 * - **面板**：两个入口各开到各的模式（Ctrl+F ⇒ 搜索、焦点进输入框；大纲 ⇒ 大纲、同一个入口再按收起）·
 *   ✕ 与 Esc（快捷键弹层栈）收起、焦点还回去、跳过去的高亮去掉 · 切走 tab 收起并出栈。
 * - **搜索**：停 300ms 自己找、回车立刻找、组字中的回车不算 · 参数逐字（问的是这个 tab 那份会话）· 每条「谁 · 第几轮 · 时刻」·
 *   全量 > 条数时说出来、滚到底续下一页（`skip`）· 回车 / F3 / ↑↓ 选下一条并跳 · 查不了说原因 · 迟到的旧结果不盖新结果。
 * - **跳**：没加载的那一条写「未加载 · 加载后跳转」，取到就跳；取失败写原因 ＋［重试］。
 * - **F4 跳得准**：命中一条还在骨架占位里、正文已被丢出前端账本（U3b 之后只留尾巴 200 条）的记录 ⇒
 *   等按偏移取回的正文落完、卡建出来、滚过去，**不标**「跳不过去」。这一格在「同步那一下就去找卡」的旧形上必红。
 *
 * # 台子
 *
 * 渲染管线是**真的**（同 `live-user-inputs.vitest.ts`）；只 mock 掉 IPC 与几个纯外部动作。
 * 查找的命中、骨架索引、按偏移取回的正文都由用例逐条写明（异源：不拿前端算出来的去对前端）。
 * **买不到**：真 webview 里的滚动落点与面板长相（jsdom 无布局）· 真后端（后端那侧的判据在 Rust 里）。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const stub = vi.hoisted(() => ({
  /** `find_in_session` 回什么（按调用顺序一份一份给；给的是 Promise 就原样回）。 */
  finds: [] as unknown[],
  /** 骨架索引：`null` ⇒ 回 `undefined`（索引要不到）。 */
  indexRows: null as null | Array<Record<string, unknown>>,
}));

vi.mock("@tauri-apps/api/core", async () => {
  const rig = await import("../../../test-support/session-viewer-rig");
  const { withSessionReads } = await import("../../../test-support/chan-fake");
  return {
    // 三问改走通道：`withSessionReads` 把一发 `chan_call` 译回「哪一问 ＋ 旧形参」、把回包译成后端成品字节。
    invoke: vi.fn(withSessionReads(async (cmd: string, args: Record<string, unknown>) => {
      if (cmd === "list_user_inputs") return rig.answerListUserInputs(args as { fromOffset: number });
      if (cmd === "find_in_session") return stub.finds.shift();
      if (cmd === "read_session_index" && stub.indexRows && args.fromOffset === 0) {
        const rows = stub.indexRows;
        return { available: true, from: 0, end: rows.length * 10, rows };
      }
      if (cmd === "read_session_range") {
        // 请求里不再带 `lineCount`（后端自己数）：这一段有几行按夹具的行边界算（o = seq × 10、n = 10）。
        const a = args as { seqBase: number; offset: number; until: number };
        return Array.from({ length: (a.until - a.offset) / 10 }, (_, k) =>
          rig.assistantLine(a.seqBase + k, `u${a.seqBase + k}`, `第 ${a.seqBase + k} 条（按偏移取回的）`),
        );
      }
      return undefined;
    })),
  };
});
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../../../src/frontend/ui/tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("../../../../src/frontend/ui/turn-notify", () => ({ turnEndNotifier: { observe: vi.fn() } }));
vi.mock("../../../../src/frontend/ui/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ resumeCommand: "cct" }),
}));
vi.mock("../../../../src/frontend/ui/remote-launch-run", () => ({
  runRemoteResume: vi.fn().mockResolvedValue(undefined),
  runRemoteResumeTmux: vi.fn().mockResolvedValue(undefined),
  runLocalResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteAttach: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { invoke } from "@tauri-apps/api/core";
import { sessionReadCalls } from "../../../test-support/chan-fake";
import {
  installViewerRig,
  assistantLine,
  userLine,
  withSession,
  settleOutline,
  type RigPayload,
} from "../../../test-support/session-viewer-rig";
import { TabManager, type Tab } from "../../../../src/frontend/ui/tabs";
import { SessionFindPanel, type SessionFindHost } from "../../../../src/frontend/ui/views/session-find";
import { dispatcher } from "../../../../src/frontend/ui/keybindings/registry";
import type { FindResult } from "../../../../src/frontend/ui/session-reads";

const hit = (uuid: string, matched = "needle", before = "a ", after = " b", turn = 0, tsMs = 0) => ({
  uuid,
  kind: "assistant",
  before,
  matched,
  after,
  turn,
  tsMs,
});
const found = (hits: ReturnType<typeof hit>[], total = hits.length): FindResult => ({
  available: true,
  hits,
  total,
});

// ── 单元：面板本身（宿主是替身） ─────────────────────────────────────────────

function panelWith(over: Partial<SessionFindHost> = {}): {
  p: SessionFindPanel;
  search: ReturnType<typeof vi.fn>;
  jumpTo: ReturnType<typeof vi.fn>;
} {
  const search = vi.fn(async () => found([]));
  const jumpTo = vi.fn(() => null);
  const p = new SessionFindPanel({ search, jumpTo, unjumpableHint: "无对应卡片", ...over } as SessionFindHost);
  document.body.appendChild(p.el);
  return { p, search, jumpTo };
}
const q = (p: SessionFindPanel, sel: string) => p.el.querySelector<HTMLElement>(sel)!;
const input = (p: SessionFindPanel) => q(p, "[data-role=find-input]") as HTMLInputElement;
const box = (p: SessionFindPanel) => q(p, "[data-role=session-find-panel]");
const status = (p: SessionFindPanel) => q(p, "[data-role=find-head]").textContent;
const press = (p: SessionFindPanel, init: KeyboardEventInit): void => {
  input(p).dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init }));
};
const enter = (p: SessionFindPanel, text: string): void => {
  input(p).value = text;
  press(p, { key: "Enter" });
};
const hitRows = (root: ParentNode) => [...root.querySelectorAll<HTMLElement>("[data-role=find-hit]")];
const stateOf = (row: HTMLElement) => row.querySelector<HTMLElement>("[data-role=find-state]")!;

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  installViewerRig();
  stub.finds = [];
  stub.indexRows = null;
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("会话内查找面板", () => {
  it("🔴 两个入口各开到各的模式；大纲入口再按一次收起；✕ 收起", () => {
    const { p } = panelWith();
    expect(box(p).hidden, "默认收着").toBe(true);
    p.open("search"); // Ctrl+F 那条路
    expect(box(p).hidden).toBe(false);
    expect(p.outline.panel.hidden, "搜索模式下大纲清单不露").toBe(true);
    expect(document.activeElement, "焦点进输入框").toBe(input(p));
    p.outline.toggle.disabled = false;
    p.outline.toggle.click(); // 大纲入口：开着「搜索」时 ⇒ 切到「大纲」
    expect(box(p).hidden).toBe(false);
    expect(p.outline.panel.hidden).toBe(false);
    expect(p.currentMode).toBe("outline");
    p.outline.toggle.click(); // 同一个入口再按 ⇒ 收起
    expect(box(p).hidden).toBe(true);
    p.open("outline");
    q(p, "[aria-label='关闭']").click();
    expect(box(p).hidden).toBe(true);
  });

  it("页签：点哪个切哪个，`aria-selected` 跟着走", () => {
    const { p } = panelWith();
    p.open("outline");
    const tabs = [...p.el.querySelectorAll<HTMLButtonElement>("[role=tab]")];
    expect(tabs.map((t) => t.dataset.key)).toEqual(["search", "outline"]);
    expect(tabs.map((t) => t.getAttribute("aria-selected"))).toEqual(["false", "true"]);
    tabs[0].click();
    expect(tabs.map((t) => t.getAttribute("aria-selected"))).toEqual(["true", "false"]);
    expect(p.currentMode).toBe("search");
    expect(p.outline.panel.hidden).toBe(true);
  });

  it("🔴 Esc 收起：出弹层栈、焦点回到打开前的地方、跳过去的高亮去掉", () => {
    dispatcher.applyOverrides({}); // 建默认键位表（main.ts 启动时做的同一步）
    dispatcher.start();
    const before = document.createElement("button");
    document.body.appendChild(before);
    before.focus();
    const flashed = document.createElement("div");
    flashed.className = "search-hit-flash";
    document.body.appendChild(flashed);
    const { p } = panelWith();
    p.open("search");
    const esc = () => press(p, { key: "Escape", code: "Escape" });
    esc();
    expect(box(p).hidden).toBe(true);
    expect(document.activeElement, "焦点没还回去").toBe(before);
    expect(flashed.classList.contains("search-hit-flash"), "关了高亮还在").toBe(false);
    esc();
    expect(box(p).hidden).toBe(true);
    p.open("outline");
    expect(box(p).hidden).toBe(false);
    esc();
    expect(box(p).hidden).toBe(true);
  });
});

describe("会话内查找 · 搜索", () => {
  it("🔴 停 300ms 自己找（打字中途不发）；回车立刻找；组字中的回车不算", async () => {
    vi.useFakeTimers();
    const s = vi.fn(async () => found([hit("u1")]));
    const { p } = panelWith({ search: s } as Partial<SessionFindHost>);
    p.open("search");
    input(p).value = "nee";
    input(p).dispatchEvent(new Event("input"));
    await vi.advanceTimersByTimeAsync(200);
    input(p).value = "needle";
    input(p).dispatchEvent(new Event("input"));
    await vi.advanceTimersByTimeAsync(299);
    expect(s, "停够 300ms 之前不许发").not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(s.mock.calls).toEqual([["needle", false, 0]]);
    input(p).value = "qie";
    press(p, { key: "Enter", isComposing: true });
    await vi.advanceTimersByTimeAsync(0);
    expect(s).toHaveBeenCalledTimes(1);
    input(p).value = "切到";
    press(p, { key: "Enter" });
    await vi.advanceTimersByTimeAsync(0);
    expect(s).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(400);
    expect(s, "回车找过了，排着的那一次不许再发").toHaveBeenCalledTimes(2);
  });

  it("🔴 带着「含工具内容」那个勾；命中按后端给的顺序列、头一行「谁 · 第几轮 · 时刻」、<mark> 里是原文", async () => {
    const at = new Date(2026, 9, 6, 1, 52).getTime();
    const s = vi.fn(async () => found([{ ...hit("u9", "NeedLe", "前 ", " 后", 3, at), kind: "user" }, hit("u2"), { ...hit("u4"), kind: "report", turn: 2 }]));
    const { p } = panelWith({ search: s } as Partial<SessionFindHost>);
    p.open("search");
    (p.el.querySelector("input[type=checkbox]") as HTMLInputElement).click();
    enter(p, "  needle  ");
    await settleOutline();
    expect(s.mock.calls.at(-1)).toEqual(["needle", true, 0]);
    const rows = hitRows(p.el);
    expect(rows.map((r) => r.dataset.hitUuid)).toEqual(["u9", "u2", "u4"]);
    expect(rows[0].firstElementChild!.textContent).toMatch(/^你 · 第 3 轮 · /);
    expect(rows[1].firstElementChild!.textContent, "第一句之前的不写轮").toBe("Claude");
    expect(rows[2].firstElementChild!.textContent, "子 agent 交回的正文命中单列一种").toBe("agent 回报 · 第 2 轮");
    expect(rows[0].querySelector("mark")!.textContent).toBe("NeedLe");
    expect(rows[0].hasAttribute("data-uuid"), "命中行不许叫 data-uuid（那是消息卡的名字）").toBe(false);
    expect(status(p)).toBe("3 条");
  });

  it("🔴 全量 > 条数 ⇒ 说出来，滚到底续下一页（skip ＝ 已列的条数）", async () => {
    const pages: FindResult[] = [found([hit("a"), hit("b")], 3), found([hit("c")], 3)];
    const search = vi.fn(async () => pages.shift()!);
    const { p } = panelWith({ search } as Partial<SessionFindHost>);
    enter(p, "x");
    await settleOutline();
    expect(status(p)).toBe("前 2/3");
    const list = q(p, "[role=listbox]");
    Object.defineProperty(list, "scrollHeight", { value: 100, configurable: true });
    Object.defineProperty(list, "clientHeight", { value: 100, configurable: true });
    list.dispatchEvent(new Event("scroll"));
    await settleOutline();
    expect(search.mock.calls.at(-1)).toEqual(["x", false, 2]);
    expect(hitRows(p.el).map((r) => r.dataset.hitUuid)).toEqual(["a", "b", "c"]);
    expect(status(p)).toBe("3 条");
    list.dispatchEvent(new Event("scroll"));
    await settleOutline();
    expect(search, "列完了不再要").toHaveBeenCalledTimes(2);
  });

  it("零条 ⇒ 无匹配；查不了 ⇒ 说原因；空查询 ⇒ 不发", async () => {
    const results: FindResult[] = [found([]), { available: false, reason: "后端版本旧", hits: [], total: 0 }];
    const search = vi.fn(async () => results.shift()!);
    const { p } = panelWith({ search } as Partial<SessionFindHost>);
    enter(p, "y");
    await settleOutline();
    expect(status(p)).toBe("无匹配「y」");
    enter(p, "z");
    await settleOutline();
    expect(status(p)).toBe("查找失败 · 后端版本旧");
    expect(hitRows(p.el).length).toBe(0);
    enter(p, "   ");
    await settleOutline();
    expect(search).toHaveBeenCalledTimes(2);
  });

  it("🔴 迟到的旧结果不盖新结果", async () => {
    let releaseOld!: (r: FindResult) => void;
    const search = vi
      .fn()
      .mockImplementationOnce(() => new Promise<FindResult>((r) => (releaseOld = r)))
      .mockImplementationOnce(async () => found([hit("new")]));
    const { p } = panelWith({ search } as Partial<SessionFindHost>);
    enter(p, "old");
    enter(p, "new");
    await settleOutline();
    releaseOld(found([hit("old1"), hit("old2")]));
    await settleOutline();
    expect(hitRows(p.el).map((r) => r.dataset.hitUuid)).toEqual(["new"]);
  });

  it("🔴 结果是框里这个词的 ⇒ 回车 / F3 下一条、Shift+F3 上一条、↓ 也走，选中即跳（到头绕回）", async () => {
    const jumpTo = vi.fn((_uuid: string) => document.createElement("div"));
    const { p } = panelWith({ search: vi.fn(async () => found([hit("a"), hit("b"), hit("c")])), jumpTo } as Partial<SessionFindHost>);
    enter(p, "x");
    await settleOutline();
    press(p, { key: "Enter" });
    press(p, { key: "F3" });
    press(p, { key: "ArrowDown" });
    press(p, { key: "ArrowDown" });
    press(p, { key: "F3", shiftKey: true });
    expect(jumpTo.mock.calls.map((c) => c[0])).toEqual(["a", "b", "c", "a", "c"]);
    expect(hitRows(p.el).map((r) => r.getAttribute("aria-selected"))).toEqual(["false", "false", "true"]);
  });

  it("🔴 跳到没加载的：先写「未加载 · 加载后跳转」，取到就撤；取失败写原因 ＋［重试］，重试再跳；落空写宿主那一句", async () => {
    let settle!: { ok: (el: HTMLElement) => void; no: (e: Error) => void };
    const jumpTo = vi.fn(() => new Promise<HTMLElement | null>((ok, no) => (settle = { ok, no })));
    const { p } = panelWith({ search: vi.fn(async () => found([hit("u5")])), jumpTo } as Partial<SessionFindHost>);
    enter(p, "needle");
    await settleOutline();
    const row = hitRows(p.el)[0];
    row.click();
    expect(stateOf(row).hidden).toBe(false);
    expect(stateOf(row).textContent).toBe("未加载 · 加载后跳转");
    settle.no(new Error("网络断开"));
    await settleOutline();
    expect(stateOf(row).textContent).toBe("加载失败 · 网络断开重试");
    stateOf(row).querySelector("button")!.click();
    expect(jumpTo).toHaveBeenCalledTimes(2);
    settle.ok(document.createElement("div"));
    await settleOutline();
    expect(stateOf(row).hidden, "取到了提示还挂着").toBe(true);
    expect(row.dataset.unjumpable).toBeUndefined();
    jumpTo.mockImplementationOnce(() => Promise.resolve(null));
    row.click();
    await settleOutline();
    expect(stateOf(row).textContent).toBe("无对应卡片");
    expect(row.dataset.unjumpable).toBe("1");
  });
});

// ── 集成：实时 tab（真 TabManager ＋ 真渲染管线） ────────────────────────────

let tm: TabManager;
let streamRootEl: HTMLElement;
const feed = (p: RigPayload): void => tm.onLine(p as never);
const peek = (sid: string): Tab => (tm as unknown as { store: { tabs: Map<string, Tab> } }).store.tabs.get(sid)!;
const findOf = (sid = "s1"): HTMLElement => peek(sid).inputsEl;
const findCalls = (): unknown[] => sessionReadCalls(vi.mocked(invoke).mock.calls, "find_in_session");

function liveSetup(): void {
  const barEl = document.createElement("div");
  streamRootEl = document.createElement("div");
  document.body.append(barEl, streamRootEl);
  tm = new TabManager(barEl, streamRootEl);
}

describe("SE2 · 实时 tab 上的查找面板", () => {
  beforeEach(liveSetup);

  it("🔴 每个 tab 一块查找面板（大纲在里面），旧的独立悬浮层零命中", () => {
    feed(userLine(1, "u1", "一"));
    feed(withSession(userLine(1, "w1", "二"), "s2"));
    // 正控：同一个谓词换成新类名 ⇒ 恰好每 tab 一块
    expect(streamRootEl.querySelectorAll(":scope > .session-find").length).toBe(2);
    expect(streamRootEl.querySelectorAll(".live-user-inputs").length).toBe(0);
    // 大纲清单挂在这块面板里（不是另一块悬浮层）；浮着的「大纲 · N」入口不再摆出来（入口是面板的页签）
    const f = findOf();
    expect(f.querySelector(".user-inputs-toggle")).toBeNull();
    expect(f.querySelector("[data-role=session-find-panel] .user-inputs")).not.toBeNull();
  });

  it("Ctrl+F（`openFind`）⇒ 开的是 active tab 那一块；切走 tab ⇒ 收起", () => {
    feed(userLine(1, "u1", "一"));
    feed(withSession(userLine(1, "w1", "二"), "s2"));
    tm.switchTo("s1");
    tm.openFind();
    const panelOf = (sid: string) => findOf(sid).querySelector<HTMLElement>("[data-role=session-find-panel]")!;
    expect(panelOf("s1").hidden).toBe(false);
    expect(panelOf("s2").hidden).toBe(true);
    tm.switchTo("s2");
    expect(panelOf("s1").hidden, "切走的 tab 还开着 ⇒ Esc 会去关一块看不见的面板").toBe(true);
  });

  it("🔴 回车 ⇒ 问后端一次，问的是这个 tab 那份会话（参数逐字）", async () => {
    feed(userLine(1, "u1", "一"));
    stub.finds = [found([hit("u1")])];
    tm.openFind();
    const inp = findOf().querySelector<HTMLInputElement>("[data-role=find-input]")!;
    inp.value = "一";
    inp.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await settleOutline();
    expect(findCalls()).toEqual([
      { origin: "<local>", jsonlPath: "/p/s1.jsonl", query: "一", includeTools: false },
    ]);
    expect(hitRows(findOf()).map((r) => r.dataset.hitUuid)).toEqual(["u1"]);
  });

  /**
   * 🔴 F4：命中落在骨架占位里、正文已被丢出前端账本 ⇒ 跳要**等按偏移取回的正文落完**再找卡。
   *
   * 台子：尾巴 `[1000,1100)` 先到（钉 floor），`[0,1000)` 后到全收纳；骨架接上后前端账本只留 `[800,1000)`
   * （U3b `keepHighest`）⇒ seq 300 的正文**只能**按偏移要回来。命中 `u300` ⇒ 点它。
   */
  it("🔴 F4 跳得准：命中还在占位里的一条 ⇒ 正文取回、卡建出来、滚过去，不标「跳不过去」", async () => {
    stub.indexRows = Array.from({ length: 1100 }, (_, k) => ({
      o: k * 10,
      n: 10,
      t: "assistant",
      u: `u${k}`,
      ch: 10,
      pl: 1,
    }));
    tm.onBatchStart();
    feed(assistantLine(1000, "u1000", "尾巴第一条"));
    const el = peek("s1").streamEl;
    Object.defineProperty(el, "scrollHeight", { value: 2000, configurable: true });
    Object.defineProperty(el, "clientHeight", { value: 800, configurable: true });
    for (let s = 1001; s < 1100; s++) feed(assistantLine(s, `u${s}`, `第 ${s} 条`));
    for (let s = 0; s < 1000; s++) feed(assistantLine(s, `u${s}`, `第 ${s} 条`));
    tm.onBatchEnd();
    await settleOutline();
    const t = peek("s1");
    expect(t.skeleton, "骨架没接上 ⇒ 这一格测的不是占位里的跳").not.toBeNull();
    expect(t.skeleton!.isPending(300)).toBe(true);
    expect(t.window.pendingCount, "前端账本只留尾巴 200 条").toBe(200);
    expect(el.querySelector('[data-uuid="u300"]'), "起点：那张卡还不在").toBeNull();

    stub.finds = [found([hit("u300")])];
    tm.openFind();
    const inp = findOf().querySelector<HTMLInputElement>("[data-role=find-input]")!;
    inp.value = "第 300";
    inp.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await settleOutline();
    const row = hitRows(findOf())[0];
    row.click();
    await settleOutline();
    const { recordReadCalls } = await import("../../../test-support/chan-fake");
    const ranges = recordReadCalls(vi.mocked(invoke).mock.calls, "read_session_range");
    expect(ranges.length, "正文真的是按偏移要回来的").toBeGreaterThan(0);
    expect(el.querySelector('[data-uuid="u300"]'), "取回之后卡建出来了").not.toBeNull();
    expect(row.dataset.unjumpable, "等到了卡却标成跳不过去 ⇒ 同步那一下就去找了").toBeUndefined();
  });
});
