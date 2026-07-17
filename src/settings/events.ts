/**
 * F82a（#56+#47）：独立设置窗口 → 主窗口的跨窗同步事件名（中立模块，零 import，避免
 * panel.ts ↔ keybindings/editor.ts 循环）。设置窗保存主题 / 行为 toggle / 快捷键改动后
 * `emit` 它；主窗口 `listen` 后重读并应用 theme + behavior + keybindings（跨 OS 窗口回调够不到）。
 */
export const SETTINGS_APPLIED_EVENT = "settings-applied";
