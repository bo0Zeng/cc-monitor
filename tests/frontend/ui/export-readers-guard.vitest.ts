/**
 * 〔零读者导出〕产品代码里的每个导出，除了定义它的那一处，至少有一处**产品代码**读它；只被测试读的也算零读者 ——
 * 那是活着的死代码（测试替它续命，产品里没人用）。
 *
 * - 人群：`src/**` 下的 `.ts`（生成物 `ui/generated/` · CSS Modules 的类型 `*.d.css.ts` · `.d.ts` 除外）的全部导出。
 * - 量具：TypeScript 编译器解出每个标识符指向的声明（别名、再导出解到底），不靠文本搜 —— 注释里提到、同名的别的东西都不算。
 *   读者 ＝ `src/**` 里任何一处指向它的标识符，声明自己的名字不算；同一个文件里用到也算（那是 `export` 多余，不是死代码）。
 * - 豁免：两种。测试复位口按名字认（`__…ForTest(s)`，模块级状态要在测试之间清，产品不该调它；连测试都不用的不豁免）；
 *   别的逐条写在 [`EXEMPT`] 里，每条一句为什么。豁免里的那一条若已经有了产品读者 / 已经删了 ⇒ 红（死条要摘）。
 * - 正控：内存里造一个小程序（产品两份 ＋ 测试一份），量具必须恰好点出「只被测试读」与「谁都不读」那两个。
 *
 * ⚠ 买不到：Rust 那一侧（`deadcode` 门禁格只看 monitor crate 的 `dead_code`，后端是库、`pub` 项编译器不报）；
 *   经字符串动态取的（`import("./x")` 之后按名字取属性）认得出，按字符串拼出模块路径的认不出（今天产品里没有）。
 */
import { describe, expect, it } from "vitest";
import ts from "typescript";
import path from "node:path";
import { REPO_ROOT } from "../../test-support/repo-root.ts";

/** 测试复位口：模块级状态在测试之间要清，产品不调（名字就是说明）。 */
const TEST_HOOK = /^__\w+ForTests?$/;

/** 逐条豁免：`文件::导出名` → 为什么它没有产品读者也留着。 */
const EXEMPT: Readonly<Record<string, string>> = {
  "src/frontend/ui/render-stream-record.ts::enableRenderCostProbe": "渲染成本秤的探针开关（秤 1 量具）；生产默认关、不常驻",
  "src/frontend/ui/render-stream-record.ts::disableRenderCostProbe": "同上，探针开关",
  "src/frontend/ui/render-stream-record.ts::readRenderCostSamples": "同上，探针读数口",
  "src/frontend/ui/cards/index.ts::resetResultTextLedger": "内存秤（秤 6）的结果字账复位口；量具，不是产品功能",
  "src/frontend/ui/height-estimate.ts::__resetUnknownCardWarnings": "测试复位口（认不出的卡只警告一次，测试之间要清）；名字早于 `…ForTests` 约定",
  "src/frontend/ui/height-estimate.ts::SKEL_OUTER": "秤 2 骨架外框的对拍口：产品用的是四个 `SKEL_*` 常量，这里把它们收成一处给真浏览器量的金样比",
  "src/frontend/ui/account-reads.ts::checkTrust": "界面里唯一一处问 `accounts-trust` 的口（帧命令两向登记要它）；那条帧命令留不留归「记录帧契约 / 手机端对账」那一刀",
};

interface Found {
  key: string;
  testReaders: number;
}

/**
 * ★ 量具：`program` 里 `isProd(文件)` 的那几份，每个自己声明的导出数产品读者；零个的交回（带测试里有几处读它）。
 */
function zeroReaderExports(program: ts.Program, root: string, isProd: (rel: string) => boolean): Found[] {
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
      if (ts.isIdentifier(n)) read(checker.getSymbolAtLocation(n), n);
      // `const { x } = await import("./m")`：按名字从模块对象上取 —— 标识符解到的是本地绑定，读的那一格要从被解构的类型上取。
      if (ts.isBindingElement(n) && ts.isObjectBindingPattern(n.parent)) {
        const key = n.propertyName ?? n.name;
        if (ts.isIdentifier(key)) read(checker.getTypeAtLocation(n.parent).getProperty(key.text), key);
      }
      ts.forEachChild(n, visit);
    };
    visit(sf);
  }
  const out: Found[] = [];
  for (const sf of program.getSourceFiles()) {
    const r = rel(sf.fileName);
    if (!isProd(r)) continue;
    const mod = checker.getSymbolAtLocation(sf);
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

const isProdFile = (rel: string): boolean =>
  rel.startsWith("src/") && rel.endsWith(".ts") && !rel.endsWith(".d.ts") && !rel.endsWith(".d.css.ts") && !rel.includes("/generated/");

const nameOf = (key: string): string => key.slice(key.indexOf("::") + 2);

describe("零读者导出：产品代码里每个导出都有产品读者", () => {
  it("★ 正控：只被测试读的、谁都不读的被点出来；产品读的、同文件用的、复位口不点", () => {
    const files: Record<string, string> = {
      "/v/src/a.ts":
        "export const used = 1;\nexport const lazy = 5;\nexport const testOnly = 2;\nexport const dead = 3;\nexport const selfUsed = 4;\nexport const twice = selfUsed + 1;\nexport function __resetAForTests(): void {}\n",
      "/v/src/b.ts": 'import { used, twice } from "./a";\nexport const b = used + twice;\nconsole.log(b);\nexport async function c(): Promise<number> {\n  const { lazy } = await import("./a");\n  return lazy + c.length;\n}\n',
      "/v/tests/a.test.ts": 'import { testOnly, __resetAForTests } from "../src/a";\nconsole.log(testOnly, __resetAForTests);\n',
    };
    const options: ts.CompilerOptions = { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext, moduleResolution: ts.ModuleResolutionKind.Bundler, lib: ["lib.es2020.d.ts", "lib.dom.d.ts"], types: [] };
    // 虚拟的那三份从内存读，标准库照常从 TypeScript 自带的那几份读（动态导入要 `Promise`）。
    const base = ts.createCompilerHost(options);
    const host: ts.CompilerHost = {
      ...base,
      fileExists: (f) => f in files || base.fileExists(f),
      readFile: (f) => files[f] ?? base.readFile(f),
      getSourceFile: (f, lang) => (f in files ? ts.createSourceFile(f, files[f], lang) : base.getSourceFile(f, lang)),
      directoryExists: (d) => Object.keys(files).some((f) => f.startsWith(`${d}/`)) || (base.directoryExists?.(d) ?? false),
      realpath: (f) => f,
      getCurrentDirectory: () => "/v",
    };
    const program = ts.createProgram(Object.keys(files), options, host);
    const got = zeroReaderExports(program, "/v", (r) => r.startsWith("src/"));
    expect(got.filter((f) => !TEST_HOOK.test(nameOf(f.key)))).toEqual([
      { key: "src/a.ts::dead", testReaders: 0 },
      { key: "src/a.ts::testOnly", testReaders: 2 },
    ]);
    expect(got.map((f) => f.key)).toContain("src/a.ts::__resetAForTests");
  });

  it("★ 全仓：零读者导出 == 豁免（复位口 ＋ 逐条登记），两向相等", () => {
    const cfg = ts.getParsedCommandLineOfConfigFile(path.join(REPO_ROOT, "tsconfig.json"), {}, {
      ...ts.sys,
      onUnRecoverableConfigFileDiagnostic: (d) => {
        throw new Error(ts.flattenDiagnosticMessageText(d.messageText, "\n"));
      },
    });
    if (!cfg) throw new Error("tsconfig.json 读不出来");
    const program = ts.createProgram(cfg.fileNames, cfg.options);
    const found = zeroReaderExports(program, REPO_ROOT, isProdFile);
    expect(found.length, "一个零读者都没扫到 —— 量具多半坏了（复位口至少有十来个）").toBeGreaterThan(5);
    // 复位口只在真有测试用它时豁免（连测试都不用的复位口一样是死代码）。
    const unexplained = found.filter((f) => !(TEST_HOOK.test(nameOf(f.key)) && f.testReaders > 0) && !(f.key in EXEMPT)).map((f) => `${f.key}（测试里 ${f.testReaders} 处）`);
    expect(unexplained, "这些导出产品里没人读：真没人用就删（连带只为它的测试与文案）；有理由留就进 EXEMPT 写一句为什么").toEqual([]);
    const stale = Object.keys(EXEMPT).filter((k) => !found.some((f) => f.key === k));
    expect(stale, "这几条豁免已经有产品读者 / 已经删了：从 EXEMPT 摘掉").toEqual([]);
  }, 120_000);
});
