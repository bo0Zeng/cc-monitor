// F69（补 D20）接线测试：钉死 PanoramaView.load 的真守卫——从未索引（symbols===0）开面板
// **不自动调 api.index（不扫描）**、显式点按钮才调。纯函数 panoramaLoadDecision 测不到这段接线
// （有人删掉 load() 里的 return、纯函数仍绿，D20 就静默回归了），故补 jsdom DOM 测试。
import { describe, it, expect, vi, beforeEach } from "vitest";
import type { Overview } from "../../../../src/frontend/ui/panorama/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../../../src/frontend/ui/keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
// 保留 panoramaLoadDecision 真实（门决策），只 mock invoke 封装（status/index/overview）。
vi.mock("../../../../src/frontend/ui/panorama/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../../../src/frontend/ui/panorama/api")>();
  return { ...actual, status: vi.fn(), index: vi.fn(), reindex: vi.fn(), overview: vi.fn() };
});

import * as api from "../../../../src/frontend/ui/panorama/api";
import { PanoramaView } from "../../../../src/frontend/ui/views/panorama";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
/** 永不 resolve 的 promise：让 load()/enableAndIndex 停在 overview await 前，避免触发 canvas draw。 */
const pending = <T>(): Promise<T> => new Promise<T>(() => {});
const msgBtn = (v: PanoramaView): HTMLButtonElement | null =>
  (v as unknown as { messageEl: HTMLElement }).messageEl.querySelector(
    "button.panorama-message-action",
  );
// 私有 load 探针（仅测试）。
const callLoad = (v: PanoramaView, repo: string): Promise<void> =>
  (v as unknown as { load: (r: string) => Promise<void> }).load(repo);

describe("F69 PanoramaView.load —— D20 默认关的真接线守卫", () => {
  let view: PanoramaView;
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.replaceChildren();
    view = new PanoramaView(() => ({ cwd: "/repo", origin: LOCAL_ORIGIN }));
  });

  it("symbols===0 → 绝不自动调 api.index（不扫描），显示「建立索引」按钮", async () => {
    vi.mocked(api.status).mockResolvedValue({ symbols: 0, stale: true, indexedAt: null });
    await callLoad(view, "/repo");
    expect(api.index).not.toHaveBeenCalled();
    const btn = msgBtn(view);
    expect(btn).toBeTruthy();
    expect(btn?.textContent).toContain("建立索引");
  });

  it("点「建立索引」按钮 → 才调 api.index（显式手势）", async () => {
    vi.mocked(api.status).mockResolvedValue({ symbols: 0, stale: true, indexedAt: null });
    vi.mocked(api.overview).mockReturnValue(pending<Overview>()); // 挂起，不触发 draw
    await callLoad(view, "/repo");
    expect(api.index).not.toHaveBeenCalled();
    msgBtn(view)!.click();
    await flush();
    // 第二个参数是撤单信号（这台撤不撤得掉由 `api.cancellable` 定），这里只钉「问的是哪个仓」。
    expect(vi.mocked(api.index).mock.calls.map((c) => c[0])).toEqual([{ origin: LOCAL_ORIGIN, path: "/repo" }]);
  });

  it("symbols>0 且非陈旧 → 直接加载 overview，不 index（不 enable-gate、不自动重扫）", async () => {
    vi.mocked(api.status).mockResolvedValue({ symbols: 5, stale: false, indexedAt: 1 });
    vi.mocked(api.overview).mockReturnValue(pending<Overview>()); // 挂起，避免 applyOverview/draw
    void callLoad(view, "/repo"); // 不 await（overview 挂起、load 不会 resolve）
    await flush();
    expect(api.index).not.toHaveBeenCalled();
    expect(api.overview).toHaveBeenCalledWith({ origin: LOCAL_ORIGIN, path: "/repo" });
  });

  it("symbols>0 且陈旧 → 自动重建（已启用仓保鲜，属正常运行、非新 opt-in）", async () => {
    vi.mocked(api.status).mockResolvedValue({ symbols: 5, stale: true, indexedAt: 1 });
    vi.mocked(api.index).mockResolvedValue({} as never);
    vi.mocked(api.overview).mockReturnValue(pending<Overview>());
    void callLoad(view, "/repo");
    await flush();
    expect(vi.mocked(api.index).mock.calls.map((c) => c[0])).toEqual([{ origin: LOCAL_ORIGIN, path: "/repo" }]);
  });

  // 要求：「要上游给的」④ · 「长活要有进度」· 「建索引可取消」（撤单那一格照旧在）。
  it("〔P7〕建索引时转圈下面那一行跟着那台报的进度走（阶段 · 做完几份 / 共几份）；取消按钮照旧在", async () => {
    vi.mocked(api.status).mockResolvedValue({ symbols: 0, stale: true, indexedAt: null });
    let report: ((p: { phase: "Parse" | "Link" | "Relink" | "Docs"; done: number; total: number }) => void) | undefined;
    vi.mocked(api.index).mockImplementation((_at, _cancel, onProgress) => {
      report = onProgress;
      return pending();
    });
    await callLoad(view, "/repo");
    msgBtn(view)!.click();
    await flush();
    const loading = (view as unknown as { loadingEl: HTMLElement }).loadingEl;
    const line = (): string | null => loading.querySelector('[data-pano="index-progress"]')?.textContent ?? null;
    expect(line()).toBe("");
    report?.({ phase: "Parse", done: 3, total: 10 });
    expect(line()).toBe("解析源文件：3 / 10");
    report?.({ phase: "Link", done: 10, total: 10 });
    expect(line()).toBe("建立调用关系：10 / 10");
    expect([...loading.querySelectorAll("button")].map((b) => b.textContent)).toEqual(["取消"]);
  });
});
