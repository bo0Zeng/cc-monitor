/**
 * 主窗口打开设置窗的每个入口落到哪（目的地的形状见 `open-settings.ts` 头注）。
 *
 * | 入口 | 落到 |
 * |---|---|
 * | 标签页栏顶「设置」· `,` · 命令面板「设置」 | 不带目的地（上次离开的地方） |
 * | 账号 chip「管理账号…」· 命令面板「管理账号…」· 账号面板里的「管理账号…」「设置…」「接入…」 | 那台的「账号」栏 |
 * | 账号面板配置下拉「管理配置…」· 策略那一格「看说明与源码 ›」 | 那台的「轮换」栏（配置页 / 策略页；给了哪份 ⇒ 打开那一份） |
 * | ↗ 浮层的［接上终端］ | 那台 ›「别名与配置文件」› 接上终端那一节 |
 * | 上下文浮层［设上限］ | 当前标签页那台 › 上下文上限那一折 |
 * | 快捷键一览「改快捷键…」 | 外观 › 快捷键那一节 |
 * | 栏底机器条那一行的修法（重新连接以外：连接设置 · 推公钥 · 比对指纹 · 更新 …） | 那台的页 |
 * | 命令面板「加入机器表… · <名字>」 | 机器页、添加框开着、名字预填 |
 * | 状态栏「待办 N」 | 文件与数据（「待办」那一栏是它的默认栏） |
 * | 计划页「未装 planned-build」［填 pb 路径…］ | 那台 › pb 路径那一行 |
 *
 * ↗ 浮层「要更新」的［更新］不开设置：就地部署（`backend-deploy.ts`，与机器卡同一处）。
 *
 * 认不出的那一节设置窗会忽略（照常打开那一页、高亮页头）。
 */
import type { Machine } from "./ipc/origin";
import type { SettingsTarget } from "./open-settings";

/** 那台的「账号」栏（没有当前标签页时调用方给本机）。 */
export function accountsOf(origin: Machine): SettingsTarget {
  return { machine: origin, tab: "acct" };
}

/** 那台 ›「别名与配置文件」› 接上终端。 */
export function connectTerminalOf(origin: Machine): SettingsTarget {
  return { machine: origin, tab: "config", anchor: "connect-terminal" };
}

/** 那台机器的页。 */
export function machinePageOf(origin: Machine): SettingsTarget {
  return { machine: origin };
}

/** 那台的上下文上限那一折（机器页「这台上的 cc-monitor」里）。 */
export function contextLimitsOf(origin: Machine): SettingsTarget {
  return { machine: origin, anchor: "context-limits" };
}

/** pb 路径那一行的锚（机器页「这台上的 cc-monitor」里）。 */
export const PB_DIR_ANCHOR = "pb-dir";

/** 那台的 pb 路径那一行。 */
export function pbDirOf(origin: Machine): SettingsTarget {
  return { machine: origin, anchor: PB_DIR_ANCHOR };
}

/** 文件与数据（「待办」那一栏）。 */
export const CHORES: SettingsTarget = { page: "data" };

/** 机器页顶上「开始用」那一块。 */
export const START: SettingsTarget = { page: "machines", anchor: "first-run" };

/** 快捷键那一节。 */
export const KEYBINDINGS: SettingsTarget = { page: "appearance", anchor: "keybindings" };

/** 机器页的「添加机器」框锚：`add-machine:<打的名字>`（框开着、名字预填；名字空 ⇒ 照常开框）。 */
export const ADD_MACHINE_ANCHOR = "add-machine:";

/** 命令面板「加入机器表… · <名字>」：机器页、添加框开着、名字预填。 */
export function addMachineOf(name: string): SettingsTarget {
  return { page: "machines", anchor: `${ADD_MACHINE_ANCHOR}${name}` };
}
