/**
 * N22：快捷键单条「恢复默认」走同一条撞键确认（默认键已被别的动作占着 ⇒ 先问）；「已改 N 项」随每次改动实时更新（不等关编辑器）。
 */
import { describe, expect, it, vi, beforeEach } from "vitest";

const confirms: Array<{ title: string }> = [];
let answer = true;
vi.mock("../../../../src/frontend/ui/kit/dialog", () => ({
  confirmDialog: (o: { title: string }) => {
    confirms.push(o);
    return Promise.resolve(answer);
  },
}));
vi.mock("../../../../src/frontend/ui/keybindings/store", () => ({ setKeybindings: () => Promise.resolve() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: () => Promise.resolve() }));

import { KeybindingsEditor } from "../../../../src/frontend/ui/keybindings/editor";
import { dispatcher } from "../../../../src/frontend/ui/keybindings/registry";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 8; i += 1) await Promise.resolve();
};

function resetBtnOf(id: string): HTMLButtonElement {
  return document.querySelector<HTMLButtonElement>(`[data-action-id="${id}"] .kb-editor-btn-reset`)!;
}

describe("快捷键编辑器 · 单条恢复默认", () => {
  beforeEach(() => {
    confirms.length = 0;
    answer = true;
    document.body.replaceChildren();
    dispatcher.applyOverrides({});
  });

  it("★ 默认键被别的动作占着 ⇒ 先问；答否 ⇒ 两条都不动；答是 ⇒ 占着的那条解绑、这条回默认", async () => {
    let changes = 0;
    new KeybindingsEditor({ onChange: () => (changes += 1) });
    // tab.next 默认 BracketRight；把它让给 tab.prev，tab.next 改成别的。
    dispatcher.setOverride("tab.next", "KeyQ");
    dispatcher.setOverride("tab.prev", "BracketRight");
    answer = false;
    resetBtnOf("tab.next").click();
    await flush();
    expect(confirms.length).toBe(1);
    expect(dispatcher.effectiveChord("tab.next")).toBe("KeyQ");
    expect(dispatcher.effectiveChord("tab.prev")).toBe("BracketRight");
    answer = true;
    resetBtnOf("tab.next").click();
    await flush();
    expect(confirms.length).toBe(2);
    expect(dispatcher.effectiveChord("tab.next")).toBe("BracketRight");
    expect(dispatcher.effectiveChord("tab.prev")).toBeNull();
    expect(changes).toBeGreaterThan(0);
  });

  it("★ 同一个键在作用范围不相交的动作上不算撞（Alt+↑：主区上一轮 · 栏里往上挪）⇒ 不问，两条都留着", async () => {
    new KeybindingsEditor({ onChange: () => {} });
    dispatcher.setOverride("tabBar.move-up", "KeyQ");
    resetBtnOf("tabBar.move-up").click();
    await flush();
    expect(confirms.length).toBe(0);
    expect(dispatcher.effectiveChord("tabBar.move-up")).toBe("Alt+ArrowUp");
    expect(dispatcher.effectiveChord("session.prev-turn")).toBe("Alt+ArrowUp");
  });

  it("★ 默认键没人占 ⇒ 不问，直接回默认；每次改动都通知外面重数「已改 N 项」", async () => {
    let changes = 0;
    new KeybindingsEditor({ onChange: () => (changes += 1) });
    dispatcher.setOverride("tab.next", "KeyQ");
    resetBtnOf("tab.next").click();
    await flush();
    expect(confirms.length).toBe(0);
    expect(dispatcher.effectiveChord("tab.next")).toBe("BracketRight");
    expect(changes).toBe(1);
  });
});

describe("快捷键编辑器 · 恢复默认那颗是纯图标按钮", () => {
  it("代码画的 reset 图标、不带字；读屏名与悬停同一句", async () => {
    const { copyText } = await import("../../../../src/frontend/ui/copy-table");
    document.body.replaceChildren();
    new KeybindingsEditor({ onChange: () => {} });
    const b = resetBtnOf("tab.next");
    expect(b.textContent).toBe("");
    expect(b.querySelector<SVGElement>("svg")?.dataset.icon).toBe("reset");
    expect([b.getAttribute("aria-label"), b.title]).toEqual([copyText("keybindingEditor.buildActionRow.resetHint"), copyText("keybindingEditor.buildActionRow.resetHint")]);
  });
});
