// 自己挑一个仓看（不跟着会话工作目录走）：「换仓…」→ 系统选目录 → 手选仓压过会话 cwd，
// 关了再开还是它；「跟随会话」撤掉手选。
//
// 🔴 **被判的那块没被 mock**：本文件 mock 的只有 IPC 出口、全局弹窗、全局浮层栈、
//   `@tauri-apps/plugin-dialog`（操作系统的选目录对话框）与 `panorama/api.status`
//   （后端边界；回 symbols=0 让加载停在「建立索引」门口，不画布）。
//   `PanoramaView` 的 evaluateRepo / load / 顶栏全是真的 —— `R0` 当场自证。
// 判法：`api.status` 的**完整调用序列**相等（走错仓就多出或少掉一项），标题整串相等。
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../../src/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../src/keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../../src/panorama/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../src/panorama/api")>();
  return { ...actual, status: vi.fn(), index: vi.fn() };
});

import { open as openDialog } from "@tauri-apps/plugin-dialog";
import * as api from "../../src/panorama/api";
import { PanoramaView } from "../../src/views/panorama";
import { LOCAL_ORIGIN } from "../../src/ipc/origin";

const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
type Probe = { repo: string | null; root: HTMLElement; titleEl: HTMLElement };
const probe = (v: PanoramaView): Probe => v as unknown as Probe;
const bar = (v: PanoramaView, key: string): HTMLButtonElement =>
  probe(v).root.querySelector(`[data-pano="${key}"]`) as HTMLButtonElement;
const statusCalls = (): string[] => vi.mocked(api.status).mock.calls.map((c) => c[0]);

describe("自己挑一个仓看", () => {
  let v: PanoramaView;
  let session: { cwd: string; origin: string } | null;
  beforeEach(() => {
    vi.clearAllMocks();
    document.body.replaceChildren();
    session = { cwd: "/work/repo", origin: LOCAL_ORIGIN };
    v = new PanoramaView(() => session);
    vi.mocked(api.status).mockResolvedValue({ symbols: 0, stale: false, indexedAt: null });
  });

  it("R0 自证：视图的仓决策没被 mock，被 mock 的只有对话框与 status", () => {
    const proto = PanoramaView.prototype as unknown as Record<string, unknown>;
    for (const m of ["evaluateRepo", "showRepo", "pickRepo", "load", "updateRepoChrome"]) {
      expect(vi.isMockFunction(proto[m]), m).toBe(false);
    }
    expect(vi.isMockFunction(openDialog)).toBe(true);
  });

  it("R1 默认跟随会话；换仓后看手选的那个，标题整串写清来源，「跟随会话」出现", async () => {
    await v.open();
    expect(probe(v).titleEl.textContent).toBe("代码全景 · repo（跟随会话）");
    expect(bar(v, "follow-session").style.display).toBe("none");
    vi.mocked(openDialog).mockResolvedValue("/other/proj");
    bar(v, "pick-repo").click();
    await flush();
    expect(statusCalls()).toEqual(["/work/repo", "/other/proj"]);
    expect(probe(v).repo).toBe("/other/proj");
    expect(probe(v).titleEl.textContent).toBe("代码全景 · proj（手选）");
    expect(probe(v).titleEl.title).toBe("/other/proj");
    expect(bar(v, "follow-session").style.display).toBe("");
  });

  it("R2 手选是粘的：关了再开还是它，不被会话 cwd 抢回去", async () => {
    vi.mocked(openDialog).mockResolvedValue("/other/proj");
    await v.open();
    bar(v, "pick-repo").click();
    await flush();
    v.close();
    session = { cwd: "/work/another", origin: LOCAL_ORIGIN }; // 期间活跃会话换了
    await v.open();
    expect(statusCalls()).toEqual(["/work/repo", "/other/proj", "/other/proj"]);
  });

  it("R3 「跟随会话」撤掉手选，回到会话 cwd", async () => {
    vi.mocked(openDialog).mockResolvedValue("/other/proj");
    await v.open();
    bar(v, "pick-repo").click();
    await flush();
    bar(v, "follow-session").click();
    await flush();
    expect(statusCalls()).toEqual(["/work/repo", "/other/proj", "/work/repo"]);
    expect(probe(v).titleEl.textContent).toBe("代码全景 · repo（跟随会话）");
    expect(bar(v, "follow-session").style.display).toBe("none");
  });

  it("R4 对话框取消 → 什么都不变（不发命令、不换仓）", async () => {
    await v.open();
    vi.mocked(openDialog).mockResolvedValue(null);
    bar(v, "pick-repo").click();
    await flush();
    expect(statusCalls()).toEqual(["/work/repo"]);
    expect(probe(v).repo).toBe("/work/repo");
  });

  it("R5 活跃会话是远端的也能看手选的本机仓（手选压过会话）", async () => {
    session = { cwd: "/remote/x", origin: "box1" };
    await v.open();
    expect(statusCalls()).toEqual([]); // 远端：不索引
    expect(probe(v).titleEl.textContent).toBe("代码全景");
    vi.mocked(openDialog).mockResolvedValue("/other/proj");
    bar(v, "pick-repo").click();
    await flush();
    expect(statusCalls()).toEqual(["/other/proj"]);
    expect(probe(v).repo).toBe("/other/proj");
  });
});
