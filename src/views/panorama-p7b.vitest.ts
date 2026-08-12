// P7b（全景 P4，#79）：多跳子图 / 影响面的**界面**那半。
// 纯分层逻辑住 `panorama/subgraph-layers.vitest.ts`；这里只钉界面接线与竞态。
import { describe, it, expect, vi, beforeEach } from "vitest";
import type { NodeView, Symbol as PanoSymbol, SubGraph, ImpactSet } from "../panorama/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../keybindings/registry", () => ({
  registerKeybindings: vi.fn(),
  unregisterKeybindings: vi.fn(),
}));
vi.mock("../panorama/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../panorama/api")>();
  return { ...actual, node: vi.fn(), subgraph: vi.fn(), impact: vi.fn() };
});

import * as api from "../panorama/api";
import { PanoramaView } from "./panorama";

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
const sym = (id: string): PanoSymbol =>
  ({ id, name: id, kind: "function", file: "lib.rs", start_line: 1 }) as unknown as PanoSymbol;
const nodeView = (id: string): NodeView =>
  ({ symbol: sym(id), callers: [], callees: [], docs: [], annotations: [] }) as NodeView;
const edge = (from: string, to: string): SubGraph["edges"][number] =>
  ({ from, to, kind: "Calls", call_site_line: null, confidence: "Exact" }) as SubGraph["edges"][number];
type Probe = { repo: string | null; sidebarEl: HTMLElement; openNodeDetail: (id: string) => Promise<void> };
const probe = (v: PanoramaView): Probe => v as unknown as Probe;

describe("P7b 多跳子图 / 影响面", () => {
  let v: PanoramaView;
  beforeEach(async () => {
    vi.clearAllMocks();
    document.body.replaceChildren();
    v = new PanoramaView(() => ({ cwd: "/repo", origin: null }));
    probe(v).repo = "/repo";
    vi.mocked(api.node).mockResolvedValue(nodeView("r"));
    await probe(v).openNodeDetail("r");
    await flush();
  });
  const el = (sel: string): HTMLElement => probe(v).sidebarEl.querySelector(sel) as HTMLElement;
  const layerHeads = (): string[] =>
    [...probe(v).sidebarEl.querySelectorAll(".panorama-layer-head")].map(
      (e) => e.textContent ?? "",
    );

  it("★ P7b-Y1：点「展开子图」才发请求，结果按跳数分层", async () => {
    // 打开详情时**不许**白拉一次（子图是双向邻域，扇出爆炸）。
    expect(api.subgraph).not.toHaveBeenCalled();
    vi.mocked(api.subgraph).mockResolvedValue({
      symbols: [],
      edges: [edge("r", "a"), edge("a", "b")],
    } as SubGraph);
    el(".panorama-subgraph-go").click();
    await flush();
    expect(api.subgraph).toHaveBeenCalledWith("/repo", "r", 1);
    expect(layerHeads()).toEqual(["第 1 跳（1 条）", "第 2 跳（1 条）"]);
  });

  it("★ P7b-Y3：「影响面」走的是 impact，不是 callers", async () => {
    vi.mocked(api.impact).mockResolvedValue({
      root: "r",
      affected: [
        { id: "a", depth: 1 },
        { id: "b", depth: 2 },
      ],
    } as ImpactSet);
    el(".panorama-impact-go").click();
    await flush();
    expect(api.impact).toHaveBeenCalledWith("/repo", "r");
    // 深度 > 1 的层要真的分出来 —— 只有一跳的话它就退化成 callers 了。
    expect(layerHeads()).toEqual(["第 1 跳（1 条）", "第 2 跳（1 条）"]);
  });

  it("★ P7b-D：慢的那次回来**不许盖掉**后点的那次（代次守卫）", async () => {
    // 用户点了「展开子图」，等不及又点「影响面」——子图那次慢，回来时不该覆盖影响面的结果。
    let releaseSub: (v: SubGraph) => void = () => {};
    vi.mocked(api.subgraph).mockReturnValue(
      new Promise<SubGraph>((r) => {
        releaseSub = r;
      }),
    );
    vi.mocked(api.impact).mockResolvedValue({
      root: "r",
      affected: [{ id: "imp", depth: 1 }],
    } as ImpactSet);

    el(".panorama-subgraph-go").click();
    await flush();
    el(".panorama-impact-go").click();
    await flush();
    expect(layerHeads()).toEqual(["第 1 跳（1 条）"]);
    const before = probe(v).sidebarEl.querySelector(".panorama-subgraph-out")!.textContent;

    releaseSub({ symbols: [], edges: [edge("r", "x"), edge("r", "y")] } as SubGraph);
    await flush();
    expect(
      probe(v).sidebarEl.querySelector(".panorama-subgraph-out")!.textContent,
      "作废的那次不许改动界面",
    ).toBe(before);
  });

  it("读不到 ⇒ 说「读不到」，不说成「邻域为空」", async () => {
    vi.mocked(api.subgraph).mockRejectedValue(new Error("no index"));
    el(".panorama-subgraph-go").click();
    await flush();
    const txt = probe(v).sidebarEl.querySelector(".panorama-subgraph-out")!.textContent ?? "";
    expect(txt).toContain("读不到");
    expect(txt).not.toContain("邻域为空");
  });
});
