/**
 * 折叠块：标题行 28，左侧箭头（右指收、下指开），右侧摘要。Enter / 空格切换，→ 展开、← 收起。
 * 展开状态由调用方按对象记住（`open` 传进来），实时更新时不收起。
 * 判据：`tests/frontend/ui/kit/components.vitest.ts`「折叠块」那一节；箭头一颗图、展开转 90° 由 `tests/frontend/ui/icons-one-source.vitest.ts` 钉。
 */
import { icon } from "./icon";
import s from "./fold.module.css";

export interface FoldSpec {
  title: string;
  summary?: string;
  open: boolean;
  body: HTMLElement;
  onToggle?: (open: boolean) => void;
  /** 不带框的一行（表单里「› 更多：…」那一类）：灰字、展开的内容不另起框。 */
  bare?: boolean;
}

const summaries = new WeakMap<HTMLElement, HTMLElement>();

/** 换折叠块标题行右侧那一句（现值随数据变时）。 */
export function setFoldSummary(root: HTMLElement, text: string): void {
  const sum = summaries.get(root);
  if (sum && sum.textContent !== text) {
    sum.textContent = text;
    sum.title = text;
  }
}

/** 折叠号：同一颗右指箭头，所在元素 `aria-expanded="true"` 时转 90° 下指（能展开的行都用它，不换图）。 */
export function foldCaret(): SVGSVGElement {
  const c = icon("caretRight", "compact");
  c.classList.add(s.foldCaret);
  return c;
}

export function fold(spec: FoldSpec): HTMLDivElement {
  const root = document.createElement("div");
  root.className = s.fold;
  if (spec.bare) root.dataset.bare = "true";
  const head = document.createElement("button");
  head.type = "button";
  head.className = s.foldHead;
  head.dataset.foldHead = "";
  head.appendChild(foldCaret());
  const t = document.createElement("span");
  t.className = s.foldTitle;
  t.textContent = spec.title;
  head.appendChild(t);
  const sum = document.createElement("span");
  sum.className = s.foldSummary;
  sum.textContent = spec.summary ?? "";
  sum.title = sum.textContent;
  summaries.set(root, sum);
  head.appendChild(sum);
  const body = document.createElement("div");
  body.className = s.foldBody;
  body.appendChild(spec.body);
  root.append(head, body);
  const set = (open: boolean): void => {
    head.setAttribute("aria-expanded", String(open));
    body.hidden = !open;
  };
  set(spec.open);
  const toggle = (open: boolean): void => {
    if (open === (head.getAttribute("aria-expanded") === "true")) return;
    set(open);
    spec.onToggle?.(open);
  };
  head.addEventListener("click", () => toggle(head.getAttribute("aria-expanded") !== "true"));
  head.addEventListener("keydown", (ev) => {
    if (ev.isComposing) return;
    if (ev.key === "ArrowRight") toggle(true);
    else if (ev.key === "ArrowLeft") toggle(false);
  });
  return root;
}
