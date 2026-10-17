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
import { copyText } from "../../../../src/frontend/ui/copy-table";

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

describe("机器卡的主机指纹那一行只读库里记着的", () => {
  it("★ 记着 ⇒ 那一行写出指纹、［忘记…］在；没记 ⇒ 写「未记下」那一句、［忘记…］藏起", () => {
    const card = new MachineCard(
      { ...HOST_DEFAULTS, label: "devbox", host: "h", user: "u" },
      { onChange: vi.fn(), onRemove: vi.fn() },
      false,
      "devbox",
    );
    const fp = card.element.querySelector<HTMLElement>(".machine-fp")!;
    const forget = card.element.querySelector<HTMLButtonElement>(".machine-conn-fp-line button")!;
    expect(forget.hidden).toBe(true);
    card.setHostKeys(["SHA256:abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG"]);
    expect(fp.title).toBe("SHA256:abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG");
    expect(forget.hidden).toBe(false);
    card.setHostKeys([]);
    expect(fp.textContent).toBe(copyText("machineCard.fp.none"));
    expect(forget.hidden).toBe(true);
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

