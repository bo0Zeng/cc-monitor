/**
 * **栏顶那一排能放的入口**（全局去处）：不属于哪一条会话的几处 ＋ 新建会话 · 存好的几组。能上栏顶的只这一张清单，缺省那一排只写在这里
 * （[`DEFAULT_HEAD`]）；用户排的那一份存 `config.json` 的 `tabBar.head`（读写只在这里：[`getTabBarHead`] · [`setTabBarHead`]；设置窗也读写它，
 * 所以不放 `tab-bar-state.ts` —— 设置窗不带 tab 管理那一截），不在里面的都进右端 ⋯
 * （⋯ 恒在最右、不进清单、编不掉）。命令面板里同名；手机抽屉里同一件事引同一个文案键（文件 · 终端名单 · 计划，`DrawerEntries.kt`）。
 */
import { copyText } from "./copy-table";
import { sanitizeRow } from "./button-row";
import { loadConfig, patchConfig, removeAt, setAt } from "./config";
import type { IconName } from "./kit/icon";
import type { ActionId } from "./keybindings/actions";

export type GlobalDest = "plan" | "files" | "grid" | "terminals" | "cc-bus";

/** 栏顶能放的一颗：全局去处 ＋ 新建会话 · 存好的几组 · 历史 · 设置。 */
export type HeadId = "new" | "sets" | "history" | "settings" | GlobalDest;

export interface HeadEntry {
  id: HeadId;
  icon: IconName;
  label: () => string;
  /** 有快捷键的那几项：悬停与 ⋯ 菜单第二格写它此刻的键。 */
  action: ActionId | null;
}

/** 清单（编辑清单下段 · ⋯ 菜单都按这个先后）。 */
export const HEAD_ENTRIES: readonly HeadEntry[] = [
  { id: "new", icon: "plus", label: () => copyText("main.cmd.newSession"), action: "app.new-session" },
  { id: "sets", icon: "list", label: () => copyText("setMenu.open.label"), action: null },
  { id: "history", icon: "history", label: () => copyText("main.cmd.openHistory"), action: "app.toggle-history" },
  { id: "plan", icon: "plan", label: () => copyText("plan.page.title"), action: "app.toggle-plan" },
  { id: "files", icon: "files", label: () => copyText("mobDrawer.item.files"), action: null },
  { id: "settings", icon: "settings", label: () => copyText("mobDrawer.settings.aria"), action: "app.open-settings" },
  { id: "grid", icon: "grid", label: () => copyText("main.cmd.openGrid"), action: null },
  { id: "terminals", icon: "terminal", label: () => copyText("main.cmd.openTerminals"), action: null },
  { id: "cc-bus", icon: "bus", label: () => copyText("main.cmd.openCcBus"), action: "app.open-cc-bus" },
];

/** 全集的 id（按清单先后）。 */
export const HEAD_IDS: readonly HeadId[] = HEAD_ENTRIES.map((e) => e.id);

/** 缺省那一排（全仓只写在这里）：新建会话 · 存好的几组 · 历史 · 计划 · 文件 · 设置。 */
export const DEFAULT_HEAD: readonly HeadId[] = ["new", "sets", "history", "plan", "files", "settings"];

/** 盘上那一格 ⇒ 栏上那几颗（按顺序）：不是表 ⇒ 缺省；认不得的丢、重复的只留头一个；空表 ⇒ 一颗都没有。 */
export function headOf(raw: unknown): HeadId[] {
  return sanitizeRow(raw, HEAD_IDS) ?? [...DEFAULT_HEAD];
}

/** 不在栏上的那几项（照清单先后）：都进 ⋯。 */
export function overflowOf(head: readonly HeadId[]): HeadId[] {
  return HEAD_IDS.filter((id) => !head.includes(id));
}

/** 那一项（清单里一定有）。 */
export function headEntry(id: HeadId): HeadEntry {
  const d = HEAD_ENTRIES.find((x) => x.id === id);
  if (!d) throw new Error(`head entry ${id}`);
  return d;
}

/** 那一处叫什么（栏顶 · ⋯ · 命令面板 · 那一页的页头同一个字）。 */
export function destLabel(id: GlobalDest): string {
  return headEntry(id).label();
}

/** 读栏顶那一排：没存 · 段坏了 · 读失败 ⇒ 缺省那一排；认不得的丢、重复的只留头一个（[`headOf`]）。 */
export async function getTabBarHead(): Promise<HeadId[]> {
  try {
    const seg = ((await loadConfig()) as Record<string, unknown>).tabBar;
    return headOf(seg && typeof seg === "object" && !Array.isArray(seg) ? (seg as Record<string, unknown>).head : undefined);
  } catch (e) {
    console.warn("getTabBarHead failed:", e);
    return [...DEFAULT_HEAD];
  }
}

/** 写栏顶那一排；`null` ＝ 恢复缺省（删掉这个键）。**只动 `tabBar.head` 这一个键**（按键补丁，段里别的键原样留着）。 */
export async function setTabBarHead(ids: readonly HeadId[] | null): Promise<void> {
  await patchConfig([ids === null ? removeAt(["tabBar", "head"]) : setAt(["tabBar", "head"], [...ids])]);
}
