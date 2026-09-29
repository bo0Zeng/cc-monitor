/**
 * 〔CFG1 · 4D〕tab 栏宽度收进 tab 栏自己的模块、经存储接入层（D §D5）。
 *
 * 守的要求（住址）：`INVARIANTS §14`「前端任何持久化到 localStorage / IndexedDB 的 key 必须以 `cc-monitor.` 开头」
 * 与它的附带契约（写共享 key 要在接入层 `LS_KEYS` 旁边被审视）· `设计/30 §1`「显式宽度 = `--tab-bar-w`」。
 */
import { describe, it, expect, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { LS_KEYS } from "../../../src/frontend/ui/local-storage";
import { mountTabBarResizer } from "../../../src/frontend/ui/tab-bar-width";

function freshApp(): HTMLElement {
  document.body.innerHTML = '<div id="app"><div id="tab-bar"></div></div>';
  return document.getElementById("app")!;
}

describe("CFG1 · tab 栏宽度", () => {
  beforeEach(() => localStorage.clear());

  it("拖一下松手 ⇒ 宽度写进 `LS_KEYS.tabBarWidth`，`--tab-bar-w` 同步；再挂一次读回来（夹在上下界里）", () => {
    const app = freshApp();
    mountTabBarResizer();
    const r = document.getElementById("tab-bar-resizer")!;
    r.dispatchEvent(new MouseEvent("mousedown", { button: 0, clientX: 200, bubbles: true }));
    document.dispatchEvent(new MouseEvent("mousemove", { buttons: 1, clientX: 250 }));
    document.dispatchEvent(new MouseEvent("mouseup", {}));
    expect(localStorage.getItem(LS_KEYS.tabBarWidth)).toBe("250");
    expect(app.style.getPropertyValue("--tab-bar-w")).toBe("250px");

    localStorage.setItem(LS_KEYS.tabBarWidth, "9999"); // 盘上一个越界值
    const app2 = freshApp();
    mountTabBarResizer();
    expect(app2.style.getPropertyValue("--tab-bar-w")).toBe("340px");
  });

  it("localStorage 抛（私密模式 / 配额）⇒ 不抛出去，栏照常挂上", () => {
    const orig = Storage.prototype.getItem;
    Storage.prototype.getItem = () => {
      throw new Error("denied");
    };
    try {
      freshApp();
      expect(() => mountTabBarResizer()).not.toThrow();
      expect(document.getElementById("tab-bar-resizer")).not.toBeNull();
    } finally {
      Storage.prototype.getItem = orig;
    }
  });

  it("`main.ts` 与 `tab-bar-width.ts` 里零处裸 `localStorage.get/setItem`（正控：`local-storage.ts` 里有）", () => {
    const raw = /\blocalStorage\.(?:getItem|setItem|removeItem)\(/;
    expect(raw.test(readFileSync("src/frontend/ui/local-storage.ts", "utf8")), "正控没命中 —— 正则坏了").toBe(true);
    for (const f of ["src/frontend/ui/main.ts", "src/frontend/ui/tab-bar-width.ts"]) {
      expect(raw.test(readFileSync(f, "utf8")), `${f} 里又直调 localStorage 了`).toBe(false);
    }
  });
});
