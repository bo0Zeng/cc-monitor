// 全局去处（不属于哪一条会话的几处顶层视图）：桌面栏顶 ⋯ 照 `global-dests.ts` 那一张清单摆；手机抽屉里同一件事引同一个文案键、同字。
// 会话那一颗「终端」是另一件事：全局那一页叫「终端名单」，两个字不许相同。
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, it, expect } from "vitest";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { GLOBAL_DESTS, destLabel } from "../../../src/frontend/ui/global-dests";
import { REPO_ROOT } from "../../test-support/repo-root";

const DRAWER = resolve(REPO_ROOT, "src/mobile/app/src/main/kotlin/com/ccmonitor/mobile/ui/drawer/DrawerEntries.kt");

/** 抽屉里那一项（`DrawerItem.<名>`）引的文案键，照 `DrawerEntries.kt` 里 `drawerEntries()` 那一行读。 */
function drawerKey(item: string): string {
  const src = readFileSync(DRAWER, "utf8");
  const m = new RegExp(`DrawerEntry\\(DrawerItem\\.${item},[^\\n]*copyText\\("([^"]+)"\\)`).exec(src);
  if (!m) throw new Error(`抽屉里没找到 ${item} 那一项`);
  return m[1];
}

describe("两端全局去处同名", () => {
  it("栏顶 ⋯ 依次是 计划 · 文件 · 会话总览 · 终端名单 · cc-bus", () => {
    expect(GLOBAL_DESTS.map((d) => d.id)).toEqual(["plan", "files", "grid", "terminals", "cc-bus"]);
    expect(GLOBAL_DESTS.map((d) => d.label())).toEqual([
      copyText("plan.page.title"),
      copyText("mobDrawer.item.files"),
      copyText("main.cmd.openGrid"),
      copyText("main.cmd.openTerminals"),
      copyText("main.cmd.openCcBus"),
    ]);
  });

  it("★ 手机抽屉里文件 · 终端名单 · 计划三项与桌面那三项同字", () => {
    for (const [item, dest] of [
      ["Files", "files"],
      ["Terminal", "terminals"],
      ["Plan", "plan"],
    ] as const) {
      expect(copyText(drawerKey(item) as never), `抽屉 ${item}`).toBe(destLabel(dest));
    }
  });

  it("全局那一页的名字与会话那一颗「终端」不同字", () => {
    expect(destLabel("terminals")).not.toBe(copyText("mobDrawer.item.terminal"));
  });
});
