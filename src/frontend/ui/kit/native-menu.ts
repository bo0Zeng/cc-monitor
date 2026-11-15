/**
 * **浏览器自带右键菜单只在这一处管**（四个窗口都由 `entry-common.ts` 装）。
 *
 * 落在可编辑的地方（文本类输入框 · 多行框 · contenteditable）或此刻有选中的文字 ⇒ 放行：剪切 · 复制 · 粘贴照用；
 * 其余一律拦：不露返回 · 刷新 · 另存为 · 打印这类网页那一套。界面自己有菜单的那几处（栏顶 · 标签页 · 会话头 · 历史行）
 * 自己 `preventDefault` 接走，与这里不打架。壳不关原生菜单（关了输入框里的那一份也没了）。
 * 判据：`tests/frontend/ui/kit/native-menu.vitest.ts`。
 */

/** 不是文本的那几种 `<input>`（勾选 · 按钮 · 滑条 …）：右键没有可剪切 · 粘贴的。 */
const NOT_TEXT = new Set(["checkbox", "radio", "button", "submit", "reset", "range", "color", "file", "image", "hidden"]);

/** 右键落在这里时该不该放行自带那一份。 */
function editable(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  if (target instanceof HTMLTextAreaElement) return true;
  if (target instanceof HTMLInputElement) return !NOT_TEXT.has(target.type);
  // 可编辑与否随最近那一层写了 contenteditable 的祖先（写 false 的那一层截断）。
  const host = target.closest("[contenteditable]");
  return host !== null && host.getAttribute("contenteditable") !== "false";
}

export function installNativeMenuGuard(doc: Document): void {
  doc.addEventListener("contextmenu", (e) => {
    if (e.defaultPrevented || editable(e.target)) return;
    const sel = doc.getSelection();
    if (sel !== null && !sel.isCollapsed && sel.toString() !== "") return;
    e.preventDefault();
  });
}
