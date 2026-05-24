/**
 * 设置面板用的 `?` 信息图标组件 + 路径工具函数。
 *
 * v1.7.13: 从 cc_integration.ts 拆出来。tooltip 用 portal 模式
 * （`position: fixed` + 加到 document.body + JS 算位置 + 边界感知），
 * 彻底绕开 .settings-body `overflow-y: auto` 裁剪和 .settings-panel
 * `transform` 对 fixed containing block 的重置。
 */

/**
 * 替换路径的文件名部分。
 * 用来从 Microsoft.PowerShell_profile.ps1 推 profile.ps1（AllHosts 选项）。
 * 兼容 Windows `\` 和 POSIX `/` 分隔符。
 */
export function swapFileName(path: string, newName: string): string {
  const lastSep = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  if (lastSep < 0) return newName;
  return path.slice(0, lastSep + 1) + newName;
}

/**
 * 创建一个 `?` 信息图标，鼠标悬停显示 tooltip。
 *
 * **设计要点（来自 v1.7.13 调试）**:
 * - tooltip 加到 `document.body` 而不是 icon 子节点 —— 因为父级
 *   `.settings-panel` 有 `transform: translateX(0)`，按 CSS spec，
 *   transformed 祖先会让 `position: fixed` 的 containing block 从 viewport
 *   重置到那个祖先 → `left/top` 不再是真 viewport 坐标。挂 body 脱离 panel
 *   子树即可。
 * - 安全：tooltip 内容用 `textContent` 写（不 innerHTML），`\n` 通过 CSS
 *   `white-space: pre-line` 渲染换行。
 * - 可访问性：mouseenter + focusin 都触发显示（键盘 Tab 可用）。
 *
 * @param text tooltip 文本，支持 `\n` 换行
 * @returns `?` 图标元素，调用方 append 到 row 里就行
 */
export function makeInfoIcon(text: string): HTMLElement {
  const wrap = document.createElement("span");
  wrap.className = "settings-info-icon";
  wrap.setAttribute("aria-label", text);
  wrap.textContent = "?";

  const tip = document.createElement("span");
  tip.className = "settings-info-tooltip";
  tip.textContent = text;
  // 关键：tip 加到 body，脱离 .settings-panel 的 transform 子树
  document.body.appendChild(tip);

  const showAndPosition = () => {
    // 先 visibility:hidden + display:block 让浏览器布局/测尺寸，再算位置 → 设
    // left/top → visibility:visible。避免 display:none 时测得 0×0 + 闪烁
    tip.style.visibility = "hidden";
    tip.style.display = "block";
    const iconRect = wrap.getBoundingClientRect();
    const tipRect = tip.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const MARGIN = 8; // 距 viewport 边界至少 8px
    const GAP = 6; // tooltip 跟 icon 间距

    // 默认放 icon 正上方居中
    let left = iconRect.left + iconRect.width / 2 - tipRect.width / 2;
    let top = iconRect.top - tipRect.height - GAP;

    // 顶部不够 → 翻到下方
    if (top < MARGIN) {
      top = iconRect.bottom + GAP;
    }
    // 下方也不够（极少见）→ 夹住到 viewport
    if (top + tipRect.height > vh - MARGIN) {
      top = Math.max(MARGIN, vh - MARGIN - tipRect.height);
    }
    // 水平方向夹住到 viewport
    if (left < MARGIN) left = MARGIN;
    if (left + tipRect.width > vw - MARGIN) left = vw - MARGIN - tipRect.width;

    tip.style.left = `${left}px`;
    tip.style.top = `${top}px`;
    tip.style.visibility = "visible";
  };

  const hide = () => {
    tip.style.display = "none";
    tip.style.visibility = "";
  };

  wrap.addEventListener("mouseenter", showAndPosition);
  wrap.addEventListener("mouseleave", hide);
  // 键盘可访问性：focus 也显示
  wrap.addEventListener("focusin", showAndPosition);
  wrap.addEventListener("focusout", hide);

  return wrap;
}
