/**
 * 「添加机器」框带上打的名字（A2-17-14-1：命令面板「加入机器表… · backup」）：
 * `~/.ssh/config` 里有那一条 ⇒ 只勾它；没有 ⇒ 落在「自己填」、名字那一格是它。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { openAddMachine } from "../../../../src/frontend/ui/settings/add-machine";
import type { ImportGroup } from "../../../../src/frontend/ui/ssh-config-reads";

const group = (label: string, host: string): ImportGroup => ({
  label,
  host,
  port: 22,
  user: "u",
  keyPath: null,
  addresses: [],
  jump: null,
  members: [{ alias: label, host, port: 22, proxyJump: null }],
  inList: false,
});

const tick = async (): Promise<void> => {
  for (let i = 0; i < 5; i++) await new Promise((r) => setTimeout(r, 0));
};

const open = async (prefill: string): Promise<HTMLElement> => {
  void openAddMachine({
    tryAdd: vi.fn(async () => null),
    imported: async () => ({ groups: [group("backup", "1.1.1.1"), group("gpu", "2.2.2.2")], partial: false }),
    add: vi.fn(async () => null),
    prefill,
  });
  await tick();
  return document.querySelector<HTMLElement>('[role="dialog"]')!;
};

beforeEach(() => document.body.replaceChildren());

describe("添加机器框带上打的名字", () => {
  it("ssh config 里有那一条 ⇒ 只勾它", async () => {
    const dlg = await open("backup");
    const picked = [...dlg.querySelectorAll<HTMLElement>(".add-machine-row")].map((r) => [
      r.querySelector<HTMLInputElement>(".add-machine-row input:not([type=checkbox])")?.value,
      r.querySelector<HTMLInputElement>('input[type="checkbox"]')!.checked,
    ]);
    expect(picked).toEqual([
      ["backup", true],
      ["gpu", false],
    ]);
  });

  it("没有那一条 ⇒ 落在「自己填」，名字那一格是它", async () => {
    const dlg = await open("nas");
    const grid = dlg.querySelector<HTMLElement>(".add-machine-grid")!;
    expect(grid.closest<HTMLElement>(".add-machine-pane")!.hidden).toBe(false);
    expect(grid.querySelector<HTMLInputElement>("input")!.value).toBe("nas");
  });
});
