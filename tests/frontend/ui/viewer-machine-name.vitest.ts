/**
 * 独立查看窗也订机器状态成品：窗口起来就订 `changed/machine`（订上问一次 `machine-read`），
 * 之后这扇窗里称呼哪台都照那台成品的 `name` —— 本机不再是「机器不明」，系统标题里远端那台也叫成品那个名字、不叫键。
 * 驱动的是真入口（`entry-viewer.ts` 的 DOMContentLoaded），只替掉宿主那几样（窗口 · 事件 · 通道 · 读历史 · 查看器本体）。
 */
import { beforeAll, describe, expect, it, vi } from "vitest";
import type { MachineState } from "../../../src/frontend/ui/generated/MachineState";
import golden from "../../__fixtures__/machine-state.golden.json";

const titles: string[] = [];
const asked: string[] = [];
const up = (golden.cases as unknown as { machine: MachineState }[])[0].machine;
const products: Record<string, MachineState> = {
  "<local>": { ...up, name: "台架本机" },
  devbox: { ...up, name: "台架工作站" },
};

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    setTitle: (t: string) => {
      titles.push(t);
      return Promise.resolve();
    },
    minimize: () => Promise.resolve(),
    isFullscreen: () => Promise.resolve(false),
    setFullscreen: () => Promise.resolve(),
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
  emit: () => Promise.resolve(),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: () => Promise.resolve() }));
vi.mock("../../../src/comms/inward/chan", () => ({
  chan: {
    subscribe: () => Promise.resolve({ want: () => {}, stop: () => {} }),
  },
}));
vi.mock("../../../src/frontend/ui/ipc/chan-caller", async (actual) => ({
  ...(await actual<typeof import("../../../src/frontend/ui/ipc/chan-caller")>()),
  budgetWithin: () => ({}),
  callTyped: vi.fn((_origin: string, cmd: string) => {
    asked.push(cmd);
    return Promise.resolve(cmd === "machine-read" ? { machines: products } : {});
  }),
}));
vi.mock("../../../src/frontend/ui/theme", () => ({ loadTheme: () => Promise.resolve({}) }));
vi.mock("../../../src/frontend/ui/keybindings/store", () => ({ getKeybindings: () => Promise.resolve({}) }));
vi.mock("../../../src/frontend/ui/history-list-reads", () => ({
  fetchList: () =>
    Promise.resolve({
      rows: [
        {
          sessionId: "s1",
          label: "支付回调验签",
          projectName: "orders",
          projectPath: "/w/orders",
          jsonlPath: "/w/s1.jsonl",
          origin: "devbox",
          agent: "claudecode",
          can: { fork: false, accounts: false },
          face: { dot: "ended", live: { is: "no" } },
        },
      ],
    }),
}));
vi.mock("../../../src/frontend/ui/views/session-viewer", () => ({
  SessionViewer: class {
    element = document.createElement("div");
    toggleSide(): void {}
    openFind(): void {}
    showBanner(): void {}
    load(): Promise<void> {
      return Promise.resolve();
    }
  },
}));

import { machineName } from "../../../src/frontend/ui/ipc/chan-caller";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";

const settle = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};

beforeAll(async () => {
  document.body.innerHTML = '<div id="app"><div id="tab-bar"></div><main id="message-stream"></main><div id="status-bar"></div></div>';
  window.history.replaceState(null, "", "/viewer.html?viewer=s1&origin=devbox");
  await import("../../../src/frontend/ui/entry-viewer");
  window.dispatchEvent(new Event("DOMContentLoaded"));
  await settle();
});

describe("查看窗的机器名照成品", () => {
  it("窗口起来就问一次各台成品", () => {
    expect(asked).toContain("machine-read");
  });

  it("本机叫成品那个名字，不是「机器不明」", () => {
    expect(machineName(LOCAL_ORIGIN)).toBe("台架本机");
  });

  it("系统标题里远端那台叫成品那个名字，不叫键", () => {
    const last = titles[titles.length - 1] ?? "";
    expect(last).toContain("台架工作站");
    expect(last).not.toContain("devbox");
  });
});
