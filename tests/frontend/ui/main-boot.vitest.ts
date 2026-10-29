/**
 * 主窗口真入口（`main.ts` 的 DOMContentLoaded）起一遍：构造那一串不撞暂时性死区、走得过「建标签页栏」那一段，
 * 一直走到读键位那一步（那之后才订会话流、说 frontend-ready）。只替掉宿主那几样（窗口 · 事件 · 通道 · 主题 · 键位）；
 * 问后端的一律挂着不回（起页那一下不靠回包往下走）。
 *
 * 10-28：输入框的构造里就地画一遍、问 `tabs.activeTab()`，而 `tabs` 在它后面才建 ⇒ ReferenceError，开页卡在 booting。
 */
import { describe, expect, it, vi } from "vitest";

const reached = vi.fn();
const never = (): Promise<never> => new Promise<never>(() => {});

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(() => never()), Channel: class {} }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    setTitle: () => Promise.resolve(),
    minimize: () => Promise.resolve(),
    isFullscreen: () => Promise.resolve(false),
    setFullscreen: () => Promise.resolve(),
    onFocusChanged: () => Promise.resolve(() => {}),
    onResized: () => Promise.resolve(() => {}),
    onCloseRequested: () => Promise.resolve(() => {}),
    listen: () => Promise.resolve(() => {}),
    label: "main",
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
  emit: () => Promise.resolve(),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: () => Promise.resolve() }));
vi.mock("../../../src/comms/inward/chan", () => ({
  chan: { subscribe: () => never(), call: () => never() },
}));
vi.mock("../../../src/frontend/ui/ipc/chan-caller", async (actual) => ({
  ...(await actual<typeof import("../../../src/frontend/ui/ipc/chan-caller")>()),
  callTyped: vi.fn(() => never()),
}));
vi.mock("../../../src/frontend/ui/theme", () => ({ loadTheme: () => Promise.resolve({}) }));
vi.mock("../../../src/frontend/ui/keybindings/store", () => ({
  getKeybindings: () => {
    reached();
    return never();
  },
}));

describe("主窗口开页", () => {
  it("★ 起一遍真的 DOMContentLoaded：构造那一串走完、走到读键位那一步（不卡在 booting）", async () => {
    document.body.innerHTML = '<div id="app"><div id="tab-bar"></div><main id="message-stream"></main><div id="status-bar"></div></div>';
    let boot: (() => Promise<void>) | null = null;
    const add = window.addEventListener.bind(window);
    const spy = vi.spyOn(window, "addEventListener").mockImplementation(((type: string, fn: EventListenerOrEventListenerObject, o?: boolean | AddEventListenerOptions) => {
      if (type === "DOMContentLoaded" && boot === null && typeof fn === "function") boot = fn as unknown as () => Promise<void>;
      else add(type, fn, o);
    }) as typeof window.addEventListener);
    await import("../../../src/frontend/ui/main");
    spy.mockRestore();
    expect(boot, "main.ts 没挂 DOMContentLoaded").not.toBeNull();
    let failed: unknown = null;
    void boot!().catch((e: unknown) => (failed = e));
    for (let i = 0; i < 50 && !reached.mock.calls.length && failed === null; i++) await new Promise((r) => setTimeout(r, 0));
    expect(failed, "开页那一串抛了").toBeNull();
    expect(reached, "没走到读键位那一步").toHaveBeenCalled();
  });
});
