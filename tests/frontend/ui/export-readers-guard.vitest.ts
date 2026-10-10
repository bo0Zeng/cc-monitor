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
import { execFileSync } from "node:child_process";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { TEST_HOOK, zeroReaderExports, type Found } from "../../test-support/zero-reader-exports.ts";

/** 量具（与正控用的同一份代码）：全仓那一趟另起一个 node 进程跑，见它的头注。 */
const METER = pathToFileURL(resolve(REPO_ROOT, "tests", "test-support", "zero-reader-exports.ts")).href;

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

const nameOf = (key: string): string => key.slice(key.indexOf("::") + 2);

describe("零读者导出：产品代码里每个导出都有产品读者", () => {
  it("★ 正控：只被测试读的、谁都不读的被点出来；产品读的、同文件用的、复位口不点", () => {
    const files: Record<string, string> = {
      "/v/src/a.ts":
        "export const used = 1;\nexport const lazy = 5;\nexport const testOnly = 2;\nexport const dead = 3;\nexport const selfUsed = 4;\nexport const twice = selfUsed + 1;\nexport function __resetAForTests(): void {}\n",
      "/v/src/b.ts": 'import { used, twice } from "./a";\nexport const b = used + twice;\nvoid b;\nexport async function c(): Promise<number> {\n  const { lazy } = await import("./a");\n  return lazy + c.length;\n}\n',
      "/v/tests/a.test.ts": 'import { testOnly, __resetAForTests } from "../src/a";\nvoid [testOnly, __resetAForTests];\n',
    };
    const options: ts.CompilerOptions = { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext, moduleResolution: ts.ModuleResolutionKind.Bundler, lib: ["lib.es5.d.ts"], types: [] };
    // 虚拟的那三份从内存读，标准库从 TypeScript 自带的读，只取 `lib.es5`（动态导入要 `Promise`，它在这一份里）。
    // ⚠ 不带 `lib.dom`：那一份两万多行，建程序时光它就占大半（带覆盖率插桩 0.8 s，门禁并跑时把这一格挤过 5 s 期限）；
    //   小程序里用 `void x` 当读者，不靠 `console`。
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
    const run = `const m = await import(${JSON.stringify(METER)}); process.stdout.write(JSON.stringify(m.zeroReaderExportsOfRepo()));`;
    const found = JSON.parse(
      execFileSync(process.execPath, ["--import", "tsx", "--input-type=module", "-e", run], {
        cwd: REPO_ROOT,
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
        maxBuffer: 16 * 1024 * 1024,
      }),
    ) as Found[];
    expect(found.length, "一个零读者都没扫到 —— 量具多半坏了（复位口至少有十来个）").toBeGreaterThan(5);
    // 复位口只在真有测试用它时豁免（连测试都不用的复位口一样是死代码）。
    const unexplained = found.filter((f) => !(TEST_HOOK.test(nameOf(f.key)) && f.testReaders > 0) && !(f.key in EXEMPT)).map((f) => `${f.key}（测试里 ${f.testReaders} 处）`);
    expect(unexplained, "这些导出产品里没人读：真没人用就删（连带只为它的测试与文案）；有理由留就进 EXEMPT 写一句为什么").toEqual([]);
    const stale = Object.keys(EXEMPT).filter((k) => !found.some((f) => f.key === k));
    expect(stale, "这几条豁免已经有产品读者 / 已经删了：从 EXEMPT 摘掉").toEqual([]);
  // 期限：全仓建一份 TS program（要类型检查器认读者），没有便宜的近路。量过：负载 3 时 3.5 s；带覆盖率 · 负载 89–150 时 62–81 s
  //   （120 s 期限过半）。放宽到 240 s；机器换了、或 quick-check 三步并行跑起来之后复量一次。
  }, 240_000);
});
