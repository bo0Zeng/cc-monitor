/**
 * `src/frontend/ui/kit/native-menu.ts` 的判据：浏览器自带右键菜单只在界面这一处管。
 *
 * 落在可编辑的地方（文本类输入框 · 多行框 · contenteditable）或此刻有选中文字 ⇒ 放行（剪切 · 复制 · 粘贴照用）；
 * 其余一律拦（不露返回 · 刷新 · 打印）。界面自己有菜单的那几处自己 `preventDefault`，与这里不打架。
 */
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { installNativeMenuGuard } from "../../../../src/frontend/ui/kit/native-menu";

/** 在 `el` 上右键：回浏览器自带那一份会不会出（`defaultPrevented` ⇒ 不出）。 */
function rightClick(el: Element): boolean {
  const e = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
  el.dispatchEvent(e);
  return !e.defaultPrevented;
}

function put<K extends keyof HTMLElementTagNameMap>(tag: K, set: (el: HTMLElementTagNameMap[K]) => void = () => {}): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  set(el);
  document.body.appendChild(el);
  return el;
}

beforeAll(() => installNativeMenuGuard(document));

afterEach(() => {
  window.getSelection()?.removeAllRanges();
  document.body.replaceChildren();
});

describe("浏览器自带右键菜单", () => {
  it("★ 空白处 · 普通文字 · 按钮上右键 ⇒ 拦", () => {
    expect(rightClick(document.body)).toBe(false);
    expect(rightClick(put("div", (d) => (d.textContent = "一段字")))).toBe(false);
    expect(rightClick(put("button"))).toBe(false);
    expect(rightClick(put("input", (i) => (i.type = "checkbox")))).toBe(false);
  });

  it("★ 文本类输入框 · 多行框 · contenteditable 里右键 ⇒ 放行（剪切 · 复制 · 粘贴）", () => {
    expect(rightClick(put("input"))).toBe(true);
    expect(rightClick(put("input", (i) => (i.type = "search")))).toBe(true);
    expect(rightClick(put("textarea"))).toBe(true);
    const ed = put("div", (d) => d.setAttribute("contenteditable", "true"));
    const inner = document.createElement("span");
    ed.appendChild(inner);
    expect(rightClick(inner)).toBe(true);
  });

  it("★ 有选中的文字 ⇒ 放行（复制）", () => {
    const p = put("p", (x) => (x.textContent = "选中这一段"));
    const r = document.createRange();
    r.selectNodeContents(p);
    window.getSelection()!.addRange(r);
    expect(rightClick(p)).toBe(true);
  });

  it("界面自己接了的那一处照它（自己 preventDefault）", () => {
    const own = put("input");
    own.addEventListener("contextmenu", (e) => e.preventDefault());
    expect(rightClick(own)).toBe(false);
  });
});
