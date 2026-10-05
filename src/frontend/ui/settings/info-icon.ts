/**
 * 设置面板用的 `?` 信息图标。
 */
import { attachTooltip } from "../kit/tooltip";

/**
 * 一个 `?` 信息图标：悬停 / 键盘焦点立刻出说明（悬停提示是全产品那一份，`kit/tooltip.ts`：挂 body、只在显示期间存在、躲窗口边）。
 *
 * @param text 说明（`\n` 换行）
 * @returns 图标元素，调用方 append 到行里就行（不需要销毁）
 */
export function makeInfoIcon(text: string): HTMLElement {
  const wrap = document.createElement("span");
  wrap.className = "settings-info-icon";
  wrap.setAttribute("aria-label", text);
  wrap.tabIndex = 0;
  wrap.textContent = "?";
  attachTooltip(wrap, text, { immediate: true });
  return wrap;
}
