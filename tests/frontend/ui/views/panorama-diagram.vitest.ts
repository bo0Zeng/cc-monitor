// PN1b 选图（`设计/97 §7.3`）的**界面接线**：选择器 · 旋钮 · 下钻 · 选中跟着走 · 画不出 · 复制。
//
// 🔴 **被判的那块没被 mock**：本文件 mock 的只有 IPC 出口、全局弹窗、全局浮层栈、
//   `panorama/api` 里的命令封装（diagramKinds / diagram / status / node / symbolsInFile ——
//   后端边界），以及 `navigator.clipboard`（操作系统边界）。`PanoramaView`、`DiagramPane`、
//   三个渲染器、诚实信号表、`agent-clip` 全是真的 —— `P0` 当场自证。
// 夹具的图种 id 是假的（`k-…`）：本仓不写上游图种名，测试也不借真名。
import { describe, it, expect, vi, beforeEach } from "vitest";
import type { NodeView } from "../../../../src/frontend/ui/panorama/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../../../src/frontend/ui/keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../../../../src/frontend/ui/panorama/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../../../src/frontend/ui/panorama/api")>();
  return {
    ...actual,
    diagramKinds: vi.fn(),
    diagram: vi.fn(),
    status: vi.fn(),
    node: vi.fn(),
    symbolsInFile: vi.fn(),
  };
});

import * as api from "../../../../src/frontend/ui/panorama/api";
import * as render from "../../../../src/frontend/ui/panorama/diagram-render";
import { DiagramPane, FIT_MAX_SCALE } from "../../../../src/frontend/ui/panorama/diagram-view";
import { honestyLine } from "../../../../src/frontend/ui/panorama/diagram-honesty";
import { CLIP_HEAD } from "../../../../src/frontend/ui/panorama/agent-clip";
import { PanoramaView } from "../../../../src/frontend/ui/views/panorama";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import * as fx from "../panorama/diagram-fixtures";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
};
type Probe = {
  root: HTMLElement;
  sidebarEl: HTMLElement;
  openNodeDetail: (id: string) => Promise<void>;
  openFileDetail: (b: { file: string; subsystem: string; isEntry: boolean }) => void;
};
const probe = (v: PanoramaView): Probe => v as unknown as Probe;
const q = <T extends Element>(v: PanoramaView, sel: string): T => probe(v).root.querySelector(sel) as T;
const select = (v: PanoramaView): HTMLSelectElement => q(v, '[data-pano="diagram-kind"]');
const choose = async (v: PanoramaView, id: string): Promise<void> => {
  const s = select(v);
  s.value = id;
  s.dispatchEvent(new Event("change"));
  await flush();
};
const lastReq = (): unknown[] => vi.mocked(api.diagram).mock.calls.at(-1) as unknown[];
const nodeView = (id: string): NodeView =>
  ({ symbol: { ...fx.callCenter, id, lang: "Rust", end_line: 9 }, callers: [], callees: [], docs: [], annotations: [] }) as unknown as NodeView;

describe("PN1b 选图（界面）", () => {
  let v: PanoramaView;
  let written: string[];
  beforeEach(async () => {
    vi.clearAllMocks();
    document.body.replaceChildren();
    written = [];
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: vi.fn((t: string) => (written.push(t), Promise.resolve())) },
    });
    vi.mocked(api.diagramKinds).mockResolvedValue(fx.kinds);
    // symbols=0 ⇒ 加载停在「建立索引」门口，不画气泡（与本文件无关的那半不启动）
    vi.mocked(api.status).mockResolvedValue({ symbols: 0, stale: false, indexedAt: 1_790_000_000 });
    vi.mocked(api.node).mockImplementation(async (_r, id) => nodeView(id));
    vi.mocked(api.symbolsInFile).mockResolvedValue([]);
    v = new PanoramaView(() => ({ cwd: "/repo", origin: LOCAL_ORIGIN }));
    await v.open();
    await flush();
  });

  it("P0 自证：选图那块、渲染器、诚实信号都是真的；被 mock 的只有后端封装", () => {
    const proto = DiagramPane.prototype as unknown as Record<string, unknown>;
    for (const m of ["setKind", "redraw", "paint", "onNode", "renderOptions"]) {
      expect(vi.isMockFunction(proto[m]), m).toBe(false);
    }
    expect(vi.isMockFunction(render.renderClusters)).toBe(false);
    expect(vi.isMockFunction(render.rendererFor)).toBe(false);
    expect(vi.isMockFunction(api.diagram)).toBe(true);
  });

  it("P1 选项从注册表现读（CP2）：值 == [气泡, …注册表 id]；要符号的图没选符号时灰掉并说「先点一个符号」", async () => {
    const opts = [...select(v).options];
    expect(opts.map((o) => o.value)).toEqual(["", "k-clusters", "k-calls", "k-types", "k-new"]);
    expect(opts.filter((o) => o.disabled).map((o) => o.textContent)).toEqual(["乙图 · 先点一个符号"]);
    await probe(v).openNodeDetail("src/a/x.rs#f");
    await flush();
    expect([...select(v).options].filter((o) => o.disabled)).toEqual([]);
  });

  it("P2 团/模块图：请求只带这张图声明的旋钮；画出节点；诚实信号那一行 == 上游 honesty 的人话", async () => {
    vi.mocked(api.diagram).mockResolvedValue(fx.view(fx.clustersDiagram));
    await choose(v, "k-clusters");
    expect(lastReq()).toEqual([{ origin: LOCAL_ORIGIN, path: "/repo" }, "k-clusters", { max_nodes: 12, certain_only: true, exclude_tests: true }]);
    expect([...probe(v).root.querySelectorAll(".panorama-diagram-canvas [data-node]")].length).toBe(3);
    expect(q<HTMLElement>(v, '[data-pano="diagram-honesty"]').textContent).toBe(honestyLine(fx.honestyFull));
    // 图例常驻（CP3）
    expect([...probe(v).root.querySelectorAll(".panorama-diagram-legend [data-conf]")].map((e) => e.getAttribute("data-conf"))).toEqual(
      ["exact", "dispatch", "guess", "mixed"],
    );
  });

  it("P3 旋钮：关掉「只看确定的」⇒ 下一次请求 certain_only=false；这张图不认的旋钮不显示", async () => {
    vi.mocked(api.diagram).mockResolvedValue(fx.view(fx.clustersDiagram));
    await choose(v, "k-clusters");
    const box = q<HTMLInputElement>(v, '[data-pano="knob-certain_only"]');
    box.checked = false;
    box.dispatchEvent(new Event("change"));
    await flush();
    expect(lastReq()).toEqual([{ origin: LOCAL_ORIGIN, path: "/repo" }, "k-clusters", { max_nodes: 12, certain_only: false, exclude_tests: true }]);
    vi.mocked(api.diagram).mockResolvedValue(fx.view(fx.typeDiagram));
    await choose(v, "k-types");
    expect(lastReq()).toEqual([{ origin: LOCAL_ORIGIN, path: "/repo" }, "k-types", { max_nodes: 12 }]);
    const shown = (k: string): boolean => (q<HTMLElement>(v, `[data-pano="knob-${k}"]`).parentElement as HTMLElement).style.display !== "none";
    expect(["certain_only", "exclude_tests", "max_nodes"].map(shown)).toEqual([false, false, true]);
  });

  it("P4 调用子图：以选中的符号为中心；点一个节点 ⇒ 以它为中心重画", async () => {
    await probe(v).openNodeDetail("src/a/x.rs#f");
    await flush();
    vi.mocked(api.diagram).mockResolvedValue(fx.view(fx.callDiagram));
    await choose(v, "k-calls");
    expect(lastReq()).toEqual([{ origin: LOCAL_ORIGIN, path: "/repo" }, "k-calls", { symbol: "src/a/x.rs#f", certain_only: true }]);
    (q<SVGGElement>(v, '[data-node="src/b/z.rs#g"]')).dispatchEvent(new MouseEvent("click"));
    await flush();
    expect(lastReq()).toEqual([{ origin: LOCAL_ORIGIN, path: "/repo" }, "k-calls", { symbol: "src/b/z.rs#g", certain_only: true }]);
  });

  it("P5 新形状：如实说「这一版还画不出」，诚实信号照常，复制 Mermaid 拿到上游原文", async () => {
    vi.mocked(api.diagram).mockResolvedValue(fx.view(fx.unknownShapeDiagram));
    await choose(v, "k-new");
    expect(q<HTMLElement>(v, ".panorama-diagram-note").textContent).toBe(copyText("diagramView.paint.unsupported", { shape: "hexagon" }));
    expect(probe(v).root.querySelectorAll(".panorama-diagram-canvas svg").length).toBe(0);
    expect(q<HTMLElement>(v, '[data-pano="diagram-honesty"]').textContent).toBe(honestyLine(fx.honestyFull));
    q<HTMLButtonElement>(v, '[data-pano="diagram-copy-mermaid"]').click();
    await flush();
    expect(written).toEqual([fx.view(fx.unknownShapeDiagram).mermaid]);
  });

  it("P6 下钻团/模块节点 ⇒ 侧栏列成员文件；点文件进文件详情；选中的文件回到图上描环", async () => {
    vi.mocked(api.diagram).mockResolvedValue(fx.view(fx.clustersDiagram));
    await choose(v, "k-clusters");
    q<SVGGElement>(v, '[data-node="src_a"]').dispatchEvent(new MouseEvent("click"));
    const rows = [...probe(v).sidebarEl.querySelectorAll('[data-pano="side-list"] button')];
    expect(rows.map((r) => r.textContent)).toEqual(["src/a/x.rs", "src/a/y.rs"]);
    (rows[1] as HTMLButtonElement).click();
    await flush();
    expect(api.symbolsInFile).toHaveBeenLastCalledWith({ origin: LOCAL_ORIGIN, path: "/repo" }, "src/a/y.rs");
    expect([...probe(v).root.querySelectorAll(".panorama-diagram-canvas [data-focus]")].map((n) => n.getAttribute("data-node"))).toEqual(
      ["src_a"],
    );
  });

  it("P7 下钻类型节点 ⇒ 侧栏列类型本身与方法；点方法进符号详情", async () => {
    vi.mocked(api.diagram).mockResolvedValue(fx.view(fx.typeDiagram));
    await choose(v, "k-types");
    q<SVGGElement>(v, '[data-node="A"]').dispatchEvent(new MouseEvent("click"));
    const rows = [...probe(v).sidebarEl.querySelectorAll('[data-pano="side-list"] button')];
    expect(rows.map((r) => r.textContent)).toEqual(["A · 类型本身", "go()"]);
    (rows[1] as HTMLButtonElement).click();
    await flush();
    expect(api.node).toHaveBeenLastCalledWith({ origin: LOCAL_ORIGIN, path: "/repo" }, "src/a/x.rs#A::go");
  });

  it("P8 复制给 agent（图）：住址 · 索引读数 · 诚实信号 · Mermaid 原文，逐行相等", async () => {
    vi.mocked(api.status).mockResolvedValue({ symbols: 0, stale: true, indexedAt: 1_790_000_000 });
    await probe(v).openNodeDetail("src/a/x.rs#f");
    await flush();
    vi.mocked(api.diagram).mockResolvedValue(fx.view(fx.callDiagram));
    await choose(v, "k-calls");
    q<HTMLButtonElement>(v, '[data-pano="diagram-copy-agent"]').click();
    await flush();
    expect(written.length).toBe(1);
    expect(written[0].split("\n")).toEqual([
      CLIP_HEAD,
      "仓：/repo",
      "对象：图「乙图」（类型 k-calls） · 中心符号 src/a/x.rs#f",
      copyText("agentClip.stamp.line", { iso: "2026-09-21T14:13:20.000Z", indexedAt: 1_790_000_000, stale: copyText("agentClip.stamp.stale") }),
      copyText("agentClip.clip.honesty", { honesty: honestyLine(fx.callDiagram.honesty) }),
      "Mermaid：",
      "flowchart LR",
      "  %% k-calls",
    ]);
  });

  it("P9 后端报错 ⇒ 说「画不出这张图」＋ 原话，不留上一张图冒充", async () => {
    vi.mocked(api.diagram).mockResolvedValueOnce(fx.view(fx.clustersDiagram));
    await choose(v, "k-clusters");
    vi.mocked(api.diagram).mockRejectedValueOnce("找不到符号 x");
    const box = q<HTMLInputElement>(v, '[data-pano="knob-exclude_tests"]');
    box.checked = false;
    box.dispatchEvent(new Event("change"));
    await flush();
    expect(q<HTMLElement>(v, ".panorama-diagram-note").textContent).toBe("画不出这张图：找不到符号 x");
    expect(probe(v).root.querySelectorAll(".panorama-diagram-canvas svg").length).toBe(0);
    expect(q<HTMLElement>(v, '[data-pano="diagram-honesty"]').textContent).toBe("");
  });
});

// 〔P3〕`设计/97 §8`「图的缩放 / 拖拽」· `99 §4.4` 同一行 · SHOTS 报备「模块图 SVG 按自然尺寸画、只占左上角」。
// 判的是**几何性质**（适配后居中且顶满一边或到封顶倍数 · 锚点下的世界点不动 · 平移量 == 拖动量），
// 不拿被测那一份 `fitViewport` 去算期望。
describe("〔P3〕图的缩放 / 拖拽：图铺满画布、按视口适配", () => {
  const W = 900;
  const H = 600;
  let v: PanoramaView;
  beforeEach(async () => {
    vi.clearAllMocks();
    document.body.replaceChildren();
    vi.mocked(api.diagramKinds).mockResolvedValue(fx.kinds);
    vi.mocked(api.status).mockResolvedValue({ symbols: 0, stale: false, indexedAt: 1_790_000_000 });
    vi.mocked(api.node).mockImplementation(async (_r, id) => nodeView(id));
    vi.mocked(api.symbolsInFile).mockResolvedValue([]);
    vi.mocked(api.diagram).mockResolvedValue(fx.view(fx.clustersDiagram));
    v = new PanoramaView(() => ({ cwd: "/repo", origin: LOCAL_ORIGIN }));
    await v.open();
    await flush();
    const canvas = q<HTMLElement>(v, ".panorama-diagram-canvas");
    Object.defineProperty(canvas, "clientWidth", { configurable: true, value: W });
    Object.defineProperty(canvas, "clientHeight", { configurable: true, value: H });
  });
  const world = (): SVGGElement => q<SVGGElement>(v, ".panorama-diagram-canvas g[data-pano-world]");
  const vpOf = (): { x: number; y: number; s: number } => {
    const m = /^translate\(([-\d.e]+) ([-\d.e]+)\) scale\(([-\d.e]+)\)$/.exec(world().getAttribute("transform") ?? "");
    expect(m, `世界那一层没有平移 / 缩放：${world().getAttribute("transform")}`).not.toBeNull();
    return { x: Number(m![1]), y: Number(m![2]), s: Number(m![3]) };
  };
  const size = (): { w: number; h: number } => ({
    w: Number(world().getAttribute("data-world-w")),
    h: Number(world().getAttribute("data-world-h")),
  });
  const fitted = (): void => {
    const { x, y, s } = vpOf();
    const { w, h } = size();
    expect(x + (w * s) / 2, "适配后没有水平居中").toBeCloseTo(W / 2, 6);
    expect(y + (h * s) / 2, "适配后没有垂直居中").toBeCloseTo(H / 2, 6);
    // 顶满一边（留 24 像素边），或到了封顶倍数 —— 不许停在自然尺寸缩在左上角
    const fillsW = Math.abs(w * s - (W - 48)) < 1e-6;
    const fillsH = Math.abs(h * s - (H - 48)) < 1e-6;
    expect(fillsW || fillsH || s === FIT_MAX_SCALE, `没铺满也没到封顶：scale=${s} world=${w}×${h}`).toBe(true);
  };

  it("Z1 画出来那一刻：SVG 铺满画布，世界那一层适配进视口（居中 · 顶满一边或到封顶）", async () => {
    await choose(v, "k-clusters");
    const svg = q<SVGSVGElement>(v, ".panorama-diagram-canvas svg");
    expect([svg.getAttribute("width"), svg.getAttribute("height")]).toEqual(["100%", "100%"]);
    fitted();
  });

  it("Z2 滚轮以指针为锚缩放：指针下的世界点缩放前后不动；「适配」回到适配", async () => {
    await choose(v, "k-clusters");
    const before = vpOf();
    const [px, py] = [123, 77];
    const wx = (px - before.x) / before.s;
    const wy = (py - before.y) / before.s;
    q<HTMLElement>(v, ".panorama-diagram-canvas").dispatchEvent(
      new WheelEvent("wheel", { deltaY: -1, clientX: px, clientY: py, cancelable: true }),
    );
    const after = vpOf();
    expect(after.s).toBeGreaterThan(before.s);
    expect(wx * after.s + after.x).toBeCloseTo(px, 6);
    expect(wy * after.s + after.y).toBeCloseTo(py, 6);
    const fit = [...probe(v).root.querySelectorAll("button")].find((b) => b.textContent === copyText("panorama.build.fit"))!;
    fit.click();
    fitted();
  });

  it("Z3 按住拖动：平移量 == 拖动量；拖完松手那一下不当点击（不下钻）；不拖的点击照常下钻", async () => {
    await choose(v, "k-clusters");
    const canvas = q<HTMLElement>(v, ".panorama-diagram-canvas");
    const node = q<SVGGElement>(v, '[data-node="src_a"]');
    const before = vpOf();
    node.dispatchEvent(new MouseEvent("mousedown", { button: 0, clientX: 10, clientY: 10, bubbles: true }));
    window.dispatchEvent(new MouseEvent("mousemove", { clientX: 60, clientY: 40 }));
    window.dispatchEvent(new MouseEvent("mouseup", { button: 0, clientX: 60, clientY: 40 }));
    node.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    const after = vpOf();
    expect([after.x - before.x, after.y - before.y, after.s]).toEqual([50, 30, before.s]);
    expect(canvas.classList.contains("is-panning")).toBe(false);
    expect(probe(v).sidebarEl.querySelectorAll('[data-pano="side-list"]').length, "拖图时顺手下钻了一个节点").toBe(0);
    node.dispatchEvent(new MouseEvent("mousedown", { button: 0, clientX: 5, clientY: 5, bubbles: true }));
    window.dispatchEvent(new MouseEvent("mouseup", { button: 0, clientX: 5, clientY: 5 }));
    node.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(probe(v).sidebarEl.querySelectorAll('[data-pano="side-list"]').length).toBe(1);
  });

  it("Z4 同一张图只换描环（选中的文件变了）⇒ 视口不动；换一张图 ⇒ 重新适配", async () => {
    await choose(v, "k-clusters");
    q<HTMLElement>(v, ".panorama-diagram-canvas").dispatchEvent(
      new WheelEvent("wheel", { deltaY: -1, clientX: 5, clientY: 5, cancelable: true }),
    );
    const zoomed = vpOf();
    probe(v).openFileDetail({ file: "src/b/z.rs", subsystem: "src/b", isEntry: false });
    await flush();
    expect(world().querySelector('[data-focus]')?.getAttribute("data-node")).toBe("src_b");
    expect(vpOf()).toEqual(zoomed);
    vi.mocked(api.diagram).mockResolvedValue(fx.view(fx.typeDiagram));
    await choose(v, "k-types");
    fitted();
  });
});
