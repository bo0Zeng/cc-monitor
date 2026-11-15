// 快捷键的作用范围：单键只在主区 / 标签页栏、没有浮层与输入焦点时生效；带修饰键的随时；
// 「主区」那几个只在主区；撤销只在没有输入焦点时；菜单键是 Shift+F10 的另一个固定键。
// 期望取自设计的那张表（动作 · 默认键 · 何时生效），不取自 `ACTIONS` 里写的 scope。
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { dispatcher, KeybindingDispatcher, type OverlayHandle } from "../../../../src/frontend/ui/keybindings/registry";
import { findAction, type ActionId } from "../../../../src/frontend/ui/keybindings/actions";
import { KeysOverview } from "../../../../src/frontend/ui/views/keys-overview";
import { copyText } from "../../../../src/frontend/ui/copy-table";

let d: KeybindingDispatcher;
let fired: string[];
const ids: ActionId[] = ["panel.toggle-tasks", "app.open-command-bar", "app.undo", "session.to-bottom", "tab.context-menu", "session.prev-turn", "tabBar.move-up", "tabBar.rename", "tabBar.focus-cycle"];

function at(where: "body" | "input" | "tabs" | "status" | "stream" | "head" | "composer"): void {
  const el = document.getElementById(`w-${where}`);
  if (where === "body") (document.activeElement as HTMLElement | null)?.blur();
  else el!.focus();
}
function press(code: string, mods: KeyboardEventInit = {}): KeyboardEvent {
  const e = new KeyboardEvent("keydown", { code, key: code, bubbles: true, cancelable: true, ...mods });
  (document.activeElement ?? document.body).dispatchEvent(e);
  return e;
}

beforeEach(() => {
  document.body.innerHTML = `
    <div id="tab-bar"><button id="w-tabs">tab</button></div>
    <div id="session-head"><button id="w-head">head</button></div>
    <main id="message-stream" tabindex="-1"><div id="w-stream" tabindex="-1"></div>
      <div data-role="composer"><textarea id="w-composer"></textarea></div></main>
    <div id="status-bar"><button id="w-status">chip</button></div>
    <input id="w-input" type="text" />`;
  // jsdom 没有布局：看不见的输入框不算「在打字」（registry 那道判），这里给它一个框。
  for (const id of ["w-input", "w-composer"]) document.getElementById(id)!.getClientRects = () => [new DOMRect(0, 0, 10, 10)] as unknown as DOMRectList;
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

  it("🔴 主区那几个（Alt+↑）：焦点在标签页栏 / 状态栏 / 输入框上不放行（栏里的 Alt+↑ 归栏）", () => {
    for (const w of ["stream", "tabs", "status", "input", "composer"] as const) {
      at(w);
      press("ArrowUp", { altKey: true });
    }
    expect(fired).toEqual(["session.prev-turn", "tabBar.move-up"]);
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

// 回到底部：Ctrl+End，焦点在主区（消息流 · 会话头 · 没焦点）或会话的输入框里都认；输入框里这一下被吃掉（光标不动）。
// 别的会打字的地方（标签页改名 · 历史搜索）· 标签页栏 · 状态栏不认，也不吃掉（那里的 Ctrl+End 归那一格自己）。
describe("快捷键 · 回到底部（Ctrl+End）", () => {
  it("🔴 主区三处与输入框里都触发；输入框里那一下被 preventDefault", () => {
    const eaten: boolean[] = [];
    for (const w of ["stream", "head", "body", "composer"] as const) {
      at(w);
      eaten.push(press("End", { ctrlKey: true }).defaultPrevented);
    }
    expect(fired).toEqual(["session.to-bottom", "session.to-bottom", "session.to-bottom", "session.to-bottom"]);
    expect(eaten).toEqual([true, true, true, true]);
  });

  it("🔴 别的输入框 · 标签页栏 · 状态栏上不触发、不吃掉；单按 End 哪里都不触发", () => {
    const eaten: boolean[] = [];
    for (const w of ["input", "tabs", "status"] as const) {
      at(w);
      eaten.push(press("End", { ctrlKey: true }).defaultPrevented);
    }
    for (const w of ["stream", "body", "composer"] as const) {
      at(w);
      press("End");
    }
    expect(fired).toEqual([]);
    expect(eaten).toEqual([false, false, false]);
  });

  it("🔴 键位总览里回到底部那一行写自己那一档（输入框里也认），不与「主区」那一档共用一个名字", () => {
    const ko = new KeysOverview({ editKeys: () => {} });
    ko.open();
    try {
      const want = findAction("session.to-bottom")!.label;
      const row = [...document.querySelectorAll<HTMLElement>('[data-role="keys-row"]')].find((r) => r.firstElementChild?.textContent === want);
      expect(row, "总览里有回到底部那一行").toBeDefined();
      expect(row!.lastElementChild?.textContent).toBe(copyText("keys.scope.session"));
      expect(copyText("keys.scope.session")).not.toBe(copyText("keys.scope.main"));
    } finally {
      ko.toggle();
    }
  });

  it("🔴 改成不带 Ctrl / Alt / ⌘ 的键（End）⇒ 只在主区认，输入框里那一下归输入框；键位总览那一档跟着写「主区」", () => {
    d.setOverride("session.to-bottom", "End");
    at("composer");
    const inBox = press("End");
    at("stream");
    press("End");
    expect(fired).toEqual(["session.to-bottom"]);
    expect(inBox.defaultPrevented).toBe(false);

    dispatcher.setOverride("session.to-bottom", "End");
    const ko = new KeysOverview({ editKeys: () => {} });
    ko.open();
    try {
      const want = findAction("session.to-bottom")!.label;
      const row = [...document.querySelectorAll<HTMLElement>('[data-role="keys-row"]')].find((r) => r.firstElementChild?.textContent === want);
      expect(row!.lastElementChild?.textContent).toBe(copyText("keys.scope.main"));
    } finally {
      ko.toggle();
      dispatcher.setOverride("session.to-bottom", "");
    }
  });

  it("🔴 输入框里别的几档照旧不放行：Ctrl+Z（无输入焦点）· Alt+↑（主区）· T（单键）· F2（栏）", () => {
    at("composer");
    press("KeyZ", { ctrlKey: true });
    press("ArrowUp", { altKey: true });
    press("KeyT");
    press("F2");
    expect(fired).toEqual([]);
  });
});

// 一个键可以分给作用范围不相交的几个动作：Alt+↑ 在主区是上一轮、在标签页栏里是往上挪一格；按下时按焦点在哪取第一个放行的。
describe("快捷键 · 一个键分给几个作用范围", () => {
  it("🔴 Alt+↑：焦点在栏里 ⇒ 往上挪；在主区 ⇒ 上一轮；在状态栏 / 输入框 ⇒ 都不", () => {
    for (const w of ["tabs", "stream", "body", "status", "input"] as const) {
      at(w);
      press("ArrowUp", { altKey: true });
    }
    expect(fired).toEqual(["tabBar.move-up", "session.prev-turn", "session.prev-turn"]);
  });

  it("🔴 「栏」那一档只在焦点在标签页栏里放行（F2 改分组名）", () => {
    for (const w of ["tabs", "stream", "body", "status", "input"] as const) {
      at(w);
      press("F2");
    }
    expect(fired).toEqual(["tabBar.rename"]);
  });

  it("🔴 主区那一条改了键，栏那一条照旧（不连带）", () => {
    d.setOverride("session.prev-turn", "Ctrl+ArrowUp");
    at("tabs");
    press("ArrowUp", { altKey: true });
    at("stream");
    press("ArrowUp", { altKey: true });
    press("ArrowUp", { ctrlKey: true });
    expect(fired).toEqual(["tabBar.move-up", "session.prev-turn"]);
  });

  it("🔴 谁占着这个键：只算作用范围相交的（编辑器的撞键确认据此）", () => {
    expect(d.whoOwns("Alt+ArrowUp", "bar")).toBe("tabBar.move-up");
    expect(d.whoOwns("Alt+ArrowUp", "main")).toBe("session.prev-turn");
    expect(d.whoOwns("Alt+ArrowUp", "idle"), "「没有输入焦点时」与两边都相交").not.toBeNull();
    expect(d.whoOwns("F2", "main"), "F2 只在栏里").toBeNull();
    expect(d.whoOwns("F2", "nav")).toBe("tabBar.rename");
  });

  it("🔴 F6：主区 ↔ 栏轮转那一键随时放行（输入框里也能跳出来）", () => {
    for (const w of ["tabs", "stream", "input"] as const) {
      at(w);
      press("F6");
    }
    expect(fired).toEqual(["tabBar.focus-cycle", "tabBar.focus-cycle", "tabBar.focus-cycle"]);
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
