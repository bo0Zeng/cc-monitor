/**
 * 通道没成 · 被拒的那一句由出错那一端写好（monitor 写在 `ChanError.message`，被拒时就是那台后端那一句），界面原样取。
 *
 * | 守什么 | 判据 |
 * |---|---|
 * | 读通道层的函数不写句子：函数体（不含里面再定义的函数）读了 `.layer` 的，零处 `copyText(` | S1 |
 * | 界面不兜「原因不明」：`.noReason` 结尾的键零引用；`Refusals` · `asSaid` · `byCode` 零处 | S2 |
 *
 * 两条都是零命中，各带一段夹具作正控（同一个扫描器认得出旧的那一形）。
 */
import ts from "typescript";
import { describe, it, expect } from "vitest";
import { productionTsFiles } from "../../test-support/production-sources.ts";
import TABLE from "../../../src/shared/copy/table.json";

const SOURCES = [...productionTsFiles("src/frontend/ui"), ...productionTsFiles("src/comms/inward")];

function parse(file: string, text: string): ts.SourceFile {
  return ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
}

function isFunctionLike(n: ts.Node): n is ts.FunctionLikeDeclaration {
  return (
    ts.isFunctionDeclaration(n) ||
    ts.isMethodDeclaration(n) ||
    ts.isArrowFunction(n) ||
    ts.isFunctionExpression(n) ||
    ts.isConstructorDeclaration(n) ||
    ts.isGetAccessorDeclaration(n) ||
    ts.isSetAccessorDeclaration(n)
  );
}

function nameOf(fn: ts.FunctionLikeDeclaration): string {
  if (fn.name && ts.isIdentifier(fn.name)) return fn.name.text;
  const p = fn.parent;
  if (ts.isVariableDeclaration(p) && ts.isIdentifier(p.name)) return p.name.text;
  if (ts.isPropertyAssignment(p) && ts.isIdentifier(p.name)) return p.name.text;
  return `<匿名:${fn.getSourceFile().getLineAndCharacterOfPosition(fn.getStart()).line + 1}>`;
}

/** 读了 `.layer`、又在同一个函数体里调 `copyText(` 的函数（`文件::函数`）。 */
function layerSayers(file: string, text: string): string[] {
  const out: string[] = [];
  const own = (fn: ts.Node): { layer: boolean; says: boolean } => {
    const r = { layer: false, says: false };
    const visit = (n: ts.Node): void => {
      if (n !== fn && isFunctionLike(n)) return;
      if (ts.isPropertyAccessExpression(n) && n.name.text === "layer") r.layer = true;
      if (ts.isCallExpression(n) && ts.isIdentifier(n.expression) && n.expression.text === "copyText") r.says = true;
      ts.forEachChild(n, visit);
    };
    visit(fn);
    return r;
  };
  const walk = (n: ts.Node): void => {
    if (isFunctionLike(n)) {
      const r = own(n);
      if (r.layer && r.says) out.push(`${file}::${nameOf(n)}`);
    }
    ts.forEachChild(n, walk);
  };
  walk(parse(file, text));
  return out;
}

const NO_REASON_KEYS = new Set(Object.keys(TABLE.entries).filter((k) => k.endsWith(".noReason")));
const GONE_NAMES = new Set(["Refusals", "asSaid", "byCode"]);

/** 引了 `.noReason` 键的字面量 · 用了兜底那几个名字的地方（`文件::那个词`）。 */
function noReasonFallbacks(file: string, text: string, keys: ReadonlySet<string>): string[] {
  const out: string[] = [];
  const walk = (n: ts.Node): void => {
    if ((ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) && (keys.has(n.text) || n.text.endsWith(".noReason"))) {
      out.push(`${file}::${n.text}`);
    }
    if (ts.isIdentifier(n) && GONE_NAMES.has(n.text)) out.push(`${file}::${n.text}`);
    ts.forEachChild(n, walk);
  };
  walk(parse(file, text));
  return out;
}

describe("S1 · 读通道层的函数不写句子", () => {
  it("★ 界面生产代码零处（正控：旧的按层说那一形会被报出来）", () => {
    const fixture = `
      function saidOfTransport(e: ChanError, machine: string): string {
        switch (e.error.layer) {
          case "hop": return copyText("control.channel.offline", { machine });
          default: return e.message;
        }
      }
      const ok = (e: ChanError) => e.error.layer === "peer" && [1].map(() => copyText("x"));
    `;
    expect(layerSayers("夹具.ts", fixture), "扫描器认不出按层说的那一形 —— 下面的零命中不作数").toEqual(["夹具.ts::saidOfTransport"]);
    expect(SOURCES.length, "一份生产源码都没读到").toBeGreaterThan(0);
    expect(SOURCES.flatMap((s) => layerSayers(s.file, s.text))).toEqual([]);
  });
});

describe("S2 · 界面不兜「原因不明」", () => {
  it("★ `.noReason` 键 · `Refusals` · `asSaid` · `byCode` 在界面生产代码零处（正控：夹具里那几形会被报出来）", () => {
    const fixture = `
      const r: Refusals = { byCode: (_c, s) => s, noReason: () => copyText("tmuxControl.kill.noReason") };
      asSaid(() => "");
    `;
    expect(noReasonFallbacks("夹具.ts", fixture, NO_REASON_KEYS).sort()).toEqual(
      ["夹具.ts::Refusals", "夹具.ts::asSaid", "夹具.ts::byCode", "夹具.ts::tmuxControl.kill.noReason"].sort(),
    );
    expect(SOURCES.flatMap((s) => noReasonFallbacks(s.file, s.text, NO_REASON_KEYS))).toEqual([]);
  });
});
