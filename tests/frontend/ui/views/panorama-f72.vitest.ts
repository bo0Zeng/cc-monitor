// F72 批注写 UI 接线测试（jsdom）：节点详情面板加批注 / 删批注 / 关联文档 → 对应 api 命令。
import { describe, it, expect, vi, beforeEach } from "vitest";
import type { NodeView, Annotation } from "../../../../src/frontend/ui/panorama/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../../../src/frontend/ui/keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../../../../src/frontend/ui/panorama/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../../../src/frontend/ui/panorama/api")>();
  return {
    ...actual,
    node: vi.fn(),
    addAnnotation: vi.fn(),
    removeAnnotation: vi.fn(),
    writeDocLink: vi.fn(),
  };
});

import * as api from "../../../../src/frontend/ui/panorama/api";
import { PanoramaView } from "../../../../src/frontend/ui/views/panorama";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
const pending = <T>(): Promise<T> => new Promise<T>(() => {});
const nodeView = (annotations: Annotation[] = []): NodeView =>
  ({
    symbol: {
      id: "src/lib.rs#f",
      name: "f",
      file: "src/lib.rs",
      kind: "function",
      lang: "rust",
      start_line: 1,
      end_line: 3,
    },
    callers: [],
    callees: [],
    docs: [],
    annotations,
  }) as unknown as NodeView;
type Probe = {
  repo: string | null;
  sidebarEl: HTMLElement;
  renderNodeDetail: (nv: NodeView) => void;
};
const probe = (v: PanoramaView): Probe => v as unknown as Probe;
const btnByText = (v: PanoramaView, t: string): HTMLButtonElement =>
  [...probe(v).sidebarEl.querySelectorAll("button")].find(
    (b) => b.textContent === t,
  ) as HTMLButtonElement;

describe("F72 批注 + doc-link 写 UI（节点详情面板）", () => {
  let v: PanoramaView;
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.replaceChildren();
    v = new PanoramaView(() => ({ cwd: "/repo", origin: LOCAL_ORIGIN }));
    probe(v).repo = "/repo";
    vi.mocked(api.node).mockReturnValue(pending()); // mutate 后的重取挂起，避免二次渲染
  });

  it("写批注 → api.addAnnotation(repo, 整个符号 id, body, author)", async () => {
    vi.mocked(api.addAnnotation).mockResolvedValue("id1");
    probe(v).renderNodeDetail(nodeView());
    const ta = probe(v).sidebarEl.querySelector(
      "textarea.panorama-ann-input",
    ) as HTMLTextAreaElement;
    ta.value = "这个函数要注意 X";
    btnByText(v, "添加批注").click();
    await flush();
    expect(api.addAnnotation).toHaveBeenCalledWith({ origin: LOCAL_ORIGIN, path: "/repo" }, "src/lib.rs#f", "这个函数要注意 X", "me");
  });

  it("空批注不提交", async () => {
    probe(v).renderNodeDetail(nodeView());
    btnByText(v, "添加批注").click();
    await flush();
    expect(api.addAnnotation).not.toHaveBeenCalled();
  });

  it("删批注 → api.removeAnnotation(repo, id)", async () => {
    vi.mocked(api.removeAnnotation).mockResolvedValue(true);
    probe(v).renderNodeDetail(
      nodeView([
        { id: "aaa", file: "src/lib.rs", symbol: "f", body: "old", author: "me", status: "Active", origin: "Human" },
      ]),
    );
    btnByText(v, "删除").click();
    await flush();
    expect(api.removeAnnotation).toHaveBeenCalledWith({ origin: LOCAL_ORIGIN, path: "/repo" }, "aaa");
  });

  it("写批注（@行号消歧 id）→ 〔P7〕整个 id 原样交（截 @行号 归上游 `SymbolRef::of`；小程序判据真引擎验查得回）", async () => {
    vi.mocked(api.addAnnotation).mockResolvedValue("id2");
    const nv = nodeView();
    (nv.symbol as unknown as { id: string }).id = "src/a.rs#f@42";
    probe(v).renderNodeDetail(nv);
    const ta = probe(v).sidebarEl.querySelector(
      "textarea.panorama-ann-input",
    ) as HTMLTextAreaElement;
    ta.value = "重载函数的批注";
    btnByText(v, "添加批注").click();
    await flush();
    expect(api.addAnnotation).toHaveBeenCalledWith({ origin: LOCAL_ORIGIN, path: "/repo" }, "src/a.rs#f@42", "重载函数的批注", "me");
  });

  it("关联文档 → api.writeDocLink(repo, doc, 符号全 id)", async () => {
    vi.mocked(api.writeDocLink).mockResolvedValue(undefined);
    probe(v).renderNodeDetail(nodeView());
    const input = probe(v).sidebarEl.querySelector(
      "input.panorama-ann-input",
    ) as HTMLInputElement;
    input.value = "docs/f.md";
    btnByText(v, "关联").click();
    await flush();
    expect(api.writeDocLink).toHaveBeenCalledWith({ origin: LOCAL_ORIGIN, path: "/repo" }, "docs/f.md", "src/lib.rs#f");
  });
});
