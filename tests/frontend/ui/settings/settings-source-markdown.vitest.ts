/**
 * ST1「屏幕上那几处星号」（判据 #5：界面上零 markdown 标记）。
 *
 * # 为什么 `ui-copy-discipline.vitest.ts` 那把尺子没逮住它们
 *
 * 那条扫的是**真渲染出来的 DOM** —— 而它为了跑得动，把账号 / MCP / 插件 / cc-bus 钩子 /
 * 终端集成这几块全 stub 掉了，其余块又跑在「IPC 全 reject」的录音机下 ⇒ 这些块里的句子
 * **从来没上过那张被扫的 DOM**。星号偏偏就住在这几块里（末段记的是同一个洞）。
 *
 * ⇒ 本条换一个人群：**设置窗入口（`src/frontend/ui/entry-settings.ts`）静态可达的每一份生产源码里的
 * 每一个字符串字面量 / 模板片段**。量具是 TypeScript 自己的语法树（不是正则切源码 ——
 * 注释里的 `**强调**` 不算，本仓的注释全是这个写法）。
 *
 * # 判据形状
 *
 * - **两向相等**：扫出来的「带成对 `**` 的串」== `REGISTERED`（今天只有一条，是别的路的写区）。
 * - **正控**：同一个抽取函数喂一段合成源码，字符串里的 `**` 必须逮到、注释里的不许逮、
 *   `console.*` 的参数不许逮（那是开发者日志，不上屏）。
 * - **人群锚**：可达集合必须含那几份已知住着界面文字的文件（否则「零命中」可能只是图没走通）。
 *
 * 〔射程〕盖不到：运行期从后端拿来的串（`backend_policy.rs` 那族，登记在
 * `ui-copy-discipline.vitest.ts::BACKEND_SIDE_DEBT`）；动态 `import()`；不经设置窗的主窗那几份。
 */
import { describe, it, expect, beforeAll } from "vitest";
import ts from "typescript";
import { existsSync, readFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";

import { toPosix } from "../../../test-support/posix-path.ts";
import { SCAN_TIMEOUT_MS } from "../../../test-support/production-sources.ts";

const ROOT = process.cwd();
const ENTRY = resolve(ROOT, "src/frontend/ui/entry-settings.ts");
const PAIRED = /\*\*[^*\n]+\*\*/g;

/**
 * 设置窗入口静态可达的源码（只跟相对路径 import / export … from）：仓相对路径 → 它的语法树。
 * 走图时建的那棵树原样交给下面的扫描（原先走图建一遍、扫描再按原文建一遍 —— 同一批文件两遍建树，
 * 带覆盖率插桩、机器负载 20 以上时撞过 30 s）。不挂父指针（走图只看顶层语句、扫描只往下走 —— 都用不着；挂父指针的建树要慢两倍多）。
 */
function reachable(entry: string): Map<string, ts.SourceFile> {
  const seen = new Map<string, ts.SourceFile>();
  const queue = [entry];
  while (queue.length) {
    const f = queue.pop()!;
    if (seen.has(f)) continue;
    const sf = ts.createSourceFile(f, readFileSync(f, "utf8"), ts.ScriptTarget.Latest, false);
    seen.set(f, sf);
    for (const st of sf.statements) {
      const spec =
        (ts.isImportDeclaration(st) || ts.isExportDeclaration(st)) &&
        st.moduleSpecifier &&
        ts.isStringLiteral(st.moduleSpecifier)
          ? st.moduleSpecifier.text
          : null;
      if (!spec || !spec.startsWith(".")) continue;
      const hit = [".ts", "/index.ts"]
        .map((ext) => resolve(dirname(f), spec + ext))
        .find((p) => existsSync(p));
      if (hit) queue.push(hit);
    }
  }
  // `relative` 在 Windows 上吐 `src\comms\…` ⇒ 规整成正斜杠，才与下面的字面量住址同形。
  return new Map([...seen].map(([f, sf]) => [toPosix(relative(ROOT, f)), sf] as const).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
}

/** 一份源码里「字符串里带成对 `**`」的每一处：`<文件>\t<命中>`。`console.*` 的参数不算。 */
export function starsIn(file: string, src: string): string[] {
  return starsInTree(file, ts.createSourceFile(file, src, ts.ScriptTarget.Latest, false));
}

/**
 * 同上，吃已建好的语法树。`console.<x>(…)` 调用的整棵子树不往下走（原先每个字面量沿父链往上找有没有这样一个祖先，
 * 有就不算 —— 判法相同，只是不再逐个字面量回溯、也不再进那棵子树）。
 */
function starsInTree(file: string, sf: ts.SourceFile): string[] {
  const out: string[] = [];
  // 字符串字面量 / 模板一定以引号或反引号起头（模板的中段 / 尾段挂在以反引号起头的模板表达式之下）⇒
  // 一棵子树的源码区间里一个引号都没有，它底下就没有字面量，整棵跳过。引号位置一趟收齐、按位置二分。
  const quotes: number[] = [];
  const text = sf.text;
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c === 34 || c === 39 || c === 96) quotes.push(i);
  }
  const hasQuote = (from: number, to: number): boolean => {
    let lo = 0;
    let hi = quotes.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (quotes[mid] < from) lo = mid + 1;
      else hi = mid;
    }
    return lo < quotes.length && quotes[lo] < to;
  };
  const walk = (n: ts.Node): void => {
    if (
      ts.isStringLiteral(n) ||
      ts.isNoSubstitutionTemplateLiteral(n) ||
      ts.isTemplateHead(n) ||
      ts.isTemplateMiddle(n) ||
      ts.isTemplateTail(n)
    ) {
      for (const m of n.text.matchAll(PAIRED)) out.push(`${file}\t${m[0]}`);
      return;
    }
    if (
      ts.isCallExpression(n) &&
      ts.isPropertyAccessExpression(n.expression) &&
      ts.isIdentifier(n.expression.expression) &&
      n.expression.expression.text === "console"
    ) {
      return;
    }
    // 模板的一段 `${x}中段${` 区间里可以一个引号都没有（反引号在头尾两段上）⇒ 这一种不按引号跳。
    if (!ts.isTemplateSpan(n) && !hasQuote(n.pos, n.end)) return;
    ts.forEachChild(n, walk);
  };
  walk(sf);
  return out;
}

/**
 * 今天还在、而**不在本路写区**的那几处（逐条写清归谁）。
 * 两向相等：谁改掉了它，这里就得删一行；谁新加一处，这里就红。
 */
const REGISTERED: Readonly<Record<string, string>> = {
  // 从前这里登着 `machine-card.ts` 的「**是同一套实现**」一行（机器页归 MC1+AL1）。
  // 那段「安装位置」长说明随 8 颗按钮 → 3 个动作一起重写掉了 ⇒ 这一行删掉，表空了。
};

describe("ST1：设置窗屏幕上的字符串里零 markdown 星号", () => {
  // 走图 ＋ 建树是这一组唯一的重活：做一次，三条共用（原先在 describe 体里走图、扫描那条再建一遍树）。
  let trees: Map<string, ts.SourceFile>;
  let files: string[];
  beforeAll(() => {
    trees = reachable(ENTRY);
    files = [...trees.keys()];
  }, SCAN_TIMEOUT_MS);

  it("人群锚：可达集合真的走到了住着界面文字的那几份", () => {
    for (const f of [
      "src/frontend/ui/settings/panel.ts",
      "src/frontend/ui/settings/accounts-section.ts",
      "src/frontend/ui/settings/account-new-form.ts",
      "src/frontend/ui/settings/ext-section.ts",
      "src/frontend/ui/settings/machine-aliases.ts", // 原 `cc_integration.ts` 并进了它
      "src/frontend/ui/settings/remote-section.ts",
      "src/frontend/ui/accounts.ts",
    ]) {
      expect(files, `${f} 不在设置窗的可达集合里 —— 图没走通，下面的零命中是空转`).toContain(f);
    }
  }, SCAN_TIMEOUT_MS);

  it("🔴 正控：字符串里的逮到、注释里的不逮、console 参数不逮、模板片段也逮", () => {
    const src = [
      "// 注释里的 **强调** 不算",
      'const a = "界面上的 **强调**";',
      "const b = `模板 ${a} 里的 **也算**`;",
      'console.debug("开发者日志 **不上屏**");',
      'const c = "单个星号 *.log 不算";',
      "const d = `头 ${a} **中段** ${b} 尾`;",
      'f(`x${g(1)}**无引号的中段**${2}y`);',
    ].join("\n");
    expect(starsIn("x.ts", src)).toEqual(["x.ts\t**强调**", "x.ts\t**也算**", "x.ts\t**中段**", "x.ts\t**无引号的中段**"]);
  });

  it("扫出来的 == 登记表（两向相等）", () => {
    const got = [...trees].flatMap(([f, sf]) => starsInTree(f, sf)).sort();
    expect(
      got,
      "设置窗的字符串里出现了 markdown 星号 —— 界面不渲染 markdown，星号会原样显示给用户。\n" +
        "强调改由句子结构或 DOM 结构承担（`data-section.ts` 的 strong() 是样板）。",
    ).toEqual(Object.keys(REGISTERED).sort());
  }, SCAN_TIMEOUT_MS);
});
