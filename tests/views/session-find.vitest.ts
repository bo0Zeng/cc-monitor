/**
 * 〔SE2 · `设计/10 §2.2b ④` · `§6 步 6`〕会话内查找面板（搜索 / 大纲两个模式）的判据。
 *
 * # 量什么
 *
 * - **F2 面板**：两个入口各开到各的模式（Ctrl+F ⇒ 搜索、焦点进输入框；大纲按钮 ⇒ 大纲、同一个按钮再按收起）·
 *   ✕ 与 Esc（快捷键弹层栈）收起 · 切走 tab 收起并出栈 · 旧的独立悬浮层 `.live-user-inputs` 零命中（带正控）。
 * - **F3 搜索**：Enter 才发、发一次、参数逐字（问的是这个 tab 那份会话）· 命中按后端给的顺序列、`<mark>` 里是原文 ·
 *   全量 > 条数时说出来 · 查不了说原因 · 迟到的旧结果不盖新结果。
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
  const rig = await import("../test-support/session-viewer-rig");
  const { withSessionReads } = await import("../test-support/chan-fake");
  return {
    // 〔C4b〕三问改走通道：`withSessionReads` 把一发 `chan_call` 译回「哪一问 ＋ 旧形参」、把回包译成后端成品字节。
    invoke: vi.fn(withSessionReads(async (cmd: string, args: Record<string, unknown>) => {
      if (cmd === "list_user_inputs") return rig.answerListUserInputs(args as { fromOffset: number });
      if (cmd === "find_in_session") return stub.finds.shift();
      if (cmd === "read_session_index" && stub.indexRows && args.fromOffset === 0) {
        const rows = stub.indexRows;
        return { available: true, from: 0, end: rows.length * 10, rows };
      }
      if (cmd === "read_session_range") {
        const a = args as { seqBase: number; lineCount: number };
        return Array.from({ length: a.lineCount }, (_, k) =>
          rig.assistantLine(a.seqBase + k, `u${a.seqBase + k}`, `第 ${a.seqBase + k} 条（按偏移取回的）`),
        );
      }
      return undefined;
    })),
  };
});
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../src/tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("../../src/turn-notify", () => ({ turnEndNotifier: { observe: vi.fn() } }));
vi.mock("../../src/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ resumeCommandLocal: "", resumeCommandRemote: "cct" }),
}));
vi.mock("../../src/remote-launch-run", () => ({
  runRemoteResume: vi.fn().mockResolvedValue(undefined),
  runRemoteResumeTmux: vi.fn().mockResolvedValue(undefined),
  runLocalResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteResumeIntoExistingTmux: vi.fn().mockResolvedValue(true),
  runRemoteAttach: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("../../src/account-restart", () => ({
  restartWithAccount: vi.fn().mockResolvedValue(undefined),
  DEFAULT_EXIT_WAIT_MS: 10_000,
}));
vi.mock("../../src/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { invoke } from "@tauri-apps/api/core";
import { sessionReadCalls } from "../test-support/chan-fake";
import {
  installViewerRig,
  assistantLine,
  userLine,
  withSession,
  settleOutline,
  type RigPayload,
} from "../test-support/session-viewer-rig";
import { TabManager, type Tab } from "../../src/tabs";
import { SessionFindPanel, type SessionFindHost } from "../../src/views/session-find";
import { dispatcher } from "../../src/keybindings/registry";
import type { FindResult } from "../../src/session-reads";

const hit = (uuid: string, matched = "needle", before = "a ", after = " b") => ({
  uuid,
  kind: "assistant",
  before,
  matched,
  after,
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
  const p = new SessionFindPanel({ search, jumpTo, unjumpableHint: "跳不过去", ...over } as SessionFindHost);
  document.body.appendChild(p.el);
  return { p, search, jumpTo };
}
const q = (p: SessionFindPanel, sel: string) => p.el.querySelector<HTMLElement>(sel)!;
const input = (p: SessionFindPanel) => q(p, ".session-find-input") as HTMLInputElement;
const box = (p: SessionFindPanel) => q(p, ".session-find-panel");
const searchPane = (p: SessionFindPanel) => q(p, ".session-find-search");
const enter = (p: SessionFindPanel, text: string): void => {
  input(p).value = text;
  input(p).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
};
const hitRows = (root: ParentNode) => [...root.querySelectorAll<HTMLButtonElement>(".session-find-hit")];

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  installViewerRig();
  stub.finds = [];
  stub.indexRows = null;
});
afterEach(() => vi.unstubAllGlobals());

describe("SE2 · F2 一块面板两个模式", () => {
  it("🔴 两个入口各开到各的模式；大纲按钮再按一次收起；✕ 收起", () => {
    const { p } = panelWith();
    expect(box(p).hidden, "默认收着").toBe(true);
    p.open("search"); // Ctrl+F 那条路
    expect(box(p).hidden).toBe(false);
    expect(searchPane(p).hidden).toBe(false);
    expect(p.outline.panel.hidden, "搜索模式下大纲清单不露").toBe(true);
    expect(document.activeElement, "焦点进输入框").toBe(input(p));
    p.outline.toggle.disabled = false;
    p.outline.toggle.click(); // 大纲按钮：开着「搜索」时 ⇒ 切到「大纲」
    expect(box(p).hidden).toBe(false);
    expect(searchPane(p).hidden).toBe(true);
    expect(p.outline.panel.hidden).toBe(false);
    expect(p.outline.toggle.getAttribute("aria-expanded")).toBe("true");
    p.outline.toggle.click(); // 同一个入口再按 ⇒ 收起
    expect(box(p).hidden).toBe(true);
    expect(p.outline.toggle.getAttribute("aria-expanded")).toBe("false");
    p.open("outline");
    q(p, ".session-find-close").click();
    expect(box(p).hidden).toBe(true);
  });

  it("模式标签：点哪个切哪个，`aria-selected` 跟着走", () => {
    const { p } = panelWith();
    p.open("outline");
    const tabs = [...p.el.querySelectorAll<HTMLButtonElement>(".session-find-mode")];
    expect(tabs.map((t) => t.dataset.mode)).toEqual(["search", "outline"]);
    tabs[0].click();
    expect(tabs.map((t) => t.getAttribute("aria-selected"))).toEqual(["true", "false"]);
    expect(p.currentMode).toBe("search");
  });

  it("🔴 Esc 收起：开着时压在快捷键弹层栈上、关了出栈（焦点在输入框里也收得起）", () => {
    dispatcher.applyOverrides({}); // 建默认键位表（main.ts 启动时做的同一步）
    dispatcher.start();
    const { p } = panelWith();
    p.open("search");
    const esc = () =>
      input(p).dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true }));
    esc();
    expect(box(p).hidden).toBe(true);
    // 出栈了：再按一次 Esc 不会去关它（它已经关了），也不会把关着的面板再打开
    esc();
    expect(box(p).hidden).toBe(true);
    // 正控：再开一次 ⇒ 再入栈 ⇒ Esc 照样收得起（上面那格不是「Esc 根本不认」绿的）
    p.open("outline");
    expect(box(p).hidden).toBe(false);
    esc();
    expect(box(p).hidden).toBe(true);
  });
});

describe("SE2 · F3 搜索", () => {
  it("🔴 Enter 才发、发一次、带着「含工具内容」那个勾；命中按后端给的顺序列、<mark> 里是原文", async () => {
    const s = vi.fn(async () => found([hit("u9", "NeedLe", "前 ", " 后"), hit("u2")]));
    const { p } = panelWith({ search: s } as Partial<SessionFindHost>);
    p.open("search");
    input(p).value = "needle";
    expect(p.el.querySelectorAll(".session-find-hit").length, "打字不发").toBe(0);
    (q(p, ".session-find-tools input") as HTMLInputElement).checked = true;
    enter(p, "  needle  ");
    await settleOutline();
    expect(s.mock.calls).toEqual([["needle", true]]);
    const rows = hitRows(p.el);
    expect(rows.map((r) => r.dataset.hitUuid)).toEqual(["u9", "u2"]);
    expect(rows[0].querySelector("mark")!.textContent).toBe("NeedLe");
    expect(rows[0].textContent).toBe("前 NeedLe 后");
    expect(rows[0].hasAttribute("data-uuid"), "命中行不许叫 data-uuid（那是消息卡的名字）").toBe(false);
    expect(q(p, ".session-find-status").textContent).toBe("2 条");
  });

  it("全量 > 条数 ⇒ 说出来；零条 ⇒ 说没找到；查不了 ⇒ 说原因；空查询 ⇒ 不发", async () => {
    const results: FindResult[] = [
      found([hit("a")], 900),
      found([]),
      { available: false, reason: "后端版本旧", hits: [], total: 0 },
    ];
    const search = vi.fn(async () => results.shift()!);
    const { p } = panelWith({ search } as Partial<SessionFindHost>);
    const status = () => q(p, ".session-find-status").textContent;
    enter(p, "x");
    await settleOutline();
    expect(status()).toBe("共 900 条，只列了前 1 条");
    enter(p, "y");
    await settleOutline();
    expect(status()).toBe("没有找到");
    enter(p, "z");
    await settleOutline();
    expect(status()).toBe("现在查不了：后端版本旧");
    expect(hitRows(p.el).length).toBe(0);
    enter(p, "   ");
    await settleOutline();
    expect(search).toHaveBeenCalledTimes(3);
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

  it("点命中行 ⇒ 交给宿主去跳；落空 ⇒ 标出来，后来跳得过去 ⇒ 标记与提示两样都撤（异步落点也一样）", async () => {
    let land: HTMLElement | null = null;
    const jumpTo = vi.fn(() => Promise.resolve(land));
    const { p } = panelWith({
      search: vi.fn(async () => found([hit("u5")])),
      jumpTo,
    } as Partial<SessionFindHost>);
    enter(p, "needle");
    await settleOutline();
    const row = hitRows(p.el)[0];
    row.click();
    await settleOutline();
    expect(jumpTo).toHaveBeenCalledWith("u5");
    expect(row.dataset.unjumpable).toBe("1");
    expect(row.title).toBe("跳不过去");
    land = document.createElement("div");
    row.click();
    await settleOutline();
    expect(row.dataset.unjumpable).toBeUndefined();
    expect(row.title).toBe("a needle b");
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
    // 大纲的开关与清单都挂在这块面板里（不是另一块悬浮层）
    const f = findOf();
    expect(f.querySelector(".user-inputs-toggle")).not.toBeNull();
    expect(f.querySelector(".session-find-panel .user-inputs")).not.toBeNull();
  });

  it("Ctrl+F（`openFind`）⇒ 开的是 active tab 那一块；切走 tab ⇒ 收起", () => {
    feed(userLine(1, "u1", "一"));
    feed(withSession(userLine(1, "w1", "二"), "s2"));
    tm.switchTo("s1");
    tm.openFind();
    const panelOf = (sid: string) => findOf(sid).querySelector<HTMLElement>(".session-find-panel")!;
    expect(panelOf("s1").hidden).toBe(false);
    expect(panelOf("s2").hidden).toBe(true);
    tm.switchTo("s2");
    expect(panelOf("s1").hidden, "切走的 tab 还开着 ⇒ Esc 会去关一块看不见的面板").toBe(true);
  });

  it("🔴 Enter ⇒ 问后端一次，问的是这个 tab 那份会话（参数逐字）", async () => {
    feed(userLine(1, "u1", "一"));
    stub.finds = [found([hit("u1")])];
    tm.openFind();
    const inp = findOf().querySelector<HTMLInputElement>(".session-find-input")!;
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
    const inp = findOf().querySelector<HTMLInputElement>(".session-find-input")!;
    inp.value = "第 300";
    inp.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await settleOutline();
    const row = hitRows(findOf())[0];
    row.click();
    await settleOutline();
    const ranges = vi.mocked(invoke).mock.calls.filter((c) => c[0] === "read_session_range");
    expect(ranges.length, "正文真的是按偏移要回来的").toBeGreaterThan(0);
    expect(el.querySelector('[data-uuid="u300"]'), "取回之后卡建出来了").not.toBeNull();
    expect(row.dataset.unjumpable, "等到了卡却标成跳不过去 ⇒ 同步那一下就去找了").toBeUndefined();
  });
});
