// settings/ 桶文件：对外只暴露 SettingsPanel（各 section 由 panel.ts 内部组装）。
export { SettingsPanel } from "./panel";
// F82a：独立设置窗口保存后的跨窗同步事件名（中立模块）。
export { SETTINGS_APPLIED_EVENT } from "./events";
