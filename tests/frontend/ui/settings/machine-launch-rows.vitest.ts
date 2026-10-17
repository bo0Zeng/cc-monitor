/**
 * 每台「这台上的 cc-monitor」里「怎么起会话」那两格（本机远端同一个组件，按 origin 问那台后端 `prefs-read` / `prefs-set`）：
 * - 恢复会话的启动程序：下拉（空 ＝ 那一家的默认启动器 · 用过的 · 存着的 · 自定义…）；读回那台存的；选了只交 `launchers` 默认那一家那一格；
 *   用过的记进界面那份预设；存失败退回读回的值 ＋ 行下一句。
 * - 恢复到 tmux 里：开关，只交 `resumeInTmux`。
 * - 那台读不懂 ⇒ 两格禁用、行下是那台写好的那一句；那台问不到 ⇒ 禁用、说读不到。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

type P = { launchers: Record<string, string>; resumeInTmux: boolean; models: Record<string, string>; permissionModes: Record<string, string>; contextLimits: Record<string, number> };
const { machines, sets, disk, emitted, modes } = vi.hoisted(() => ({
  machines: new Map<string, { prefs: P; state?: string; reason?: string } | Error>(),
  /** 那台 `session-open-facts` 里默认那一家的权限档。 */
  modes: new Map<string, { value: string; text: string }[]>(),
  sets: [] as [string, unknown][],
  disk: { cfg: {} as Record<string, unknown>, failSet: null as Error | null },
  emitted: [] as string[],
}));
const reply = (m: { prefs: P; state?: string; reason?: string }) => ({
  state: m.state ?? "ok",
  reason: m.reason ?? null,
  detail: null,
  path: null,
  prefs: structuredClone(m.prefs),
});
vi.mock("../../../../src/frontend/ui/machine-prefs", () => ({
  readPrefs: async (origin: string) => {
    const m = machines.get(origin);
    if (!m || m instanceof Error) throw m ?? new Error("没有这台");
    return reply(m);
  },
  setPrefs: async (origin: string, patch: Partial<P>) => {
    if (disk.failSet) throw disk.failSet;
    sets.push([origin, patch]);
    const m = machines.get(origin) as { prefs: P };
    for (const [k, v] of Object.entries(patch.launchers ?? {})) {
      if (v === "") delete m.prefs.launchers[k];
      else m.prefs.launchers[k] = v;
    }
    if (patch.resumeInTmux !== undefined) m.prefs.resumeInTmux = patch.resumeInTmux;
    for (const [k, v] of Object.entries((patch as { permissionModes?: Record<string, string | null> }).permissionModes ?? {})) {
      if (v === null) delete m.prefs.permissionModes[k];
      else m.prefs.permissionModes[k] = v;
    }
    return reply(m);
  },
}));
vi.mock("../../../../src/frontend/ui/new-session-reads", async () => {
  const { DEFAULT_AGENT: kind } = await import("../../../../src/frontend/ui/generated/agent-profile-table");
  return {
    askFacts: async (origin: string) => ({ agents: [{ kind, permissionModes: modes.get(origin) ?? [] }] }),
  };
});
vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: {
    load_config: () => Promise.resolve(structuredClone(disk.cfg)),
    patch_config: (a: { edits: { path: string[]; value: unknown }[] }) => {
      for (const e of a.edits) disk.cfg[e.path[0]] = e.value;
      return Promise.resolve();
    },
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(async (n: string) => void emitted.push(n)), listen: vi.fn() }));

import { machineLaunchRows } from "../../../../src/frontend/ui/settings/machine-launch-rows";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { AGENT_PROFILE_TABLE, DEFAULT_AGENT } from "../../../../src/frontend/ui/generated/agent-profile-table";

const settle = () => new Promise((r) => setTimeout(r, 0));
const optionLabels = (b: HTMLButtonElement): string[] => {
  b.click();
  const got = [...document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]')].map((i) => i.textContent ?? "");
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  b.click();
  return got;
};
const choose = (b: HTMLButtonElement, label: string): void => {
  b.click();
  [...document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]')].find((i) => i.textContent?.startsWith(label))!.click();
};
const mount = (origin: string): HTMLElement => {
  const host = document.createElement("div");
  const m = machineLaunchRows(origin, origin);
  host.append(...m.rows);
  document.body.replaceChildren(host);
  m.load();
  return host;
};
const blank = (): P => ({ launchers: {}, resumeInTmux: false, models: {}, permissionModes: {}, contextLimits: {} });
const launcher = AGENT_PROFILE_TABLE.find((r) => r.agent === DEFAULT_AGENT)!.defaultLauncher;

beforeEach(() => {
  machines.clear();
  machines.set("devbox", { prefs: { ...blank(), launchers: { [DEFAULT_AGENT]: "cc" }, resumeInTmux: true } });
  machines.set("gpu", { prefs: blank() });
  sets.length = 0;
  emitted.length = 0;
  disk.cfg = { resumeCommandPresets: ["ccm --tmux"] };
  disk.failSet = null;
  modes.clear();
  modes.set("devbox", [
    { value: "acceptEdits", text: "自动接受编辑" },
    { value: "plan", text: "计划模式" },
  ]);
});

describe("每台那一格启动程序：问那台", () => {
  it("★ 下拉：默认那一家的启动器（空）· 用过的 · 那台存着的 · 自定义…；读回之前不可改", async () => {
    const row = mount("devbox");
    const sel = row.querySelector<HTMLButtonElement>("[data-role=resume-select]")!;
    expect(sel.disabled, "读回之前不可改").toBe(true);
    await settle();
    expect(sel.disabled).toBe(false);
    const labels = optionLabels(sel);
    expect(labels[0]).toContain(launcher);
    expect(labels.slice(1)).toEqual(["ccm --tmux", "cc", copyText("resumeSelect.option.custom")]);
    expect(sel.dataset.value).toBe("cc");
  });

  it("★ 选了只交那台 launchers 默认那一家那一格（别的台不动）；自定义的记进用过的", async () => {
    const row = mount("gpu");
    const sel = row.querySelector<HTMLButtonElement>("[data-role=resume-select]")!;
    await settle();
    expect(sel.dataset.value).toBe("");
    choose(sel, copyText("resumeSelect.option.custom"));
    const custom = row.querySelector<HTMLInputElement>("[data-role=resume-custom]")!;
    custom.value = " cct ";
    custom.dispatchEvent(new Event("change"));
    await settle();
    await settle();
    expect(sets).toEqual([["gpu", { launchers: { [DEFAULT_AGENT]: "cct" } }]]);
    expect(disk.cfg.resumeCommandPresets).toEqual(["cct", "ccm --tmux"]);
    expect(emitted).toContain("settings-applied");
    choose(sel, launcher);
    await settle();
    expect(sets.at(-1)).toEqual(["gpu", { launchers: { [DEFAULT_AGENT]: "" } }]);
  });

  it("★ 存失败 ⇒ 退回读回的值 ＋ 行下一句", async () => {
    const row = mount("devbox");
    const sel = row.querySelector<HTMLButtonElement>("[data-role=resume-select]")!;
    await settle();
    disk.failSet = new Error("写不进-xyz");
    choose(sel, "ccm --tmux");
    await settle();
    expect(sel.dataset.value).toBe("cc");
    const err = [...row.querySelectorAll<HTMLElement>(".settings-row-error")].find((e) => !e.hidden)!;
    expect(err.textContent).toContain("写不进-xyz");
  });
});

describe("越层启动器诊断跟着那一格", () => {
  it("★ 存着的 / 选的那条不经 ccm ⇒ 行下摆那一条诊断；选回默认 ⇒ 收起", async () => {
    disk.cfg = { resumeCommandPresets: ["cct"] };
    const row = mount("gpu");
    const sel = row.querySelector<HTMLButtonElement>("[data-role=resume-select]")!;
    await settle();
    const warn = row.querySelector<HTMLElement>(".settings-launcher-warning")!;
    expect(warn.hidden).toBe(true);
    choose(sel, "cct");
    await settle();
    expect(warn.hidden).toBe(false);
    expect(warn.textContent).toBe(copyText("launcherDiagnostics.diagnoseRemoteLauncher.bypassesCcm"));
  });
});

describe("每台那一格「恢复到 tmux 里」：问那台", () => {
  it("★ 读回那台的；拨了只交 resumeInTmux", async () => {
    const row = mount("devbox");
    await settle();
    const sw = row.querySelector<HTMLButtonElement>("[data-role=resume-tmux] [role=switch]")!;
    expect(sw.getAttribute("aria-checked")).toBe("true");
    sw.click();
    await settle();
    expect(sets).toEqual([["devbox", { resumeInTmux: false }]]);
    expect(sw.getAttribute("aria-checked")).toBe("false");
  });
});

describe("那台读不懂 / 问不到", () => {
  it("★ 读不懂 ⇒ 两格禁用，行下是那台写好的那一句", async () => {
    machines.set("bad", { prefs: blank(), state: "unreadable", reason: "那份偏好读不懂-abc" });
    const row = mount("bad");
    await settle();
    expect(row.querySelector<HTMLButtonElement>("[data-role=resume-select]")!.disabled).toBe(true);
    expect(row.querySelector("[data-role=resume-tmux] [role=switch]")!.getAttribute("aria-disabled")).toBe("true");
    expect(row.textContent).toContain("那份偏好读不懂-abc");
  });

  it("★ 问不到 ⇒ 禁用，说读不到", async () => {
    machines.set("off", new Error("通道断了-q"));
    const row = mount("off");
    await settle();
    expect(row.querySelector<HTMLButtonElement>("[data-role=resume-select]")!.disabled).toBe(true);
    expect(row.textContent).toContain(copyText("machinePrefs.read.failed"));
  });
});

describe("每台那一格「新会话的权限档」：问那台", () => {
  it("★ 首项「默认」＋ 那台默认那一家的档；选一档 ⇒ 恰好一问 prefs-set {permissionModes: {那一家: 那一档}}；选回「默认」⇒ 交 null", async () => {
    const row = mount("devbox");
    await settle();
    const r = row.querySelector<HTMLElement>("[data-role=permission-mode]")!;
    expect(r.hidden).toBe(false);
    const b = r.querySelector<HTMLButtonElement>("button")!;
    expect(optionLabels(b)).toEqual([copyText("resumeSelect.option.defaultBare"), "自动接受编辑", "计划模式"]);
    choose(b, "计划模式");
    await settle();
    expect(sets).toEqual([["devbox", { permissionModes: { [DEFAULT_AGENT]: "plan" } }]]);
    choose(b, copyText("resumeSelect.option.defaultBare"));
    await settle();
    expect(sets[1]).toEqual(["devbox", { permissionModes: { [DEFAULT_AGENT]: null } }]);
  });

  it("那一家没有权限档 ⇒ 这一行不画", async () => {
    const row = mount("gpu");
    await settle();
    expect(row.querySelector<HTMLElement>("[data-role=permission-mode]")!.hidden).toBe(true);
  });
});
