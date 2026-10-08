/**
 * 🔴 **D**（第一刀 · 步 4）：**异步失败落在那一块上**。
 * （E「点击侧给 pending」那一半的 `withPending` 产品里没人用，连同它的判据删了。）
 *
 * # D 要防的
 *
 * `§1.3 D` 逐字：今天 `void` 掉的 reject → `main.ts` 那条全局兜底 → 状态栏上一行 `REJ:`。
 * 状态栏离出事的那一块十万八千里 —— 用户看到「机器列表是空的」，而原因印在屏幕另一头。
 *
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { ipc } = vi.hoisted(() => ({
  ipc: { calls: [] as string[], gate: null as null | (() => void) },
}));

vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: new Proxy(
    {},
    {
      get: (_t, name: string) => () => {
        ipc.calls.push(name);
        // 每一发都挂着不回 —— 「往返期间」这件事在测试里就是这么造出来的。
        return new Promise((resolve) => {
          ipc.gate = () => resolve(undefined);
        });
      },
    },
  ),
}));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn() }));

import { __resetMachineContextForTests } from "../../../../src/frontend/ui/settings/machine-context";

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("（步 4）：异步失败落在那一块上，不再只打到状态栏", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    __resetMachineContextForTests();
  });

  it("🔴 `RemoteSection.refresh()` 读配置失败 ⇒ 这一块自己的 banner 上写出原因", async () => {
    vi.resetModules();
    vi.doMock("../../../../src/frontend/ui/remote-config", async () => {
      const real = await vi.importActual<Record<string, unknown>>(
        "../../../../src/frontend/ui/remote-config",
      );
      return {
        ...real,
        readRemoteConfig: () => Promise.reject(new Error("READ_BOOM")),
      };
    });
    const { RemoteSection } = await import("../../../../src/frontend/ui/settings/remote-section");
    const sec = new RemoteSection({ headless: true });
    document.body.appendChild(sec.element);
    await tick();
    await tick();
    const banner = sec.element.querySelector<HTMLElement>(".settings-banner");
    expect(banner, "这一块得有个地方说话").not.toBeNull();
    expect(
      banner!.textContent,
      "失败原因没有落在这一块上 —— 那它就只在状态栏上，离现场十万八千里",
    ).toContain("READ_BOOM");
    expect(banner!.className).toContain("settings-banner-show");
    vi.doUnmock("../../../../src/frontend/ui/remote-config");
  });
});
