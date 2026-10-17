/**
 * 「运行于」的默认：照**那台**的偏好（`prefs-read` 的 `resumeInTmux`，那台后端那份 `prefs.json`）；
 * 那台答过没有 tmux ⇒ 不用 tmux（没问到 ⇒ 照那一格）。没问过那台 ⇒ 这一次先按不进 tmux、后台问一次，问回来记下；
 * 设置改过了（`forgetResumeDefaults`）⇒ 忘掉记着的、下次重问。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { prefs, asked } = vi.hoisted(() => ({
  prefs: new Map<string, boolean | Error>(),
  asked: [] as string[],
}));
vi.mock("../../../src/frontend/ui/machine-prefs", () => ({
  readPrefs: async (origin: string) => {
    asked.push(origin);
    const v = prefs.get(origin) ?? false;
    if (v instanceof Error) throw v;
    return { state: "ok", reason: null, detail: null, path: null, prefs: { launchers: {}, resumeInTmux: v, models: {}, contextLimits: {} } };
  },
}));

import { defaultPick } from "../../../src/frontend/ui/resume-menu";
import { forgetResumeDefaults, noteMachineTmux } from "../../../src/frontend/ui/resume-defaults";
import { standingsFrom } from "../../../src/frontend/ui/sessions-where";

const settle = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  forgetResumeDefaults();
  prefs.clear();
  asked.length = 0;
});

describe("「运行于」的默认：照那台的偏好，那台没 tmux 回落不用 tmux", () => {
  it("★ 每台照自己那一格：没问过先按不进 tmux、问那台一次；问回来照那台的", async () => {
    prefs.set("devbox", true);
    prefs.set("gpu", false);
    expect(defaultPick("devbox").tmux, "还没问到").toBe(false);
    expect(asked).toEqual(["devbox"]);
    await settle();
    expect(defaultPick("devbox").tmux).toBe(true);
    expect(defaultPick("gpu").tmux).toBe(false);
    await settle();
    expect(defaultPick("gpu").tmux).toBe(false);
    expect(asked, "问过的不再问").toEqual(["devbox", "gpu"]);
  });

  it("★ 那台答过没有 tmux（sessions-where 回 no_tmux）⇒ 那台回落不用 tmux，别的台照它自己那一格", async () => {
    prefs.set("win-box", true);
    prefs.set("devbox", true);
    defaultPick("win-box");
    defaultPick("devbox");
    await settle();
    standingsFrom("win-box", { results: [{ sid: "s1", standing: "no_tmux", names: [], terminals: [] }], run: "direct" });
    expect(defaultPick("win-box").tmux).toBe(false);
    expect(defaultPick("devbox").tmux).toBe(true);
  });

  it("那台后来答了有 tmux ⇒ 又照那一格", async () => {
    prefs.set("box2", true);
    defaultPick("box2");
    await settle();
    noteMachineTmux("box2", false);
    expect(defaultPick("box2").tmux).toBe(false);
    standingsFrom("box2", {
      results: [{ sid: "s1", standing: "idle", names: ["cc-1"], terminals: [{ host: "tmux", terminal: "cc-1" }] }],
      run: "direct",
    });
    expect(defaultPick("box2").tmux).toBe(true);
  });

  it("★ 设置改过了 ⇒ 忘掉记着的，下次重问、照新的", async () => {
    prefs.set("devbox", true);
    defaultPick("devbox");
    await settle();
    expect(defaultPick("devbox").tmux).toBe(true);
    prefs.set("devbox", false);
    forgetResumeDefaults();
    defaultPick("devbox");
    await settle();
    expect(defaultPick("devbox").tmux).toBe(false);
    expect(asked).toEqual(["devbox", "devbox"]);
  });

  it("问不到那台 ⇒ 不进 tmux，不一直重问（改过设置才再问）", async () => {
    prefs.set("off", new Error("通道断了"));
    defaultPick("off");
    await settle();
    expect(defaultPick("off").tmux).toBe(false);
    expect(asked).toEqual(["off"]);
  });
});
