/**
 * 「按身份原地画 · 重画后保焦点」全界面只住 `kit/morph.ts` 一处（A17-16）：别的面板再自己写一份就红。
 *
 * 认两种手写的样子（就是 A17-16 删掉的那九处的样子）：
 * ① 手写原地摆：`insertBefore(` 前后几行里有 `nextSibling` 游标（「留着的不动、不在位才挪」那一圈）⇒ 用 kit `place`。
 * ② 手写保焦点：整块重画（`replaceChildren(` · `swapPopoverContent(`）之前几行记下 `document.activeElement`、之后几行又 `.focus(` ⇒ 用 kit `morph`。
 * 正控：两种样子各一段合成源码要认得出，`kit/morph.ts` 自己那几行不算。
 */
import { describe, it, expect } from "vitest";
import { productionTsFiles } from "../../test-support/production-sources";

const HOME = "src/frontend/ui/kit/morph.ts";
const WINDOW = 12;

/** 一份源码里手写的那两种样子：`形 行号`。 */
function handWritten(text: string): string[] {
  const lines = text.split("\n");
  const near = (i: number, back: number, ahead: number, re: RegExp): boolean => lines.slice(Math.max(0, i - back), i + ahead).some((l) => re.test(l));
  const hits: string[] = [];
  lines.forEach((l, i) => {
    if (/\binsertBefore\(/.test(l) && near(i, 6, 7, /\bnextSibling\b/)) hits.push(`手写原地摆 ${i + 1}`);
    if (/\b(replaceChildren|swapPopoverContent)\(/.test(l) && near(i, WINDOW, 0, /document\.activeElement/) && near(i, 0, WINDOW, /\.focus\(/))
      hits.push(`手写保焦点 ${i + 1}`);
  });
  return hits;
}

describe("按身份原地画只住 kit/morph.ts", () => {
  it("正控：两种手写的样子都认得出；只记 activeElement 不重画的不算", () => {
    const place = ["let cur = host.firstChild;", "for (const n of items) {", "  if (n === cur) cur = cur.nextSibling;", "  else host.insertBefore(n, cur);", "}"].join("\n");
    const focus = ["const was = document.activeElement;", "root.replaceChildren(b);", "if (was) find(was)?.focus();"].join("\n");
    const pop = ["const focused = old.contains(document.activeElement);", "swapPopoverContent(anchor, next);", "if (focused) next.focus();"].join("\n");
    expect(handWritten(place)).toEqual(["手写原地摆 4"]);
    expect(handWritten(focus)).toEqual(["手写保焦点 2"]);
    expect(handWritten(pop)).toEqual(["手写保焦点 2"]);
    expect(handWritten("const a = document.activeElement;\nbox.replaceChildren(x);")).toEqual([]);
  });

  it("kit/morph.ts 之外零命中", () => {
    const files = productionTsFiles("src/frontend").filter((f) => !f.file.endsWith(".d.css.ts"));
    expect(files.some((f) => f.file === HOME), "kit/morph.ts 不在扫的那批里 —— 路径变了").toBe(true);
    const hits = files.filter((f) => f.file !== HOME).flatMap((f) => handWritten(f.text).map((h) => `${f.file} ${h}`));
    expect(hits, "又有一处自己写了一份原地画 / 保焦点 —— 换成 kit/morph.ts 的 place / morph").toEqual([]);
  });
});
