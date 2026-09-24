/**
 * 〔UP1 · `设计/30 §3` 性能八条〕tab 栏的**操作次数**判据 —— 量 DOM 写 / 读 / 监听器个数，不量墙钟。
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
 * 设计与读数住 `调研/第四波记录/UP1.md`（本文件不抄数）。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn(), openUrl: vi.fn() }));

import { TabBarView, type TabBarViewHost } from "../src/tab-bar-view";
import { TabStore } from "../src/tab-store";
import type { TabBarPrefs } from "../src/tab-bar-prefs";
import type { Tab } from "../src/tab-model";
import type { TabCollection } from "../src/tab-collections";

/** tab 栏视图只读 `Tab` 的这几格；其余字段它不碰（`tab-bar-view.ts::updateTabButton` / `updateAccountBadge`）。 */
function fakeTab(sid: string, over: Partial<Tab> = {}): Tab {
  return {
    sessionId: sid,
    title: `标题-${sid}`,
    status: "live",
    pinned: false,
    cwd: "/w",
    origin: null,
    kind: null,
    activity: null,
    tmuxIdle: false,
    forkedFromSessionId: null,
    unread: 0,
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
  for (let i = 0; i < n; i++) {
    const sid = `s${i}`;
    const remote = i % 2 === 1;
    store.tabs.set(sid, fakeTab(sid, { origin: remote ? "pi" : null }));
    store.orderedIds.push(sid);
    if (remote) store.accountLastByS.set(sid, i % 4 === 1 ? "alice" : "bob");
  }
  store.activeId = "s0";
  const prefs = {
    collections: grouped > 0 ? [{ id: "c1", name: "组一", members: store.orderedIds.slice(0, grouped) }] : [],
    commitCollections: vi.fn().mockResolvedValue(undefined),
  };
  const host: TabBarViewHost = {
    refreshTabBar: vi.fn(),
    openTabCwd: vi.fn().mockResolvedValue(undefined),
    bringTerminalToFront: vi.fn().mockResolvedValue(undefined),
    bringRemoteTerminalToFront: vi.fn().mockResolvedValue(undefined),
    closeTab: vi.fn(),
    switchTo: vi.fn(),
    beginDrag: vi.fn(),
    takeSuppressedClick: vi.fn().mockReturnValue(false),
    openMenu: vi.fn(),
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
      // 正控：一颗按钮真变了 ⇒ 那一颗的 10 个开关各调一次（别的按钮仍然 0）。
      (r.store.tabs.get("s5") as { pinned: boolean }).pinned = true;
      r.view.refresh();
      expect(spy).toHaveBeenCalledTimes(10);
    } finally {
      spy.mockRestore();
    }
  });

  it("一个 tab 的红绿灯变了 ⇒ 恰好 1 条记录、落在那颗按钮上（两向：别的按钮 0 条）", () => {
    const r = make(20, 4);
    r.take();
    (r.store.tabs.get("s7") as { activity: unknown }).activity = { status: "idle", waitingFor: null };
    r.view.refresh();
    const recs = r.take();
    expect(recs.map((x) => ownerSid(r, x))).toEqual(["s7"]);
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

describe("P4：启动 N 个会话各报一次红绿灯 ⇒ DOM 写总数是 N，不是 N²", () => {
  /** 照 `tabs.ts::syncActivitySnapshot`：每个会话一次 `updateActivity` ⇒ 每次一次整刷。 */
  const startupWrites = (n: number): number => {
    const r = make(n);
    r.take();
    for (const sid of r.store.orderedIds) {
      (r.store.tabs.get(sid) as { activity: unknown }).activity = { status: "idle", waitingFor: null };
      r.view.refresh();
    }
    return r.take().length;
  };
  it("N = 10 ⇒ 10 条；N = 30 ⇒ 30 条", () => {
    expect(startupWrites(10)).toBe(10);
    expect(startupWrites(30)).toBe(30);
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
  it("散 tab 仍排在**最后一个**组容器之后（`P7a3-Y2`「未归组的照常在后面」；两个组，第二个是后建的）", () => {
    const r = make(6, 2);
    r.prefs.collections.push({ id: "c2", name: "组二", members: ["s2"] });
    r.view.refresh();
    const kids = [...r.bar.children];
    expect(kids.map((e) => (e.classList.contains("tab-group") ? "G" : "t")).join("")).toBe("GGttt");
  });
});
