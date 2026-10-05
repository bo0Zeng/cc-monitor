/**
 * 一个组件一个家：对话框 · 弹出菜单 · 悬停提示 · toast 栈这几样，全仓只在 `src/frontend/ui/kit/` 里建（零命中 ＋ 正控）。
 *
 * 认法（生产 TS，注释不算）：给元素挂这几种角色（`role="dialog" / "alertdialog" / "menu" / "menuitem*" / "tooltip"`）·
 * `aria-modal` · 自己建 toast 栈 · 自己在 `document` / `window` 上挂 `pointerdown` 去关一个浮层（「点外面关」那一套只许菜单里有）。
 */
import { describe, it, expect } from "vitest";
import { productionTsFiles, SCAN_TIMEOUT_MS } from "../../../test-support/production-sources.ts";
import { stripCodeComments } from "../../../evidence/S25-class-ledger.ts";

const NEEDLES: { what: string; re: RegExp }[] = [
  { what: "对话框角色", re: /setAttribute\(\s*"role"\s*,\s*"(?:alert)?dialog"/ },
  { what: "模态标记", re: /"aria-modal"/ },
  { what: "菜单角色", re: /setAttribute\(\s*"role"\s*,\s*"menu(?:item\w*)?"/ },
  { what: "悬停提示角色", re: /setAttribute\(\s*"role"\s*,\s*"tooltip"/ },
  { what: "toast 栈", re: /toast-stack/ },
  { what: "点外面关浮层", re: /(?:document|window)\.addEventListener\(\s*"pointerdown"/ },
];

function hitsOf(file: string, text: string): string[] {
  const code = stripCodeComments(text);
  return NEEDLES.filter((n) => n.re.test(code)).map((n) => `${file}：${n.what}`);
}

const FILES = productionTsFiles("src");
const inKit = (f: string): boolean => f.startsWith("src/frontend/ui/kit/");

describe("一个组件一个家（对话框 · 菜单 · 悬停提示 · toast）", () => {
  it(
    "kit 之外零命中",
    () => {
      expect(FILES.length).toBeGreaterThan(100);
      expect(FILES.filter((f) => !inKit(f.file)).flatMap((f) => hitsOf(f.file, f.text))).toEqual([]);
    },
    SCAN_TIMEOUT_MS,
  );

  it("🔴 正控：同一把尺子在 kit 里认得出每一样（不然上面是空转），注释里写的不算", () => {
    const kit = FILES.filter((f) => inKit(f.file)).flatMap((f) => hitsOf(f.file, f.text));
    for (const n of NEEDLES) expect(kit.some((h) => h.endsWith(`：${n.what}`)), `kit 里认不出「${n.what}」`).toBe(true);
    expect(hitsOf("x.ts", '// el.setAttribute("role", "menu")\nconst a = 1;')).toEqual([]);
    expect(hitsOf("x.ts", 'el.setAttribute("role", "menuitemradio");')).toEqual(["x.ts：菜单角色"]);
  });
});
