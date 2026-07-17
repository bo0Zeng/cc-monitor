// F72 批注写 UI 接线测试（jsdom）：节点详情面板加批注 / 删批注 / 关联文档 → 对应 api 命令。
import { describe, it, expect, vi, beforeEach } from "vitest";
import type { NodeView, Annotation } from "../panorama/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../panorama/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../panorama/api")>();
  return {
    ...actual,
    node: vi.fn(),
    addAnnotation: vi.fn(),
    removeAnnotation: vi.fn(),
    writeDocLink: vi.fn(),
  };
});

import * as api from "../panorama/api";
import { PanoramaView, symbolSegForAnnotation } from "./panorama";

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
    v = new PanoramaView(() => ({ cwd: "/repo", origin: null }));
    probe(v).repo = "/repo";
    vi.mocked(api.node).mockReturnValue(pending()); // mutate 后的重取挂起，避免二次渲染
  });

  it("写批注 → api.addAnnotation(repo, file, 符号段, body, author)", async () => {
    vi.mocked(api.addAnnotation).mockResolvedValue("id1");
    probe(v).renderNodeDetail(nodeView());
    const ta = probe(v).sidebarEl.querySelector(
      "textarea.panorama-ann-input",
    ) as HTMLTextAreaElement;
    ta.value = "这个函数要注意 X";
    btnByText(v, "添加批注").click();
    await flush();
    expect(api.addAnnotation).toHaveBeenCalledWith("/repo", "src/lib.rs", "f", "这个函数要注意 X", "me");
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
        { id: "aaa", file: "src/lib.rs", symbol: "f", body: "old", author: "me", status: "Active" },
      ]),
    );
    btnByText(v, "删除").click();
    await flush();
    expect(api.removeAnnotation).toHaveBeenCalledWith("/repo", "aaa");
  });

  it("symbolSegForAnnotation：镜像 core split_sym_id（截 @行号消歧，否则同名多符号静默丢失）", () => {
    expect(symbolSegForAnnotation("src/a.rs#f")).toBe("f");
    expect(symbolSegForAnnotation("src/a.rs#Type::method")).toBe("Type::method");
    // @行号消歧 id：必须截掉 @42，否则 annotations_for 查询段("f")对不上写入段("f@42")。
    expect(symbolSegForAnnotation("src/a.rs#f@42")).toBe("f");
    expect(symbolSegForAnnotation("src/a.rs#Type::m@88")).toBe("Type::m");
    expect(symbolSegForAnnotation("noHash")).toBeNull();
  });

  it("写批注（@行号消歧 id）→ addAnnotation 传截断后的段 f（回归：防静默丢失）", async () => {
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
    expect(api.addAnnotation).toHaveBeenCalledWith("/repo", "src/lib.rs", "f", "重载函数的批注", "me");
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
    expect(api.writeDocLink).toHaveBeenCalledWith("/repo", "docs/f.md", "src/lib.rs#f");
  });
});
