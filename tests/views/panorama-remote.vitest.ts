// 〔RM1c · 第四波〕远端会话的全景（jsdom）：看那台机器上的仓。
// 〔RM1d · V110〕远端也能写批注 / 文档关联（那台算、那台后端的文件管理写）⇒ RM1c 那一拍的「只读」取消。
//
// 🔴 **被判的那块没被 mock**：`PanoramaView` 的 evaluateRepo / load / 节点详情 / 审批队列都是真的，
//   写入口背后的 `panorama/api.ts` 写函数也是真的（一路走到 `invoke`）；
//   被 mock 的只有读那几条封装（status / node / listAnnotations …）与 `invoke` 本身。
// 判法：① 远端会话打开全景 ⇒ `status` 恰问那台机器的那个路径；
//       ② 远端仓的节点详情：写入口集合 == 本机仓的（两向，同一套步骤并排跑）；
//       ③ 点远端仓的「添加批注」/「批准」⇒ 发出去的恰是 `panorama_edit`，origin 是那台机器、仓是那台上的路径。
import { describe, it, expect, vi, beforeEach } from "vitest";
import type { Annotation, NodeView } from "../../src/panorama/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../src/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../src/keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../../src/panorama/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../src/panorama/api")>();
  return { ...actual, status: vi.fn(), node: vi.fn(), listAnnotations: vi.fn(), diagramKinds: vi.fn() };
});

import { invoke } from "@tauri-apps/api/core";
import * as api from "../../src/panorama/api";
import { PanoramaView } from "../../src/views/panorama";
import { LOCAL_ORIGIN, type Origin } from "../../src/ipc/origin";

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
type Probe = {
  sidebarEl: HTMLElement;
  root: HTMLElement;
  titleEl: HTMLElement;
  renderNodeDetail: (nv: NodeView) => void;
};
const probe = (v: PanoramaView): Probe => v as unknown as Probe;
const buttons = (v: PanoramaView): string[] =>
  [...probe(v).sidebarEl.querySelectorAll("button")].map((b) => b.textContent ?? "");
const clickButton = (v: PanoramaView, text: string): void => {
  const b = [...probe(v).sidebarEl.querySelectorAll("button")].find((x) => x.textContent === text);
  expect(b, `没有「${text}」按钮`).toBeTruthy();
  (b as HTMLButtonElement).click();
};
const edits = (): { cmd: string; args: Record<string, unknown> }[] =>
  vi
    .mocked(invoke)
    .mock.calls.filter((c) => c[0] === "panorama_edit")
    .map((c) => ({ cmd: c[0] as string, args: c[1] as Record<string, unknown> }));
const nodeView = (): NodeView =>
  ({
    symbol: { id: "src/lib.rs#f", name: "f", file: "src/lib.rs", kind: "Function", lang: "Rust", start_line: 1, end_line: 3 },
    callers: [],
    callees: [],
    docs: [],
    annotations: [
      { id: "a1", file: "src/lib.rs", symbol: "f", body: "旧批注", author: "me", status: "Active", origin: "Human" },
    ],
  }) as unknown as NodeView;
const queue: Annotation[] = [
  { id: "p1", file: "src/lib.rs", symbol: "f", body: "提议", author: "agent", status: "Proposed", origin: "Agent" },
  { id: "a1", file: "src/lib.rs", symbol: "f", body: "生效", author: "me", status: "Active", origin: "Human" },
] as Annotation[];

async function openAt(origin: Origin, cwd: string): Promise<PanoramaView> {
  const v = new PanoramaView(() => ({ cwd, origin }));
  await v.open();
  await flush();
  return v;
}

describe("远端会话的全景（RM1c · RM1d）", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.replaceChildren();
    // symbols=0 ⇒ 停在「建立索引」门口（不画布，与本文件无关的那半不启动）
    vi.mocked(api.status).mockResolvedValue({ symbols: 0, stale: false, indexedAt: null });
    vi.mocked(api.diagramKinds).mockResolvedValue([]);
    vi.mocked(api.listAnnotations).mockResolvedValue(queue);
    vi.mocked(api.node).mockResolvedValue(nodeView());
    vi.mocked(invoke).mockResolvedValue("a2");
  });

  it("M0 自证：视图与只读判定是真的，被 mock 的只有后端封装", () => {
    const proto = PanoramaView.prototype as unknown as Record<string, unknown>;
    for (const m of ["evaluateRepo", "load", "renderNodeDetail", "renderAnnotationQueue"]) {
      expect(vi.isMockFunction(proto[m]), m).toBe(false);
    }
    for (const w of ["addAnnotation", "approveAnnotation", "removeAnnotation", "writeDocLink"] as const) {
      expect(vi.isMockFunction(api[w]), w).toBe(false);
    }
    expect(vi.isMockFunction(api.status)).toBe(true);
  });

  it("M1 远端会话 ⇒ 问那台机器上的那个路径（不再挡在门外）", async () => {
    const v = await openAt("box1", "/srv/proj");
    expect(vi.mocked(api.status).mock.calls.map((c) => c[0])).toEqual([{ origin: "box1", path: "/srv/proj" }]);
    expect(probe(v).titleEl.textContent).toBe("代码全景 · proj · 远端 box1（跟随会话）");
    // 图种注册表也按那台机器取（两台的全景程序版本可以不同）。
    expect(vi.mocked(api.diagramKinds).mock.calls.map((c) => c[0])).toContain("box1");
  });

  it("M2 远端仓的节点详情与审批队列：写入口集合 == 本机仓的（两向）", async () => {
    const entries = async (origin: Origin, cwd: string): Promise<string[][]> => {
      const v = await openAt(origin, cwd);
      probe(v).renderNodeDetail(nodeView());
      const detail = buttons(v).sort();
      const hasTextarea = probe(v).sidebarEl.querySelector("textarea") !== null;
      (probe(v).root.querySelector('[data-pano="ann-queue"]') as HTMLButtonElement).click();
      await flush();
      return [detail, [String(hasTextarea)], buttons(v).sort()];
    };
    const remote = await entries("box1", "/srv/proj");
    const local = await entries(LOCAL_ORIGIN, "/home/me/proj");
    // 反空真：本机那一侧真的有写入口（不然两边都空也「相等」）。
    for (const w of ["添加批注", "删除", "关联"]) expect(local[0], w).toContain(w);
    for (const w of ["批准", "驳回", "删除"]) expect(local[2], w).toContain(w);
    expect(remote).toEqual(local);
  });

  it("M3 点远端仓的写入口 ⇒ 发的恰是 panorama_edit，origin / 仓是那台机器上的", async () => {
    const v = await openAt("box1", "/srv/proj");
    probe(v).renderNodeDetail(nodeView());
    (probe(v).sidebarEl.querySelector("textarea") as HTMLTextAreaElement).value = "新批注";
    clickButton(v, "添加批注");
    await flush();
    (probe(v).root.querySelector('[data-pano="ann-queue"]') as HTMLButtonElement).click();
    await flush();
    clickButton(v, "批准");
    await flush();
    expect(edits().map((e) => [e.args.origin, e.args.repo, e.args.op])).toEqual([
      ["box1", "/srv/proj", "add_annotation"],
      ["box1", "/srv/proj", "approve_annotation"],
    ]);
    expect(edits()[0].args.args).toEqual({ file: "src/lib.rs", symbol: "f", body: "新批注", author: "me" });
  });
});
