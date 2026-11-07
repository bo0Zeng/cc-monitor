/**
 * **全局去处**：不属于哪一条会话的几处顶层视图。桌面栏顶 ⋯ 照这一张清单摆（`main.ts`），命令面板里同名。
 * 手机抽屉里同一件事引同一个文案键（文件 · 终端名单 · 计划，`DrawerEntries.kt`）；会话那一颗「终端」是另一件事、另一个词。
 */
import { copyText } from "./copy-table";
import type { IconName } from "./kit/icon";
import type { ActionId } from "./keybindings/actions";

export type GlobalDest = "plan" | "files" | "grid" | "terminals" | "cc-bus";

export interface GlobalDestItem {
  id: GlobalDest;
  icon: IconName;
  label: () => string;
  /** 有快捷键的那几项：菜单第二格写它此刻的键。 */
  action: ActionId | null;
}

export const GLOBAL_DESTS: readonly GlobalDestItem[] = [
  { id: "plan", icon: "plan", label: () => copyText("plan.page.title"), action: "app.toggle-plan" },
  { id: "files", icon: "files", label: () => copyText("mobDrawer.item.files"), action: null },
  { id: "grid", icon: "grid", label: () => copyText("main.cmd.openGrid"), action: null },
  { id: "terminals", icon: "terminal", label: () => copyText("main.cmd.openTerminals"), action: null },
  { id: "cc-bus", icon: "bus", label: () => copyText("main.cmd.openCcBus"), action: "app.open-cc-bus" },
];

/** 那一处叫什么（栏顶 ⋯ · 命令面板 · 那一页的页头同一个字）。 */
export function destLabel(id: GlobalDest): string {
  const d = GLOBAL_DESTS.find((x) => x.id === id);
  if (!d) throw new Error(`global dest ${id}`);
  return d.label();
}
