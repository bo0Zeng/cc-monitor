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
