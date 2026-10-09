// 快捷键的作用范围：单键只在主区 / 标签页栏、没有浮层与输入焦点时生效；带修饰键的随时；
// 「主区」那几个只在主区；撤销只在没有输入焦点时；菜单键是 Shift+F10 的另一个固定键。
// 期望取自设计的那张表（动作 · 默认键 · 何时生效），不取自 `ACTIONS` 里写的 scope。
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { KeybindingDispatcher, type OverlayHandle } from "../../../../src/frontend/ui/keybindings/registry";
import type { ActionId } from "../../../../src/frontend/ui/keybindings/actions";

let d: KeybindingDispatcher;
let fired: string[];
const ids: ActionId[] = ["panel.toggle-tasks", "app.open-command-bar", "app.undo", "session.to-bottom", "tab.context-menu", "session.prev-turn"];

function at(where: "body" | "input" | "tabs" | "status" | "stream"): void {
  const el = document.getElementById(`w-${where}`);
  if (where === "body") (document.activeElement as HTMLElement | null)?.blur();
  else el!.focus();
}
function press(code: string, mods: KeyboardEventInit = {}): void {
  (document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent("keydown", { code, key: code, bubbles: true, cancelable: true, ...mods }));
}

beforeEach(() => {
  document.body.innerHTML = `
    <div id="tab-bar"><button id="w-tabs">tab</button></div>
    <main id="message-stream" tabindex="-1"><div id="w-stream" tabindex="-1"></div></main>
    <div id="status-bar"><button id="w-status">chip</button></div>
    <input id="w-input" type="text" />`;
  // jsdom 没有布局：看不见的输入框不算「在打字」（registry 那道判），这里给它一个框。
  const input = document.getElementById("w-input")!;
  input.getClientRects = () => [new DOMRect(0, 0, 10, 10)] as unknown as DOMRectList;
  d = new KeybindingDispatcher();
  fired = [];
  for (const id of ids) d.bind(id, () => fired.push(id));
  d.applyOverrides({});
  d.start();
});
afterEach(() => {
  window.removeEventListener("keydown", (d as unknown as { onKeyDown: EventListener }).onKeyDown, true);
});

describe("快捷键 · 作用范围", () => {
  it("🔴 单键：主区与标签页栏放行；输入框、状态栏按钮上不放行", () => {
    for (const w of ["body", "stream", "tabs", "input", "status"] as const) {
      at(w);
      press("KeyT");
    }
    expect(fired).toEqual(["panel.toggle-tasks", "panel.toggle-tasks", "panel.toggle-tasks"]);
  });

  it("🔴 单键：上面压着一层浮层 ⇒ 不放行；那一层自己声明要的 ⇒ 放行", () => {
    const layer: OverlayHandle = { handleEsc: () => true };
    d.pushOverlay(layer);
    press("KeyT");
    expect(fired).toEqual([]);
    d.popOverlay(layer);
    d.pushOverlay({ handleEsc: () => true, passes: ["panel.toggle-tasks"] });
    press("KeyT");
    expect(fired).toEqual(["panel.toggle-tasks"]);
  });

  it("🔴 带修饰键的随时：输入框里 Ctrl+K 也打开命令面板；单键动作改成带修饰键的键 ⇒ 也随时", () => {
    at("input");
    press("KeyK", { ctrlKey: true });
    d.setOverride("panel.toggle-tasks", "Ctrl+KeyT");
    press("KeyT", { ctrlKey: true });
    expect(fired).toEqual(["app.open-command-bar", "panel.toggle-tasks"]);
  });

  it("🔴 撤销：没有输入焦点时才是我们的（输入框里的 Ctrl+Z 归输入框）", () => {
    at("input");
    press("KeyZ", { ctrlKey: true });
    expect(fired).toEqual([]);
    at("stream");
    press("KeyZ", { ctrlKey: true });
    at("status");
    press("KeyZ", { ctrlKey: true });
    expect(fired).toEqual(["app.undo", "app.undo"]);
  });

  it("🔴 主区那几个（End · Alt+↑）：焦点在标签页栏 / 状态栏上不放行", () => {
    for (const w of ["stream", "tabs", "status", "input"] as const) {
      at(w);
      press("End");
      press("ArrowUp", { altKey: true });
    }
    expect(fired).toEqual(["session.to-bottom", "session.prev-turn"]);
  });

  it("🔴 右键菜单：Shift+F10 与菜单键都开；主区或标签页栏放行，输入框里不放行", () => {
    at("tabs");
    press("F10", { shiftKey: true });
    press("ContextMenu");
    at("input");
    press("ContextMenu");
    expect(fired).toEqual(["tab.context-menu", "tab.context-menu"]);
  });
});

// 按住「下一个 tab」这一类单键：每一下都要先判「焦点在不在输入框里」。那一判以前一上来就对焦点元素读 getClientRects ——
// 读几何会逼浏览器当场把上一下切出来的那个 tab 整个排版（还没画就要排；按住切时每一下都排一遍，台架 WebKitGTK 读数里这一项最大）。
// 焦点根本不是会打字的元素（没焦点 · 标签页栏 · 消息流 · 状态栏的按钮）⇒ 不读几何；只有会打字的那几种才看它有没有渲染盒。
describe("快捷键 · 判输入焦点不读几何", () => {
  it("焦点不在会打字的元素上（没焦点 / 标签页栏 / 消息流 / 状态栏）⇒ 一次 getClientRects 都不调", () => {
    let reads = 0;
    const orig = Element.prototype.getClientRects;
    Element.prototype.getClientRects = function (this: Element) {
      reads++;
      return orig.call(this);
    };
    try {
      for (const w of ["body", "tabs", "stream", "status"] as const) {
        at(w);
        press("KeyT");
      }
    } finally {
      Element.prototype.getClientRects = orig;
    }
    expect(reads, "判一下焦点就逼一次排版 ⇒ 按住单键切 tab 时每一下都把上一个 tab 排一遍").toBe(0);
  });
});
