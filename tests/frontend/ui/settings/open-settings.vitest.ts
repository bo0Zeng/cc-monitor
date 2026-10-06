// ST1「点设置有反馈」：在路上说在路上、同一时刻只一趟、失败说出来。
import { describe, it, expect, vi, beforeEach } from "vitest";

const { cmd, toast } = vi.hoisted(() => ({
  cmd: { open: vi.fn() },
  toast: vi.fn(),
}));
vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: { open_settings_window: (t: string | null) => cmd.open(t) },
}));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ toast: toast }));

import { openSettingsWindow, parseSettingsTarget } from "../../../../src/frontend/ui/settings/open-settings";
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

  // 主窗口打开设置的每个入口都带目的地（`settings-dest.ts` 头注那张表）；只有「设置」按钮 · 命令面板「设置」· 快捷键三处不带。
  it("主窗那几个入口全走它：`main.ts` 里裸的 `commands.open_settings_window()` 零处；不带目的地的恰好三处，其余每处都给 `settings-dest` 里的目的地", () => {
    const code = readFileSync("src/frontend/ui/main.ts", "utf8")
      .split("\n")
      .filter((l) => !l.trim().startsWith("//"))
      .join("\n");
    expect([...code.matchAll(/commands\.open_settings_window\(/g)].length).toBe(0);
    const calls = [...code.matchAll(/\bopenSettingsWindow\(([^)]*\)?)/g)].map((m) => m[1]);
    const bare: string[] = calls.filter((a) => a === ")" || a === "settingsTrigger)");
    expect(bare.length, "不带目的地的：栏顶「设置」· 命令面板「设置」· 快捷键").toBe(3);
    const rest = calls.filter((a) => !bare.includes(a));
    expect(rest.length, "带目的地的入口（账号 chip · 命令面板管理账号 · 账号面板 · ↗ 接上终端 · 上下文 · 快捷键一览 · 机器看不见）").toBe(7);
    for (const a of rest) expect(a, "带目的地的那几处都取 settings-dest 的").toMatch(/^undefined, dest\./);
  });

  it("目的地：原样（JSON）交给壳；不带就是 null；设置窗那一侧只收四格里的非空串，别的忽略", async () => {
    cmd.open.mockResolvedValue(undefined);
    await openSettingsWindow(null, { machine: "devbox", tab: "acct" });
    expect(cmd.open).toHaveBeenLastCalledWith('{"machine":"devbox","tab":"acct"}');
    await openSettingsWindow();
    expect(cmd.open).toHaveBeenLastCalledWith(null);
    expect(parseSettingsTarget('{"page":"logs","machine":" ","extra":1,"tab":2}')).toEqual({ page: "logs" });
    expect(parseSettingsTarget({ anchor: "resume" })).toEqual({ anchor: "resume" });
    for (const bad of ["{", "[]", 3, null, undefined]) expect(parseSettingsTarget(bad)).toBeNull();
  });

  it("本模块不 import 设置面板（主窗的模块图里不该有它）", () => {
    const src = readFileSync("src/frontend/ui/settings/open-settings.ts", "utf8");
    const imports = [...src.matchAll(/^import .* from "([^"]+)";/gm)].map((m) => m[1]).sort();
    // + 取文口（失败 toast 那句进了文案表）—— 它不是设置面板。
    expect(imports).toEqual(["../copy-table", "../ipc/commands", "../kit/toast"]);
  });
});
