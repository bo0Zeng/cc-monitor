/**
 * F82a（#56+#47）：独立设置窗口 → 主窗口的跨窗同步事件名（中立模块，零 import，避免
 * panel.ts ↔ keybindings/editor.ts 循环）。设置窗保存主题 / 行为 toggle / 快捷键改动后
 * `emit` 它；主窗口 `listen` 后重读并应用 theme + behavior + keybindings（跨 OS 窗口回调够不到）。
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
