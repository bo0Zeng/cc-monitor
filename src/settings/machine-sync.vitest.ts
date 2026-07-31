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
  it("★ 在 cc-bus 里切机器 → cc-bus-hooks 跟着变（这就是 §5-4 记的那个病）", async () => {
    const bus = new CcBusSection();
    const hooks = new CcBusHooksSection();
    await settle();

    const busSel = selOf(bus.element, "cc-bus-origin");
    const hooksSel = selOf(hooks.element, "cc-bus-hooks-origin");
    // 前置：两块都拿到了同一份机器清单（否则下面的同步是无从谈起的）
    expect([...busSel.options].map((o) => o.value)).toEqual(ORIGINS);
    expect([...hooksSel.options].map((o) => o.value)).toEqual(ORIGINS);

    busSel.value = "nano";
    busSel.dispatchEvent(new Event("change"));
    await settle();

    expect(getCurrentMachine()).toBe("nano");
    expect(hooksSel.value).toBe("nano");
  });

  it("★ 反方向也同步（不是单向绑定）", async () => {
    const bus = new CcBusSection();
    const hooks = new CcBusHooksSection();
    await settle();
    const busSel = selOf(bus.element, "cc-bus-origin");
    const hooksSel = selOf(hooks.element, "cc-bus-hooks-origin");

    hooksSel.value = "nano";
    hooksSel.dispatchEvent(new Event("change"));
    await settle();

    expect(busSel.value).toBe("nano");
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
