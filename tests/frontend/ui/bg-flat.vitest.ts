/**
 * 〔BG1〕tab 代码里「按 bg 分叉」只住登记的那几处；tab 栏通用代码零处。
 *
 * ## 守的要求（住址）
 *
 * - 用户裁决 `设计/99 §1` **V125**〔选〕「删掉树」，原话：「后台（bg）会话不再自动挂到宿主下排成树：
 *   删 `placeInTree` 树状挂载、`dragBlockOf` 拖拽例外、`.tab-bg` 那套；bg 会话就是普通 tab、平铺，
 *   只留「显示 bg 会话」开关 —— 与 `30 §7`「不做自动归组、集合是唯一分类维」一致」。
 * - `设计/30 §7`：「自动归组 / 自动固定 | 同一条先例：『手动建，不要自动，纯手动』」。
 * - `设计/30 §1` 不变量 3：「**正交而非枚举** —— 会话状态 · `pinned` · 集合归属（你怎么分类），各管一件事。」
 *
 * ## 两条判据
 *
 * **B1 · TS**：人群 = 盘上 `src/frontend/ui/tabs.ts` ＋ `src/tab-*.ts` 全部（现派生，不手写名单）。走 TS 的 AST
 *   （注释天然不在 AST 里），针 = 字符串字面量 `"bg"` / `"interactive"` / 含 `tab-bg` 的串 ·
 *   标识符 `isBg…` · `bgName`（`import` 语句里的不算）。命中按 `(文件, 所在的顶层声明 / 类方法)` 归并，
 *   **与 `REGISTERED` 两向相等**。tab 栏通用代码（落位 `tab-store` · 拖拽 `tab-bar-drag` · 落点算术
 *   `tab-drop` · 渲染 `tab-bar-view` · 集合 `tab-collections`）在表里**一行都没有** ⇒ 长出来就红。
 *   `bgName` 算针是因为 `if (tab.bgName)` 同样是按 bg 分叉、却不带字面量；于是只搬运它的几处也得登记。
 *
 * **B2 · CSS**：`src/**` 下全部 `.css`（现派生）剥 `/* … *\/` 注释后，类选择器 `.tab-bg` 零处。
 *   正控：同一个扫描器在同一批文件里找得到 `.drop-onto`（`设计/30 §D.5` 的拖动描边类，tab 栏样式真在扫描面里）。
 *
 * ## 人群与同波别路
 *
 * 同波 FE1 / CFG1 / W5-UI 若在 `src/frontend/ui/tabs.ts` / `src/tab-*.ts` 里新写 `bgName` / `"bg"` / `"interactive"`，
 * 或把登记的那几处挪进别的函数 / 别的文件 ⇒ B1 红。修法：确认它不是按 bg 分叉（只搬运）⇒ 登记一行并写理由；
 * 是分叉 ⇒ 别在 tab 代码里分，或先问 V125。
 *
 * ⚠ 行为那一半（拖拽 / 集合 / 落位对 bg tab 与普通 tab 相同）在 `tests/frontend/ui/tabs.vitest.ts` 末尾
 *   「〔BG1〕」`describe.each`：本文件只管「代码里没有分叉的地方」，那组管「分叉即便换了写法也看得见」。
 */
import ts from "typescript";
import { describe, it, expect } from "vitest";
import { productionCssFiles, productionTsFiles } from "../../test-support/production-sources.ts";

/** `(文件, 所在声明)` —— 今天 tab 代码里按 bg 分叉 / 搬运 bg 字段的全部住处，逐条写理由。 */
const REGISTERED: ReadonlyArray<readonly [string, string, string]> = [
  ["src/frontend/ui/tab-model.ts", "Tab", "字段声明：bg 任务名（pidfile 的 name）"],
  ["src/frontend/ui/tab-model.ts", "isBgKind", "「是不是 bg」的唯一判法（唯一拼 \"interactive\" 字面量的地方）"],
  [
    "src/frontend/ui/tab-model.ts",
    "computeTitleFor",
    "标题 `⚙ 任务名` —— V125 没点名删它（BG1.md §5 问 1 待主会话拍）；拍删就删这一行",
  ],
  ["src/frontend/ui/tabs.ts", "TabManager.ensureTab", "同 sid 两份身份（bg-spare 谎报父 sid）⇒ interactive 恒压过 bg 的升格；建 tab 时带上任务名"],
  ["src/frontend/ui/tabs.ts", "TabManager.computeTitle", "搬运：把任务名交给 `computeTitleFor`"],
  ["src/frontend/ui/tab-bar-prefs.ts", "TabBarPrefs.pinRecordFor", "搬运：固定条落盘带上任务名，复活时标题不丢"],
];

/** 人群：`src/` 顶层的 `tabs.ts` ＋ `tab-*.ts`（遍历借 `production-sources.ts` 那一个家）。 */
function tabSources(): { file: string; text: string }[] {
  return productionTsFiles("src").filter((s) => /^src\/frontend\/ui\/(tabs|tab-[^/]*)\.ts$/.test(s.file));
}

/** 一个节点所在的「顶层声明 / 类方法」名。类成员记成 `类.成员`。 */
function ownerOf(n: ts.Node): string {
  let member: string | null = null;
  let cur: ts.Node | undefined = n;
  let top = "<模块顶层>";
  while (cur && !ts.isSourceFile(cur)) {
    const p: ts.Node | undefined = cur.parent;
    if (p && ts.isClassDeclaration(p) && member === null) {
      const m = cur as ts.ClassElement;
      member = m.name && (ts.isIdentifier(m.name) || ts.isStringLiteral(m.name)) ? m.name.text : "constructor";
    }
    if (p && ts.isSourceFile(p)) {
      if (ts.isFunctionDeclaration(cur) || ts.isClassDeclaration(cur) || ts.isInterfaceDeclaration(cur) || ts.isTypeAliasDeclaration(cur)) {
        top = cur.name?.text ?? "<匿名>";
      } else if (ts.isVariableStatement(cur)) {
        top = cur.declarationList.declarations.map((d) => d.name.getText()).join(",");
      }
      if (ts.isClassDeclaration(cur) && member !== null) top = `${top}.${member}`;
    }
    cur = p;
  }
  return top;
}

/** 一份 TS 源码里按 bg 分叉 / 搬运 bg 字段的住处集合（`名字` 形，文件名由调用方拼）。 */
export function bgSitesOf(fileName: string, text: string): Set<string> {
  const sf = ts.createSourceFile(fileName, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const out = new Set<string>();
  const visit = (n: ts.Node): void => {
    if (ts.isImportDeclaration(n)) return;
    const hit =
      ((ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) &&
        (n.text === "bg" || n.text === "interactive" || n.text.includes("tab-bg"))) ||
      (ts.isTemplateExpression(n) && n.getText().includes("tab-bg")) ||
      (ts.isIdentifier(n) && (n.text === "bgName" || /^isBg/.test(n.text)));
    if (hit) out.add(ownerOf(n));
    ts.forEachChild(n, visit);
  };
  visit(sf);
  return out;
}

/** 类选择器 `.<cls>` 出现在哪几份 CSS 里（剥 `/* … *\/` 之后）。 */
export function cssFilesWithClass(files: ReadonlyArray<readonly [string, string]>, cls: string): string[] {
  const re = new RegExp(`\\.${cls.replace(/[-]/g, "\\-")}(?![\\w-])`);
  return files.filter(([, text]) => re.test(text.replace(/\/\*[\s\S]*?\*\//g, ""))).map(([f]) => f);
}

describe("〔BG1 · V125〕tab 代码里按 bg 分叉只住登记的那几处", () => {
  it("正控：原树状落位那一行式子与 `.tab-bg` 开关各被逮到；注释里的同样字样不算", () => {
    const old = [
      "class TabStore {",
      "  private placeInTree(tab: Tab): void {",
      '    const isBg = tab.kind !== null && tab.kind !== "interactive";',
      "  }",
      "  // 注释：kind !== \"interactive\" · bgName · tab-bg 都不算",
      "  /** bgName */ ok(): void {}",
      "}",
      "function paint(r: Refs, bg: boolean): void {",
      '  r.root.classList.toggle("tab-bg", bg);',
      "}",
    ].join("\n");
    expect([...bgSitesOf("x.ts", old)].sort()).toEqual(["TabStore.placeInTree", "paint"]);
  });

  it("人群从盘上派生，题面点名的五份 tab 栏通用代码都在里面", () => {
    const files = tabSources().map((s) => s.file);
    for (const f of [
      "src/frontend/ui/tab-store.ts",
      "src/frontend/ui/tab-bar-drag.ts",
      "src/frontend/ui/tab-bar-view.ts",
      "src/frontend/ui/tab-drop.ts",
      "src/frontend/ui/tab-collections.ts",
    ]) {
      expect(files, `${f} 不在人群里 —— 改名 / 搬家了就改本条，别让它悄悄离开扫描面`).toContain(f);
    }
  });

  it("★ B1：命中的 `(文件, 所在声明)` == 登记表（两向相等）；tab 栏通用代码零处", () => {
    const got = new Set<string>();
    for (const { file, text } of tabSources()) {
      for (const owner of bgSitesOf(file, text)) got.add(`${file} :: ${owner}`);
    }
    const want = new Set(REGISTERED.map(([f, o]) => `${f} :: ${o}`));
    expect(want.size, "登记表里有重复行").toBe(REGISTERED.length);
    expect([...got].sort()).toEqual([...want].sort());
  });

  it("★ B2：`src/**` 的 CSS 里 `.tab-bg` 零处（正控 `.drop-onto` 在）", () => {
    const files = productionCssFiles("src").map((s) => [s.file, s.text] as const);
    expect(
      cssFilesWithClass([["x.css", "/* .tab-bg */ .tab.tab-bg::before { content: '⌞'; }"]], "tab-bg"),
      "合成正控：真规则逮得到（注释里那一处不算）",
    ).toEqual(["x.css"]);
    expect(
      cssFilesWithClass([["x.css", ".tab.tab-bgx {} /* .tab-bg */"]], "tab-bg"),
      "合成反控：前缀相同的别的类、注释里的字样都不算",
    ).toEqual([]);
    expect(cssFilesWithClass(files, "drop-onto"), "正控：tab 栏样式真在扫描面里（它今天住的那一份）").toEqual([
      "src/frontend/ui/styles.css",
    ]);
    expect(cssFilesWithClass(files, "tab-bg")).toEqual([]);
  });
});
