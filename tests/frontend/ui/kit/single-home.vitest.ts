/**
 * 一个组件一个家：对话框 · 弹出菜单 · 悬停提示 · toast 栈这几样，全仓只在 `src/frontend/ui/kit/` 里建（零命中 ＋ 正控）。
 *
 * 认法（生产 TS，注释不算）：给元素挂这几种角色（`role="dialog" / "alertdialog" / "menu" / "menuitem*" / "tooltip"`）·
 * `aria-modal` · 自己建 toast 栈 · 自己在 `document` / `window` 上挂 `pointerdown` 去关一个浮层（「点外面关」那一套只许菜单里有）。
 *
 * 按钮与建元素的小函数也各一个家：按钮只经 `kit/button.ts`（设置页另一套按钮类在 TS 与 CSS 里都零命中），
 * `el()` 只有 `kit/el.ts` 那一份（kit 外不许再写本地 `function el(` / `const el = (`）。
 */
import { describe, it, expect } from "vitest";
import { productionCssFiles, productionTsFiles } from "../../../test-support/production-sources.ts";
import { stripComments } from "../../../test-support/strip-comments.ts";

const NEEDLES: { what: string; re: RegExp }[] = [
  { what: "对话框角色", re: /setAttribute\(\s*"role"\s*,\s*"(?:alert)?dialog"/ },
  { what: "模态标记", re: /"aria-modal"/ },
  { what: "菜单角色", re: /setAttribute\(\s*"role"\s*,\s*"menu(?:item\w*)?"/ },
  { what: "悬停提示角色", re: /setAttribute\(\s*"role"\s*,\s*"tooltip"/ },
  { what: "toast 栈", re: /toast-stack/ },
  { what: "点外面关浮层", re: /(?:document|window)\.addEventListener\(\s*"pointerdown"/ },
];

function hitsOf(file: string, text: string): string[] {
  const code = stripComments(text, "ts");
  return NEEDLES.filter((n) => n.re.test(code)).map((n) => `${file}：${n.what}`);
}

const FILES = productionTsFiles("src");
const inKit = (f: string): boolean => f.startsWith("src/frontend/ui/kit/");

describe("一个组件一个家（对话框 · 菜单 · 悬停提示 · toast）", () => {
  it(
    "kit 之外零命中",
    () => {
      expect(FILES.filter((f) => !inKit(f.file)).flatMap((f) => hitsOf(f.file, f.text))).toEqual([]);
    },
  );

  it("🔴 正控：同一把尺子在 kit 里认得出每一样（不然上面是空转），注释里写的不算", () => {
    const kit = FILES.filter((f) => inKit(f.file)).flatMap((f) => hitsOf(f.file, f.text));
    for (const n of NEEDLES) expect(kit.some((h) => h.endsWith(`：${n.what}`)), `kit 里认不出「${n.what}」`).toBe(true);
    expect(hitsOf("x.ts", '// el.setAttribute("role", "menu")\nconst a = 1;')).toEqual([]);
    expect(hitsOf("x.ts", 'el.setAttribute("role", "menuitemradio");')).toEqual(["x.ts：菜单角色"]);
  });
});

/** 设置页不许再有的那套按钮类（拼出来写，免得本文件自己成了全仓 grep 的一处命中）。 */
const OLD_BTN = ["settings", "btn"].join("-");

const PIECE_NEEDLES: { what: string; re: RegExp }[] = [
  { what: "设置页按钮类", re: new RegExp(OLD_BTN) },
  { what: "本地 el()", re: /\bfunction\s+el\s*[<(]|\bconst\s+el\s*=\s*(?:<[^>]*>\s*)?\(/ },
];

function pieceHitsOf(file: string, text: string): string[] {
  const code = stripComments(text, "ts");
  return PIECE_NEEDLES.filter((n) => n.re.test(code)).map((n) => `${file}：${n.what}`);
}

/** CSS 里只认选择器里写的那个类（注释去掉）。 */
function cssHitsOf(file: string, text: string): string[] {
  const code = stripComments(text, "css");
  return new RegExp(`\\.${OLD_BTN}\\b`).test(code) ? [`${file}：设置页按钮类`] : [];
}

describe("按钮只经 kit/button · el() 只留 kit 那一份", () => {
  it("kit 之外零命中（TS 与 CSS）", () => {
    const ts = FILES.filter((f) => !inKit(f.file)).flatMap((f) => pieceHitsOf(f.file, f.text));
    const css = productionCssFiles("src").flatMap((f) => cssHitsOf(f.file, f.text));
    expect([...ts, ...css]).toEqual([]);
  });

  it("🔴 正控：kit 那份 el() 认得出；两样写法各认得出；注释里写的不算", () => {
    const kitEl = FILES.find((f) => f.file === "src/frontend/ui/kit/el.ts");
    expect(kitEl, "kit/el.ts 不在").toBeDefined();
    expect(pieceHitsOf(kitEl!.file, kitEl!.text)).toEqual([`${kitEl!.file}：本地 el()`]);
    expect(pieceHitsOf("x.ts", `b.className = "${OLD_BTN}";`)).toEqual(["x.ts：设置页按钮类"]);
    expect(pieceHitsOf("x.ts", "function el<K extends keyof HTMLElementTagNameMap>(tag: K) {}")).toEqual(["x.ts：本地 el()"]);
    expect(pieceHitsOf("x.ts", "const el = (tag: string) => document.createElement(tag);")).toEqual(["x.ts：本地 el()"]);
    expect(pieceHitsOf("x.ts", "const el = document.activeElement;\n// function el(\nconst a = 1;")).toEqual([]);
    expect(cssHitsOf("x.css", `.${OLD_BTN}:hover { color: red; }`)).toEqual(["x.css：设置页按钮类"]);
    expect(cssHitsOf("x.css", `/* .${OLD_BTN} */ .a { color: red; }`)).toEqual([]);
  });
});
