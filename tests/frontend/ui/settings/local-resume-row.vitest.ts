/**
 * 本机「这台上的 cc-monitor → 恢复命令」那一格：下拉（候选来自适配层画像 ＋ 用过的 ＋ 自定义…，另有「通用设置」）；
 * 读回存过的值；选了只写那一个键；存失败退回存过的值 ＋ 行下一句。
 */
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
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { AGENT_PROFILE_TABLE, DEFAULT_AGENT } from "../../../../src/frontend/ui/generated/agent-profile-table";

const settle = () => new Promise((r) => setTimeout(r, 0));
/** 恢复命令那个下拉（kit 的框是一颗按钮）：面板里那几项的字、点选一项。 */
const optionLabels = (b: HTMLButtonElement): string[] => {
  b.click();
  const got = [...document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]')].map((i) => i.textContent ?? "");
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  b.click();
  return got;
};
const choose = (b: HTMLButtonElement, label: string): void => {
  b.click();
  [...document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]')].find((i) => i.textContent === label)!.click();
};


beforeEach(() => {
  disk.cfg = { localResumeCommand: "cc", resumeCommand: "ccm", resumeCommandPresets: ["ccm --tmux"] };
  disk.fail = null;
  patches.length = 0;
});

describe("本机那一格恢复命令", () => {
  it("★ 是下拉：通用设置 · 适配层画像给的启动器 · 用过的 · 存着的 · 自定义…（界面不写死哪一家的命令）", async () => {
    const row = localResumeRow();
    const sel = row.querySelector<HTMLButtonElement>("[data-role=resume-select]")!;
    expect(sel.disabled, "读回之前不可改").toBe(true);
    await settle();
    const launcher = AGENT_PROFILE_TABLE.find((r) => r.agent === DEFAULT_AGENT)!.defaultLauncher;
    expect(optionLabels(sel)).toEqual([copyText("resumeSelect.option.inherit"), launcher, "ccm --tmux", "cc", copyText("resumeSelect.option.custom")]);
    expect(sel.dataset.value).toBe("cc");
  });

  it("★ 选了只写 localResumeCommand 一个键（不拿旧值盖通用页那一格）；自定义… 展开一格、失焦存", async () => {
    const row = localResumeRow();
    const sel = row.querySelector<HTMLButtonElement>("[data-role=resume-select]")!;
    await settle();
    choose(sel, copyText("resumeSelect.option.inherit"));
    await settle();
    expect(JSON.stringify(patches)).toContain('"localResumeCommand"');
    expect(JSON.stringify(patches)).not.toContain('"resumeCommand"');
    const custom = row.querySelector<HTMLInputElement>("[data-role=resume-custom]")!;
    expect(custom.hidden).toBe(true);
    choose(sel, copyText("resumeSelect.option.custom"));
    expect(custom.hidden, "选「自定义…」展开一格").toBe(false);
    custom.value = " cct ";
    custom.dispatchEvent(new Event("change"));
    await settle();
    expect(JSON.stringify(patches.at(-1))).toContain('"cct"');
    expect(sel.dataset.value).toBe("cct");
  });

  it("★ 存失败 ⇒ 退回存过的值 ＋ 行下一句", async () => {
    const row = localResumeRow();
    const sel = row.querySelector<HTMLButtonElement>("[data-role=resume-select]")!;
    await settle();
    disk.fail = new Error("写不进-xyz");
    choose(sel, "ccm --tmux");
    await settle();
    expect(sel.dataset.value).toBe("cc");
    const err = row.querySelector<HTMLElement>(".settings-row-error")!;
    expect(err.hidden).toBe(false);
    expect(err.textContent).toContain("写不进-xyz");
  });
});
