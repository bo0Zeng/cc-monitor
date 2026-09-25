// 〔RM1c · 第四波〕远端会话的全景（jsdom）：看那台机器上的仓；第一拍**只读**。
//
// 🔴 **被判的那块没被 mock**：`PanoramaView` 的 evaluateRepo / load / 节点详情 / 审批队列都是真的，
//   `panorama/api.ts` 的 `canWriteAnnotations` / `REMOTE_WRITE_REFUSED` 也是真的；
//   被 mock 的只有后端边界那几条封装（status / node / listAnnotations …）。
// 判法：① 远端会话打开全景 ⇒ `status` 恰问那台机器的那个路径（不再说「仅支持本地仓库」）；
//       ② 远端仓的节点详情里**没有**写入口（添加批注 / 删除 / 关联），换成那一句只读说明；
//       ③ 远端仓的审批队列列得出来，但**没有**批准 / 驳回按钮；
//       ④ 阴性对照：同一套步骤换成本机仓，写入口都在、只读说明不在。
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
const readOnlyNotes = (v: PanoramaView): string[] =>
  [...probe(v).sidebarEl.querySelectorAll('[data-pano="remote-readonly"]')].map((n) => n.textContent ?? "");
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

describe("远端会话的全景（RM1c）", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.replaceChildren();
    // symbols=0 ⇒ 停在「建立索引」门口（不画布，与本文件无关的那半不启动）
    vi.mocked(api.status).mockResolvedValue({ symbols: 0, stale: false, indexedAt: null });
    vi.mocked(api.diagramKinds).mockResolvedValue([]);
    vi.mocked(api.listAnnotations).mockResolvedValue(queue);
  });

  it("M0 自证：视图与只读判定是真的，被 mock 的只有后端封装", () => {
    const proto = PanoramaView.prototype as unknown as Record<string, unknown>;
    for (const m of ["evaluateRepo", "load", "renderNodeDetail", "renderAnnotationQueue"]) {
      expect(vi.isMockFunction(proto[m]), m).toBe(false);
    }
    expect(vi.isMockFunction(api.canWriteAnnotations)).toBe(false);
    expect(vi.isMockFunction(api.status)).toBe(true);
  });

  it("M1 远端会话 ⇒ 问那台机器上的那个路径（不再挡在门外）", async () => {
    const v = await openAt("box1", "/srv/proj");
    expect(vi.mocked(api.status).mock.calls.map((c) => c[0])).toEqual([{ origin: "box1", path: "/srv/proj" }]);
    expect(probe(v).titleEl.textContent).toBe("代码全景 · proj · 远端 box1（跟随会话）");
    // 图种注册表也按那台机器取（两台的全景程序版本可以不同）。
    expect(vi.mocked(api.diagramKinds).mock.calls.map((c) => c[0])).toContain("box1");
  });

  it("M2 远端仓的节点详情：没有写入口，换成那一句只读说明", async () => {
    const v = await openAt("box1", "/srv/proj");
    probe(v).renderNodeDetail(nodeView());
    const b = buttons(v);
    for (const w of ["添加批注", "删除", "关联"]) expect(b, w).not.toContain(w);
    expect(readOnlyNotes(v)).toEqual([api.REMOTE_WRITE_REFUSED, api.REMOTE_WRITE_REFUSED]);
    expect(probe(v).sidebarEl.querySelector("textarea")).toBeNull();
  });

  it("M3 远端仓的审批队列：列得出来，没有批准 / 驳回 / 删除", async () => {
    const v = await openAt("box1", "/srv/proj");
    (probe(v).root.querySelector('[data-pano="ann-queue"]') as HTMLButtonElement).click();
    await flush();
    expect(vi.mocked(api.listAnnotations).mock.calls.map((c) => c[0])).toEqual([{ origin: "box1", path: "/srv/proj" }]);
    expect(probe(v).sidebarEl.querySelectorAll(".panorama-ann-row").length).toBe(2);
    const b = buttons(v);
    for (const w of ["批准", "驳回", "删除"]) expect(b, w).not.toContain(w);
    expect(readOnlyNotes(v)).toEqual([api.REMOTE_WRITE_REFUSED]);
  });

  it("M4 阴性对照：同一套步骤换成本机仓，写入口都在、只读说明不在", async () => {
    const v = await openAt(LOCAL_ORIGIN, "/home/me/proj");
    probe(v).renderNodeDetail(nodeView());
    for (const w of ["添加批注", "删除", "关联"]) expect(buttons(v), w).toContain(w);
    expect(readOnlyNotes(v)).toEqual([]);
    (probe(v).root.querySelector('[data-pano="ann-queue"]') as HTMLButtonElement).click();
    await flush();
    for (const w of ["批准", "驳回", "删除"]) expect(buttons(v), w).toContain(w);
    expect(readOnlyNotes(v)).toEqual([]);
  });
});
