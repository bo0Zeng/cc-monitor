// 机器页切到「别名与配置文件」那一栏 ⇒ 告诉那一栏里带 `data-config-shown` 的块（它们第一次收到才问那台）；切到「账号」不告诉。
import { describe, it, expect } from "vitest";
import { buildMachinePage } from "../../../../src/frontend/ui/settings/machine-page";
import { CONFIG_SHOWN_EVENT } from "../../../../src/frontend/ui/settings/events";

describe("机器页 · 别名与配置文件那一栏露出来", () => {
  it("切到那一栏才派事件、每切一次派一次；切到账号栏不派", () => {
    const page = buildMachinePage({ pageId: "machine:devbox", name: "devbox", meta: "", ccMonitor: [], menu: () => [] });
    document.body.replaceChildren(page.element);
    const block = document.createElement("div");
    block.dataset.configShown = "";
    const inner = document.createElement("div");
    inner.appendChild(block);
    page.slots.config.appendChild(inner);
    let n = 0;
    block.addEventListener(CONFIG_SHOWN_EVENT, () => (n += 1));
    page.tabs.navigate("machine:devbox#acct");
    expect(n).toBe(0);
    page.tabs.navigate("machine:devbox#config");
    expect(n).toBe(1);
    page.tabs.navigate("machine:devbox#acct");
    page.tabs.navigate("machine:devbox#config");
    expect(n).toBe(2);
  });
});
