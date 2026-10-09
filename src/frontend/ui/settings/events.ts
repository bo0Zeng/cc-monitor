/**
 * 设置窗 ↔ 主窗口的跨窗事件名（中立模块、零 import，免得 panel.ts ↔ keybindings/editor.ts 成环）。
 * 设置窗保存主题 / 行为开关 / 快捷键后 `emit`；主窗口 `listen` 后重读并应用（跨 OS 窗口回调够不到）。
 */
export const SETTINGS_APPLIED_EVENT = "settings-applied";

/** 设置窗「重新对齐」做完 → 主窗口（载荷 `{ origin }`）：标出那台上记录没了的固定条。 */
export const RESYNC_DONE_EVENT = "resync-done";

/**
 * 主窗口用快捷键翻了「自动跟随 / 自动切到前台」→ 设置窗（载荷：翻完之后这两格的值）：开着的那一页开关跟着变，
 * 免得它之后用旧值把这一下写回去。
 */
export const BEHAVIOR_TOGGLED_EVENT = "behavior-toggled";

export interface BehaviorToggled {
  autoFollowUserActive: boolean;
  bringMonitorToFrontOnUserActive: boolean;
}

/**
 * 设置窗账号页的指路框 → 主窗口（载荷 {@link OpenAccountPanel}）：主窗口打开当前标签页的账号面板并滚到那一节
 * （时间轴 · 默认轮换都住在那里，设置窗里不重画）。形状写在壳那一侧 `ui_contract.rs::events` 的注释里（窗到窗，壳不经手）。
 */
export const OPEN_ACCOUNT_PANEL_EVENT = "open-account-panel";

export interface OpenAccountPanel {
  /** 哪台（本机 = `LOCAL_ORIGIN`）。 */
  machine: string;
  anchor: "timeline" | "default-rotation";
}

/**
 * 设置窗里一节要带目的地跳到别处（同一窗口内，DOM 冒泡事件；`detail` 是 `SettingsTarget` 那一形）：设置窗外框收到就落过去。
 * 例：账号页表下「共用 MCP：别名与配置文件」⇒ 同一台的「别名与配置文件」栏。
 */
export const SETTINGS_GO_EVENT = "settings-go";

/**
 * 机器页切到「别名与配置文件」那一栏：派到那一栏里每个带 `data-config-shown` 的块上（不冒泡）。
 * 那一块第一次收到才问那台，之后每次收到重读一遍。
 */
export const CONFIG_SHOWN_EVENT = "settings-config-shown";
