/**
 * 〔TL3 · 审计 F 🔴-5〕「是不是本机」只在 `src/frontend/ui/ipc/origin.ts` 判：别处零直比 `LOCAL_ORIGIN` / `"<local>"`。
 *
 * ## 守的要求（住址）
 *
 * - `设计/00 §2.5` ① 今天那一句，逐字：「TS 侧 `origin` 去 `null`、本机只有 `"<local>"` 一种写法（判定唯一住址 `src/frontend/ui/ipc/origin.ts`）」。
 * - `设计/01 §5` **D1**「一个判定只有一个家」—— 「同一条规则有两份实现，它们就会漂；而漂开的后果是静默的错，不是报错」。
 * - `src/frontend/ui/ipc/origin.ts` 头注，逐字：「「是不是本机」只在这里判（[`isLocalOrigin`] / [`isRemoteOrigin`]），调用处不许自己比」。
 *
 * 出处：审计 F 🔴-5 —— 那条规则没有判据守，`=== LOCAL_ORIGIN` 在调用处长回来 8 处（`account-restart` · `remote-launch-run` ·
 * `settings/backend-section` · `tab-menu` · `tab-session-actions` ×4），TL3 全改 `isLocalOrigin`；
 * `settings/config-surface-section.ts::answersFor` 那一处比的是**回声**，改成与远端那一支同形的 `said === origin`。
 *
 * ## 判据
 *
 * **人群**：`productionTsFiles("src")` 全部（现派生；`generated/` 不在里面 —— 那是 ts-rs 的类型生成物）。
 * **针**（TS AST，注释天然不在里面）：相等 / 不等（`===` `!==` `==` `!=`）任一侧是「本机那个值」—— 标识符 `LOCAL_ORIGIN`
 * 或它在本文件 `import { LOCAL_ORIGIN as X }` 的别名 · 属性访问 `.LOCAL_ORIGIN` · 字符串字面量 `"<local>"`；以及 `case` 上的它。
 * **判**：`origin.ts` 之外命中 == ∅（零命中）；`origin.ts` 里命中的所在函数集合 == `{isLocalOrigin, isRemoteOrigin}`（两向相等 ——
 * 这一半同时是真仓上的正控：同一个扫描器在那两处看得见这一形）。合成正控四阳两阴见第一条。
 *
 * ## 买不到
 *
 * - 转一手再比（`const L = LOCAL_ORIGIN; x === L`）· `Set` / `Map` 查表 · `startsWith("<")` 之类的变形。
 * - Rust 侧的「是不是本机」（`origin.rs::Route` 一族）：不在 `00 §2.5 ①` 这一句（说的是 TS 侧）的射程。
 */
import ts from "typescript";
import { describe, it, expect } from "vitest";
import { productionTsFiles } from "../../test-support/production-sources.ts";

const HOME = "src/frontend/ui/ipc/origin.ts";

/** 一个节点所在的函数 / 方法名（类方法记成 `类.方法`）；都不在 ⇒ `<模块顶层>`。 */
function ownerOf(n: ts.Node): string {
  for (let cur: ts.Node | undefined = n; cur; cur = cur.parent) {
    if (ts.isFunctionDeclaration(cur) && cur.name) return cur.name.text;
    if (ts.isMethodDeclaration(cur) && cur.name && ts.isIdentifier(cur.name)) {
      const cls = cur.parent;
      return ts.isClassDeclaration(cls) && cls.name ? `${cls.name.text}.${cur.name.text}` : cur.name.text;
    }
    if (ts.isVariableDeclaration(cur) && ts.isIdentifier(cur.name) && cur.initializer && (ts.isArrowFunction(cur.initializer) || ts.isFunctionExpression(cur.initializer))) {
      return cur.name.text;
    }
  }
  return "<模块顶层>";
}

/** 一份源码里「自己比本机那个值」的全部命中：`[所在函数, 行号]`。 */
export function localComparesOf(fileName: string, text: string): Array<[string, number]> {
  const sf = ts.createSourceFile(fileName, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const aliases = new Set<string>(["LOCAL_ORIGIN"]);
  for (const st of sf.statements) {
    if (!ts.isImportDeclaration(st)) continue;
    const nb = st.importClause?.namedBindings;
    if (nb && ts.isNamedImports(nb)) {
      for (const el of nb.elements) {
        if ((el.propertyName ?? el.name).text === "LOCAL_ORIGIN") aliases.add(el.name.text);
      }
    }
  }
  const unwrap = (e: ts.Expression): ts.Expression => (ts.isParenthesizedExpression(e) ? unwrap(e.expression) : e);
  const isLocalValue = (raw: ts.Expression): boolean => {
    const e = unwrap(raw);
    if (ts.isIdentifier(e)) return aliases.has(e.text);
    if (ts.isPropertyAccessExpression(e)) return e.name.text === "LOCAL_ORIGIN";
    if (ts.isStringLiteral(e) || ts.isNoSubstitutionTemplateLiteral(e)) return e.text === "<local>";
    return false;
  };
  const EQ = new Set([
    ts.SyntaxKind.EqualsEqualsEqualsToken,
    ts.SyntaxKind.ExclamationEqualsEqualsToken,
    ts.SyntaxKind.EqualsEqualsToken,
    ts.SyntaxKind.ExclamationEqualsToken,
  ]);
  const out: Array<[string, number]> = [];
  const line = (n: ts.Node): number => sf.getLineAndCharacterOfPosition(n.getStart(sf)).line + 1;
  const visit = (n: ts.Node): void => {
    if (ts.isBinaryExpression(n) && EQ.has(n.operatorToken.kind) && (isLocalValue(n.left) || isLocalValue(n.right))) {
      out.push([ownerOf(n), line(n)]);
    }
    if (ts.isCaseClause(n) && isLocalValue(n.expression)) out.push([ownerOf(n), line(n)]);
    ts.forEachChild(n, visit);
  };
  visit(sf);
  return out;
}

describe("〔TL3 · 🔴-5〕「是不是本机」只在 origin.ts 判", () => {
  it("正控（合成）：别名 · 字面量 · 反着写 · case 四阳；当参数传 · 注释里写两阴", () => {
    const src = [
      'import { LOCAL_ORIGIN as HERE } from "./ipc/origin";',
      "function a(o: string): boolean { return o === HERE; }",
      'function b(o: string): boolean { return "<local>" !== o; }',
      "function c(o: string): boolean { return (LOCAL_ORIGIN) == o; }",
      "function d(o: string): number { switch (o) { case LOCAL_ORIGIN: return 1; default: return 0; } }",
      "function e(o: string): void { send(LOCAL_ORIGIN, o); }",
      "// function f(o) { return o === LOCAL_ORIGIN; }",
    ].join("\n");
    expect(localComparesOf("x.ts", src).map(([o]) => o).sort()).toEqual(["a", "b", "c", "d"]);
  });

  it("人群从盘上派生，家那一份在里面", () => {
    expect(productionTsFiles("src").map((s) => s.file)).toContain(HOME);
  });

  it("★ origin.ts 之外零直比（零命中）；origin.ts 里恰好是那两个判定（两向相等，同时是正控）", () => {
    const away: string[] = [];
    const home = new Set<string>();
    for (const { file, text } of productionTsFiles("src")) {
      for (const [owner, ln] of localComparesOf(file, text)) {
        if (file === HOME) home.add(owner);
        else away.push(`${file}:${ln}（${owner}）`);
      }
    }
    expect(
      away,
      "调用处在自己比「是不是本机」—— 改 `isLocalOrigin(origin)` / `isRemoteOrigin(origin)`（`src/frontend/ui/ipc/origin.ts`，`设计/00 §2.5 ①` · `01 §5 D1`）",
    ).toEqual([]);
    expect([...home].sort(), "origin.ts 里那两个判定变了形 / 扫描器看不见它们 —— 上一格的零命中此刻不可信").toEqual([
      "isLocalOrigin",
      "isRemoteOrigin",
    ]);
  });
});
