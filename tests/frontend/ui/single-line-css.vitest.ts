/**
 * 单行文字不压按钮（10-08 用户：会话名字太长时跟右边那几个按钮叠在一起）。
 *
 * 浏览器里量外接框的那一半住截图工具（`tests/shots/scenes/layout-check.ts`，每张图都量）；门禁里没有排版引擎，
 * 这里钉住让它成立的那几条样式：
 *
 * ① 单行省略只住一处：`styles/shared.css` 的全局类 `.one-line`（不折行 · 超出藏起 · 省略号）。CSS 模块用
 *    `composes: one-line from global`，全局 CSS 画的那几处在 TS 里挂这个类 ⇒ 别处再写一份 `text-overflow` 就红。
 * ② 登记的右侧按钮组：不缩（宽度固定，永远不被压）。
 * ③ 标签页行尾按钮盖在标题尾巴上：底色每一层都垫着不透的 `--bg-2`（只写半透明的选中色 ⇒ 标题字透出来，就是那次撞见的样子）。
 * ④ 悬停给全名只住一处：单行件截断了，悬停出它自己的正文由 `kit/tooltip.ts` 给（行为测在 `kit/tooltip.vitest.ts`）⇒
 *    TS 里再写 `X.title = <X 自己的正文>` 就红。显示的不是全文、悬停给另一句的是说明，照旧写 title。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import postcss, { type Rule } from "postcss";
import { nonEmpty } from "../../test-support/non-empty";
import { sourceFiles } from "../../test-support/production-sources";

const UI = resolve(__dirname, "../../../src/frontend/ui");
const HOME = "src/frontend/ui/styles/shared.css";

/** 一份 CSS 里，选择器恰好是 `sel`（逗号分开的任一段）的规则里各声明的最后取值。 */
function declsOf(file: string, sel: string): Map<string, string> {
  const out = new Map<string, string>();
  postcss.parse(readFileSync(resolve(UI, file), "utf8")).walkRules((r: Rule) => {
    if (!r.selectors.map((x) => x.trim()).includes(sel)) return;
    r.walkDecls((d) => void out.set(d.prop, d.value));
  });
  return out;
}


/** 一段 CSS 里自己写的省略号：`选择器:行号`。`isHome` 时 `.one-line` 那一家不算。 */
function ownEllipsis(css: string, isHome: boolean): string[] {
  const hits: string[] = [];
  postcss.parse(css).walkDecls("text-overflow", (d) => {
    const sel = (d.parent as Rule).selector;
    if (isHome && sel === ".one-line") return;
    hits.push(`${sel}:${d.source?.start?.line}`);
  });
  return hits;
}

/** 一段 TS 里内联写的省略号（两种写法）。 */
const TS_ELLIPSIS = /textOverflow|text-overflow/;

const files = sourceFiles("src", { ext: [".ts", ".css"], generated: true });
const moduleCss = files.filter((f) => f.file.endsWith(".module.css"));

/** ② 右侧按钮组：文件 · 选择器 · 是什么。 */
const FIXED_BUTTONS: [string, string, string][] = [
  ["session-head.module.css", ".shExtra", "会话头［恢复 ▾］那一格"],
  ["session-head.module.css", ".shActs", "会话头右侧按钮"],
  ["styles.css", ".tab-trail", "标签页行尾徽标"],
  ["kit/fold.module.css", ".foldTitle", "折叠块标题（窄了让摘要先省略，标题不挤成竖排）"],
];

describe("单行文字不压按钮", () => {
  it("① 单行省略的家：shared.css 的 .one-line 恰好是 不折行 · 藏起 · 省略号", () => {
    expect(declsOf("styles/shared.css", ".one-line")).toEqual(
      new Map([
        ["overflow", "hidden"],
        ["white-space", "nowrap"],
        ["text-overflow", "ellipsis"],
      ]),
    );
  });

  it("① 正控：扫描认得出自己写的一份，也认得出家", () => {
    expect(ownEllipsis(".a { overflow: hidden; text-overflow: ellipsis; }", false)).toEqual([".a:1"]);
    expect(ownEllipsis(".one-line { text-overflow: ellipsis; }", false)).toEqual([".one-line:1"]);
    expect(ownEllipsis(".one-line { text-overflow: ellipsis; }", true)).toEqual([]);
    expect(TS_ELLIPSIS.test('el.style.textOverflow = "ellipsis"')).toBe(true);
  });

  it("① 别处的 CSS 里没有自己写的省略号（模块 composes: one-line from global，全局 CSS 由 TS 挂 one-line）", () => {
    const hits = nonEmpty(files.filter((f) => f.file.endsWith(".css"))).flatMap((f) => ownEllipsis(f.text, f.file === HOME).map((h) => `${f.file} ${h}`));
    expect(hits).toEqual([]);
  });

  it("① TS 里没有内联写的省略号", () => {
    const ts = nonEmpty(files.filter((f) => f.file.endsWith(".ts") && !f.file.endsWith(".d.css.ts")));
    expect(ts.filter((f) => TS_ELLIPSIS.test(f.text)).map((f) => f.file)).toEqual([]);
  });

  it("① 模块里的 one-line 都写成 from global（不是本地同名类）", () => {
    const uses: string[] = [];
    for (const f of moduleCss) {
      postcss.parse(f.text).walkDecls("composes", (d) => {
        if (/\bone-line\b/.test(d.value)) uses.push(`${f.file}:${d.source?.start?.line} ${d.value.trim()}`);
      });
    }
    nonEmpty(uses);
    expect(uses.filter((u) => !u.endsWith(" one-line from global"))).toEqual([]);
  });

  it.each(FIXED_BUTTONS)("② %s %s（%s）：不缩", (file, sel) => {
    const d = declsOf(file, sel);
    const flex = d.get("flex");
    expect(flex === "none" || d.get("flex-shrink") === "0" || /^\S+ 0( |$)/.test(flex ?? ""), `${sel} 的 flex = ${flex}`).toBe(true);
  });

  it("③ 标签页行尾按钮：每条给它上底色的规则，最底下那一层都是不透的 --bg-2", () => {
    const root = postcss.parse(readFileSync(resolve(UI, "styles.css"), "utf8"));
    const backgrounds: string[] = [];
    root.walkRules((r: Rule) => {
      if (!r.selectors.some((x) => /\.tab-acts$/.test(x.trim()))) return;
      r.walkDecls(/^background(-color)?$/, (d) => void backgrounds.push(`${r.selector} → ${d.value}`));
    });
    nonEmpty(backgrounds);
    for (const b of backgrounds) expect(b, "最底层不是 --bg-2 ⇒ 半透明，标题字会透出来").toMatch(/(^|,)\s*var\(--bg-2\)\s*$/);
  });
});

/** 一段 TS 里「title 就是自己的正文」那几行：`行号: 那一句`。三种写法：`X.title = X.textContent` · 同一份里 `X.textContent = 同一式` · `const X = el(…, 同一式)`。 */
function ownTextTitles(ts: string): string[] {
  const hits: string[] = [];
  for (const m of ts.matchAll(/([\w$.]+)\.title = ([^;\n]+);/g)) {
    const [whole, rx, rhs] = m as unknown as [string, string, string];
    const esc = (x: string): string => x.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const same =
      rhs === `${rx}.textContent` ||
      rhs === `${rx}.textContent ?? ""` ||
      new RegExp(`${esc(rx)}\\.textContent = ${esc(rhs)};`).test(ts) ||
      new RegExp(`const ${esc(rx)} = el\\([^;]*, ${esc(rhs)}\\);`).test(ts);
    if (same) hits.push(`${ts.slice(0, m.index).split("\n").length}: ${whole}`);
  }
  return hits;
}

describe("悬停给全名只住 kit 一处", () => {
  it("④ 正控：认得出三种写法，说明（另一句）不算", () => {
    expect(ownTextTitles("a.title = a.textContent;")).toEqual(["1: a.title = a.textContent;"]);
    expect(ownTextTitles('a.b.textContent = name;\na.b.title = name;')).toEqual(["2: a.b.title = name;"]);
    expect(ownTextTitles('const c = el("span", "x", machineName(m));\nc.title = machineName(m);')).toEqual(["2: c.title = machineName(m);"]);
    expect(ownTextTitles('a.textContent = name;\na.title = copyText("x.hint");')).toEqual([]);
  });

  it("④ TS 里没有「title ＝ 自己正文」（单行件截断了，悬停全名由 kit/tooltip 给）", () => {
    const ts = nonEmpty(files.filter((f) => f.file.endsWith(".ts") && !f.file.endsWith(".d.css.ts")));
    expect(ts.flatMap((f) => ownTextTitles(f.text).map((h) => `${f.file}:${h}`))).toEqual([]);
  });
});
