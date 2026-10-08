/**
 * 主窗口打开设置窗的每个入口落到哪（目的地的形状见 `settings/open-settings.ts` 头注）。
 *
 * | 入口 | 落到 |
 * |---|---|
 * | 标签页栏顶「设置」· `,` · 命令面板「设置」 | 不带目的地（上次离开的地方） |
 * | 账号 chip「管理账号…」· 命令面板「管理账号…」· 账号面板里的「管理账号…」「设置…」「接入…」 | 那台的「账号」栏 |
 * | ↗ 浮层的［接上终端］ | 那台 ›「别名与配置文件」› 接上终端那一节 |
 * | 上下文浮层［设上限］ | 通用 › 上下文上限那一节 |
 * | 快捷键一览「改快捷键…」 | 外观 › 快捷键那一节 |
 * | 一台机器一直看不见时提示里的「打开设置」 | 那台的页 |
 * | 状态栏「要你动手 N」 | 文件与数据（「要你动手」那一栏是它的默认栏） |
 *
 * ↗ 浮层「要更新」的［更新］不开设置：就地部署（`backend-deploy.ts`，与机器卡同一处）。
 *
 * 认不出的那一节设置窗会忽略（照常打开那一页、高亮页头）。
 */
import type { Origin } from "./ipc/origin";
import type { SettingsTarget } from "./settings/open-settings";

/** 那台的「账号」栏（没有当前标签页时调用方给本机）。 */
export function accountsOf(origin: Origin): SettingsTarget {
  return { machine: origin, tab: "acct" };
}

/** 那台 ›「别名与配置文件」› 接上终端。 */
export function connectTerminalOf(origin: Origin): SettingsTarget {
  return { machine: origin, tab: "config", anchor: "connect-terminal" };
}

/** 那台机器的页。 */
export function machineOf(origin: Origin): SettingsTarget {
  return { machine: origin };
}

/** 上下文上限那一节。 */
export const CONTEXT_LIMITS: SettingsTarget = { page: "general", anchor: "context-limits" };

/** 文件与数据（「要你动手」那一栏）。 */
export const CHORES: SettingsTarget = { page: "data" };

/** 快捷键那一节。 */
export const KEYBINDINGS: SettingsTarget = { page: "appearance", anchor: "keybindings" };
