/**
 * 〔零读者导出〕的量具本身（判据住 `tests/frontend/ui/export-readers-guard.vitest.ts`）：TypeScript 编译器解出每个标识符指向的声明，
 * 数 `src/**` 里每个导出的产品读者。
 *
 * 两种用法、同一份代码：
 * - 判据里 import 它，在内存里造的小程序上跑正控；
 * - 全仓那一趟由判据另起一个 node 进程（`--import tsx`，同 `npm test` 里那几套 tsx 套件）调 [`zeroReaderExportsOfRepo`]（印一行 JSON）：
 *   建全仓程序（一千一百多份源 ＋ lib.dom）是纯 CPU 的活，留在 vitest 的 worker 里跑会挨覆盖率插桩
 *   （V8 精确覆盖率把 typescript 自己也拖慢，空机 5 s 的活带覆盖率 25 s，负载下 62–81 s，压着 120 s 期限过半）；
 *   子进程不插桩，读数一样。
 */
import ts from "typescript";
import path from "node:path";
import { REPO_ROOT } from "./repo-root.ts";

/** 测试复位口：模块级状态在测试之间要清，产品不调（名字就是说明）。 */
export const TEST_HOOK = /^__\w+ForTests?$/;

export interface Found {
  key: string;
  testReaders: number;
}

/**
 * ★ 量具：`program` 里 `isProd(文件)` 的那几份，每个自己声明的导出数产品读者；零个的交回（带测试里有几处读它）。
 */
export function zeroReaderExports(program: ts.Program, root: string, isProd: (rel: string) => boolean): Found[] {
  const checker = program.getTypeChecker();
  const rel = (f: string): string => path.relative(root, f).split(path.sep).join("/");
  const orig = (s: ts.Symbol): ts.Symbol => {
    let x = s;
    while (x.flags & ts.SymbolFlags.Alias) {
      const next = checker.getAliasedSymbol(x);
      if (next === x) break;
      x = next;
    }
    return x;
  };
  const modules = program
    .getSourceFiles()
    .filter((sf) => isProd(rel(sf.fileName)))
    .map((sf) => ({ sf, mod: checker.getSymbolAtLocation(sf) }));
  // 只解**可能**指向人群里某个导出的标识符：它的字面是某个导出的导出名 / 本地名，或者它是默认导入的那个名字。
  //   别的文件要读一个导出，必经一处写着它导出名的地方（具名导入 · 再导出 · `ns.名` · 解构 `{ 名 }`），默认导出经导入子句的名字；
  //   本文件里用到走它的本地名。⇒ 字面不在这张表里的标识符解出来不会落到人群里，不用问编译器。
  //   原先全仓二十六万多个标识符逐个解（每解一个都牵动类型检查），空机 7 s、带覆盖率插桩近 20 s，压着负载撞 120 s 期限。
  //   🔴 表若漏了一种读法，后果只会是「多报零读者」（吵闹的红），不会把真零读者放过去。
  const names = new Set<string>();
  for (const { mod } of modules) {
    for (const e of mod ? checker.getExportsOfModule(mod) : []) {
      names.add(e.name);
      const o = orig(e);
      names.add(o.name);
      for (const d of o.declarations ?? []) {
        const id = (d as ts.NamedDeclaration).name;
        if (id && ts.isIdentifier(id)) names.add(id.text);
      }
    }
  }
  const maybeExport = (id: ts.Identifier): boolean =>
    names.has(id.text) || ts.isImportClause(id.parent) || ts.isImportEqualsDeclaration(id.parent);
  const prodReads = new Map<ts.Symbol, number>();
  const testReads = new Map<ts.Symbol, number>();
  for (const sf of program.getSourceFiles()) {
    const r = rel(sf.fileName);
    if (r.startsWith("..") || r.includes("node_modules/")) continue;
    const m = isProd(r) || r.startsWith("src/") ? prodReads : testReads;
    const read = (sym: ts.Symbol | undefined, at: ts.Node): void => {
      if (!sym) return;
      const o = orig(sym);
      // 声明自己的名字不算读者。
      if (!(o.declarations ?? []).some((d) => (d as ts.NamedDeclaration).name === at)) m.set(o, (m.get(o) ?? 0) + 1);
    };
    const visit = (n: ts.Node): void => {
      if (ts.isIdentifier(n) && maybeExport(n)) read(checker.getSymbolAtLocation(n), n);
      // `const { x } = await import("./m")`：按名字从模块对象上取 —— 标识符解到的是本地绑定，读的那一格要从被解构的类型上取。
      if (ts.isBindingElement(n) && ts.isObjectBindingPattern(n.parent)) {
        const key = n.propertyName ?? n.name;
        if (ts.isIdentifier(key) && names.has(key.text)) read(checker.getTypeAtLocation(n.parent).getProperty(key.text), key);
      }
      ts.forEachChild(n, visit);
    };
    visit(sf);
  }
  const out: Found[] = [];
  for (const { sf, mod } of modules) {
    const r = rel(sf.fileName);
    if (!mod) continue;
    for (const e of checker.getExportsOfModule(mod)) {
      const o = orig(e);
      if (o.declarations?.[0]?.getSourceFile() !== sf) continue; // 再导出别人的，记在原处
      if ((prodReads.get(o) ?? 0) > 0) continue;
      out.push({ key: `${r}::${e.name}`, testReaders: testReads.get(o) ?? 0 });
    }
  }
  return out.sort((a, b) => a.key.localeCompare(b.key));
}

export const isProdFile = (rel: string): boolean =>
  rel.startsWith("src/") && rel.endsWith(".ts") && !rel.endsWith(".d.ts") && !rel.endsWith(".d.css.ts") && !rel.includes("/generated/");

/** 全仓那一趟：照仓根 `tsconfig.json` 建程序，量 `src/**` 的产品文件。 */
export function zeroReaderExportsOfRepo(): Found[] {
  const cfg = ts.getParsedCommandLineOfConfigFile(path.join(REPO_ROOT, "tsconfig.json"), {}, {
    ...ts.sys,
    onUnRecoverableConfigFileDiagnostic: (d) => {
      throw new Error(ts.flattenDiagnosticMessageText(d.messageText, "\n"));
    },
  });
  if (!cfg) throw new Error("tsconfig.json 读不出来");
  const program = ts.createProgram(cfg.fileNames, cfg.options);
  return zeroReaderExports(program, REPO_ROOT, isProdFile);
}
