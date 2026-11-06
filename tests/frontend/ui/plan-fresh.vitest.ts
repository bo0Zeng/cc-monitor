/**
 * 新登记的仓库还没有图（pb repo 建了 project，等某个会话认领它画图）：那一屏怎么说、按钮去哪、交接说明怎么给。
 * 另有开页那一下：问 `plan-list` 不带 fresh · 记住的那一片当场就读 · 读不成又没有上一份 ⇒ 正文写后端那一句 ＋［重试］。
 * 期望手写，夹具是合成的中性例子。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../src/frontend/ui/copy-table", () => ({
  copyText: (k: string, a?: Record<string, unknown>) => (a ? `${k}(${Object.entries(a).map(([x, y]) => `${x}=${String(y)}`).join(",")})` : k),
}));
// 这台没有别的机器（开页先问的那一下）。
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(() => Promise.resolve([])) }));
vi.mock("../../../src/frontend/ui/plan-reads", async (orig) => ({ ...(await orig<object>()), fetchPlanList: vi.fn(), fetchPlanRead: vi.fn() }));
vi.mock("../../../src/frontend/ui/kit/skeleton", () => ({
  skeletonRows: () => {
    const el = document.createElement("div");
    el.dataset.skeleton = "1";
    return el;
  },
}));

import { isFresh } from "../../../src/frontend/ui/views/plan-model.ts";
import { freshPane } from "../../../src/frontend/ui/views/plan-bits.ts";
import { handoffBox } from "../../../src/frontend/ui/views/plan-cell.ts";
import { PlanView } from "../../../src/frontend/ui/views/plan.ts";
import { fetchPlanList, fetchPlanRead, type PlanList, type PlanSlice } from "../../../src/frontend/ui/plan-reads.ts";
import { LS_KEYS } from "../../../src/frontend/ui/local-storage.ts";
import { LOCAL } from "../../../src/frontend/ui/ipc/origin.ts";

const slice = (extra: Partial<PlanSlice> = {}): PlanSlice => ({ name: "invoice", domain: "软件", bare: false, error: null, stale: null, cells: [], top: [], blocks: [], ...extra }) as unknown as PlanSlice;

const HANDOFF = "你负责 invoice 的 project：先 pb claim project <认领码>，再 pb where。认领码只在人登记这个仓库与重发时印出来；丢了请人敲 `pb reissue project`";

describe("无图：哪一个仓库算", () => {
  it("读好了、一个节点都没有 ⇒ 算", () => {
    expect(isFresh(slice())).toBe(true);
  });
  it("有节点 · 读不成又没读好过 · 这一刻读不成 ⇒ 都不算（各有各的那一屏）", () => {
    expect(isFresh(slice({ cells: [{ id: "A1" }] as unknown as PlanSlice["cells"] }))).toBe(false);
    expect(isFresh(slice({ bare: true }))).toBe(false);
    expect(isFresh(slice({ error: "图.md 读不成" }))).toBe(false);
  });
});

describe("无图那一屏", () => {
  it("一句「名字 · 领域 · 无图」＋ 一行怎么让它有 ＋［起新会话…］，点了按工作区根开框", () => {
    const opened: string[] = [];
    const el = freshPane(slice(), "/work/ledger", (cwd) => opened.push(cwd));
    expect(el.textContent).toContain("plan.fresh.title(name=invoice,domain=软件)");
    expect(el.textContent).toContain("plan.fresh.hint");
    const btn = [...el.querySelectorAll("button")].find((b) => b.textContent?.includes("plan.fresh.newSession"));
    expect(btn, "没有［起新会话…］").toBeTruthy();
    btn!.click();
    expect(opened).toEqual(["/work/ledger"]);
    expect(el.textContent).not.toContain("/work/ledger");
  });
  it("pb 没给领域 ⇒ 标题不带那一段", () => {
    const el = freshPane(slice({ domain: null }), "/w", () => {});
    expect(el.textContent).toContain("plan.fresh.titleBare(name=invoice)");
  });
  it("带上 project 的交接说明 ⇒ 那一句原样（不带认领码，句末 pb 写了码找谁要）＋［复制］，界面不另拼一句", () => {
    const el = freshPane(slice(), "/w", () => {}, handoffBox({ handoff: HANDOFF, handoffError: null }));
    expect(el.querySelector(".plan-handoff")?.textContent).toContain(HANDOFF);
    expect([...el.querySelectorAll("button")].some((b) => b.getAttribute("aria-label") === "plan.handoff.copy" || b.textContent?.includes("plan.handoff.copy"))).toBe(true);
    expect(el.textContent).not.toContain("reissue project <");
  });
});

describe("交接说明给不了", () => {
  it("handoff 为 null ⇒ 照出 pb 说的为什么，没有［复制］", () => {
    const el = handoffBox({ handoff: null, handoffError: "project 不在当前仓库：先请人敲 `pb repo invoice`" });
    expect(el.textContent).toContain("plan.handoff.none(reason=project 不在当前仓库：先请人敲 `pb repo invoice`)");
    expect(el.querySelector("button")).toBeNull();
  });
});

describe("开页那一下", () => {
  const listMock = fetchPlanList as unknown as ReturnType<typeof vi.fn>;
  const readMock = fetchPlanRead as unknown as ReturnType<typeof vi.fn>;
  const LIST: PlanList = {
    pb: { state: "ok", said: null },
    workspaces: [
      {
        workspace: "/work/ledger",
        repo: null,
        auto: false,
        rev: "r1",
        stale: null,
        needCount: 0,
        bySession: {},
        slices: [{ name: "invoice", domain: null, current: true, progress: null, needCount: 0, error: null, stale: null, said: {} as PlanList["workspaces"][number]["slices"][number]["said"] }],
      },
    ],
  };
  let view: PlanView | null = null;
  const settle = async (): Promise<void> => {
    for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
  };
  beforeEach(() => {
    listMock.mockReset();
    readMock.mockReset();
    localStorage.clear();
  });
  afterEach(() => {
    view?.close();
    view = null;
  });

  it("问 plan-list 不带 fresh（认过的目录不重起 pb；fresh 只留给添加工作区）", async () => {
    listMock.mockResolvedValue(LIST);
    readMock.mockReturnValue(new Promise(() => {}));
    view = new PlanView();
    await view.open();
    expect(listMock).toHaveBeenCalled();
    for (const call of listMock.mock.calls) expect(call[1], "开页发的 plan-list 带了 fresh").toBe(false);
  });

  it("记住的那一片当场就读，不等 plan-list 回来", async () => {
    localStorage.setItem(LS_KEYS.planPrefs, JSON.stringify({ last: `${LOCAL}\u0000/work/ledger\u0000invoice`, dirs: {} }));
    listMock.mockReturnValue(new Promise(() => {}));
    readMock.mockReturnValue(new Promise(() => {}));
    view = new PlanView();
    await view.open();
    expect(readMock).toHaveBeenCalledWith(LOCAL, "/work/ledger");
  });

  it("plan-read 读不成、又没有上一份 ⇒ 正文写后端那一句 ＋［重试］，不是一直转的骨架；点了重读", async () => {
    listMock.mockResolvedValue(LIST);
    readMock.mockRejectedValueOnce(new Error("后端·读计划超时"));
    view = new PlanView();
    await view.open();
    await settle();
    const root = document.body;
    expect(root.textContent).toContain("后端·读计划超时");
    expect(root.querySelector("[data-skeleton]"), "还在画骨架").toBeNull();
    const retry = [...root.querySelectorAll("button")].find((b) => b.textContent?.includes("front.act.retry"));
    expect(retry, "没有［重试］").toBeTruthy();
    readMock.mockReturnValue(new Promise(() => {}));
    const before = readMock.mock.calls.length;
    retry!.click();
    expect(readMock.mock.calls.length).toBe(before + 1);
  });
});
