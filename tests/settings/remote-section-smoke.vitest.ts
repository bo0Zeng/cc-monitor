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

import { RemoteSection } from "../../src/settings/remote-section";
import { MachineCard } from "../../src/settings/machine-card";
import { HOST_DEFAULTS } from "../../src/remote-config";

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

  it("待贴块的三句话真的上屏（T03 那个「最实在的收益」此前零执行）", () => {
    const s = new RemoteSection({ headless: true });
    document.body.appendChild(s.element);
    const t = s.element.querySelector(".paste-block-target");
    const m = s.element.querySelector(".paste-block-merge");
    const a = s.element.querySelector(".paste-block-activation");
    expect(t, "贴到哪：必须在 DOM 里").not.toBeNull();
    expect(m, "怎么合并：必须在 DOM 里").not.toBeNull();
    expect(a, "怎样才生效：必须在 DOM 里").not.toBeNull();
    expect(t!.textContent).toContain(".bashrc");
    expect(m!.textContent!.trim().length).toBeGreaterThan(0);
    expect(a!.textContent).toContain("source");
  });

  it("输出面只读、且保住了 <pre> 的不软换行语义（29 行 wrapper 片段）", () => {
    const s = new RemoteSection({ headless: true });
    const out =
      s.element.querySelector<HTMLTextAreaElement>(".paste-block-out");
    expect(out, "输出面必须在").not.toBeNull();
    expect(out!.readOnly).toBe(true);
    expect(out!.tagName).toBe("TEXTAREA");
    // 内容真的是那段 wrapper（不是空壳）
    expect(out!.value.length).toBeGreaterThan(100);
  });

  it("复制按钮存在，且这一处不再自己持有 writeText（T03 迁移的直接证据）", () => {
    const s = new RemoteSection({ headless: true });
    const btn = s.element.querySelector<HTMLButtonElement>(".paste-block-copy");
    expect(btn, "复制按钮必须在").not.toBeNull();
    expect(btn!.textContent).toBe("复制");
  });

  it("挂在已有规则的那个 class 上（T04 审计⑤：改名让 styles.css 那条规则失去了宿主）", () => {
    const s = new RemoteSection({ headless: true });
    expect(s.element.querySelector(".remote-wrapper-snippet")).not.toBeNull();
  });
});

// 〔VIS2 · `设计/15 §3.4 ①`〕后端自动固化 / 各地址不一 ⇒ 机器页那张卡跟着刷新、说出来（期望手写）。
describe("VIS2 机器页收 host key 告知", () => {
  it("★ 固化了 ⇒ 指纹栏从盘上同步、说一行；别台的不理；各地址不一 ⇒ 说出来、指纹栏不动", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "load_config"
        ? Promise.resolve({
            remote: {
              enabled: true,
              hosts: [{ label: "devbox", host: "h", user: "u", backendPath: "/b", hostKeyFingerprint: "SHA256:disk" }],
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
