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
/**
 * E60：**活着的 tooltip 注册表**（tip → 它属于哪个图标）。
 *
 * 见 `makeInfoIcon` 头注「泄漏是怎么消掉的」那一段。稳态是 **0 或 1** 条 ——
 * tooltip 只在显示期间存在于 DOM 里。
 */
const liveTooltips = new Map<HTMLElement, HTMLElement>();

/**
 * 清掉「主人已经不在 DOM 里」的 tooltip。
 *
 * 覆盖的是唯一一个 hide 兜不住的时序：**正显示着的时候图标被销毁**
 *（`rebuildCards()` 在鼠标悬停期间跑）。此时 `mouseleave` 永远不会来。
 * 每次要显示新 tooltip 前扫一次即可 —— 最多同时残留一条，且下一次悬停就清掉。
 */
function sweepOrphanTooltips(): void {
  for (const [tip, owner] of [...liveTooltips]) {
    if (!owner.isConnected) {
      tip.remove();
      liveTooltips.delete(tip);
    }
  }
}

/** 仅供测试：当前挂在 body 上的 tooltip 数。 */
export function __liveTooltipCountForTests(): number {
  return liveTooltips.size;
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
 * # E60：泄漏是怎么消掉的 —— **不是加 `destroy()`**
 *
 * 原来构造时就把 tooltip `appendChild(document.body)`，而全文件**没有任何回收路径**。
 * `rebuildCards()` 每次重建全部 `MachineCard`，每开一次设置窗跑两遍；调用点已从 16 涨到 24。
 * `settings-ia/STATUS.md` 里自己立过一条硬前置：「**必须先于任何页面化**」——
 * 而页面化（S4b-1/S4b-2）已经做完了，门被越过去了，且没有任何门禁会红。
 *
 * **但补一个 `destroy()` 是错的修法**：那要 24 个调用点**每一个**都记得调，
 * 而它们今天全都只是 `append(makeInfoIcon(...))`。一个靠 24 处自觉维持的不变量
 * 迟早会破，而且破了照样没人知道 —— 与原来的问题同构，只是把责任转嫁了。
 *
 * ⇒ 改成**结构上不可能泄漏**：tooltip **只在显示期间存在**。
 * 显示时才 `appendChild`，隐藏时 `remove()`。图标被销毁时 tooltip 本就不在 DOM 里，
 * 无需任何人做任何事。唯一兜不住的时序（**正显示着**的时候图标被销毁 ⇒
 * `mouseleave` 永不到来）由 `sweepOrphanTooltips()` 在下一次显示前扫掉，
 * 残留上限恒为 1 条。
 *
 * @param text tooltip 文本，支持 `\n` 换行
 * @returns `?` 图标元素，调用方 append 到 row 里就行（**不需要销毁**）
 */
export function makeInfoIcon(text: string): HTMLElement {
  const wrap = document.createElement("span");
  wrap.className = "settings-info-icon";
  wrap.setAttribute("aria-label", text);
  wrap.textContent = "?";

  let tip: HTMLElement | null = null;

  const showAndPosition = () => {
    sweepOrphanTooltips();
    if (!tip) {
      tip = document.createElement("span");
      tip.className = "settings-info-tooltip";
      tip.textContent = text;
    }
    if (!tip.isConnected) {
      // 关键：tip 加到 body，脱离 .settings-panel 的 transform 子树
      document.body.appendChild(tip);
      liveTooltips.set(tip, wrap);
    }
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
    if (!tip) return;
    // **摘出 DOM**，不是只 display:none —— 后者正是原来那条泄漏。
    liveTooltips.delete(tip);
    tip.remove();
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
