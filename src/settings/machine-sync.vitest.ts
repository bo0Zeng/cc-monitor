/**
 * S4a：**跨分节同步**的行为测试 —— 这才是本轮要交付的东西。
 *
 * `machine-context.vitest.ts` 钉的是 store 本体；这里钉的是「四块真的接上去了」。
 * 主计划 §5-4 记的病是：`accounts` / `mcp` / `cc-bus` / `cc-bus-hooks` 各维护一份
 * `this.origin`，用户在一处切了机器，另外三处还停在上一台。
 *
 * 用 cc-bus 与 cc-bus-hooks 两块做主验（它们都是朴素 `<select>`，形状可比），
 * 外加 MCP（唯一能表示「本机」的那块）验 null 分支。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));
vi.mock("../accounts", () => ({
  fetchAccounts: vi.fn().mockResolvedValue({ accounts: [] }),
  selectableAccounts: () => [],
}));

import { invoke } from "@tauri-apps/api/core";
import { CcBusSection } from "./cc-bus-section";
import { CcBusHooksSection } from "./cc-bus-hooks-section";
import {
  getCurrentMachine,
  setCurrentMachine,
  __resetMachineContextForTests,
} from "./machine-context";

const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;

const ORIGINS = ["aya", "nano"];

function routeInvoke(cmd: string): unknown {
  switch (cmd) {
    case "list_remote_mcp_origins":
      return ORIGINS;
    case "read_cc_bus_state":
      return { agents: [], skipped: 0 };
    case "diagnose_cc_bus_hooks":
    case "diagnose_remote_cc_bus_hooks":
      return { entries: [] };
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
  /**
   * **E59 改写了这两条。** 原来它们用 `cc-bus-hooks` 的下拉既当驱动又当观察点，
   * 而那个下拉**已经删了** —— 本分节只作为机器详情页上的一块存在，页头就是选择器。
   *
   * 要守的性质没变（「在一处切机器，别处跟着变」），变的是**谁能驱动**：
   * - `cc-bus-section` 仍有下拉 —— 它住在**顶层 cc-bus 驾驶舱视图**里，那儿没有页上下文，
   *   它的选择器就是上下文本身（这是 E59 只删三处、留这一处的理由）。
   * - `cc-bus-hooks` 只**跟随**，不驱动。
   * - 机器详情页那条真实驱动路径是**路由切页 → store**，所以下面第二条直接驱动 store，
   *   那才是生产里的形状。
   */
  it("★ 在 cc-bus 驾驶舱里切机器 → cc-bus-hooks 跟着变（§5-4 记的那个病）", async () => {
    const bus = new CcBusSection();
    const hooks = new CcBusHooksSection();
    await settle();

    const busSel = selOf(bus.element, "cc-bus-origin");
    // 前置：驾驶舱那块拿到了机器清单（否则下面的同步无从谈起）
    expect([...busSel.options].map((o) => o.value)).toEqual(ORIGINS);
    // E59：hooks 那块不再有下拉，只有只读显示
    expect(hooks.element.querySelector("select.cc-bus-hooks-origin")).toBeNull();

    busSel.value = "nano";
    busSel.dispatchEvent(new Event("change"));
    await settle();

    expect(getCurrentMachine()).toBe("nano");
    expect(hooks.element.querySelector(".cc-bus-hooks-origin")?.textContent).toBe("nano");
  });

  it("★ 直接驱动 store（= 机器详情页切页那条真实路径）→ 两块都跟上", async () => {
    const bus = new CcBusSection();
    const hooks = new CcBusHooksSection();
    await settle();
    const busSel = selOf(bus.element, "cc-bus-origin");

    setCurrentMachine("nano");
    await settle();

    expect(busSel.value).toBe("nano");
    expect(hooks.element.querySelector(".cc-bus-hooks-origin")?.textContent).toBe("nano");
  });

  it("★ 切到「本机」时，只列远端的分节**原地不动**（不乱选一台）", async () => {
    // 已知的半截状态：这两块的下拉只列远端，表示不了本机。乱选一台比不动更糟
    // ——用户会以为自己在看本机，其实在对某台远端下命令。
    // S4b 的机器详情页会从根上解决（本机那一页不含只对远端有意义的分节）。
    const bus = new CcBusSection();
    await settle();
    const busSel = selOf(bus.element, "cc-bus-origin");
    busSel.value = "nano";
    busSel.dispatchEvent(new Event("change"));
    await settle();

    setCurrentMachine(null);
    await settle();
    expect(busSel.value).toBe("nano"); // 原地不动
    expect(getCurrentMachine()).toBeNull(); // store 本身照实记录
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
    new CcBusHooksSection();
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
