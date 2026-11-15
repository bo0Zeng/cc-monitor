/**
 * 计划页照后端成品摆、不自己判：需手动那条的负责人只照 `need.who` 挂 · 格属于哪一块只照 `rootOf` 找 ·
 * 手加的目录写进那台偏好（`prefs-set {planDirs}`），`plan-list` 不再带目录 · 哪台说 pb 不在就照哪台那一句、设置开那一台。
 * 期望手写，夹具是合成的中性例子。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../../src/frontend/ui/copy-table", () => ({
  copyText: (k: string, a?: Record<string, unknown>) => (a ? `${k}(${Object.entries(a).map(([x, y]) => `${x}=${String(y)}`).join(",")})` : k),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(() => Promise.resolve([])) }));
vi.mock("../../../src/frontend/ui/machine-feed", async (orig) => ({ ...(await orig<object>()), readMachineNames: vi.fn(() => Promise.resolve(["<local>"])) }));
vi.mock("../../../src/frontend/ui/plan-reads", async (orig) => ({ ...(await orig<object>()), fetchPlanList: vi.fn(), fetchPlanRead: vi.fn() }));
vi.mock("../../../src/frontend/ui/machine-prefs", () => ({ readPrefs: vi.fn(), setPrefs: vi.fn() }));
vi.mock("../../../src/frontend/ui/kit/dialog", async (orig) => ({ ...(await orig<object>()), askText: vi.fn() }));
vi.mock("../../../src/frontend/ui/open-settings", () => ({ openSettingsWindow: vi.fn(() => Promise.resolve()) }));
vi.mock("../../../src/frontend/ui/ipc/chan-caller", async (orig) => ({ ...(await orig<object>()), callTyped: vi.fn(() => Promise.resolve({ pb: { state: "ok", said: null }, workspaces: [] })) }));

import { needBar, type ReviewHost } from "../../../src/frontend/ui/views/plan-review.ts";
import { outlineRows } from "../../../src/frontend/ui/views/plan-model.ts";
import { PlanView } from "../../../src/frontend/ui/views/plan.ts";
import { fetchPlanList, type PlanList, type PlanNeed, type PlanSlice, type PlanWho } from "../../../src/frontend/ui/plan-reads.ts";
import { readMachineNames } from "../../../src/frontend/ui/machine-feed.ts";
import { readPrefs, setPrefs } from "../../../src/frontend/ui/machine-prefs.ts";
import { askText } from "../../../src/frontend/ui/kit/dialog.ts";
import { openSettingsWindow } from "../../../src/frontend/ui/open-settings.ts";
import { callTyped } from "../../../src/frontend/ui/ipc/chan-caller.ts";

const OWNER = { id: "s1", kind: "session", sid: "s1", needs: null, face: null, label: null } as unknown as PlanWho;

const slice = (extra: Partial<PlanSlice> = {}): PlanSlice =>
  ({
    name: "invoice",
    domain: null,
    bare: false,
    error: null,
    stale: null,
    top: ["A1"],
    cells: [{ id: "A1", title: "导出", parent: null, children: [], look: "open", rootOf: null, said: { trail: null, blockRoot: null } }],
    blocks: [{ id: "B1", cells: ["A1"], owner: OWNER, phase: "开发", at: null, said: { name: "导出", inside: null } }],
    needs: [],
    ...extra,
  }) as unknown as PlanSlice;

const host = (): ReviewHost =>
  ({
    origin: () => "<local>",
    workspace: () => "/w",
    who: () => {
      const el = document.createElement("span");
      el.dataset.who = "1";
      return el;
    },
    sessionName: () => "",
    switchTo: () => {},
    resume: () => {},
    ack: () => {},
    next: null,
    reread: () => {},
  }) as unknown as ReviewHost;

const need = (who: string | null): PlanNeed =>
  ({ key: "ask:B1@s1", kind: "ask", block: "B1", cell: "A1", sid: "s1", acked: false, title: "后端·问人", lines: [], pos: null, who }) as unknown as PlanNeed;

describe("需手动那条的负责人", () => {
  it("只照后端给的 who：有才挂，问人那一种没有 who 也不挂", () => {
    const sl = slice();
    expect(needBar(host(), sl, need("后端·负责人"), { i: 1, n: 1 }).querySelector("[data-who]")).not.toBeNull();
    expect(needBar(host(), sl, need(null), { i: 1, n: 1 }).querySelector("[data-who]")).toBeNull();
  });
});

describe("格属于哪一块", () => {
  it("只照 rootOf 找：rootOf 为 null ⇒ 不挂块，即使 blocks[].cells 列着它", () => {
    const rows = outlineRows(slice(), { filter: "all", folded: new Set(), query: "", needsCells: new Set() });
    expect(rows[0]?.block).toBeNull();
    const rooted = slice({ cells: [{ id: "A1", title: "导出", parent: null, children: [], look: "open", rootOf: "B1", said: { trail: null, blockRoot: "后端·整句" } }] as unknown as PlanSlice["cells"] });
    expect(outlineRows(rooted, { filter: "all", folded: new Set(), query: "", needsCells: new Set() })[0]?.block?.id).toBe("B1");
  });
});

describe("plan-list 的入参", () => {
  it("只带 fresh，不带目录", async () => {
    const real = await vi.importActual<typeof import("../../../src/frontend/ui/plan-reads.ts")>("../../../src/frontend/ui/plan-reads.ts");
    await real.fetchPlanList("<local>", true);
    const call = (callTyped as unknown as ReturnType<typeof vi.fn>).mock.calls.at(-1);
    expect(call?.[1]).toBe("plan-list");
    expect(call?.[2]).toEqual({ fresh: true });
  });
});

describe("计划页整页那一句", () => {
  const listMock = fetchPlanList as unknown as ReturnType<typeof vi.fn>;
  const empty = (said: string | null, state: "ok" | "missing" = "ok"): PlanList => ({ pb: { state, said }, workspaces: [] });
  let view: PlanView | null = null;
  const settle = async (): Promise<void> => {
    for (let i = 0; i < 8; i++) await new Promise((r) => setTimeout(r, 0));
  };
  beforeEach(() => {
    listMock.mockReset();
    localStorage.clear();
    document.body.innerHTML = "";
  });
  afterEach(() => {
    view?.close();
    view = null;
  });

  it("加目录 ⇒ 写进本机偏好（prefs-set {planDirs}，接在已有的后面），plan-list 不带目录", async () => {
    listMock.mockResolvedValue(empty(null));
    (readPrefs as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({ state: "ok", prefs: { planDirs: ["/old"] } });
    (setPrefs as unknown as ReturnType<typeof vi.fn>).mockResolvedValue({ state: "ok", prefs: { planDirs: ["/old", "/new"] } });
    (askText as unknown as ReturnType<typeof vi.fn>).mockResolvedValue("/new");
    view = new PlanView();
    await view.open();
    await settle();
    const add = [...document.body.querySelectorAll("button")].find((b) => b.textContent?.includes("plan.empty.add"));
    expect(add, "没有［添加工作区］").toBeTruthy();
    add!.click();
    await settle();
    expect(setPrefs).toHaveBeenCalledWith("<local>", { planDirs: ["/old", "/new"] });
    for (const c of listMock.mock.calls) expect(c.length, "plan-list 还带着目录").toBeLessThanOrEqual(2);
  });

  it("非本机那台回 pb 不在 ⇒ 摆那台写的那一句，设置开的是那一台", async () => {
    (readMachineNames as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(["<local>", "box"]);
    listMock.mockImplementation((m: string) => Promise.resolve(m === "box" ? empty("box·未装 planned-build", "missing") : empty(null)));
    view = new PlanView();
    await view.open();
    await settle();
    expect(document.body.textContent).toContain("box·未装 planned-build");
    const set = [...document.body.querySelectorAll("button")].find((b) => b.textContent?.includes("plan.empty.setPb"));
    expect(set, "没有［填 pb 路径］").toBeTruthy();
    set!.click();
    expect((openSettingsWindow as unknown as ReturnType<typeof vi.fn>).mock.calls.at(-1)?.[1]).toMatchObject({ machine: "box" });
  });
});

describe("负责人那一枚的名字", () => {
  it("只照后端写好的 label：标签页叫别的、对不上标签页都不改，也不截 id", () => {
    const v = new PlanView();
    const who = { ...OWNER, sid: "0123456789abcdef", label: "后端·会话标题" } as unknown as PlanWho;
    v.tabOf = () => ({ title: "标签页·别的名", face: null, origin: "<local>" }) as unknown as ReturnType<PlanView["tabOf"]>;
    expect(v.whoChip(who).textContent).toContain("后端·会话标题");
    expect(v.whoChip(who).textContent).not.toContain("标签页·别的名");
    v.tabOf = () => null;
    expect(v.whoChip(who).textContent).toBe("后端·会话标题");
    const review = (v as unknown as { review: ReviewHost }).review;
    expect(review.sessionName(who)).toBe("后端·会话标题");
    v.close();
  });
});
