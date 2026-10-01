/**
 * 🔴 **D ＋ E**（第一刀 · 步 4）：
 * **异步失败落在那一块上**（D）· **点击侧给 pending**（E）。
 *
 * # D 要防的
 *
 * `§1.3 D` 逐字：今天 `void` 掉的 reject → `main.ts` 那条全局兜底 → 状态栏上一行 `REJ:`。
 * 状态栏离出事的那一块十万八千里 —— 用户看到「机器列表是空的」，而原因印在屏幕另一头。
 *
 * # E 要防的
 *
 * `§1.3 E`：`await` ＋ disable ＋ 回执。防的是 `backend-section.ts` 补审 `A1` 那个
 * 真发生过的窗口：**连点两下 = 两次往返**，第二次的结果盖掉第一次，而屏幕上看不出来。
 *
 * # 反空真
 *
 * E 那一条不用「按钮变灰了没有」当主锚（那只是个属性，谁都能设）——
 * 主锚是**往返次数的相等断言**：按住期间再点 N 下，IPC 计数**恒等于 1**。
 * 并配一格**反向锚**：放开之后再点，计数必须变成 2（否则「永远按住」也能让上一条绿）。
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
vi.mock("../../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));

import { ConfigSurfaceSection } from "../../../../src/frontend/ui/settings/config-surface-section";
import { __resetMachineContextForTests } from "../../../../src/frontend/ui/settings/machine-context";
import { withPending } from "../../../../src/frontend/ui/settings/pending";

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("（步 4）：点击侧 pending —— 按住期间不许发第二趟", () => {
  beforeEach(() => {
    ipc.calls = [];
    ipc.gate = null;
    document.body.replaceChildren();
    __resetMachineContextForTests();
  });

  it("🔴 主锚：按住期间连点 4 下，往返次数**恒等于 1**", async () => {
    const sec = new ConfigSurfaceSection();
    document.body.appendChild(sec.element);
    const rescan = [...sec.element.querySelectorAll("button")].find(
      (b) => b.textContent === "重新扫描",
    )!;
    expect(rescan, "找不到「重新扫描」—— 下面的断言会量错地方").toBeTruthy();
    expect(ipc.calls.length, "构造期就不该发（步 2）").toBe(0);

    rescan.click();
    await tick();
    expect(rescan.dataset.pending, "按下之后要有 pending 这个身份").toBe("1");
    expect(rescan.disabled).toBe(true);
    for (let i = 0; i < 4; i++) rescan.click();
    await tick();
    expect(
      ipc.calls.filter((c) => c === "footprint_client_facts").length,
      "按住期间又发出去了第二趟 —— 那正是补审 A1 记的那个「双起」窗口",
    ).toBe(1);
  });

  it("🔴 反向锚：放开之后再点，往返次数必须变成 2（不许靠「永远按住」绿）", async () => {
    const sec = new ConfigSurfaceSection();
    document.body.appendChild(sec.element);
    const rescan = [...sec.element.querySelectorAll("button")].find(
      (b) => b.textContent === "重新扫描",
    )!;
    rescan.click();
    await tick();
    // 本机那一栏两拍（monitor 事实 → 本机后端），两拍各放开一次。
    ipc.gate?.();
    await tick();
    ipc.gate?.();
    await tick();
    await tick();
    expect(rescan.disabled, "回来了就得放开 —— 一直按住的话用户连重试都做不到").toBe(false);
    expect(rescan.dataset.pending).toBeUndefined();
    expect(rescan.textContent, "文字要还原，不能永远停在「扫描中…」").toBe("重新扫描");
    rescan.click();
    await tick();
    expect(ipc.calls.filter((c) => c === "footprint_client_facts").length).toBe(2);
  });

  it("失败也要放开（`finally`）—— 失败就永远按住比没有 pending 更糟", async () => {
    const btn = document.createElement("button");
    btn.textContent = "干活";
    await expect(
      withPending(btn, "忙…", () => Promise.reject(new Error("boom"))),
    ).rejects.toThrow("boom");
    expect(btn.disabled).toBe(false);
    expect(btn.textContent).toBe("干活");
    expect(btn.dataset.pending).toBeUndefined();
  });
});

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
