/**
 * 按身份原地画：全界面**只这一处**管「重画之后，人手上正在做的不丢」。
 *
 * - [`place`]：容器的孩子换成这一串（节点由调用方按身份留着）—— 留着的原地不动、次序不对才挪、没了的摘掉；
 *   焦点若被挪丢了（浏览器挪节点会失焦）还回去。面板按身份留节点的（任务 · agent · 网格 · 标签栏）都调它。
 * - [`morph`]：整棵重画的（设置各页 · 编辑浮层 · 历史列表 · 轮换板）把新树交给它：新树整棵换进来，但**正打着字的那一格**
 *   （焦点所在 · 或改过没交：值不等于画出来时的值）把人手上的值 · 光标 · 焦点搬到新树里同一格上。认同一格靠 `data-key`
 *   （新旧树里同一个 `data-key`、同一种元素）；新树里没有那个身份了 ⇒ 那一格跟着没。焦点在别的东西上（一行 · 一颗按钮）⇒
 *   新树里同一个 `data-key` 的那一个接过焦点（焦点在它里面也算：认最近的那个带 `data-key` 的祖先）。
 *
 * 为什么搬值、不留旧节点：新树里的按钮 · 回调认的是新树里那一格（闭包捕获的是新画的节点与新那份数据），留旧节点 ⇒
 * 同一行的［发送］读到的是没人打过字的那一格，改完交上去的是旧那份表单。画出来时的值记在 `defaultValue` 上（基准）。
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

const FIELDS = "input[data-key], textarea[data-key], select[data-key]";
const isField = (e: Element): e is Field => e instanceof HTMLInputElement || e instanceof HTMLTextAreaElement || e instanceof HTMLSelectElement;
const isCheck = (f: Field): f is HTMLInputElement => f instanceof HTMLInputElement && (f.type === "checkbox" || f.type === "radio");
const idOf = (f: Field): string => `${f.tagName}\u0000${f.dataset.key}`;

/** 人手上那一格的值（勾选框是勾没勾；多选下拉是选中的那几项）。 */
type Held = { checked?: boolean; value?: string; picked?: string[]; sel: [number, number] | null };

function hold(f: Field): Held {
  if (isCheck(f)) return { checked: f.checked, sel: null };
  if (f instanceof HTMLSelectElement) return { picked: [...f.selectedOptions].map((o) => o.value), sel: null };
  return { value: f.value, sel: selectionOf(f) };
}

/** 记下画出来时的值（之后跟它比，就知道人改没改过）。 */
function stamp(f: Field): void {
  if (isCheck(f)) f.defaultChecked = f.checked;
  else if (f instanceof HTMLSelectElement) for (const o of f.options) o.defaultSelected = o.selected;
  else f.defaultValue = f.value;
}

/** 改过没交：值不等于画出来时的值。 */
function dirty(f: Field): boolean {
  if (f instanceof HTMLSelectElement) return [...f.options].some((o) => o.selected !== o.defaultSelected);
  if (isCheck(f)) return f.checked !== f.defaultChecked;
  return f.value !== f.defaultValue;
}

function put(f: Field, h: Held): void {
  if (isCheck(f) && h.checked !== undefined) f.checked = h.checked;
  else if (f instanceof HTMLSelectElement && h.picked) for (const o of f.options) o.selected = h.picked.includes(o.value);
  else if (h.value !== undefined && !(f instanceof HTMLSelectElement)) f.value = h.value;
}

/** `host` 的孩子换成新树 `fresh`；正打着字的那几格把值搬过去（见文件头）。`keepScroll` ⇒ 换完把 `host` 的滚动位置还回去；`fallback` ⇒ 焦点原在里面、新树里找不到同一个身份时交给它。 */
export function morph(host: Element, fresh: readonly Node[], opts: { keepScroll?: boolean; fallback?: HTMLElement } = {}): void {
  const focused = document.activeElement instanceof HTMLElement && host.contains(document.activeElement) ? document.activeElement : null;
  const held = new Map<string, Held>();
  for (const f of host.querySelectorAll<Field>(FIELDS)) if (f === focused || dirty(f)) held.set(idOf(f), hold(f));
  const focusId = focused && isField(focused) && focused.dataset.key !== undefined ? idOf(focused) : null;
  const focusKey = focused?.closest<HTMLElement>("[data-key]")?.dataset.key;
  const top = host.scrollTop;
  let take: HTMLElement | null = null;
  for (const n of fresh) {
    if (!(n instanceof Element)) continue;
    for (const f of [...(isField(n) && n.dataset.key !== undefined ? [n] : []), ...n.querySelectorAll<Field>(FIELDS)]) {
      if (host.contains(f)) continue; // 调用方留着的旧节点：值 · 基准都是它自己的
      stamp(f);
      const h = held.get(idOf(f));
      if (h) put(f, h);
      if (focusId !== null && idOf(f) === focusId) take = f;
    }
  }
  host.replaceChildren(...fresh);
  if (opts.keepScroll) host.scrollTop = top;
  if (!focused) return;
  if (host.contains(focused)) return refocus(focused, focusId === null ? null : (held.get(focusId)?.sel ?? null)); // 调用方把那个节点本身留在新树里了
  if (take) return refocus(take, held.get(focusId!)?.sel ?? null);
  // 焦点在一个不是输入框的上面（一行 · 一颗按钮）⇒ 新树里同一个身份的那一个接过焦点。
  const next = focusKey === undefined ? null : [...host.querySelectorAll<HTMLElement>("[data-key]")].find((e) => e.dataset.key === focusKey);
  if (next) refocus(next, null);
  else if (opts.fallback) refocus(opts.fallback, null);
}

function selectionOf(f: HTMLInputElement | HTMLTextAreaElement): [number, number] | null {
  try {
    return f.selectionStart === null || f.selectionEnd === null ? null : [f.selectionStart, f.selectionEnd];
  } catch {
    return null; // 不支持选区的那几种 input（number · email …）
  }
}

function refocus(el: HTMLElement, sel: [number, number] | null): void {
  if (!el.isConnected) return;
  if (document.activeElement !== el) el.focus({ preventScroll: true });
  if (sel && (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) {
    try {
      el.setSelectionRange(sel[0], sel[1]);
    } catch {
      // 同上
    }
  }
}
