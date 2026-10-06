/**
 * `src/frontend/ui/resume-menu.ts` 的判据：「恢复」那一组选项怎么摆（标签页「恢复 ▸」与历史页「恢复 ▾」同一个组件）。
 *
 * 1. 没给号 ⇒ 只有顶层 tmux / 直连两项，跟随默认的号。
 * 2. 给了号 ⇒ 分隔线之后每个号一项、各嵌 tmux / 直连；基座那一项交 `useBase`，具名的交号名。
 */
import { describe, it, expect } from "vitest";
import { resumeMenuItems, type ResumePick } from "../../../src/frontend/ui/resume-menu";
import { copyText } from "../../../src/frontend/ui/copy-table";

describe("恢复那一组", () => {
  it("没给号 ⇒ tmux / 直连两项，跟随", () => {
    const got: ResumePick[] = [];
    const items = resumeMenuItems([], (p) => got.push(p));
    expect(items.map((i) => i.label)).toEqual(["tmux", copyText("tabMenu.containerLeaves.direct")]);
    items[0].onClick?.();
    items[1].onClick?.();
    expect(got).toEqual([
      { tmux: true, account: undefined, useBase: false },
      { tmux: false, account: undefined, useBase: false },
    ]);
  });

  it("给了号 ⇒ 每个号一项、各嵌 tmux / 直连", () => {
    const got: ResumePick[] = [];
    const items = resumeMenuItems(
      [
        { kind: "base", label: "基座" },
        { kind: "account", name: "work", label: "work" },
      ],
      (p) => got.push(p),
    );
    expect(items.map((i) => (i.divider ? "—" : i.label))).toEqual(["tmux", copyText("tabMenu.containerLeaves.direct"), "—", "基座", "work"]);
    items[3].submenu?.[0].onClick?.();
    items[4].submenu?.[1].onClick?.();
    expect(got).toEqual([
      { tmux: true, account: undefined, useBase: true },
      { tmux: false, account: "work", useBase: false },
    ]);
  });
});
