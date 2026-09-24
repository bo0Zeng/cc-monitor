/**
 * CP2a · 抽表机制（`调研/设计/91 §5.1.1` 四个决定）的判据。表住 `src/shared/copy/table.json`，
 * 取文口住 `src/copy-table.ts::copyText`。
 *
 * # 判什么
 *
 * 1. **表 ↔ 引用两向相等**（本拍的主判据）：表里每个 key 在生产代码里至少被 `copyText("…")`
 *    引用一次；生产代码里每个 `copyText("…")` 的 key 都在表里。两侧异源：一侧是 JSON，
 *    一侧是用 TypeScript 编译器把 `src/**.ts` 真解析出来的调用点（不是 grep）。
 * 2. **调用点的参数 == 表里的 args**（`91 §5.1.1` 决定 3）：第二个实参必须是对象字面量，
 *    属性名集合与表里 `args` 相等。
 * 3. **绕不过去**：key 不是字面量、参数对象里有展开、把 `copyText` 改名导入或当值传走 ⇒ 红
 *    —— 否则那一处引用本判据看不见，两向相等就是在一个缺角的集合上成立的。
 * 4. **表的形状**：key 三段式 · kind 五档 · 占位符只许具名 · `args` 与 `zh` 里的占位符两向相等。
 * 5. **`kind == "title"` 的条数不为 0**（`91 §5.1.1` 决定 4 的反空真：全填成 body 会让 R5 静默空转）。
 *
 * 标题以 `[C-xx]` 开头的两条是 `src/shared/copy/rules.json` 里那两条「机检」规矩的实现
 * （`copy-rules.vitest.ts` 按这个前缀做规矩 ↔ 实现两向对拍，改标题要连规矩表一起改）。
 *
 * # 不判什么（诚实段）
 *
 * - **Rust 一侧没有调用点**（本波只抽了一个 TS 样板区），本文件也不扫 `.rs`。
 *   Rust 读口落地那一拍要把 `.rs` 的调用点加进「引用」那一侧，否则两向相等只对 TS 成立。
 * - **不判表里的文案写得好不好**：规矩住 `src/shared/copy/rules.json`，判据住 `copy-rules.vitest.ts`。
 * - **不判全集**：没抽进表的文案今天仍是散在源码里的字面量，归普查（K-T68）与 CP1 台账管。
 */
import ts from "typescript";
import { describe, expect, it } from "vitest";

import { productionTsFiles } from "../test-support/production-sources.ts";
import { loadTable, NAMED_PH, type Table } from "./copy-support.ts";

const KINDS = new Set(["title", "control", "action", "body", "error"]);
const KEY_RE = /^[a-z][A-Za-z0-9]*\.[a-z][A-Za-z0-9]*\.[a-z][A-Za-z0-9]*$/;
/** 取文口自己住的文件：它里头的 `copyText` 是定义，不是引用。 */
const HOME = "src/copy-table.ts";
const FN = "copyText";

/** 表自己的形状问题。 */
export function tableProblems(table: Table): string[] {
  const p: string[] = [];
  for (const [key, e] of Object.entries(table)) {
    if (!KEY_RE.test(key)) p.push(`${key}：key 不是 <面>.<场景>.<变体> 三段式`);
    if (!KINDS.has(e.kind)) p.push(`${key}：kind「${e.kind}」不在五档里`);
    if (typeof e.zh !== "string" || e.zh.trim() === "") p.push(`${key}：zh 是空的`);
    if (!Array.isArray(e.args)) {
      p.push(`${key}：没写 args（没有参数也要写 []）`);
      continue;
    }
    const zh = e.zh ?? "";
    const named = [...zh.matchAll(NAMED_PH)].map((m) => m[1]);
    // 具名占位符剥掉之后还剩花括号 ⇒ 位置式 `{}` / `{0}` 或者半截括号。
    if (/[{}]/.test(zh.replace(NAMED_PH, ""))) p.push(`${key}：有不具名的占位符或落单的花括号`);
    const a = new Set(e.args);
    const n = new Set(named);
    if (a.size !== e.args.length) p.push(`${key}：args 有重复`);
    for (const x of n) if (!a.has(x)) p.push(`${key}：zh 里有 {${x}}，args 没登记`);
    for (const x of a) if (!n.has(x)) p.push(`${key}：args 登记了 ${x}，zh 里没有这个占位符`);
  }
  return p;
}

export interface Ref {
  file: string;
  key: string;
  args: string[];
}

/** 用 TypeScript 编译器读一份源码里的全部 `copyText` 调用点。 */
export function refsIn(file: string, text: string): { refs: Ref[]; problems: string[] } {
  const refs: Ref[] = [];
  const problems: string[] = [];
  const sf = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const at = (n: ts.Node): string => `${file}:${sf.getLineAndCharacterOfPosition(n.getStart()).line + 1}`;
  const visit = (n: ts.Node): void => {
    if (ts.isImportSpecifier(n) && (n.propertyName ?? n.name).text === FN && n.propertyName)
      problems.push(`${at(n)}：把 ${FN} 改名导入 —— 改了名的调用本判据看不见`);
    if (ts.isIdentifier(n) && n.text === FN && file !== HOME) {
      const parent = n.parent;
      const isCallee = ts.isCallExpression(parent) && parent.expression === n;
      const isImport = ts.isImportSpecifier(parent);
      if (!isCallee && !isImport) problems.push(`${at(n)}：${FN} 被当值用了（不是直接调用）—— 本判据看不见它会被拿去取哪条`);
    }
    if (ts.isCallExpression(n) && ts.isIdentifier(n.expression) && n.expression.text === FN && file !== HOME) {
      const [k, a] = n.arguments;
      if (!k || !(ts.isStringLiteral(k) || ts.isNoSubstitutionTemplateLiteral(k))) {
        problems.push(`${at(n)}：${FN} 的 key 不是字面量`);
      } else {
        const args: string[] = [];
        if (a !== undefined) {
          if (!ts.isObjectLiteralExpression(a)) problems.push(`${at(n)}：${FN} 的参数不是对象字面量`);
          else
            for (const prop of a.properties) {
              if ((ts.isPropertyAssignment(prop) || ts.isShorthandPropertyAssignment(prop)) && !ts.isComputedPropertyName(prop.name))
                args.push(prop.name.getText(sf).replace(/^["']|["']$/g, ""));
              else problems.push(`${at(n)}：${FN} 的参数对象里有展开或计算属性名`);
            }
        }
        refs.push({ file: at(n), key: k.text, args });
      }
    }
    ts.forEachChild(n, visit);
  };
  visit(sf);
  return { refs, problems };
}

/** 表 ↔ 引用的两向差，加上参数对不上的调用点。 */
export function crossCheck(table: Table, refs: Ref[]): { unreferenced: string[]; unknown: string[]; badArgs: string[] } {
  const used = new Set(refs.map((r) => r.key));
  const unreferenced = Object.keys(table).filter((k) => !used.has(k)).sort();
  const unknown = refs.filter((r) => !(r.key in table)).map((r) => `${r.file} ${r.key}`);
  const badArgs = refs
    .filter((r) => r.key in table)
    .filter((r) => [...r.args].sort().join(",") !== [...table[r.key].args].sort().join(","))
    .map((r) => `${r.file} ${r.key}：给了 [${r.args.join(",")}]，表里要 [${table[r.key].args.join(",")}]`);
  return { unreferenced, unknown, badArgs };
}

describe("CP2a · 文案表：形状", () => {
  const table = loadTable();

  it("表读得出条目（反空真：0 条时下面每一条都是空转）", () => {
    expect(Object.keys(table).length).toBeGreaterThan(0);
  });

  it("[C-K1] key 三段式 · kind 五档 · 占位符只许具名 · args 与占位符两向相等", () => {
    expect(tableProblems(table)).toEqual([]);
  });

  it("[C-K2] kind == title 的条数不为 0（全填成 body 会让 R5 静默空转）", () => {
    expect(Object.values(table).filter((e) => e.kind === "title").length).toBeGreaterThan(0);
  });
});

describe("CP2a · 文案表 ↔ 生产代码引用", () => {
  const table = loadTable();
  const files = productionTsFiles("src");
  const all = files
    .filter((f) => new RegExp(`\\b${FN}\\b`).test(f.text))
    .map((f) => refsIn(f.file, f.text));
  const refs = all.flatMap((x) => x.refs);
  const problems = all.flatMap((x) => x.problems);

  it("正控：扫到了生产源码，也扫到了取文口自己", () => {
    expect(files.length, "一个生产 .ts 都没扫到 —— 下面的相等是空集对空集").toBeGreaterThan(100);
    expect(files.map((f) => f.file)).toContain(HOME);
    expect(refs.length, "一个 copyText 调用点都没找到 —— 样板区没接上，或者解析器坏了").toBeGreaterThan(0);
  });

  it("没有绕过静态读的写法（非字面 key · 改名导入 · 当值传 · 展开参数）", () => {
    expect(problems).toEqual([]);
  });

  it("★ 表里的 key == 代码里引用的 key（两向）", () => {
    const { unreferenced, unknown } = crossCheck(table, refs);
    expect(unknown, "代码引用了表里没有的 key").toEqual([]);
    expect(unreferenced, "表里有、代码里没人引用的 key（死文案）").toEqual([]);
  });

  it("★ 每个调用点给的参数 == 表里登记的 args", () => {
    expect(crossCheck(table, refs).badArgs).toEqual([]);
  });
});

describe("CP2a · 文案表判据自己会不会死（正控）", () => {
  const t: Table = {
    "a.b.c": { kind: "body", zh: "你好 {name}", args: ["name"] },
  };

  it("形状：两段 key / 未知 kind / 位置式占位符 / args 与占位符不相等 —— 各红一次", () => {
    expect(tableProblems({ "a.b": t["a.b.c"] }).join()).toMatch(/三段式/);
    expect(tableProblems({ "a.b.c": { ...t["a.b.c"], kind: "label" } }).join()).toMatch(/五档/);
    expect(tableProblems({ "a.b.c": { kind: "body", zh: "第 {} 条", args: [] } }).join()).toMatch(/不具名/);
    expect(tableProblems({ "a.b.c": { kind: "body", zh: "你好 {name}", args: [] } }).join()).toMatch(/没登记/);
    expect(tableProblems({ "a.b.c": { kind: "body", zh: "你好", args: ["name"] } }).join()).toMatch(/没有这个占位符/);
  });

  it("引用：真调用读得出 key 与参数；四种绕法各被逮住", () => {
    const ok = refsIn("src/x.ts", 'import { copyText } from "./copy-table";\nf(copyText("a.b.c", { name }));\n');
    expect(ok.problems).toEqual([]);
    expect(ok.refs.map((r) => [r.key, r.args])).toEqual([["a.b.c", ["name"]]]);
    expect(refsIn("src/x.ts", "copyText(k);").problems.join()).toMatch(/不是字面量/);
    expect(refsIn("src/x.ts", 'import { copyText as t } from "./copy-table";').problems.join()).toMatch(/改名导入/);
    expect(refsIn("src/x.ts", "const f = copyText;").problems.join()).toMatch(/当值用了/);
    expect(refsIn("src/x.ts", 'copyText("a.b.c", { ...o });').problems.join()).toMatch(/展开/);
  });

  it("对拍：表里多一条 / 代码多引一条 / 参数给错 —— 各红一次", () => {
    const r: Ref[] = [{ file: "x", key: "a.b.c", args: ["name"] }];
    expect(crossCheck({ ...t, "a.b.d": t["a.b.c"] }, r).unreferenced).toEqual(["a.b.d"]);
    expect(crossCheck(t, [...r, { file: "y", key: "z.z.z", args: [] }]).unknown).toEqual(["y z.z.z"]);
    expect(crossCheck(t, [{ file: "x", key: "a.b.c", args: [] }]).badArgs.length).toBe(1);
  });
});
