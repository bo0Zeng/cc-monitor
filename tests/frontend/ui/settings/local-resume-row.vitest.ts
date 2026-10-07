/** 本机「这台上的 cc-monitor → 恢复命令」那一格：读回存过的值；失焦只写那一个键；存失败退回存过的值 ＋ 行下一句。 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { disk, patches } = vi.hoisted(() => ({
  disk: { cfg: {} as Record<string, unknown>, fail: null as Error | null },
  patches: [] as unknown[],
}));
vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: {
    load_config: () => Promise.resolve(structuredClone(disk.cfg)),
    patch_config: (a: { edits: unknown }) => {
      if (disk.fail) return Promise.reject(disk.fail);
      patches.push(a.edits);
      return Promise.resolve();
    },
  },
}));

import { localResumeRow } from "../../../../src/frontend/ui/settings/local-resume-row";

const settle = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  disk.cfg = { localResumeCommand: "cc", resumeCommand: "ccm" };
  disk.fail = null;
  patches.length = 0;
});

describe("本机那一格恢复命令", () => {
  it("★ 读回存过的值；改了失焦只写 localResumeCommand 一个键（不拿旧值盖通用页那一格）", async () => {
    const row = localResumeRow();
    const input = row.querySelector<HTMLInputElement>("input")!;
    expect(input.disabled, "读回之前不可改").toBe(true);
    await settle();
    expect(input.value).toBe("cc");
    input.value = " cct ";
    input.dispatchEvent(new Event("change"));
    await settle();
    expect(JSON.stringify(patches)).toContain("localResumeCommand");
    expect(JSON.stringify(patches)).toContain('"cct"');
    expect(JSON.stringify(patches)).not.toContain("resumeCommand\"");
  });

  it("★ 存失败 ⇒ 退回存过的值 ＋ 行下一句", async () => {
    const row = localResumeRow();
    const input = row.querySelector<HTMLInputElement>("input")!;
    await settle();
    disk.fail = new Error("写不进-xyz");
    input.value = "zz";
    input.dispatchEvent(new Event("change"));
    await settle();
    expect(input.value).toBe("cc");
    const err = row.querySelector<HTMLElement>(".settings-row-error")!;
    expect(err.hidden).toBe(false);
    expect(err.textContent).toContain("写不进-xyz");
  });
});
