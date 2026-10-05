// 设置 → 通用 → 恢复 →「终端」那一行：画的是壳交来的事实（自动 · 探到的 · 自定义），存的是 config.json 的 `terminal`。
import { describe, it, expect, vi, beforeEach } from "vitest";

const ipc = vi.hoisted(() => ({ choices: vi.fn(), patch: vi.fn() }));
vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: { terminal_choices: ipc.choices, patch_config: ipc.patch },
}));

import { buildTerminalRow, optionsOf, selectedOf } from "../../../../src/frontend/ui/settings/terminal-row";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const facts = (auto: string | null, found: string[], setting = "") => ({ applies: true, auto, found, setting });
const tick = () => new Promise((r) => setTimeout(r, 0));
const selectOf = (el: HTMLElement) => el.querySelector<HTMLButtonElement>("[data-role=terminal-select]")!;
const customOf = (el: HTMLElement) => el.querySelector<HTMLInputElement>("[data-role=terminal-custom]")!;
const choose = (b: HTMLButtonElement, label: string): void => {
  b.click();
  [...document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]')].find((i) => (i.textContent ?? "").startsWith(label))!.click();
};

beforeEach(() => {
  ipc.choices.mockReset();
  ipc.patch.mockReset().mockResolvedValue(undefined);
  document.body.replaceChildren();
});

describe("终端那一行", () => {
  it("Windows（壳说不用挑）⇒ 不出现", async () => {
    ipc.choices.mockResolvedValue({ applies: false, auto: null, found: [], setting: "" });
    expect(await buildTerminalRow()).toBeNull();
  });

  it("下拉几项：自动（灰字是它会挑谁 / 未找到）· 探到的各个 · 自定义…；停在哪一项照设置", async () => {
    expect(optionsOf(facts("ptyxis", ["ptyxis", "xterm"])).map((o) => [o.value === "" ? "" : o.label, o.note ?? ""])).toEqual([
      ["", "ptyxis"],
      ["ptyxis", ""],
      ["xterm", ""],
      [copyText("settingsPanel.terminal.custom"), ""],
    ]);
    expect(optionsOf(facts(null, []))[0].note).toBe(copyText("settingsPanel.terminal.autoNone"));
    expect(selectedOf(facts("ptyxis", ["ptyxis", "xterm"], "xterm")).value).toBe("xterm");
    expect(selectedOf(facts("ptyxis", ["ptyxis"], "wezterm start --")).custom).toBe("wezterm start --");

    ipc.choices.mockResolvedValue(facts("ptyxis", ["ptyxis"], "wezterm start --"));
    const el = (await buildTerminalRow())!;
    expect(customOf(el).hidden).toBe(false);
    expect(customOf(el).value).toBe("wezterm start --");
    expect(el.dataset.anchor, "设置入口直达这一格").toBe("terminal");
  });

  it("选一个 ⇒ 存它；回到自动 ⇒ 删掉那一键；自定义填了才存；存失败 ⇒ 行下一句", async () => {
    ipc.choices.mockResolvedValue(facts("ptyxis", ["ptyxis", "xterm"]));
    const el = (await buildTerminalRow())!;
    document.body.appendChild(el);
    choose(selectOf(el), "xterm");
    choose(selectOf(el), copyText("settingsPanel.terminal.auto"));
    await tick();
    expect(ipc.patch.mock.calls.map((c) => c[0])).toEqual([
      { edits: [{ op: "set", path: ["terminal"], value: "xterm" }] },
      { edits: [{ op: "remove", path: ["terminal"] }] },
    ]);
    ipc.patch.mockClear();
    choose(selectOf(el), copyText("settingsPanel.terminal.custom"));
    expect(ipc.patch).not.toHaveBeenCalled();
    expect(customOf(el).hidden).toBe(false);
    customOf(el).value = "  kitty  ";
    customOf(el).dispatchEvent(new Event("change"));
    await tick();
    expect(ipc.patch.mock.calls.map((c) => c[0])).toEqual([{ edits: [{ op: "set", path: ["terminal"], value: "kitty" }] }]);

    ipc.patch.mockRejectedValue(new Error("disk full"));
    choose(selectOf(el), "ptyxis");
    await tick();
    const err = el.querySelector<HTMLElement>(".settings-row-error")!;
    expect(err.hidden).toBe(false);
    expect(err.textContent).toBe(copyText("settings.behavior.saveFailedLine", { why: "Error: disk full" }));
  });
});
