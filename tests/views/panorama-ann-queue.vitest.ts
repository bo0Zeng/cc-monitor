// 批注审批队列（`设计/97` CP5 的人那一半）：顶栏「批注审批」→ 列全部批注 → 批准 / 驳回。
//
// 🔴 **被判的是那一块界面本身，它不许被 mock**：本文件只 mock 了四样东西 ——
//   `@tauri-apps/api/core`（IPC 出口）· `error-toast`（全局弹窗）· `keybindings/registry`
//   （全局浮层栈）· `panorama/api` 里的**三条命令封装**（listAnnotations / approveAnnotation /
//   removeAnnotation，即后端边界）。`PanoramaView` 与它画出来的 DOM 全是真的；
//   下面 `Q0` 那条把这件事当场验掉（渲染方法不是 mock、侧栏就挂在视图根下）。
// 买到：按钮 → 命令 → 重列 这条接线，与「待审 / 已生效」按 core 给的 status 分组。
// **买不到**：core 那侧「Proposed 对 agent 不可见」—— 那一半在
//   `tests/bridge/panorama_tests.rs` 的真引擎判据里。
import { describe, it, expect, vi, beforeEach } from "vitest";
import type { Annotation } from "../../src/panorama/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../src/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../src/keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../../src/panorama/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../src/panorama/api")>();
  return {
    ...actual,
    listAnnotations: vi.fn(),
    approveAnnotation: vi.fn(),
    removeAnnotation: vi.fn(),
  };
});

import * as api from "../../src/panorama/api";
import { showActionFailureToast } from "../../src/error-toast";
import { PanoramaView } from "../../src/views/panorama";
import { LOCAL_ORIGIN } from "../../src/ipc/origin";

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
const ann = (id: string, status: string, author = "agent-x", origin = "Agent"): Annotation =>
  ({ id, file: "src/lib.rs", symbol: "f", body: `正文 ${id}`, author, status, origin }) as Annotation;
type Probe = { repo: string | null; root: HTMLElement; sidebarEl: HTMLElement };
const probe = (v: PanoramaView): Probe => v as unknown as Probe;
const ids = (v: PanoramaView, which: "proposed" | "active"): string[] =>
  [
    ...probe(v).sidebarEl.querySelectorAll(
      `[data-pano="ann-queue-${which}"] .panorama-ann-row`,
    ),
  ].map((r) => (r as HTMLElement).dataset.annId ?? "");
const rowBtn = (v: PanoramaView, id: string, text: string): HTMLButtonElement =>
  [
    ...probe(v).sidebarEl.querySelectorAll(`.panorama-ann-row[data-ann-id="${id}"] button`),
  ].find((b) => b.textContent === text) as HTMLButtonElement;
const openQueue = async (v: PanoramaView): Promise<void> => {
  (probe(v).root.querySelector('[data-pano="ann-queue"]') as HTMLButtonElement).click();
  await flush();
};

describe("批注审批队列", () => {
  let v: PanoramaView;
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.replaceChildren();
    v = new PanoramaView(() => ({ cwd: "/repo", origin: LOCAL_ORIGIN }));
    probe(v).repo = "/repo";
  });

  it("Q0 自证：被判的那块没被 mock —— 渲染方法是真的，侧栏挂在视图根下", () => {
    const proto = PanoramaView.prototype as unknown as Record<string, unknown>;
    expect(vi.isMockFunction(proto.renderAnnotationQueue)).toBe(false);
    expect(vi.isMockFunction(proto.showAnnotationQueue)).toBe(false);
    expect(probe(v).root.contains(probe(v).sidebarEl)).toBe(true);
    // 被 mock 的只有那三条命令封装（后端边界）。
    expect(vi.isMockFunction(api.listAnnotations)).toBe(true);
  });

  it("Q1 顶栏按钮 → listAnnotations(repo)，按 core 的 status 分成「待审 / 已生效」两节", async () => {
    vi.mocked(api.listAnnotations).mockResolvedValue([
      ann("p1", "Proposed"),
      ann("a1", "Active", "me"),
      ann("p2", "Proposed"),
    ]);
    await openQueue(v);
    expect(api.listAnnotations).toHaveBeenCalledWith({ origin: LOCAL_ORIGIN, path: "/repo" });
    expect(ids(v, "proposed")).toEqual(["p1", "p2"]);
    expect(ids(v, "active")).toEqual(["a1"]);
    expect(probe(v).sidebarEl.querySelector(".panorama-sidebar-subtitle")?.textContent).toBe(
      "2 条待审 · 1 条已生效",
    );
    // 已生效那一节没有「批准」按钮（批准只对待审有意义）。
    expect(rowBtn(v, "a1", "批准")).toBeUndefined();
  });

  it("Q2 批准 → approveAnnotation(repo, id) 恰一次、不删，然后重列", async () => {
    vi.mocked(api.listAnnotations)
      .mockResolvedValueOnce([ann("p1", "Proposed"), ann("a1", "Active", "me")])
      .mockResolvedValueOnce([ann("a1", "Active", "me"), ann("p1", "Active")]);
    vi.mocked(api.approveAnnotation).mockResolvedValue(true);
    await openQueue(v);
    rowBtn(v, "p1", "批准").click();
    await flush();
    await flush();
    expect(vi.mocked(api.approveAnnotation).mock.calls).toEqual([[{ origin: LOCAL_ORIGIN, path: "/repo" }, "p1"]]);
    expect(api.removeAnnotation).not.toHaveBeenCalled();
    expect(api.listAnnotations).toHaveBeenCalledTimes(2);
    expect(ids(v, "proposed")).toEqual([]);
    expect(ids(v, "active")).toEqual(["a1", "p1"]);
  });

  it("Q3 驳回 → removeAnnotation(repo, id)，不走 approve", async () => {
    vi.mocked(api.listAnnotations).mockResolvedValue([ann("p2", "Proposed")]);
    vi.mocked(api.removeAnnotation).mockResolvedValue(true);
    await openQueue(v);
    rowBtn(v, "p2", "驳回").click();
    await flush();
    expect(vi.mocked(api.removeAnnotation).mock.calls).toEqual([[{ origin: LOCAL_ORIGIN, path: "/repo" }, "p2"]]);
    expect(api.approveAnnotation).not.toHaveBeenCalled();
  });

  it("Q4 批准回 false（那条已不在）→ 如实弹提示，不装作批准成功", async () => {
    vi.mocked(api.listAnnotations).mockResolvedValue([ann("p1", "Proposed")]);
    vi.mocked(api.approveAnnotation).mockResolvedValue(false);
    await openQueue(v);
    rowBtn(v, "p1", "批准").click();
    await flush();
    expect(vi.mocked(showActionFailureToast).mock.calls.map((c) => c[0])).toEqual([
      "那条批注已不在",
    ]);
  });

  it("Q5 不认识的状态不许静默吞掉 —— 单独数出来", async () => {
    vi.mocked(api.listAnnotations).mockResolvedValue([
      ann("p1", "Proposed"),
      ann("x1", "Rejected"),
    ]);
    await openQueue(v);
    const notes = [...probe(v).sidebarEl.querySelectorAll(".panorama-side-note")].map(
      (n) => n.textContent,
    );
    expect(notes.filter((t) => t?.startsWith("另有 "))).toEqual([
      "另有 1 条批注的状态这里认不出来，没有列出。",
    ]);
  });

  it("Q6 无本地仓 → 不发命令，弹提示", async () => {
    probe(v).repo = null;
    await openQueue(v);
    expect(api.listAnnotations).not.toHaveBeenCalled();
    expect(vi.mocked(showActionFailureToast).mock.calls.map((c) => c[0])).toEqual(["无法列批注"]);
  });

  it("Q7 CP6：已生效那一节把「人写的」与「agent 提议、人批准的」分开写（数据来自上游 origin）", async () => {
    vi.mocked(api.listAnnotations).mockResolvedValue([
      ann("a1", "Active", "me", "Human"),
      ann("a2", "Active", "bot", "Agent"),
      ann("a3", "Active", "old", "Unrecorded"),
    ]);
    await openQueue(v);
    const who = [
      ...probe(v).sidebarEl.querySelectorAll('[data-pano="ann-queue-active"] .panorama-ann-author'),
    ].map((e) => e.textContent);
    expect(who).toEqual([
      "me · 人写 · src/lib.rs#f",
      "bot · agent 提议 · src/lib.rs#f",
      "old · 来源未记录 · src/lib.rs#f",
    ]);
  });
});
