/**
 * 按身份原地画：全界面**只这一处**管「重画之后，人手上正在做的不丢」。
 *
 * - [`place`]：容器的孩子换成这一串（节点由调用方按身份留着）—— 留着的原地不动、次序不对才挪、没了的摘掉；
 *   焦点若被挪丢了（浏览器挪节点会失焦）还回去。面板按身份留节点的（任务 · agent · 网格 · 标签栏）都调它。
 * - [`morph`]：整棵重画的（设置各页 · 编辑浮层 · 历史列表）把新树交给它：新树换进来，但**正打着字的那一格**
 *   （焦点所在 · 或改过没交：值不等于画出来时的值）留旧的那个节点 —— 字 · 光标 · 输入法都在。认同一格靠 `data-key`
 *   （新旧树里同一个 `data-key`、同一种元素）；新树里没有那个身份了 ⇒ 那一格跟着没。焦点在别的东西上（一行 · 一颗按钮）⇒
 *   新树里同一个 `data-key` 的那一个接过焦点（焦点在它里面也算：认最近的那个带 `data-key` 的祖先）。
 *
 * 判据：`tests/frontend/ui/kit/morph.vitest.ts`；别处不许自己再写一份（`tests/frontend/ui/morph-only.vitest.ts`）。
 */

/** 容器 `host` 的孩子换成 `items`（按这个次序）。 */
export function place(host: Node, items: readonly Node[]): void {
  const focused = document.activeElement;
  const hadFocus = focused instanceof HTMLElement && host.contains(focused);
  let cur: ChildNode | null = host.firstChild;
  for (const n of items) {
    if (n === cur) cur = cur.nextSibling;
    else host.insertBefore(n, cur);
  }
  while (cur) {
    const next: ChildNode | null = cur.nextSibling;
    cur.remove();
    cur = next;
  }
  if (hadFocus) refocus(focused, null);
}

type Field = HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement;

/** 改过没交：值不等于画出来时的值（勾选框看勾没勾）。 */
function dirty(f: Field): boolean {
  if (f instanceof HTMLSelectElement) return [...f.options].some((o) => o.selected !== o.defaultSelected);
  if (f instanceof HTMLInputElement && (f.type === "checkbox" || f.type === "radio")) return f.checked !== f.defaultChecked;
  return f.value !== f.defaultValue;
}

/** `host` 的孩子换成新树 `fresh`；正打着字的那几格留旧节点（见文件头）。`keepScroll` ⇒ 换完把 `host` 的滚动位置还回去。 */
export function morph(host: Element, fresh: readonly Node[], opts: { keepScroll?: boolean } = {}): void {
  const focused = document.activeElement instanceof HTMLElement && host.contains(document.activeElement) ? document.activeElement : null;
  const held = new Map<string, HTMLElement>();
  for (const f of host.querySelectorAll<Field>("input[data-key], textarea[data-key], select[data-key]"))
    if (f === focused || dirty(f)) held.set(`${f.tagName}\u0000${f.dataset.key}`, f);
  if (focused?.dataset.key !== undefined) held.set(`${focused.tagName}\u0000${focused.dataset.key}`, focused);
  const sel = focused instanceof HTMLInputElement || focused instanceof HTMLTextAreaElement ? selectionOf(focused) : null;
  const top = host.scrollTop;
  if (held.size > 0) {
    const swap = (n: Node): void => {
      if (!(n instanceof Element)) return;
      const nodes = [n, ...n.querySelectorAll<HTMLElement>("[data-key]")];
      for (const e of nodes) {
        if (!(e instanceof HTMLElement) || e.dataset.key === undefined) continue;
        const old = held.get(`${e.tagName}\u0000${e.dataset.key}`);
        if (old && old !== e) e.replaceWith(old);
      }
    };
    fresh.forEach(swap);
  }
  // 新树里的那个就是旧节点本身（调用方把它留着了）⇒ 照样放；replaceChildren 会把它先摘下再放回。
  host.replaceChildren(...fresh);
  if (opts.keepScroll) host.scrollTop = top;
  if (!focused) return;
  if (host.contains(focused)) return refocus(focused, sel);
  // 焦点在一个不是输入框的上面（一行 · 一颗按钮）⇒ 新树里同一个身份的那一个接过焦点。
  const key = focused.closest<HTMLElement>("[data-key]")?.dataset.key;
  const next = key === undefined ? null : [...host.querySelectorAll<HTMLElement>("[data-key]")].find((e) => e.dataset.key === key);
  if (next) refocus(next, null);
}

function selectionOf(f: HTMLInputElement | HTMLTextAreaElement): [number, number] | null {
  try {
    return f.selectionStart === null || f.selectionEnd === null ? null : [f.selectionStart, f.selectionEnd];
  } catch {
    return null; // 不支持选区的那几种 input（number · email …）
  }
}

function refocus(el: HTMLElement, sel: [number, number] | null): void {
  if (!el.isConnected || document.activeElement === el) return;
  el.focus({ preventScroll: true });
  if (sel && (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) {
    try {
      el.setSelectionRange(sel[0], sel[1]);
    } catch {
      // 同上
    }
  }
}
