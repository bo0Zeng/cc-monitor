/**
 * 恢复的两样默认：
 * - 恢复命令：每台那一格（远端在机器表那台 · 本机在 `localResumeCommand`）优先，空 ⇒ 通用页那一格（`resumeCommand`）。
 * - 「运行于」：照通用页「恢复到 tmux 里」；那台答过没有 tmux ⇒ 不用 tmux（没问到 ⇒ 照那一格）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { disk } = vi.hoisted(() => ({ disk: { cfg: {} as Record<string, unknown> } }));
vi.mock("../../../src/frontend/ui/ipc/commands", () => ({
  commands: { load_config: () => Promise.resolve(structuredClone(disk.cfg)) },
}));

import { resumeCommandFor } from "../../../src/frontend/ui/remote-config";
import { defaultPick } from "../../../src/frontend/ui/resume-menu";
import { noteMachineTmux, setResumeInTmux } from "../../../src/frontend/ui/resume-defaults";
import { decodeStandings } from "../../../src/frontend/ui/sessions-where";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";

const host = (label: string, resumeCommand: string) => ({ label, host: `${label}.lan`, user: "u", port: 22, resumeCommand });

describe("恢复命令：那台那一格优先，空 ⇒ 通用页那一格", () => {
  beforeEach(() => {
    disk.cfg = { resumeCommand: "ccm", remote: { hosts: [host("devbox", ""), host("gpu", "cct")] } };
  });

  it("★ 远端：那台填了用那台的；没填用通用页那一格", async () => {
    expect(await resumeCommandFor("gpu")).toBe("cct");
    expect(await resumeCommandFor("devbox")).toBe("ccm");
  });

  it("★ 本机：本机那一格填了用它；没填用通用页那一格（一格默认管所有机器）", async () => {
    expect(await resumeCommandFor(LOCAL_ORIGIN)).toBe("ccm");
    disk.cfg.localResumeCommand = " cc ";
    expect(await resumeCommandFor(LOCAL_ORIGIN)).toBe("cc");
  });

  it("旧的两格（resumeCommandLocal / resumeCommandRemote）不再被读（不迁移）", async () => {
    disk.cfg = { resumeCommandLocal: "old-l", resumeCommandRemote: "old-r", remote: { hosts: [host("devbox", "")] } };
    expect(await resumeCommandFor(LOCAL_ORIGIN)).toBe("");
    expect(await resumeCommandFor("devbox")).toBe("");
  });
});

describe("「运行于」的默认：照通用页那一格，那台没 tmux 回落不用 tmux", () => {
  beforeEach(() => setResumeInTmux(false));

  it("★ 那一格关 ⇒ 不用 tmux；开 ⇒ tmux 里", () => {
    expect(defaultPick("devbox").tmux).toBe(false);
    setResumeInTmux(true);
    expect(defaultPick("devbox").tmux).toBe(true);
  });

  it("★ 那台答过没有 tmux（sessions-where 回 no_tmux）⇒ 那台回落不用 tmux，别的台照那一格", () => {
    setResumeInTmux(true);
    decodeStandings("win-box", ["s1"], { results: [{ sid: "s1", standing: "no_tmux", names: [], terminals: [] }] });
    expect(defaultPick("win-box").tmux).toBe(false);
    expect(defaultPick("devbox").tmux).toBe(true);
  });

  it("那台后来答了有 tmux ⇒ 又照那一格", () => {
    setResumeInTmux(true);
    noteMachineTmux("box2", false);
    expect(defaultPick("box2").tmux).toBe(false);
    decodeStandings("box2", ["s1"], {
      results: [{ sid: "s1", standing: "idle", names: ["cc-1"], terminals: [{ host: "tmux", terminal: "cc-1" }] }],
    });
    expect(defaultPick("box2").tmux).toBe(true);
  });
});
