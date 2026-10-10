/**
 * 〔性能八条〕tab 栏的**操作次数**判据 —— 量 DOM 写 / 读 / 监听器个数，不量墙钟。
 *
 * # 量具
 *
 * - **DOM 写** ＝ `MutationObserver`（subtree ＋ attributes ＋ childList ＋ characterData）收到的记录条数，
 *   用 `takeRecords()` 同步取。它与浏览器「让样式失效」是同一个口径：DOM 规范上
 *   `classList.toggle(x, 布尔)` 状态没变时不跑 update steps、不排记录；属性赋值（`title = …`）
 *   不管值变没变都排一条。jsdom 照规范实现 —— 下面「量具自检」那一格给正控。
 * - **DOM 读** ＝ 在实例上盖一个计数的 getter（`barEl.children`）或替身（`getBoundingClientRect`）。
 *
 * # 判据全是相等（`w4-common` 第 4 条：不用地板）
 *
 * 设计与读数住（本文件不抄数）。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { closeMenu } from "../../../src/frontend/ui/kit/menu";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn(), openUrl: vi.fn() }));
// ↗ 只在 Windows 上渲（`terminal-front.ts`）；这里要把它那一条委托路也走一遍 ⇒ 让它渲出来。
vi.mock("../../../src/frontend/ui/terminal-front", () => ({ terminalFrontAvailable: () => true }));

import { TabBarView, type TabBarViewHost } from "../../../src/frontend/ui/tab-bar-view";
import { TabBarDrag } from "../../../src/frontend/ui/tab-bar-drag";
import { TabStreamView, type TabStreamHost } from "../../../src/frontend/ui/tab-stream-view";
import { TabStore } from "../../../src/frontend/ui/tab-store";
import type { TabBarPrefs } from "../../../src/frontend/ui/tab-bar-prefs";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import type { TabCollection } from "../../../src/frontend/ui/tab-collections";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { ENDED, LIVE } from "../../../src/frontend/ui/tab-session-state";

/** tab 栏视图只读 `Tab` 的这几格；其余字段它不碰（`tab-bar-view.ts::updateTabButton` / `updateAccountBadge`）。 */
function fakeTab(sid: string, over: Partial<Tab> = {}): Tab {
  return {
    sessionId: sid,
    title: `标题-${sid}`,
    state: LIVE,
    pinned: false,
    group: null, // 组员关系是 tab 自己的属性
    cwd: "/w",
    origin: LOCAL_ORIGIN,
    background: false,
    activity: null,
    forkedFromSessionId: null,
    writers: [],
    unread: 0,
    needs: null,
    pending: [],
    lastSay: null,
    ...over,
  } as unknown as Tab;
}

interface Rig {
  store: TabStore;
  bar: HTMLElement;
  view: TabBarView;
  prefs: { collections: TabCollection[] };
  host: TabBarViewHost;
  /** 自上次取数以来 tab 栏子树收到的 mutation 记录。 */
  take: () => MutationRecord[];
  dispose: () => void;
}

/**
 * N 个 tab：一半本机、一半远端（远端那一半挂账号徽章 —— 那正是 P2 说的每次重造的那一块），
 * 前 `grouped` 个进一个集合（组头名字那一处守卫也在射程里）。
 */
function rig(n: number, grouped = 0): Rig {
  const store = new TabStore();
  store.accountReadyOrigins.add("pi");
  // 账号头像只在「这个会话的号 ≠ 那台的默认号」时出：那台默认号是 dave ⇒ 远端那一半都与它不同。
  store.currentByOrigin.set("pi", "dave");
  for (let i = 0; i < n; i++) {
    const sid = `s${i}`;
    const remote = i % 2 === 1;
    store.tabs.set(sid, fakeTab(sid, { origin: remote ? "pi" : LOCAL_ORIGIN }));
    store.orderedIds.push(sid);
    if (remote) store.accountLastByS.set(sid, i % 4 === 1 ? "alice" : "bob");
  }
  store.activeId = "s0";
  // 组表只有 `{id, name}`；前 `grouped` 个 tab 自己带组 id。
  for (const sid of store.orderedIds.slice(0, grouped)) store.tabs.get(sid)!.group = "c1";
  const prefs = {
    collections: grouped > 0 ? [{ id: "c1", name: "组一" }] : [],
    dissolveGroup: vi.fn().mockResolvedValue(undefined),
    renameGroup: vi.fn().mockResolvedValue(undefined),
  };
  const host: TabBarViewHost = {
    refreshTabBar: vi.fn(),
    closeEndedIn: vi.fn(),
    openTabCwd: vi.fn().mockResolvedValue(undefined),
    bringTerminalToFront: vi.fn().mockResolvedValue(undefined),
    bringRemoteTerminalToFront: vi.fn().mockResolvedValue(undefined),
    closeTab: vi.fn(),
    switchTo: vi.fn(),
    pick: vi.fn(),
    isSelected: vi.fn().mockReturnValue(false),
    beginDrag: vi.fn(),
    takeSuppressedClick: vi.fn().mockReturnValue(false),
    beginGroupDrag: vi.fn(),
    takeSuppressedHeadClick: vi.fn().mockReturnValue(false),
    openMenu: vi.fn(),
    toggleSelect: vi.fn(),
    openMenuAt: vi.fn(),
    rereadAll: vi.fn().mockResolvedValue(undefined),
    reconnect: vi.fn(),
  };
  const bar = document.createElement("div");
  bar.id = "tab-bar";
  document.body.appendChild(bar);
  const view = new TabBarView(store, prefs as unknown as TabBarPrefs, bar, host);
  view.refresh();
  const mo = new MutationObserver(() => {});
  mo.observe(bar, { subtree: true, attributes: true, childList: true, characterData: true });
  return {
    store,
    bar,
    view,
    prefs,
    host,
    take: () => mo.takeRecords(),
    dispose: () => {
      mo.disconnect();
      bar.remove();
    },
  };
}

/** 一条记录落在哪颗 tab 按钮上（按钮之外 ⇒ `null`）。 */
function ownerSid(r: Rig, rec: MutationRecord): string | null {
  const node = rec.target instanceof Element ? rec.target : rec.target.parentElement;
  const root = node?.closest(".tab");
  if (!root) return null;
  for (const [sid, refs] of r.view.tabButtons) if (refs.root === root) return sid;
  return null;
}

let rigs: Rig[] = [];
const make = (n: number, grouped = 0): Rig => {
  const r = rig(n, grouped);
  rigs.push(r);
  return r;
};
beforeEach(() => {
  rigs = [];
});
afterEach(() => {
  for (const r of rigs) r.dispose();
});

describe("量具自检（正控：这把尺子量得到写，也分得清写与没写）", () => {
  it("toggle 状态没变 ⇒ 0 条；title 赋同一个值 ⇒ 1 条；toggle 真变 ⇒ 1 条", () => {
    const r = make(1);
    const root = r.view.tabButtons.get("s0")!.root;
    root.classList.toggle("active", root.classList.contains("active"));
    expect(r.take().length).toBe(0);
    const same = root.title;
    root.title = same;
    expect(r.take().length).toBe(1);
    root.classList.toggle("zzz", true);
    expect(r.take().length).toBe(1);
  });
});

describe("P2 ＋ P7 ＋ P1：整刷改成差量刷", () => {
  it("稳态整刷（20 个 tab，一半带账号徽章，4 个在组里，什么都没变）⇒ tab 栏子树 0 条 DOM 写", () => {
    const r = make(20, 4);
    expect(r.bar.querySelectorAll(".acct-avatar").length, "前置：远端那一半的账号徽章真画出来了").toBe(10);
    expect(r.bar.querySelectorAll(".tab-group").length, "前置：组容器在").toBe(1);
    r.take();
    r.view.refresh();
    r.view.refresh();
    expect(r.take().length).toBe(0);
  });

  it("稳态整刷连 `classList.toggle` 都不调（「上次画出去的样子」那一层早退；toggle 本身不写 DOM，这一格量的是调用次数）", () => {
    const r = make(20, 4);
    const spy = vi.spyOn(DOMTokenList.prototype, "toggle");
    try {
      r.view.refresh();
      expect(spy).toHaveBeenCalledTimes(0);
      // 正控：一颗按钮真变了 ⇒ 那一颗的每个开关各调一次（别的按钮仍然 0）。
      // 〔「删掉树」〕10 → 9：少了 `.tab-bg` 那一个开关（bg tab 不再有自己的样式）。
      // 9 → 10：多选里的样子 `.selected` 那一个开关。
      // 10 → 11：说不清（那台看不见）`.unseen` 那一个开关。
      // 11 → 13：跑完你还没看（`.unseen-done`，标题 600）· 在等你（`.waiting-you`，窄窗字母琥珀）。
      (r.store.tabs.get("s5") as { pinned: boolean }).pinned = true;
      r.view.refresh();
      expect(spy).toHaveBeenCalledTimes(13);
    } finally {
      spy.mockRestore();
    }
  });

  it("一个 tab 的红绿灯变了 ⇒ 恰好 3 条记录、全落在那颗按钮上（类 · 状态点的样子 · 状态点的读屏名；两向：别的按钮 0 条）", () => {
    const r = make(20, 4);
    r.take();
    (r.store.tabs.get("s7") as { activity: unknown }).activity = { doing: "idle", waitingFor: null };
    r.view.refresh();
    const recs = r.take();
    expect(recs.map((x) => ownerSid(r, x))).toEqual(["s7", "s7", "s7"]);
    expect(recs.map((x) => x.attributeName).sort()).toEqual(["aria-label", "class", "data-state"]);
    expect(r.view.tabButtons.get("s7")!.root.classList.contains("act-idle")).toBe(true);
  });

  it("账号快照换了（`refreshAccountBadges`）⇒ 只有账号真变了的那一颗重画徽章", () => {
    const r = make(8);
    r.take();
    r.view.refreshAccountBadges();
    expect(r.take().length, "账号没变：一条都不写").toBe(0);
    r.store.accountLastByS.set("s3", "carol");
    r.view.refreshAccountBadges();
    const owners = new Set(r.take().map((x) => ownerSid(r, x)));
    expect([...owners]).toEqual(["s3"]);
    expect(r.view.tabButtons.get("s3")!.acctBadge.textContent).toBe("ca");
  });
});

describe("P4：启动 N 个会话各报一次红绿灯 ⇒ DOM 写总数是 3N，不是 N²（每颗：类 · 状态点 · 读屏名）", () => {
  /** 照 `tabs.ts::syncActivitySnapshot`：每个会话一次 `updateActivity` ⇒ 每次一次整刷。 */
  const startupWrites = (n: number): number => {
    const r = make(n);
    r.take();
    for (const sid of r.store.orderedIds) {
      (r.store.tabs.get(sid) as { activity: unknown }).activity = { doing: "idle", waitingFor: null };
      r.view.refresh();
    }
    return r.take().length;
  };
  it("N = 10 ⇒ 30 条；N = 30 ⇒ 90 条", () => {
    expect(startupWrites(10)).toBe(30);
    expect(startupWrites(30)).toBe(90);
  });
});

describe("P6：整刷不把 `barEl.children` 物化成数组", () => {
  /** 在实例上盖一个计数的 `children` getter（原型链上那一份照常返回）。 */
  const countChildrenReads = (el: HTMLElement): { n: number } => {
    const c = { n: 0 };
    let proto: object | null = Object.getPrototypeOf(el);
    let desc: PropertyDescriptor | undefined;
    while (proto && !desc) {
      desc = Object.getOwnPropertyDescriptor(proto, "children");
      proto = Object.getPrototypeOf(proto);
    }
    const get = desc!.get!;
    Object.defineProperty(el, "children", {
      configurable: true,
      get() {
        c.n += 1;
        return get.call(this);
      },
    });
    return c;
  };
  it("有组、有散 tab 的整刷 ⇒ 读 `barEl.children` 0 次（正控：量具读一次记 1）", () => {
    const r = make(6, 3);
    const c = countChildrenReads(r.bar);
    r.view.refresh();
    expect(c.n).toBe(0);
    void r.bar.children;
    expect(c.n).toBe(1);
  });
  it("组与散的混排：组在它第一个组员那一格（两个组，第二个是后建的、组员在前面那些之后）", () => {
    const r = make(6, 2);
    r.prefs.collections.push({ id: "c2", name: "组二" });
    r.store.tabs.get("s2")!.group = "c2";
    r.view.refresh();
    // 栏顶一排 · 需手动 · 离线条 · 列表四块恒在；组与散 tab 都在列表里。
    expect([...r.bar.children].map((e) => e.className)).toEqual(["tab-bar-head", "tab-needs", "tab-machine-down", "tab-list"]);
    const kids = [...r.view.listEl.children];
    const kind = (e: Element): string => (e.classList.contains("tab-group") ? "G" : e === r.view.slotTail ? "S" : "t");
    expect(kids.map(kind).join(""), "占位标签页那一格恒在最末").toBe("GGtttS");
  });
});

describe("P3：拖拽时矩形只量一次、落点标记只动变了的那两个", () => {
  /** 栏里 N 个 tab（全散着），每颗按钮一条 40px 的带；每量一次记一笔。 */
  const dragRig = (n: number): { r: Rig; drag: TabBarDrag; reads: { n: number } } => {
    const r = make(n);
    const reads = { n: 0 };
    r.store.orderedIds.forEach((sid, i) => {
      r.view.tabButtons.get(sid)!.root.getBoundingClientRect = () => {
        reads.n += 1;
        return { top: i * 40, height: 40, bottom: i * 40 + 40, left: 0, right: 100 } as DOMRect;
      };
    });
    const drag = new TabBarDrag(
      r.store,
      {
        collections: [],
        collectionsLoaded: false,
        persistOrder: vi.fn().mockResolvedValue(undefined),
        snapshot: () => ({ order: [], groups: [], groupOf: new Map() }),
        offerUndo: vi.fn(),
      } as unknown as TabBarPrefs,
      r.bar,
      r.view,
      { refreshTabBar: vi.fn(), openInNewWindow: vi.fn().mockResolvedValue(undefined), renameGroupNow: vi.fn(), selectedFor: (sid: string) => [sid] },
    );
    return { r, drag, reads };
  };
  const move = (y: number): void => {
    document.dispatchEvent(new MouseEvent("mousemove", { buttons: 1, clientX: 10, clientY: y, bubbles: true }));
  };
  const down = (r: Rig, drag: TabBarDrag, sid: string): void => {
    const root = r.view.tabButtons.get(sid)!.root;
    drag.begin(new MouseEvent("mousedown", { button: 0, clientX: 10, clientY: 0 }), sid, root);
  };
  afterEach(() => {
    document.dispatchEvent(new MouseEvent("mouseup", { clientX: 10, clientY: 0, bubbles: true }));
  });

  it("起拖后 12 次 mousemove ⇒ 矩形一共量 N 次（N = 8 与 N = 20 各一遍）", () => {
    for (const n of [8, 20]) {
      const { r, drag, reads } = dragRig(n);
      down(r, drag, "s0");
      for (let k = 0; k < 12; k++) move(30 + k * 25);
      expect(reads.n, `N = ${n}`).toBe(n);
      document.dispatchEvent(new MouseEvent("mouseup", { clientX: 10, clientY: 0, bubbles: true }));
    }
  });

  it("tab 栏滚了 / 窗口尺寸变了 ⇒ 缓存作废，下一次 mousemove 再量 N 次（不是每次都量）", () => {
    const n = 10;
    const { r, drag, reads } = dragRig(n);
    down(r, drag, "s0");
    move(50);
    move(60);
    expect(reads.n).toBe(n);
    r.bar.dispatchEvent(new Event("scroll"));
    move(70);
    move(80);
    expect(reads.n).toBe(2 * n);
    window.dispatchEvent(new Event("resize"));
    move(90);
    expect(reads.n).toBe(3 * n);
  });

  it("拖完再拖：上一轮的缓存不带进下一轮（新一轮起拖重量 N 次）", () => {
    const n = 6;
    const { r, drag, reads } = dragRig(n);
    down(r, drag, "s0");
    move(50);
    document.dispatchEvent(new MouseEvent("mouseup", { clientX: 10, clientY: 50, bubbles: true }));
    down(r, drag, "s1");
    move(120);
    expect(reads.n).toBe(2 * n);
  });

  it("落点没变的 mousemove ⇒ 落点标记 0 次 DOM 写；换一个落点 ⇒ 插入线挪一次", () => {
    const { r, drag } = dragRig(20);
    down(r, drag, "s0");
    move(130); // 起拖 ＋ 第一次画（落在 s3 之前）
    const line = [...document.body.children].find((e) => e instanceof HTMLElement && e.style.top !== "" && !e.classList.contains("tab-drag-ghost")) as HTMLElement;
    expect(line, "插入线画出来了（量具自检）").toBeDefined();
    const writes: MutationRecord[] = [];
    const mo = new MutationObserver((m) => writes.push(...m));
    mo.observe(document.body, { attributes: true, subtree: true, attributeFilter: ["class", "style", "hidden"] });
    const flush = (): number => {
      writes.push(...mo.takeRecords());
      return writes.filter((w) => w.target !== document.querySelector(".tab-drag-ghost")).length;
    };
    try {
      move(131);
      move(132);
      expect(flush(), "同一个落点").toBe(0);
      move(250); // 换到另一格
      expect(flush()).toBeGreaterThan(0);
      expect(line.style.top, "线挪到了新那一格的上沿").toBe(`${6 * 40 - 1}px`);
    } finally {
      mo.disconnect();
    }
    void r;
  });
});

describe("P8：事件委托 —— 每个 tab 零监听器，整条栏恒定那几个", () => {
  it("建 TabBarView：手势五个在 barEl 上（点 · 右键 · 按下 · 列表键按下 / 松开）、悬停两组（卡 · 行尾动作）各四个在列表上、栏顶「刷新」五个 ·「需手动」菜单进出两个；之后新建 M 个 tab 的整刷里 `addEventListener` 0 次（M = 5 与 M = 40）", () => {
    for (const m of [5, 40]) {
      const spy = vi.spyOn(EventTarget.prototype, "addEventListener");
      try {
        const r = make(0);
        const on = (el: EventTarget): string[] =>
          spy.mock.calls.filter((_, i) => spy.mock.contexts[i] === el).map((c) => c[0] as string).sort();
        expect(on(r.bar), "手势：恰好五个委托").toEqual(["click", "contextmenu", "keydown", "keyup", "mousedown"]);
        expect(on(r.view.listEl), "悬停：卡与行尾动作两组委托").toEqual(["focusin", "focusin", "focusout", "focusout", "mouseout", "mouseout", "mouseover", "mouseover"]);
        // 栏顶「刷新」一份悬停提示（五个）·「需手动」菜单的进 / 出两个（固定，不随 tab 数涨）。
        expect(spy.mock.calls.length, "构造总数恒定").toBe(5 + 8 + 5 + 2);
        spy.mockClear();
        for (let i = 0; i < m; i++) {
          r.store.tabs.set(`n${i}`, fakeTab(`n${i}`));
          r.store.orderedIds.push(`n${i}`);
        }
        r.view.refresh();
        expect(r.view.tabButtons.size).toBe(m);
        expect(spy).toHaveBeenCalledTimes(0);
      } finally {
        spy.mockRestore();
      }
    }
  });

  describe("委托之后每条手势路的行为（原先 10 个监听器各管的那一件）", () => {
    const btn = (r: Rig, sid: string): HTMLElement => r.view.tabButtons.get(sid)!.root;
    const sub = (r: Rig, sid: string, cls: string): HTMLElement =>
      btn(r, sid).querySelector(`.${cls}`) as HTMLElement;
    /** 冒到 document 的事件（验「子按钮上的事件不往上冒」那半）。 */
    const seenAtDoc = (type: string): { n: number; off: () => void } => {
      const c = { n: 0, off: () => {} };
      const f = (): void => {
        c.n += 1;
      };
      document.addEventListener(type, f);
      c.off = () => document.removeEventListener(type, f);
      return c;
    };

    it("点按钮 ⇒ 切过去；拖完那一下的 click 被吞 ⇒ 不切", () => {
      const r = make(3);
      btn(r, "s1").click();
      expect(r.host.switchTo).toHaveBeenCalledWith("s1");
      (r.host.takeSuppressedClick as ReturnType<typeof vi.fn>).mockReturnValueOnce(true);
      btn(r, "s2").click();
      expect(r.host.switchTo).toHaveBeenCalledTimes(1);
    });

    it("📂 / ↗ / × ⇒ 各自的动作、不切 tab、click 不冒到 document", () => {
      const r = make(4); // s1 / s3 是远端
      const doc = seenAtDoc("click");
      try {
        sub(r, "s0", "tab-cwd").click();
        expect(r.host.openTabCwd).toHaveBeenCalledWith("s0");
        sub(r, "s0", "tab-focus").click();
        expect(r.host.bringTerminalToFront).toHaveBeenCalledWith("s0");
        sub(r, "s1", "tab-focus").click();
        expect(r.host.bringRemoteTerminalToFront).toHaveBeenCalledWith("s1");
        sub(r, "s2", "tab-close").click();
        expect(r.host.closeTab).toHaveBeenCalledWith("s2");
        expect(r.host.switchTo).not.toHaveBeenCalled();
        expect(doc.n).toBe(0);
        // 正控：点按钮本体的 click 照常冒到 document
        btn(r, "s0").click();
        expect(doc.n).toBe(1);
      } finally {
        doc.off();
      }
    });

    it("↗ 在已结束的 tab 上 ⇒ 什么都不做", () => {
      const r = make(2);
      r.store.tabs.get("s0")!.state = ENDED;
      sub(r, "s0", "tab-focus").click();
      expect(r.host.bringTerminalToFront).not.toHaveBeenCalled();
      expect(r.host.bringRemoteTerminalToFront).not.toHaveBeenCalled();
    });

    it("左键按下按钮 ⇒ 起拖（带上 sid 与按钮根）；按在子按钮上 ⇒ 不起拖、不冒到 document", () => {
      const r = make(2);
      const doc = seenAtDoc("mousedown");
      try {
        const e = new MouseEvent("mousedown", { button: 0, bubbles: true });
        btn(r, "s1").dispatchEvent(e);
        expect(r.host.beginDrag).toHaveBeenCalledWith(e, "s1", btn(r, "s1"));
        expect(doc.n).toBe(1);
        for (const cls of ["tab-cwd", "tab-focus", "tab-close"]) {
          sub(r, "s1", cls).dispatchEvent(new MouseEvent("mousedown", { button: 0, bubbles: true }));
        }
        expect(r.host.beginDrag).toHaveBeenCalledTimes(1);
        expect(doc.n).toBe(1);
      } finally {
        doc.off();
      }
    });

    it("中键：已结束的 ⇒ 关掉并 preventDefault；活着的 ⇒ 不关；按在 × 上 ⇒ 不关（原先 × 吞掉了 mousedown）", () => {
      const r = make(3);
      r.store.tabs.get("s0")!.state = ENDED;
      r.store.tabs.get("s2")!.state = ENDED;
      const e = new MouseEvent("mousedown", { button: 1, bubbles: true, cancelable: true });
      btn(r, "s0").dispatchEvent(e);
      expect(r.host.closeTab).toHaveBeenCalledWith("s0");
      expect(e.defaultPrevented).toBe(true);
      btn(r, "s1").dispatchEvent(new MouseEvent("mousedown", { button: 1, bubbles: true, cancelable: true }));
      sub(r, "s2", "tab-close").dispatchEvent(new MouseEvent("mousedown", { button: 1, bubbles: true, cancelable: true }));
      expect(r.host.closeTab).toHaveBeenCalledTimes(1);
      expect(r.host.beginDrag).not.toHaveBeenCalled();
    });

    it("右键（按钮本体或子按钮上）⇒ 开这个 tab 的菜单并 preventDefault", () => {
      const r = make(2);
      const e1 = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
      btn(r, "s0").dispatchEvent(e1);
      const e2 = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
      sub(r, "s1", "tab-cwd").dispatchEvent(e2);
      expect((r.host.openMenu as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[1])).toEqual(["s0", "s1"]);
      expect([e1.defaultPrevented, e2.defaultPrevented]).toEqual([true, true]);
    });

    it("悬停卡委托在列表上：指针进一行 500ms 出卡（第一行是标题全名）、移到下一行立刻换；行上不挂原生提示", () => {
      vi.useFakeTimers();
      try {
        const r = make(2);
        const tips = (): HTMLElement[] => [...document.querySelectorAll<HTMLElement>('[role="tooltip"]')];
        btn(r, "s0").dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
        vi.advanceTimersByTime(499);
        expect(tips()).toHaveLength(0);
        vi.advanceTimersByTime(1);
        expect(tips().map((t) => t.querySelector(".tab-hover-title")?.textContent)).toEqual(["s0"]);
        btn(r, "s0").dispatchEvent(new MouseEvent("mouseout", { bubbles: true, relatedTarget: btn(r, "s1") }));
        btn(r, "s1").dispatchEvent(new MouseEvent("mouseover", { bubbles: true, relatedTarget: btn(r, "s0") }));
        expect(tips().map((t) => t.querySelector(".tab-hover-title")?.textContent), "同组立刻换").toEqual(["s1"]);
        expect(btn(r, "s0").hasAttribute("title")).toBe(false);
        for (const t of tips()) t.remove();
      } finally {
        vi.useRealTimers();
      }
    });

    it("组头上的点击不当成 tab 手势（委托只认 tab 按钮）", () => {
      const r = make(3, 2);
      (r.bar.querySelector(".tab-group-more") as HTMLElement).dispatchEvent(
        new MouseEvent("mousedown", { button: 0, bubbles: true }),
      );
      r.bar.querySelector(".tab-group-head")!.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, cancelable: true }),
      );
      expect(r.host.beginDrag).not.toHaveBeenCalled();
      expect(r.host.openMenu).not.toHaveBeenCalled();
      expect(r.host.switchTo).not.toHaveBeenCalled();
      expect(document.querySelector("[role=menu]"), "组头右键开的是组的菜单").not.toBeNull();
      closeMenu();
    });

    it("barEl 里一个不是本视图建的 `.tab` 元素上的手势 ⇒ 不分派（sid 只从本视图那张表里认）", () => {
      const r = make(2);
      const foreign = document.createElement("button");
      foreign.className = "tab";
      r.bar.appendChild(foreign);
      foreign.click();
      foreign.dispatchEvent(new MouseEvent("mousedown", { button: 0, bubbles: true }));
      expect(r.host.switchTo).not.toHaveBeenCalled();
      expect(r.host.beginDrag).not.toHaveBeenCalled();
    });
  });
});

describe("P5：切 tab 只写 4 次 class，与 tab 数无关（代码不改，钉住现状）", () => {
  /**
   * `tab-stream-view.ts::showOnly` 遍历全部 tab 各 `toggle` 两次 —— 但 DOM 规范上 `force` 与现状一致的
   * `toggle` 不写 ⇒ 真写的只有旧 active 与新 active 的 streamEl ＋ inputsEl。要的「只动两个元素」
   * 在 DOM 写这一层今天就成立；剩下的 O(N) 只是 JS 循环。这一格把它钉住，免得哪天有人把 toggle 换成无条件写。
   */
  const switchWrites = (n: number): number[] => {
    const store = new TabStore();
    const root = document.createElement("div");
    document.body.appendChild(root);
    for (let i = 0; i < n; i++) {
      const streamEl = document.createElement("div");
      streamEl.className = "stream";
      const inputsEl = document.createElement("div");
      root.append(streamEl, inputsEl);
      store.tabs.set(`s${i}`, { sessionId: `s${i}`, streamEl, inputsEl, turnFold: { release: () => {}, flushStale: () => {} }, turnRail: { el: document.createElement("div"), shown: () => {} }, stream: { park: () => {} } } as unknown as Tab);
    }
    const view = new TabStreamView(store, root, {} as TabStreamHost);
    const mo = new MutationObserver(() => {});
    mo.observe(root, { subtree: true, attributes: true, attributeFilter: ["class"] });
    const out: number[] = [];
    for (const sid of ["s0", `s${n - 1}`, "s1"]) {
      view.showOnly(sid);
      out.push(mo.takeRecords().length);
    }
    mo.disconnect();
    root.remove();
    return out;
  };
  it("N = 10 与 N = 40：第一次切（没有旧 active）2 条，之后每次切 4 条", () => {
    expect(switchWrites(10)).toEqual([2, 4, 4]);
    expect(switchWrites(40)).toEqual([2, 4, 4]);
  });
});
