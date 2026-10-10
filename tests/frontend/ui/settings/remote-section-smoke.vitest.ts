// T07 DoD①：`RemoteSection` 的 smoke 测试。
//
// **它此前从未被任何测试执行过**——`remote-section.vitest.ts` 17 条全是纯函数，
// 全文 `new RemoteSection` 出现 0 次；T03 迁移后那个待贴块只被结构性守卫按**源码文本**
// 查过 `contains("buildPasteBlock")`。而 T03 的 commit 把它称为"这次抽象最实在的收益"
// ——一个从没被执行过的收益。这条测试就是补那个洞。
//
// **源码文本扫描 ≠ 行为测试**：这是本会话反复栽的形状，所以这里真 `new` 一次。
import { describe, it, expect, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...a: unknown[]) => invokeMock(...a),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
/** 设置窗订的事件：按名字记下处理函数（测试里直接推一帧）。 */
const eventHandlers = new Map<string, (e: { payload: unknown }) => void>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, h: (e: { payload: unknown }) => void) => {
    eventHandlers.set(name, h);
    return Promise.resolve(() => eventHandlers.delete(name));
  },
}));

import { RemoteSection } from "../../../../src/frontend/ui/settings/remote-section";
import { MachineCard } from "../../../../src/frontend/ui/settings/machine-card";
import { HOST_DEFAULTS } from "../../../../src/frontend/ui/remote-config";

beforeEach(() => {
  invokeMock.mockReset();
  // 构造期有 1 处 `void this.load*`；给它一个能 resolve 的形状，别让未捕获 rejection
  // 污染别的测试
  invokeMock.mockResolvedValue([]);
  document.body.textContent = "";
});

describe("RemoteSection 真构造一次（此前 0 次执行）", () => {
  it("构造不抛，且 element 挂得上 DOM", () => {
    let s: RemoteSection | undefined;
    expect(() => {
      s = new RemoteSection({ headless: true });
    }).not.toThrow();
    document.body.appendChild(s!.element);
    expect(s!.element.isConnected).toBe(true);
  });

});

describe("VIS2 机器页收 host key 告知", () => {
  it("★ 固化了 ⇒ 指纹栏从盘上同步、说一行；别台的不理；各地址不一 ⇒ 说出来、指纹栏不动", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "load_config"
        ? Promise.resolve({
            remote: {
              enabled: true,
              hosts: [{ label: "devbox", host: "h", user: "u", hostKeyFingerprint: "SHA256:disk" }],
            },
          })
        : Promise.resolve([]),
    );
    const card = new MachineCard(
      { ...HOST_DEFAULTS, label: "devbox", host: "h", user: "u" },
      { onChange: vi.fn(), onRemove: vi.fn() },
      false,
      "devbox",
    );
    const text = () =>
      [card.element, ...Object.values(card.parts())].map((e) => e.textContent ?? "").join("\n");
    await card.onHostKeyNotice({ origin: "other", kind: "host_key_pinned", message: "别台" });
    expect(card.collect().hostKeyFingerprint).toBe("");
    expect(text()).not.toContain("别台");
    await card.onHostKeyNotice({ origin: "devbox", kind: "host_key_differs", message: "几个地址不一样：a SHA256:x；b SHA256:y" });
    expect(card.collect().hostKeyFingerprint, "不一致时不该替人填").toBe("");
    expect(text()).toContain("几个地址不一样：a SHA256:x；b SHA256:y");
    await card.onHostKeyNotice({ origin: "devbox", kind: "host_key_pinned", message: "已记下" });
    expect(card.collect().hostKeyFingerprint).toBe("SHA256:disk");
    expect(text()).toContain("已记下");
  });
});

describe("机器卡折叠指示：代码画的箭头，不是字符", () => {
  it("legend 上那枚是 caretRight 图标、不带字；点 legend 折起 ⇒ aria-expanded 翻成 false", () => {
    const card = new MachineCard({ ...HOST_DEFAULTS, label: "devbox", host: "h", user: "u" }, { onChange: vi.fn(), onRemove: vi.fn() }, false, "devbox");
    document.body.appendChild(card.element);
    const legend = card.element.querySelector<HTMLElement>(".remote-machine-legend");
    const t = card.element.querySelector<HTMLElement>(".remote-machine-toggle");
    expect(t?.textContent).toBe("");
    expect(t?.querySelector<SVGElement>("svg")?.dataset.icon).toBe("caretRight");
    const before = legend?.getAttribute("aria-expanded");
    legend?.click();
    expect(legend?.getAttribute("aria-expanded")).not.toBe(before);
    expect(t?.textContent).toBe("");
  });
});

describe("重读机器表之后，上一批机器卡不再收告知（不留在全窗那张名单上）", () => {
  it("★ 刷新三次（＝ 设置窗关了再开三次）⇒ 一条「指纹已记下」只让当前那张卡去盘上读一次", async () => {
    let reads = 0;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "load_config") reads++;
      return cmd === "load_config"
        ? Promise.resolve({ remote: { enabled: true, hosts: [{ label: "devbox", host: "h", user: "u", hostKeyFingerprint: "SHA256:disk" }] } })
        : Promise.resolve([]);
    });
    const s = new RemoteSection({ headless: true });
    document.body.appendChild(s.element);
    const readsPerNotice = async (): Promise<number> => {
      const notice = eventHandlers.get("remote-health");
      expect(notice, "机器卡订了 remote-health").toBeDefined();
      reads = 0;
      notice!({ payload: { origin: "devbox", kind: "host_key_pinned", message: "已记下" } });
      await new Promise((r) => setTimeout(r, 0));
      return reads;
    };
    await s.refresh();
    const once = await readsPerNotice();
    expect(once, "当前那张卡去盘上读了").toBeGreaterThan(0);
    await s.refresh();
    await s.refresh();
    expect(await readsPerNotice(), "刷新几次都只有当前那一张卡去读盘（上一批卡已经作废）").toBe(once);
  });
});

