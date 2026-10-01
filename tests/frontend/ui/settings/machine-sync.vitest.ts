/**
 * S4a：**跨分节同步**的行为测试 —— 这才是本轮要交付的东西。
 *
 * `machine-context.vitest.ts` 钉的是 store 本体；这里钉的是「四块真的接上去了」。
 * 记的病是：几块分节各维护一份 `this.origin`，用户在一处切了机器，另外几处还停在上一台。
 *
 * 用 cc-bus 驾驶舱做主验：它的下拉既能驱动共用 store，也跟着 store 走（机器详情页切页那条真实路径）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));
// 读面拆去了 `account-reads.ts`，规则留在 `accounts.ts`：两处各桩各的。
vi.mock("../../../../src/frontend/ui/account-reads", () => ({
  fetchAccounts: vi.fn().mockResolvedValue({ accounts: [] }),
}));
vi.mock("../../../../src/frontend/ui/accounts", () => ({
  selectableAccounts: () => [],
}));

import { invoke } from "@tauri-apps/api/core";
import { CcBusSection } from "../../../../src/frontend/ui/settings/cc-bus-section";
import {
  getCurrentMachine,
  setCurrentMachine,
  __resetMachineContextForTests,
} from "../../../../src/frontend/ui/settings/machine-context";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";

const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;

const ORIGINS = ["devbox", "nano"];

function routeInvoke(cmd: string): unknown {
  switch (cmd) {
    case "list_remote_mcp_origins":
      return ORIGINS;
    case "read_cc_bus_state":
      return { agents: [], skipped: 0 };
    // 钩子诊断改走通道（`chan_call`）：本条不看它的结果，落 `default`。
    default:
      return undefined;
  }
}

/** 起一块分节并等它把 origin 下拉填好。 */
async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
}

function selOf(el: HTMLElement, cls: string): HTMLSelectElement {
  const s = el.querySelector<HTMLSelectElement>(`.${cls}`);
  if (!s) throw new Error(`select not found: ${cls}`);
  return s;
}

beforeEach(() => {
  __resetMachineContextForTests();
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) =>
    Promise.resolve(routeInvoke(cmd)),
  );
});

describe("S4a 跨分节机器同步", () => {
  it("★ 在 cc-bus 驾驶舱里切机器 → 共用 store 跟着变（别的分节都只跟随 store）", async () => {
    const bus = new CcBusSection();
    await settle();
    const busSel = selOf(bus.element, "cc-bus-origin");
    // P4a：驾驶舱的清单 = 远端们 + **末尾一项「本机」**（追加在末尾：`select` 默认取第一项）。
    expect([...busSel.options].map((o) => o.value)).toEqual([...ORIGINS, "<local>"]);
    busSel.value = "nano";
    busSel.dispatchEvent(new Event("change"));
    await settle();
    expect(getCurrentMachine()).toBe("nano");
  });

  it("★ 直接驱动 store（= 机器详情页切页那条真实路径）→ 驾驶舱跟上", async () => {
    const bus = new CcBusSection();
    await settle();
    const busSel = selOf(bus.element, "cc-bus-origin");
    setCurrentMachine("nano");
    await settle();
    expect(busSel.value).toBe("nano");
  });

  it("★ P4a：切到「本机」时，驾驶舱**跟着切**（它已经表示得了本机）", async () => {
    // ⚠ 本条原来钉的是「原地不动」，而它自己的注释逐字承认那是**已知的半截状态**：
    // 「这两块的下拉只列远端，**表示不了本机**……乱选一台比不动更糟」。
    // P4a 把驾驶舱那半补上了（后端读面支持 `<local>`，下拉多了「本机」一项）
    // ⇒ 半截状态在这一块不存在了，「原地不动」也就不再是对的行为：
    // store 说切本机、而面板还显示着某台远端，那才是在骗人。
    const bus = new CcBusSection();
    await settle();
    const busSel = selOf(bus.element, "cc-bus-origin");
    busSel.value = "nano";
    busSel.dispatchEvent(new Event("change"));
    await settle();

    setCurrentMachine(LOCAL_ORIGIN);
    await settle();
    expect(busSel.value).toBe("<local>"); // 跟着切到「本机」
    // store 与选择器现在是同一个表示（本机 = `LOCAL_ORIGIN`），原先那处换算没有了。
    expect(getCurrentMachine()).toBe(LOCAL_ORIGIN);
    // 本机派生已走后端 `bus-spawn` 原语（本机、远端同一条路）⇒ 切到本机时派生按钮**不再**禁用。
    //   原来这里钉的是「写面在本机没有对侧 ⇒ 当场禁用」，那个前提被 BS1b 拆掉了（`refuse_local_write` 已删）。
    const spawn = bus.element.querySelector(".cc-bus-spawn-go") as HTMLButtonElement;
    expect(spawn.disabled).toBe(false);
  });

  it("切到清单里没有的机器 → 原地不动（清单还没加载 / 那台已被删）", async () => {
    const bus = new CcBusSection();
    await settle();
    const busSel = selOf(bus.element, "cc-bus-origin");
    const before = busSel.value;
    setCurrentMachine("从来没有过这台");
    await settle();
    expect(busSel.value).toBe(before);
  });

  it("★ 同值重复切换不重复发请求（否则四块互相激起 ssh 往返 = 变相轮询）", async () => {
    const bus = new CcBusSection();
    await settle();
    const busSel = selOf(bus.element, "cc-bus-origin");

    busSel.value = "nano";
    busSel.dispatchEvent(new Event("change"));
    await settle();
    const after1 = mockInvoke.mock.calls.length;

    // 再切同一台三次
    for (let i = 0; i < 3; i++) {
      busSel.dispatchEvent(new Event("change"));
      await settle();
    }
    expect(mockInvoke.mock.calls.length).toBe(after1);
  });
});
