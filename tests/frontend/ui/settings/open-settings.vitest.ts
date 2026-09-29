// ST1「点设置有反馈」（`设计/70 §1.2` ④ · `§1.3 E`）：在路上说在路上、同一时刻只一趟、失败说出来。
import { describe, it, expect, vi, beforeEach } from "vitest";

const { cmd, toast } = vi.hoisted(() => ({
  cmd: { open: vi.fn() },
  toast: vi.fn(),
}));
vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: { open_settings_window: () => cmd.open() },
}));
vi.mock("../../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: toast }));

import { openSettingsWindow } from "../../../../src/frontend/ui/settings/open-settings";
import { readFileSync } from "node:fs";

describe("ST1 点设置有反馈", () => {
  beforeEach(() => {
    cmd.open.mockReset();
    toast.mockReset();
    document.documentElement.style.cursor = "";
  });

  it("在路上：按钮灰 ＋ aria-busy、光标 progress；再点（任一入口）并进同一趟；回来全复原", async () => {
    let release!: () => void;
    cmd.open.mockReturnValue(new Promise<void>((r) => (release = r)));
    const btn = document.createElement("button");
    const a = openSettingsWindow(btn);
    expect(btn.disabled).toBe(true);
    expect(btn.getAttribute("aria-busy")).toBe("true");
    expect(document.documentElement.style.cursor).toBe("progress");
    const b = openSettingsWindow(); // 快捷键 / 命令面板那一路
    expect(b).toBe(a);
    expect(cmd.open, "在路上又建了一次窗").toHaveBeenCalledTimes(1);
    release();
    await a;
    expect(btn.disabled).toBe(false);
    expect(btn.hasAttribute("aria-busy")).toBe(false);
    expect(document.documentElement.style.cursor).toBe("");
    // 回来之后再点是新的一趟（不是永远并进旧的）。
    cmd.open.mockResolvedValue(undefined);
    await openSettingsWindow();
    expect(cmd.open).toHaveBeenCalledTimes(2);
  });

  it("失败：落在一条提示上（不再是未捕获 rejection），按钮复原", async () => {
    cmd.open.mockRejectedValue(new Error("建窗失败"));
    const btn = document.createElement("button");
    await expect(openSettingsWindow(btn)).resolves.toBeUndefined();
    expect(toast).toHaveBeenCalledTimes(1);
    expect(String(toast.mock.calls[0][1])).toContain("建窗失败");
    expect(btn.disabled).toBe(false);
  });

  it("主窗那六个入口全走它：`main.ts` 里裸的 `commands.open_settings_window()` 零处、helper 调用 6 处", () => {
    const code = readFileSync("src/frontend/ui/main.ts", "utf8")
      .split("\n")
      .filter((l) => !l.trim().startsWith("//"))
      .join("\n");
    expect([...code.matchAll(/commands\.open_settings_window\(/g)].length).toBe(0);
    expect([...code.matchAll(/\bopenSettingsWindow\(/g)].length).toBe(6);
  });

  it("本模块不 import 设置面板（主窗的模块图里不该有它）", () => {
    const src = readFileSync("src/frontend/ui/settings/open-settings.ts", "utf8");
    const imports = [...src.matchAll(/^import .* from "([^"]+)";/gm)].map((m) => m[1]).sort();
    // 〔CP2b〕+ 取文口（失败 toast 那句进了文案表）—— 它不是设置面板。
    expect(imports).toEqual(["../copy-table", "../error-toast", "../ipc/commands"]);
  });
});
