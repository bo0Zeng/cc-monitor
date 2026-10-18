/**
 * 历史页（`src/frontend/ui/views/history.ts`）的判据。
 *
 * 1. 开页即给焦点（R5W-H08）；问本机后端那一列（`history-browse`）：一问照手上的 ＋ 每台远端各一问带它的 `fresh`（慢的不挡别的）；
 *    几台在后端并好（行按 `at`、按今天 / 昨天 … 分段），界面照项画、不并不排。
 * 2. 敲字 ⇒ 停 150 ms 带 `query` 再问那一窗的整份（不带记号，后端搜全部会话，R2-2-5）；不在界面里过滤。
 * 3. 一台没读到 ⇒ 列表顶「{机器} 离线 · 未列出［重新连接］」，点了带它的 `fresh` 再问（R2-2-6）；别的台照画。
 * 8. 虚拟滚动：只问看得见的那一窗（上下各多带 20 项），滚到哪问到哪；整列高照总数与段头垫好。
 * 4. 回车 ⇒ 内容搜索；失败 ⇒ 清掉旧结果、换成错误条（R5W-H06）。
 * 5. 行上的按钮照后端给的 `can` 画：在跑的是「切过去」（不起第二份）；Codex 行恢复按它自己那一家起（E1）。
 * 6. 删除只问一次（R2-2-2）；在跑的不问、直接说「运行中 · 先结束」。
 * 7. 分叉挂在父会话下（默认收起，「1 个分叉 ▸」）；父会话被筛掉 ⇒ 后端带来的 `context` 行淡显（R5W-H05）。
 */
import { goldenFace, liveFace } from "../../../test-support/session-faces";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
}));
/** 查看器替身：只把宿主拼好的头摆出来（头里的按钮由历史页建，判据点的就是它们）、记下最后一次 `load` 与头下那一条。 */
const viewerStub = vi.hoisted(() => ({ last: null as null | Record<string, unknown>, banner: null as HTMLElement | null }));
vi.mock("../../../../src/frontend/ui/views/session-viewer", () => ({
  viewerPath: (t: string) => Object.assign(document.createElement("span"), { textContent: t }),
  SessionViewer: class {
    element = document.createElement("div");
    load(o: Record<string, unknown>): Promise<void> {
      viewerStub.last = o;
      const h = o.head as { lead?: HTMLElement; badges?: HTMLElement[]; actions?: HTMLElement; meta?: HTMLElement[] } | undefined;
      const head = document.createElement("div");
      head.dataset.role = "viewer-head";
      head.append(...(h?.lead ? [h.lead] : []), ...(h?.badges ?? []), ...(h?.actions ? [h.actions] : []), ...(h?.meta ?? []));
      this.element.replaceChildren(head);
      return Promise.resolve();
    }
    showBanner(el: HTMLElement | null): void {
      viewerStub.banner = el;
    }
    dispose(): void {}
    openFind(): void {}
  },
}));
// 恢复交那台（`session-open`）：这里换成 spy，判的是交了什么；发 · 说 · 开窗住 `session-open.vitest.ts`。
vi.mock("../../../../src/frontend/ui/session-open", () => ({ openSession: vi.fn().mockResolvedValue(true) }));
// 那台的号由 `resumeAccounts` 现问（账号库 ＋ 额度账）；这里只替它答，菜单怎么摆用真的。
vi.mock("../../../../src/frontend/ui/resume-menu", async (orig) => {
  const real = await orig<typeof import("../../../../src/frontend/ui/resume-menu")>();
  return {
    ...real,
    resumeAccounts: vi.fn(async (_origin: string, _agent: string, f: { hasAccounts: boolean; agentName: string; last?: string }) =>
      f.hasAccounts
        ? {
            kind: "list",
            items: [
              { name: "work", label: "work", quota: "5h 41%", last: f.last === "work" },
              { name: "home", label: "home", quota: null, last: f.last === "home" },
              { name: null, label: "不指定账号 · ~/.claude", quota: null, last: false },
            ],
          }
        : { kind: "none", agentName: f.agentName },
    ),
  };
});
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn(), undoToast: vi.fn() }));
vi.mock("../../../../src/frontend/ui/behavior", () => ({
  getBehavior: () => Promise.resolve({ resumeCommand: "" }),
}));
vi.mock("../../../../src/frontend/ui/remote-config", () => ({ resumeCommandFor: () => Promise.resolve("") }));

import { invoke } from "@tauri-apps/api/core";
import { HistoryView } from "../../../../src/frontend/ui/views/history";
import { toast } from "../../../../src/frontend/ui/kit/toast";
import { openSession } from "../../../../src/frontend/ui/session-open";
import { resumeAccounts } from "../../../../src/frontend/ui/resume-menu";
import { chanArgsJson, chanReply, type ChanCallArgs } from "../../../test-support/chan-fake";
import { answerAskDialog, answerAskText, noAskDialog } from "../../../test-support/ask-dialog-driver.ts";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { closeMenu } from "../../../../src/frontend/ui/kit/menu";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const NOW = Date.now();

function row(over: Record<string, unknown>): Record<string, unknown> {
  const sid = String(over.sessionId);
  const at = Number(over.at ?? NOW - 60_000);
  return {
    agent: "claude",
    agentTag: null,
    projectDir: "-w-p",
    projectPath: "/w/p",
    projectName: "p",
    group: "claude:/w/p",
    aiTitle: null,
    firstUserExcerpt: "说过的话",
    title: `标题 ${sid}`,
    label: `标题 ${sid}`,
    untitled: false,
    startedAt: at,
    updatedAt: at,
    at,
    // 那台写好的三格（界面照抄）：一天之内的算「今天」，更早的放进一个月份段。
    atText: "12:00",
    sectionText: at > NOW - 86_400_000 ? copyText("history.section.today") : copyText("history.section.month", { month: 9 }),
    spanText: "09:10–12:00",
    jsonlPath: `/h/.claude/projects/-w-p/${sid}.jsonl`,
    messageCountApprox: 3,
    isBg: false,
    starred: false,
    customTitle: null,
    hidden: null,
    face: goldenFace("ended"),
    can: { resume: "yes", accounts: true, fork: true, delete: "yes" },
    ...over,
  };
}

type Rec = Record<string, unknown>;
/** 每台的会话与组（`origin` 缺 = 本机）；`fail` 里的那几台读不到（`""` = 本机）；`notice` ＝ 那台说注解没并上的那一句。 */
let world: { local: Rec[]; dev: Rec[]; groups: { local: Rec[]; dev: Rec[] }; fail: Set<string>; stale: Set<string>; notice: Record<string, string>; search: "ok" | "fail"; searchAt: (origin: string) => Searched };

/**
 * 本机后端 `history-browse` 的替身：照入参里的几台把行按 `at` 并成一列（同一刻照台的先后）、分叉挂在展开了的父会话下、
 * 按时间看时段头记在头里、按项目看时组头 ＋ 开着的组里那几行；只回 `range` 那一窗（整份那一形）。并法本身的判据在后端（对拍金样）。
 */
function browse(args: Rec): Rec {
  const machines: [string, Rec[]][] = [];
  if (args.here) machines.push(["", world.local]);
  if ((args.remotes as string[]).includes("dev")) machines.push(["dev", world.dev.map((r) => ({ ...r, origin: "dev" }))]);
  const q = String(args.query ?? "");
  const head = machines.map(([o, rs]) => {
    // `stale` 里的那台：读不到、行照上一次的副本出。
    const read = world.stale.has(o) ? { is: "stale", why: "w", at: "09:30" } : world.fail.has(o) ? { is: "failed", why: `${o || "本机"} 读不出` } : { is: "ok", notice: world.notice[o] ?? null };
    const n = read.is !== "failed" ? rs.filter((r) => !q || String(r.label).includes(q)).length : 0;
    return { ...(o ? { origin: o } : {}), total: n, read };
  });
  const ok = machines.filter(([o]) => !world.fail.has(o));
  const rows = ok.flatMap(([, rs]) => rs.filter((r) => !q || String(r.label).includes(q))).sort((a, b) => Number(b.at) - Number(a.at));
  const id = (r: Rec, sid = r.sessionId) => `${String(r.origin ?? "")}\u0000${String(sid)}`;
  const present = new Set(rows.map((r) => id(r)));
  const nested = (r: Rec) => r.forkedFromSessionId !== undefined && present.has(id(r, r.forkedFromSessionId));
  const forks = new Set((args.forks as { origin?: string; sid: string }[]).map((f) => `${f.origin ?? ""}\u0000${f.sid}`));
  const lines: Rec[] = [];
  const push = (r: Rec, child: boolean): void => {
    const kids = rows.filter((k) => k.forkedFromSessionId !== undefined && id(k, k.forkedFromSessionId) === id(r));
    const open = forks.has(id(r));
    lines.push({ kind: "row", row: r, child, forks: kids.length, forksOpen: open, orphan: !child && r.forkedFromSessionId !== undefined && !nested(r) });
    if (open) for (const k of kids) push(k, true);
  };
  const sections: Rec[] = [];
  const groups = ok.flatMap(([o]) => (o ? world.groups.dev.map((g) => ({ ...g, origin: o })) : world.groups.local));
  if (args.by === "time") {
    let sec = "";
    for (const r of rows.filter((r) => !nested(r))) {
      if (r.sectionText !== sec) sections.push({ at: lines.length, text: (sec = String(r.sectionText)) });
      push(r, false);
    }
  } else {
    const fold = args.groups as { open: boolean; except: { origin?: string; key: string }[] };
    for (const g of groups) {
      const flipped = fold.except.some((e) => (e.origin ?? "") === String(g.origin ?? "") && e.key === g.key);
      const open = q !== "" || fold.open !== flipped;
      lines.push({ kind: "group", group: g, open });
      if (open) for (const r of rows.filter((r) => r.group === g.key && String(r.origin ?? "") === String(g.origin ?? "") && !nested(r))) push(r, false);
    }
  }
  const range = args.range as { from: number; limit: number };
  const win = lines.slice(range.from, range.from + range.limit);
  const key = (i: number) => `${range.from + i}`;
  return {
    held: "h",
    whole: true,
    same: false,
    order: win.map((_, i) => key(i)),
    up: Object.fromEntries(win.map((l, i) => [key(i), l])),
    head: { total: lines.length, info: { machines: head, projects: groups.length, grouped: groups.reduce((n, g) => n + Number(g.count), 0), sections } },
  };
}

/** 一台搜内容答什么（替身）：那一份回包 · 没答上。 */
type Searched = { sessions: Rec[]; unreadable: number; skipped: string[] } | "down";

/** 交本机后端的那几问（界面只问本机后端：远端那台由它自己去问）。 */
function calls(op: string): Record<string, unknown>[] {
  return invokeMock.mock.calls
    .filter((c) => c[0] === "chan_call" && (c[1] as ChanCallArgs).op === op)
    .map((c) => chanArgsJson(c[1] as ChanCallArgs) as Record<string, unknown>);
}

/** 要本机后端先同步 dev 那台副本（`fresh`）的次数。 */
function syncs(): number {
  return calls("history-browse").filter((a) => (a.fresh as string[]).includes("dev")).length;
}

/** 钟往前拨 `ms`（只假 `setTimeout`：页里的两处停顿 —— 敲字之后 · 方向键走行之后 —— 拨到界上判，不真等）。 */
const flush = async (ms = 0): Promise<void> => {
  await vi.advanceTimersByTimeAsync(ms);
};

async function opened(): Promise<HistoryView> {
  const v = new HistoryView();
  await v.open();
  await flush();
  return v;
}

const rows = (): HTMLElement[] => [...document.querySelectorAll<HTMLElement>('[role="option"][data-key]')];
const titles = (): string[] => rows().map((r) => r.querySelector("span")?.textContent ?? "");
const byText = (sel: string, text: string): HTMLElement | undefined =>
  [...document.querySelectorAll<HTMLElement>(sel)].find((e) => (e.textContent ?? "").includes(text));

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  // jsdom 没有布局：滚到哪一行量不了，这里只要它不抛。
  Element.prototype.scrollIntoView = () => {};
  document.body.replaceChildren();
  vi.mocked(toast).mockClear();
  vi.mocked(openSession).mockClear();
  vi.mocked(resumeAccounts).mockClear();
  viewerStub.last = null;
  viewerStub.banner = null;
  world = {
    local: [],
    dev: [],
    groups: { local: [], dev: [] },
    fail: new Set(),
    stale: new Set(),
    notice: {},
    search: "ok",
    searchAt: (o) =>
      o === LOCAL_ORIGIN
        ? {
            sessions: [{ agent: "claude", sessionId: "a", projectPath: "/w/p", projectName: "p", jsonlPath: "/x/a.jsonl", title: "标题 a", label: "标题 a", starred: false, hidden: null, updatedAt: NOW, hits: [{ uuid: "u1", tsMs: NOW, kind: "user", before: "**前", matched: "词", after: "`后`" }], moreHits: 0, hitsTruncated: false, isBg: false, face: goldenFace("ended"), can: { resume: "yes", accounts: true, fork: true, delete: "yes" } }],
            unreadable: 0,
            skipped: [],
          }
        : { sessions: [], unreadable: 0, skipped: [] },
  };
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, a: unknown) => {
    if (cmd === "list_remote_mcp_origins") return ["dev"];
    if (cmd !== "chan_call") return undefined;
    const c = a as ChanCallArgs;
    const args = chanArgsJson(c) as Record<string, unknown>;
    if (c.op === "history-browse") return chanReply(browse(args));
    if (c.op === "history-search-merge") {
      // 替真后端：它自己问到各台（`world.searchAt`）、合一份（远端的行添那台的名字、本机不添；行尾 · 时间段两格那台写好）。本机没答上 ⇒ 整次失败。
      if (world.search === "fail") return Promise.reject(new Error("搜不动"));
      const per = (args.machines as string[]).map((o) => [o, world.searchAt(o)] as const);
      const got = per.filter((p): p is readonly [string, Exclude<Searched, "down">] => p[1] !== "down");
      const ss: Record<string, unknown>[] = got.flatMap(([o, r]) => r.sessions.map((x) => ({ ...x, ...(o === LOCAL_ORIGIN ? {} : { origin: o }), atText: "12:00", spanText: "12:00–12:00" })));
      return chanReply({
        totalHits: ss.reduce((n, x) => n + (x.hits as unknown[]).length + (x.moreHits as number), 0),
        sessionCount: ss.length,
        truncated: false,
        sessions: ss,
        unreadable: got.reduce((n, [, r]) => n + r.unreadable, 0),
        skipped: [...new Set(got.flatMap(([, r]) => r.skipped))],
        down: per.filter((p) => p[1] === "down").map(([o]) => ({ origin: o, said: `${o} 问不到` })),
      });
    }
    // 这台恢复默认放在哪（那台答的；勾画在它上面）。
    if (c.op === "sessions-where") return chanReply({ results: (args.sids as string[]).map((sid) => ({ sid, standing: "none", names: [], terminals: [] })), run: "direct" });
    if (c.op === "history-annotate") return chanReply({ entry: { starred: false, customTitle: null, hide: "on", updatedAt: 1 }, prev: null });
    if (c.op === "files-delete-session") return chanReply({ path: "/x" });
    if (c.op === "history-forget") return chanReply({ removed: true, sid: "a" });
    return undefined;
  });
});
afterEach(() => {
  vi.useRealTimers();
});

describe("开页 · 问本机后端那一列 · 按时间", () => {
  it("开页即给焦点；一问照手上的 ＋ dev 同步一问；照后端并好的项画、分段；远端行带机器标签", async () => {
    world.local = [row({ sessionId: "a", at: NOW - 1000 }), row({ sessionId: "c", at: NOW - 3 * 86_400_000 - 1 })];
    world.dev = [row({ sessionId: "b", at: NOW - 2000 })];
    const v = await opened();
    expect(document.activeElement?.classList.contains("history-search")).toBe(true);
    // 开页：一问照手上的（马上答）、dev 那一份要本机后端先同步一次（`fresh`），两问都要两台、都不带记号。
    expect(calls("history-browse").map((a) => [a.here, a.remotes, a.fresh, "held" in a])).toEqual([
      [true, ["dev"], [], false],
      [true, ["dev"], ["dev"], false],
    ]);
    expect(calls("history-list"), "历史页不再逐台问").toEqual([]);
    expect(syncs()).toBe(1);
    expect(rows().map((r) => r.dataset.key?.split("\u0000")[1])).toEqual(["a", "b", "c"]);
    expect(byText('[role="option"]', "标题 b")?.textContent).toContain("dev");
    // 分段头照抄那台写好的字：同一个字的挨着的行一段，换了字起新的一段。
    const body = document.body.textContent ?? "";
    expect(body.indexOf(copyText("history.section.today")), "今天那一段在前").toBeGreaterThanOrEqual(0);
    expect(body.indexOf(copyText("history.section.month", { month: 9 })), "更早那一行起了月份段").toBeGreaterThan(body.indexOf(copyText("history.section.today")));
    v.close();
  });

  it("敲字 ⇒ 停 150 ms 后带 query 再问那一窗的整份（不带记号），界面不自己筛", async () => {
    world.local = [row({ sessionId: "a" }), row({ sessionId: "z", label: "别的" })];
    const v = await opened();
    invokeMock.mock.calls.length = 0;
    const input = document.querySelector<HTMLInputElement>(".history-search")!;
    input.value = "标题";
    input.dispatchEvent(new Event("input"));
    await flush(149);
    expect(calls("history-browse"), "还没停够就问了").toEqual([]);
    await flush(1);
    expect(calls("history-browse").map((a) => [a.query, a.fresh, a.range, "held" in a])).toEqual([["标题", [], { from: 0, limit: 21 }, false]]);
    expect(syncs(), "敲字不该再去问那台").toBe(0);
    expect(titles()).toEqual(["标题 a"]);
    v.close();
  });
});

describe("一台没读到", () => {
  it("dev 读不到 ⇒ 顶上一条［重新连接］，点了带 dev 的 fresh 再问；本机照画", async () => {
    world.local = [row({ sessionId: "a" })];
    world.fail.add("dev");
    const v = await opened();
    expect(titles()).toEqual(["标题 a"]);
    const msg = copyText("history.list.machineDown", { machine: "dev" });
    expect(document.body.textContent).toContain(msg);
    invokeMock.mock.calls.length = 0;
    world.fail.clear();
    world.dev = [row({ sessionId: "b", at: NOW })];
    byText("button", copyText("history.list.reconnect"))!.click();
    await flush();
    expect(calls("history-browse").map((a) => [a.remotes, a.sort, a.fresh])).toEqual([
      [["dev"], "activity", []],
      [["dev"], "activity", ["dev"]],
    ]);
    expect(syncs(), "重新连接 ⇒ 再问那台一次").toBe(1);
    expect(titles()).toEqual(["标题 b", "标题 a"]);
    expect(document.body.textContent).not.toContain(msg);
    v.close();
  });
});

describe("虚拟滚动：只问看得见的那一窗", () => {
  it("开页只要看得见的那几项（上下各多带 20）；滚下去问到那一窗、画的就是那一窗；整列高照总数与段头垫好", async () => {
    // 300 行：前 100 行在「今天」，余下的在九月那一段（两个段头）。
    world.local = Array.from({ length: 300 }, (_, i) => row({ sessionId: `s${i}`, at: i < 100 ? NOW - 1000 - i : NOW - 2 * 86_400_000 - i }));
    world.dev = [];
    const height = vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) {
      return this.getAttribute("role") === "listbox" ? 520 : 0;
    });
    try {
      const v = await opened();
      // 520 高 ⇒ 看得见 0–10 项，再往下多带 20。
      expect(calls("history-browse").map((a) => a.range)).toEqual([
        { from: 0, limit: 31 },
        { from: 0, limit: 31 },
      ]);
      const list = document.querySelector<HTMLElement>('[role="listbox"]')!;
      const spacer = list.querySelector<HTMLElement>("[class*=hvSpacer]")!;
      expect(spacer.style.height, "300 行 × 52 ＋ 两个段头 × 32").toBe(`${300 * 52 + 2 * 32}px`);
      expect(rows()).toHaveLength(31);
      invokeMock.mock.calls.length = 0;
      // 滚到第 150 项那里（越过了第一个段头 ＋ 100 行 ＋ 第二个段头）。
      list.scrollTop = 32 + 100 * 52 + 32 + 50 * 52;
      list.dispatchEvent(new Event("scroll"));
      await flush();
      expect(calls("history-browse").map((a) => [a.range, a.fresh])).toEqual([[{ from: 130, limit: 51 }, []]]);
      expect(rows().map((r) => r.dataset.key?.split("\u0000")[1])).toEqual(Array.from({ length: 51 }, (_, i) => `s${130 + i}`));
      expect(list.querySelector<HTMLElement>("[class*=hvBlock]")!.style.top, "那一窗摆在第 130 项的高度上").toBe(`${32 + 130 * 52 + 32}px`);
      // 粘在顶上的段头写的是看得见的第一项那一段。
      expect(list.querySelector<HTMLElement>('[data-pin="true"]')!.textContent).toBe(copyText("history.section.month", { month: 9 }));
      // 在那一窗里再滚一点 ⇒ 不再问。
      invokeMock.mock.calls.length = 0;
      list.scrollTop += 52 * 3;
      list.dispatchEvent(new Event("scroll"));
      await flush();
      expect(calls("history-browse")).toEqual([]);
      v.close();
    } finally {
      height.mockRestore();
    }
  });
});

describe("推送叫回来的重问", () => {
  it("那台推「清单变了」⇒ 页开着、那台在这一页上才再问那一窗（远端带它的 fresh，本机不带）；改名 · 星标 · 隐藏 · 删之后页里不另问", async () => {
    world.local = [row({ sessionId: "a" })];
    world.dev = [row({ sessionId: "b", at: NOW - 120_000 })];
    const v = await opened();
    invokeMock.mock.calls.length = 0;
    v.onListMoved("dev");
    await flush();
    expect(calls("history-browse").map((a) => a.fresh)).toEqual([[], ["dev"]]);
    invokeMock.mock.calls.length = 0;
    v.onListMoved(LOCAL_ORIGIN);
    await flush();
    expect(calls("history-browse").map((a) => a.fresh)).toEqual([[]]);
    // 页里的动作之后不另问：那台的推送会叫回来。
    invokeMock.mock.calls.length = 0;
    rows()[0].dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 3, clientY: 3 }));
    [...document.querySelectorAll<HTMLElement>('[role^="menuitem"]')].find((b) => b.textContent?.startsWith(copyText("history.menu.star")))!.click();
    await flush();
    expect(calls("history-annotate")).toHaveLength(1);
    expect(calls("history-browse"), "标星之后页里又问了一次").toEqual([]);
    // 不在这一页上的那台 · 关了页 ⇒ 不问。
    byText("button", copyText("history.page.filter"))!.click();
    await flush();
    byText("label", "dev")!.querySelector("input")!.click();
    await flush();
    invokeMock.mock.calls.length = 0;
    v.onListMoved("dev");
    v.close();
    v.onListMoved(LOCAL_ORIGIN);
    await flush();
    expect(calls("history-browse")).toEqual([]);
    // 筛掉的那台记在本机界面偏好里，别带进后面几条。
    localStorage.clear();
  });
});

describe("一台读不到 · 有上一次的副本", () => {
  it("行照副本出，列表顶说是几时读到的 ＋［重新连接］（带它的 fresh）；同步再成了标记清掉", async () => {
    world.local = [row({ sessionId: "a" })];
    world.dev = [row({ sessionId: "b", at: NOW - 120_000 })];
    world.stale.add("dev");
    const v = await opened();
    expect(titles()).toEqual(["标题 a", "标题 b"]);
    const said = copyText("history.list.machineStale", { machine: "dev", at: "09:30" });
    expect(document.body.textContent).toContain(said);
    expect(document.body.textContent).not.toContain(copyText("history.list.machineDown", { machine: "dev" }));
    invokeMock.mock.calls.length = 0;
    world.stale.clear();
    byText("button", copyText("history.list.reconnect"))!.click();
    await flush();
    expect(syncs()).toBe(1);
    expect(document.body.textContent, "同步再成了还标着旧").not.toContain(said);
    v.close();
  });
});

describe("内容搜索", () => {
  it("回车搜内容；再搜失败 ⇒ 旧结果清掉、换成错误条", async () => {
    world.local = [row({ sessionId: "a" })];
    const v = await opened();
    const input = document.querySelector<HTMLInputElement>(".history-search")!;
    input.value = "词";
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await flush();
    expect(document.querySelector("mark")?.textContent).toBe("词");
    // 片段去行内排版记号（与会话内查找同一个口子 `find-strip.ts::plainSnippet`）
    expect(document.querySelector("mark")?.parentElement?.textContent).toBe("前词后");
    world.search = "fail";
    vi.spyOn(console, "warn").mockImplementation(() => {});
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await flush();
    expect(document.querySelector("mark")).toBeNull();
    expect(document.body.textContent).toContain(copyText("history.search.failed", { why: "" }).trim().split("·")[0].trim());
    v.close();
  });

  it("★ 命中那一行的名字 · 星标 · 隐藏照那台在命中行上带来的画（清单里没有这一行也一样，界面不拿清单去对）", async () => {
    world.local = [];
    const base = world.searchAt;
    world.searchAt = (o) => {
      const r = base(o);
      if (r === "down" || o !== LOCAL_ORIGIN) return r;
      return { ...r, sessions: r.sessions.map((x) => ({ ...x, label: "改过的名", starred: true, hidden: { by: "self", said: "占位理由" } })) };
    };
    const v = await opened();
    const input = document.querySelector<HTMLInputElement>(".history-search")!;
    input.value = "词";
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await flush();
    const body = document.body.textContent ?? "";
    expect(body).toContain("改过的名");
    expect(body).toContain(copyText("history.row.hidden"));
    expect(document.querySelector('[role="option"] svg[data-icon="star"]'), "星标那一枚没画").not.toBeNull();
    v.close();
  });
});

describe("行上的动作照 can 画", () => {
  it("在跑的 ⇒「切过去」，点了切主窗口、不起第二份", async () => {
    world.local = [row({ sessionId: "a", face: liveFace("working"), can: { resume: "switch", accounts: true, fork: true, delete: "live" } })];
    const v = await opened();
    const went: string[] = [];
    v.switchTo = (sid) => went.push(sid);
    byText("button", copyText("history.row.switch"))!.click();
    await flush();
    expect(went).toEqual(["a"]);
    expect(openSession).not.toHaveBeenCalled();
    expect(v.isVisible()).toBe(false);
  });

  it("Codex 行恢复 ⇒ 交那台的只有那一条（哪一家那台从记录里读）", async () => {
    world.local = [row({ sessionId: "cx", agent: "codex", agentTag: "Codex", face: goldenFace("unclear"), can: { resume: "yes", accounts: false, fork: false, delete: "unsure" } })];
    const v = await opened();
    expect(byText('[role="option"]', "Codex")).toBeTruthy();
    byText("button", copyText("history.row.resume"))!.click();
    await flush();
    expect(vi.mocked(openSession).mock.calls[0].slice(0, 2)).toEqual([LOCAL_ORIGIN, { what: { kind: "resume", sid: "cx" }, local: true }]);
    v.close();
  });
});

describe("删除", () => {
  it("只问一次；在跑的不问、说「运行中 · 先结束」", async () => {
    world.local = [row({ sessionId: "a" }), row({ sessionId: "l", face: liveFace("working"), can: { resume: "switch", accounts: true, fork: true, delete: "live" } })];
    const v = await opened();
    const key = (sid: string) => rows().find((r) => r.dataset.key?.endsWith(sid))!;
    key("l").click();
    key("l").dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", bubbles: true }));
    await flush();
    expect(noAskDialog()).toBe(true);
    expect(vi.mocked(toast).mock.calls.map((c) => c[0])).toContain(copyText("history.delete.liveHint"));
    key("a").click();
    key("a").dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", bubbles: true }));
    await flush();
    await answerAskDialog(true);
    await flush();
    expect(noAskDialog()).toBe(true);
    v.close();
  });
});

describe("分叉", () => {
  it("挂在父会话下、默认收起；父会话是 context ⇒ 淡显", async () => {
    world.local = [
      row({ sessionId: "kid", at: NOW - 1000, forkedFromSessionId: "dad", forkedFromMessageUuid: "m" }),
      row({ sessionId: "dad", at: NOW - 5000, context: true }),
    ];
    const v = await opened();
    expect(rows().map((r) => r.dataset.key?.split("\u0000")[1])).toEqual(["dad"]);
    expect(rows()[0].dataset.context).toBe("true");
    invokeMock.mock.calls.length = 0;
    byText("button", copyText("history.row.forks", { n: 1 }))!.click();
    await flush();
    // 展开交后端摊：下一问带上展开了的那一行。
    expect(calls("history-browse").map((a) => a.forks)).toEqual([[{ sid: "dad" }]]);
    expect(rows().map((r) => r.dataset.key?.split("\u0000")[1])).toEqual(["dad", "kid"]);
    expect(rows()[1].dataset.child).toBe("true");
    v.close();
  });
});

describe("按项目", () => {
  it("组按后端给的序列出、收着；点开出那一组的行；「全部展开」全开", async () => {
    world.local = [row({ sessionId: "a" }), row({ sessionId: "b", group: "claude:/w/q", projectPath: "/w/q", projectName: "q" })];
    const groups = [
      { key: "claude:/w/p", agent: "claude", projectName: "p", projectPath: "/w/p", projectDir: "-w-p", count: 1, hasLive: false, starred: false, lastActivity: NOW, order: 2, failed: null },
      { key: "claude:/w/q", agent: "claude", projectName: "q", projectPath: "/w/q", projectDir: "-w-q", count: 1, hasLive: false, starred: false, lastActivity: NOW, order: 1, failed: null },
      { key: "dir:-w-x", agent: "claude", projectName: "-w-x", projectPath: "", projectDir: "-w-x", count: 0, hasLive: null, starred: false, lastActivity: 0, order: 0, failed: "读不了" },
    ];
    world.groups.local = groups;
    const v = await opened();
    byText('[role="tab"]', copyText("history.page.byProject"))!.click();
    await flush();
    const heads = (): HTMLElement[] => [...document.querySelectorAll<HTMLElement>('[role="treeitem"]')];
    expect(heads().map((h) => h.textContent?.slice(0, 1))).toEqual(["p", "q", "-"]);
    expect(rows()).toEqual([]);
    expect(heads()[2].textContent).toContain(copyText("history.group.failed"));
    expect(document.body.textContent).toContain(copyText("history.list.countProjects", { p: 3, n: 2 }));
    invokeMock.mock.calls.length = 0;
    heads()[0].click();
    await flush();
    expect(calls("history-browse").map((a) => [a.by, a.groups])).toEqual([["project", { open: false, except: [{ key: "claude:/w/p" }] }]]);
    expect(titles()).toEqual(["标题 a"]);
    byText("button", copyText("history.list.expandAll"))!.click();
    await flush();
    expect(titles()).toEqual(["标题 a", "标题 b"]);
    invokeMock.mock.calls.length = 0;
    byText("button", copyText("history.group.retry"))!.click();
    await flush();
    expect(calls("history-browse").map((a) => a.fresh)).toEqual([[]]);
    byText('[role="tab"]', copyText("history.page.byTime"))!.click();
    v.close();
  });
});

describe("筛选", () => {
  it("显示已隐藏 · 时间 · 排序都带进下一问；去掉一台 ⇒ 不再问它；筛选按钮上记几项", async () => {
    world.local = [row({ sessionId: "a" })];
    const v = await opened();
    byText("button", copyText("history.page.filter"))!.click();
    await flush();
    const box = (label: string) => byText("label", label)!.querySelector("input")!;
    invokeMock.mock.calls.length = 0;
    box(copyText("history.filter.showHidden")).click();
    await flush();
    expect(calls("history-browse").map((a) => a.hidden)).toEqual([true]);
    invokeMock.mock.calls.length = 0;
    box(copyText("history.filter.time7d")).click();
    box(copyText("history.filter.sortCreated")).click();
    await flush();
    expect(calls("history-browse").at(-1)).toMatchObject({ within_days: 7, sort: "created", hidden: true });
    invokeMock.mock.calls.length = 0;
    box("dev").click();
    await flush();
    box(copyText("history.filter.time30d")).click();
    await flush();
    expect(calls("history-browse").map((a) => a.remotes)).toEqual([[], []]);
    expect(byText("button", copyText("history.page.filter"))!.textContent).toContain("4");
    v.close();
  });
});

describe("键盘与菜单", () => {
  it("↓ 从搜索框进列表、走行；Ctrl+回车恢复；菜单里标星 · 隐藏（可撤）· 改标题", async () => {
    world.local = [row({ sessionId: "a" }), row({ sessionId: "b", at: NOW - 120_000 })];
    const v = await opened();
    const input = document.querySelector<HTMLInputElement>(".history-search")!;
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    expect(document.activeElement?.getAttribute("data-key")?.endsWith("a")).toBe(true);
    document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    expect(document.activeElement?.getAttribute("data-key")?.endsWith("b")).toBe(true);
    document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "End", bubbles: true }));
    document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Home", bubbles: true }));
    document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", ctrlKey: true, bubbles: true }));
    await flush();
    expect(vi.mocked(openSession).mock.calls[0][1]).toMatchObject({ what: { kind: "resume", sid: "a" } });
    const v2 = await opened();
    const menuItem = (label: string) => [...document.querySelectorAll<HTMLElement>('[role^="menuitem"]')].find((b) => b.textContent?.startsWith(label));
    const openMenuOn = (sid: string) => rows().find((r) => r.dataset.key?.endsWith(sid))!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 3, clientY: 3 }));
    openMenuOn("a");
    menuItem(copyText("history.menu.star"))!.click();
    await flush();
    expect(calls("history-annotate").at(-1)).toEqual({ sid: "a", patch: { starred: true } });
    openMenuOn("b");
    menuItem(copyText("history.menu.hide"))!.click();
    await flush();
    expect(calls("history-annotate").at(-1)).toEqual({ sid: "b", patch: { hide: "on" } });
    const { undoToast } = await import("../../../../src/frontend/ui/kit/toast");
    expect(vi.mocked(undoToast)).toHaveBeenCalled();
    // 撤销同标签栏那一处：照回包里改之前那一格交回去（原本没有单独设置 ⇒ `rule`，同一个交命令的口）。
    (vi.mocked(undoToast).mock.calls.at(-1)![1] as () => void)();
    await flush();
    expect(calls("history-annotate").at(-1)).toEqual({ sid: "b", patch: { hide: "rule" } });
    openMenuOn("a");
    menuItem(copyText("history.menu.rename"))!.click();
    await answerAskText("新名字");
    await flush();
    expect(calls("history-annotate").at(-1)).toEqual({ sid: "a", patch: { customTitle: "新名字" } });
    document.activeElement?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    v2.close();
    v.close();
  });
});

describe("内容头 · 恢复 ▾（乙4-④⑤）", () => {
  const head = (): HTMLElement => document.querySelector<HTMLElement>('[data-role="viewer-head"]')!;
  const inHead = (text: string): HTMLButtonElement | undefined =>
    [...head().querySelectorAll<HTMLButtonElement>("button")].find((b) => (b.textContent ?? "").includes(text) || b.getAttribute("aria-label") === text);
  const menuItems = (): string[] => [...document.querySelectorAll<HTMLElement>('[role^="menuitem"]')].map((e) => e.textContent ?? "");
  const menuItem = (label: string): HTMLElement | undefined =>
    [...document.querySelectorAll<HTMLElement>('[role^="menuitem"]')].find(
      (b) => b.querySelector('[data-part="label"]')?.textContent === label || b.textContent?.startsWith(label),
    );
  // 前面那几格的筛选（关掉 dev 那台）记在本机，别带进来。
  beforeEach(() => localStorage.clear());
  const showRow = async (sid: string): Promise<void> => {
    rows().find((r) => r.dataset.key?.endsWith(sid))!.click();
    await flush(250);
  };

  it("已结束的行：头上［恢复 ▾］＋ 新窗口 ＋ ⋯；第二行 项目 · 本机 · 路径 · 时间段；▾ ＝ 账号 · 运行于两组单选 ＋「在此目录新建会话」", async () => {
    world.local = [row({ sessionId: "a", lastAccount: "work" })];
    const v = await opened();
    await showRow("a");
    expect(viewerStub.last).toMatchObject({ displayTitle: "标题 a", origin: "<local>", cwd: "/w/p" });
    const main = inHead(copyText("history.row.resume"))!;
    const pick = (account: string, tmux: boolean): string =>
      copyText("resumeMenu.hint.pick", {
        pick: copyText("resumeMenu.pick.account", { account, run: tmux ? copyText("resumeMenu.hint.tmux") : copyText("resumeMenu.run.direct") }),
      });
    expect(main.title, "没开过 ▾：上次的号 · 照机器设置").toBe(
      copyText("resumeMenu.hint.pick", { pick: copyText("resumeMenu.pick.account", { account: "work", run: copyText("resumeMenu.run.machine") }) }),
    );
    expect(inHead(copyText("history.menu.openWindow"))).toBeTruthy();
    expect(inHead(copyText("history.row.more"))).toBeTruthy();
    expect(head().textContent).toContain("p");
    expect(head().textContent).toContain(copyText("history.filter.local"));
    expect(head().textContent).toContain("/w/p");
    expect(head().textContent, "时间段照抄那台写好的 spanText").toContain("09:10–12:00");
    inHead(copyText("history.resume.more"))!.click();
    await flush();
    expect(vi.mocked(resumeAccounts).mock.calls[0].slice(0, 2)).toEqual(["<local>", "claude"]);
    const headings = [...document.querySelectorAll<HTMLElement>('[role="menu"] [role="presentation"]')].map((e) => e.textContent);
    expect(headings).toEqual([copyText("resumeMenu.group.account"), copyText("resumeMenu.group.run")]);
    const items = menuItems();
    const work = menuItem("work")!;
    expect(work.querySelector(".acct-avatar"), "号前面它的头像").not.toBeNull();
    expect(work.querySelector('[data-part="detail"]')?.textContent, "号后面它的 5h，上次用的那个标「上次」").toBe(copyText("resumeMenu.account.quotaLast", { quota: "5h 41%" }));
    expect(items.indexOf(work.textContent ?? ""), "账号组在最上面").toBe(0);
    expect(menuItem("work")!.getAttribute("aria-checked")).toBe("true");
    expect(menuItem(copyText("resumeMenu.run.direct"))!.getAttribute("aria-checked")).toBe("true");
    expect(items).toContain(copyText("history.menu.newInDir"));
    // 点单选只挪勾、菜单不关、什么都不起；主按钮的悬停跟着改
    menuItem("home")!.click();
    menuItem(copyText("resumeMenu.run.tmux"))!.click();
    await flush();
    expect(document.querySelector('[role="menu"]'), "点单选菜单不关").not.toBeNull();
    expect(menuItem("work")!.getAttribute("aria-checked")).toBe("false");
    expect(menuItem("home")!.getAttribute("aria-checked")).toBe("true");
    expect(menuItem(copyText("resumeMenu.run.tmux"))!.getAttribute("aria-checked")).toBe("true");
    expect(openSession).not.toHaveBeenCalled();
    expect(main.title).toBe(pick("home", true));
    // 主按钮按勾着的那一组起：tmux 那一支（不依赖标签页对象），带点名的号
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    main.click();
    await flush();
    expect(vi.mocked(openSession).mock.calls.map((c) => c.slice(0, 2))).toEqual([
      [LOCAL_ORIGIN, { what: { kind: "resume", sid: "a" }, account: { kind: "named", name: "home" }, run: "tmux", local: true }],
    ]);
    v.close();
  });

  it("★ 主按钮 ＝ 人没动过的那一种：恰好一问、不带号也不带放在哪（那台照跟随 · 照自己的设置）；没起来 ⇒ 不关历史页；起了 ⇒ 关", async () => {
    world.local = [row({ sessionId: "a" })];
    const v = await opened();
    await showRow("a");
    vi.mocked(openSession).mockResolvedValueOnce(false);
    inHead(copyText("history.row.resume"))!.click();
    await flush();
    expect(vi.mocked(openSession).mock.calls.map((c) => c.slice(0, 2))).toEqual([[LOCAL_ORIGIN, { what: { kind: "resume", sid: "a" }, local: true }]]);
    expect(v.isVisible()).toBe(true);
    inHead(copyText("history.row.resume"))!.click();
    await flush();
    expect(v.isVisible(), "起了 ⇒ 关历史页").toBe(false);
  });

  it("勾基座 ⇒ 主按钮按基座起（不用 tmux 那一支）；勾只在这一页开着时记", async () => {
    world.dev = [row({ sessionId: "r" })];
    const v = await opened();
    await showRow("r");
    inHead(copyText("history.resume.more"))!.click();
    await flush();
    expect(vi.mocked(resumeAccounts).mock.calls[0][0]).toBe("dev");
    menuItem("不指定账号 · ~/.claude")!.click();
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    inHead(copyText("history.row.resume"))!.click();
    await flush();
    expect(vi.mocked(openSession).mock.calls.at(-1)!.slice(0, 2)).toEqual(["dev", { what: { kind: "resume", sid: "r" }, account: { kind: "base" }, local: false }]);
    v.close();
    await v.open();
    await flush();
    await showRow("r");
    expect(inHead(copyText("history.row.resume"))!.title, "重开页 ⇒ 回到默认那一组").toBe(
      copyText("resumeMenu.hint.pick", { pick: copyText("resumeMenu.run.machine") }),
    );
    v.close();
  });

  it("Codex 行：▾ 不问账号，灰一行「Codex · 无账号维」；在跑的 ⇒［切过去］；分身 ⇒ 恢复灰着说为什么", async () => {
    world.local = [
      row({ sessionId: "cx", agent: "codex", agentTag: "Codex", can: { resume: "yes", accounts: false, fork: false, delete: "yes" } }),
      row({ sessionId: "l", face: liveFace("working"), can: { resume: "switch", accounts: true, fork: true, delete: "live" } }),
      row({ sessionId: "bg", isBg: true, can: { resume: "bg", accounts: true, fork: false, delete: "yes" } }),
    ];
    const v = await opened();
    await showRow("cx");
    expect(inHead(copyText("history.row.resume"))!.title).toBe(
      copyText("resumeMenu.hint.pick", { pick: copyText("resumeMenu.pick.account", { account: "Codex", run: copyText("resumeMenu.run.machine") }) }),
    );
    inHead(copyText("history.resume.more"))!.click();
    await flush();
    expect(vi.mocked(resumeAccounts).mock.calls[0][2]).toMatchObject({ hasAccounts: false, agentName: "Codex" });
    const na = menuItem(copyText("history.resume.noAccounts", { agent: "Codex" }))!;
    expect((na as HTMLButtonElement).disabled).toBe(true);
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await showRow("l");
    expect(inHead(copyText("history.row.switch"))).toBeTruthy();
    expect(inHead(copyText("history.resume.more"))).toBeUndefined();
    await showRow("bg");
    expect(inHead(copyText("history.row.resume"))!.getAttribute("aria-disabled")).toBe("true");
    expect(inHead(copyText("history.row.resume"))!.title).toBe(copyText("history.row.bgHint"));
    v.close();
  });
});

describe("窄档 / 中档 · 列表宽度可拖（乙4-① ⑧）", () => {
  beforeEach(() => localStorage.clear());
  const split = (): HTMLElement => document.querySelector<HTMLElement>('[data-pane]')!;
  const sizer = (): HTMLElement => document.querySelector<HTMLElement>('[role="separator"]')!;

  it("拖那一道改列表宽（夹在 320–640）、松手记下，下次开页照它；← → 一步 16", async () => {
    world.local = [row({ sessionId: "a" })];
    const v = await opened();
    const root = document.querySelector<HTMLElement>(".history-view")!;
    const sz = sizer();
    expect(sz.getAttribute("aria-valuenow")).toBe("420");
    sz.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 420, pointerId: 1 }));
    sz.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 900, pointerId: 1 }));
    expect(root.style.getPropertyValue("--hv-list-w")).toBe("640px");
    sz.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, clientX: 100, pointerId: 1 }));
    expect(root.style.getPropertyValue("--hv-list-w")).toBe("320px");
    sz.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    expect(root.style.getPropertyValue("--hv-list-w")).toBe("336px");
    v.close();
    const w = new HistoryView();
    await w.open();
    expect(document.querySelectorAll<HTMLElement>(".history-view")[0].style.getPropertyValue("--hv-list-w")).toBe("336px");
    w.close();
  });

  it("窄档：点开一行 ⇒ 内容盖满；头左端「列表」与 Esc 都回列表、焦点回那一行；方向键走行不翻", async () => {
    vi.stubGlobal("matchMedia", (q: string) => ({ matches: q.includes("799"), media: q }));
    try {
      world.local = [row({ sessionId: "a" }), row({ sessionId: "b", at: NOW - 120_000 })];
      const v = await opened();
      expect(split().dataset.pane).toBe("list");
      rows()[0].click();
      expect(split().dataset.pane).toBe("content");
      await flush(250);
      const back = [...document.querySelectorAll<HTMLButtonElement>('[data-role="viewer-head"] button')].find((b) => b.textContent === copyText("history.content.toList"))!;
      back.click();
      expect(split().dataset.pane).toBe("list");
      expect(document.activeElement?.getAttribute("data-key")?.endsWith("a")).toBe(true);
      document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
      await flush(250);
      expect(split().dataset.pane, "方向键只走行").toBe("list");
      document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
      expect(split().dataset.pane).toBe("content");
      // Esc 走快捷键的弹层栈（台子里没起分发器）⇒ 直接问历史页那一层。
      expect((v as unknown as { handleEsc(): boolean }).handleEsc()).toBe(true);
      expect(split().dataset.pane).toBe("list");
      expect(v.isVisible(), "Esc 先回列表，不关历史页").toBe(true);
      v.close();
    } finally {
      vi.unstubAllGlobals();
    }
  });
});

describe("键盘：列表里每个键做什么", () => {
  beforeEach(() => localStorage.clear());
  const key = (k: string, mod: KeyboardEventInit = {}, at: Element = document.activeElement!): void => {
    at.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true, ...mod }));
  };
  const focusedSid = (): string | undefined => document.activeElement?.getAttribute("data-key")?.split("\u0000")[1];

  it("↑ 在第一行 ⇒ 回搜索框；PageDown / PageUp 一页一页走（夹在两头）；End / Home 到两头", async () => {
    world.local = ["a", "b", "c"].map((sid, i) => row({ sessionId: sid, at: NOW - (i + 1) * 1000 }));
    const v = await opened();
    document.querySelector<HTMLInputElement>(".history-search")!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    expect(focusedSid()).toBe("a");
    key("PageDown");
    expect(focusedSid(), "jsdom 没高度 ⇒ 一页 = 1 行").toBe("b");
    key("End");
    expect(focusedSid()).toBe("c");
    key("PageDown");
    expect(focusedSid(), "夹在最后一行").toBe("c");
    key("PageUp");
    expect(focusedSid()).toBe("b");
    key("Home");
    expect(focusedSid()).toBe("a");
    key("ArrowUp");
    expect(document.activeElement?.classList.contains("history-search")).toBe(true);
    v.close();
  });

  it("回车 ⇒ 右边看它、焦点进内容；Shift+回车 ⇒ 新窗口；F2 ⇒ 改标题；Delete ⇒ 删；ContextMenu / Shift+F10 ⇒ 菜单；没选中时这几个键不接", async () => {
    world.local = [row({ sessionId: "a" })];
    const v = await opened();
    const list = document.querySelector<HTMLElement>('[role="listbox"]')!;
    list.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await flush();
    expect(viewerStub.last, "没选中就打开了").toBeNull();
    document.querySelector<HTMLInputElement>(".history-search")!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    key("Enter");
    await flush();
    expect(viewerStub.last).toMatchObject({ displayTitle: "标题 a" });
    expect(document.querySelector<HTMLElement>("[class*=hvSplit]")!.dataset.pane).toBe("content");
    invokeMock.mock.calls.length = 0;
    key("Enter", { shiftKey: true }, rows()[0]);
    await flush();
    expect(invokeMock.mock.calls.filter((c) => c[0] === "open_session_in_new_window").map((c) => c[1])).toEqual([{ sessionId: "a", origin: "<local>", title: "标题 a" }]);
    key("F2", {}, rows()[0]);
    await answerAskText(null);
    await flush();
    key("Delete", {}, rows()[0]);
    await answerAskDialog(false);
    await flush();
    expect(calls("files-delete-session")).toEqual([]);
    const menus = (): number => document.querySelectorAll('[role="menu"]').length;
    key("F10", {}, rows()[0]);
    expect(menus(), "只按 F10 不开菜单").toBe(0);
    key("ContextMenu", {}, rows()[0]);
    expect(menus()).toBe(1);
    closeMenu();
    key("F10", { shiftKey: true }, rows()[0]);
    expect(menus()).toBe(1);
    closeMenu();
    v.close();
  });
});

describe("列表顶上的提示条 · 空着时说什么", () => {
  beforeEach(() => localStorage.clear());

  it("本机读不出 ⇒ 红条［重试］再问（不同步远端）；那台说清单不全 ⇒ 黄条［重试］带它的 fresh；没有截断那一句（滚到哪问到哪）", async () => {
    world.fail.add("");
    world.dev = [row({ sessionId: "b" })];
    world.notice.dev = "partial";
    const v = await opened();
    expect(document.body.textContent).toContain(copyText("history.list.localFailed"));
    expect(document.body.textContent).toContain(copyText("history.list.notice"));
    expect(document.body.textContent).not.toContain(copyText("history.list.truncated"));
    invokeMock.mock.calls.length = 0;
    world.fail.clear();
    byText("[class*=strip], div", copyText("history.list.localFailed"));
    const retries = [...document.querySelectorAll<HTMLButtonElement>("button")].filter((b) => b.textContent === copyText("history.list.retry"));
    expect(retries).toHaveLength(2);
    retries[0].click();
    await flush();
    expect(calls("history-browse").map((a) => a.fresh)).toEqual([[]]);
    expect(syncs()).toBe(0);
    retries[1].click();
    await flush();
    expect(calls("history-browse").map((a) => a.fresh).at(-1)).toEqual(["dev"]);
    expect(syncs()).toBe(1);
    v.close();
  });

  it("一行都没有：带着字 / 筛选 ⇒「没有匹配」＋［清除筛选］（清掉字与筛选、重问）；什么都没筛 ⇒ 说还没有会话", async () => {
    const v = await opened();
    expect(document.body.textContent).toContain(copyText("history.list.empty"));
    expect(document.body.textContent).not.toContain(copyText("history.list.noMatch"));
    const input = document.querySelector<HTMLInputElement>(".history-search")!;
    input.value = "x";
    input.dispatchEvent(new Event("input"));
    await flush(200);
    expect(document.body.textContent).toContain(copyText("history.list.noMatch"));
    invokeMock.mock.calls.length = 0;
    byText("button", copyText("history.list.clearFilter"))!.click();
    await flush(200);
    expect(input.value).toBe("");
    expect(calls("history-browse").at(-1)!.query, "清掉之后最后那一问还带着字").toBeUndefined();
    v.close();
  });
});

describe("内容搜索的其余几态", () => {
  beforeEach(() => localStorage.clear());
  const enter = (q: string): void => {
    const input = document.querySelector<HTMLInputElement>(".history-search")!;
    input.value = q;
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  };

  it("搜着时列表头转圈 ＋［停止］，点了回到清单、晚到的结果不画", async () => {
    world.local = [row({ sessionId: "a" })];
    const held: (() => void)[] = [];
    const base = invokeMock.getMockImplementation() as (cmd: string, a: unknown) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, a: unknown) => {
      if (cmd === "chan_call" && (a as ChanCallArgs).op === "history-search-merge") await new Promise<void>((r) => held.push(r));
      return base(cmd, a);
    });
    const v = await opened();
    enter("词");
    await flush();
    expect(document.body.textContent).toContain(copyText("history.search.searching"));
    byText("button", copyText("history.search.stop"))!.click();
    expect(titles()).toEqual(["标题 a"]);
    for (const r of held) r();
    await flush();
    await flush();
    expect(held.length, "只问本机后端一次").toBe(1);
    expect(document.querySelector("mark"), "停了之后结果又画上来").toBeNull();
    expect(titles()).toEqual(["标题 a"]);
    v.close();
  });

  it("没搜到 ⇒ 说没有、没含工具输出时给［含工具输出与思考］（点了带上它再搜）；那台没答 / 跳过的那一家 / 读不了的份数各一条", async () => {
    world.local = [row({ sessionId: "a" })];
    world.searchAt = (o) => (o === "dev" ? "down" : { sessions: [], unreadable: 2, skipped: ["codex"] });
    const v = await opened();
    enter("无");
    await flush();
    const body = document.body.textContent ?? "";
    expect(body).toContain(copyText("history.search.noMatch", { q: "无" }));
    expect(body).toContain(copyText("history.search.machineDown", { machine: "dev" }));
    expect(body).toContain(copyText("history.search.skipped", { agent: "codex" }));
    expect(body).toContain(copyText("history.search.unreadable", { n: 2 }));
    invokeMock.mock.calls.length = 0;
    const tools = [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("history.filter.tools"))!;
    tools.click();
    await flush();
    expect(calls("history-search-merge").map((a) => (a.search as Record<string, unknown>).include_tools)).toContain(true);
    expect([...document.querySelectorAll<HTMLButtonElement>("button")].some((b) => b.textContent === copyText("history.filter.tools")), "已含工具输出还给那颗").toBe(false);
    v.close();
  });
});

describe("筛选浮层的其余几格 · 按项目收起与新建", () => {
  beforeEach(() => localStorage.clear());

  it("那台读不到 ⇒ 机器那一格标「离线」；勾回一台 ⇒ 带它的 fresh 再问；搜内容时改「谁说的」/ 含工具输出 ⇒ 当场重搜", async () => {
    world.local = [row({ sessionId: "a" })];
    world.fail.add("dev");
    const v = await opened();
    byText("button", copyText("history.page.filter"))!.click();
    await flush();
    const box = (label: string) => byText("label", label)!.querySelector("input")!;
    expect(byText("label", "dev")!.textContent).toContain(copyText("history.filter.offline"));
    box("dev").click();
    await flush();
    invokeMock.mock.calls.length = 0;
    box("dev").click();
    await flush();
    expect(calls("history-browse").map((a) => [a.remotes, a.fresh])).toEqual([
      [["dev"], []],
      [["dev"], ["dev"]],
    ]);
    invokeMock.mock.calls.length = 0;
    box(copyText("history.filter.scopeYou")).click();
    await flush();
    expect(calls("history-search-merge"), "没在搜内容就去搜了").toEqual([]);
    const input = document.querySelector<HTMLInputElement>(".history-search")!;
    input.value = "词";
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await flush();
    const fb = byText("button", copyText("history.page.filter"))!;
    if (fb.getAttribute("aria-expanded") !== "true") fb.click();
    await flush();
    invokeMock.mock.calls.length = 0;
    box(copyText("history.filter.scopeAgent")).click();
    await flush();
    expect(calls("history-search-merge").map((a) => (a.search as Record<string, unknown>).scope)).toContain("assistant");
    invokeMock.mock.calls.length = 0;
    box(copyText("history.filter.tools")).click();
    await flush();
    expect(
      calls("history-search-merge").map((a) => [a.machines, (a.search as Record<string, unknown>).include_tools]),
      "勾上工具 ⇒ 再问本机后端一次（两台都在里头）、带上工具",
    ).toEqual([[[LOCAL_ORIGIN, "dev"], true]]);
    expect(byText("button", copyText("history.page.filter"))!.textContent).toContain("2");
    v.close();
  });

  it("按项目：点开的组再点收起；组头的［新建］在那个目录起新会话", async () => {
    world.local = [row({ sessionId: "a" })];
    const groups = [{ key: "claude:/w/p", agent: "claude", projectName: "p", projectPath: "/w/p", projectDir: "-w-p", count: 1, hasLive: false, starred: false, lastActivity: NOW, order: 1, failed: null }];
    world.groups.local = groups;
    const v = await opened();
    byText('[role="tab"]', copyText("history.page.byProject"))!.click();
    await flush();
    const head = (): HTMLElement => document.querySelector<HTMLElement>('[role="treeitem"]')!;
    head().click();
    await flush();
    expect(titles()).toEqual(["标题 a"]);
    head().click();
    await flush();
    expect(titles()).toEqual([]);
    byText("button", copyText("history.list.collapseAll"))!.click();
    await flush();
    expect(titles()).toEqual([]);
    invokeMock.mock.calls.length = 0;
    const newBtn = [...head().querySelectorAll<HTMLButtonElement>("button")].find((b) => (b.getAttribute("aria-label") ?? b.textContent ?? "").length > 0 && b !== head())!;
    newBtn.click();
    await flush();
    expect(document.querySelector('[role="dialog"]'), "没开起新会话框").not.toBeNull();
    document.querySelector('[role="dialog"]')?.remove();
    byText('[role="tab"]', copyText("history.page.byTime"))!.click();
    v.close();
  });
});
