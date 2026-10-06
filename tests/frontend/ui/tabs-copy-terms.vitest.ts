// @vitest-environment node
/**
 * tab 层的界面文字**按术语表改词**之后，不许再长回来。
 *
 * 拆 `tabs.ts` 时顺手收的小尾巴：两处「建议在远端重装 ccm 助手」与两处「拉前失败」toast ⇒
 * 按 `src/shared/copy/terms.json` 改词（CP1 台账同拍）。改成了：
 * - 「拉前失败」→「切到终端窗口失败」（`拉前` 禁，术语表 say：「切到终端窗口」）；
 * - 串味提示与杀会话确认框里那句 ⇒ 不说标记（`@ccm_sid` 禁：说后果「认不出是哪个会话」），
 *   不派「重装 ccm 助手」这件活（「ccm 助手」是 `ccm` 那一条里点名禁的叫法）。
 *
 * # 判据：tab 层 12 份的**字符串字面量**里，这几个词零命中（带正控）
 *
 * - 词从哪来：`拉前` / `@ccm_sid` 两条的扫描器**取自术语表本身**（`scannerOf`，异源：表是 CP2a 立的，
 *   不是本文件定的）；「ccm 助手」在表里没有独立条目（写在 `ccm` 那一条的语境里），本文件补一个字面扫描器。
 * - 扫什么：用 TypeScript 编译器真解析出来的字符串 / 模板字面量（注释不算 —— 注释里讲历史是允许的）。
 * - 正控：同一套扫描器喂改之前的原文，必须每个词都命中（否则零命中是因为扫描器坏了）；
 *   抽取器自检：12 份里抽得出字面量，且认得出改之后的新词。
 *
 * # 买不到
 *
 * - 「改得好不好」（那是 CP1 台账的人工裁决）；tab 层之外的同类文案（归 CP2 全量抽表那一波）。
 *
 * # 第四个词：术语表的「英文实现词」（invoke / Win32 …）
 *
 * 两处「切到终端窗口失败」toast 的正文原是 `invoke 超时 …ms（后端 Win32 调用可能卡住）` 那句 Error
 * （CP1 裁 `改·§2.1` →「切到终端窗口超时」）。改完之后同一套零命中也管它：扫描器照样**取自术语表**
 * （「英文实现词」那一条，禁），正控是改之前那句原文。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import ts from "typescript";
import { describe, expect, it } from "vitest";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { loadTable, loadTerms, scannerOf } from "../../copy/copy-support.ts";

/** tab 层：拆之前的 `tabs.ts` 拆成的这 12 份（与 `tabs-split-graph.vitest.ts` 的登记表同一群）。 */
const TAB_LAYER = [
  "src/frontend/ui/tabs.ts",
  "src/frontend/ui/tab-model.ts",
  "src/frontend/ui/tab-drop.ts",
  "src/frontend/ui/tab-store.ts",
  "src/frontend/ui/tab-router.ts",
  "src/frontend/ui/tab-session-facts.ts",
  "src/frontend/ui/tab-stream-view.ts",
  "src/frontend/ui/tab-bar-view.ts",
  "src/frontend/ui/tab-bar-drag.ts",
  "src/frontend/ui/tab-bar-prefs.ts",
  "src/frontend/ui/tab-menu.ts",
  "src/frontend/ui/tab-session-actions.ts",
];

/** 一份源码里全部字符串 / 模板字面量的文字（模板的各段拼起来，插值处留空）。 */
export function stringLiterals(src: string): string[] {
  const sf = ts.createSourceFile("x.ts", src, ts.ScriptTarget.Latest, true);
  const out: string[] = [];
  const visit = (n: ts.Node): void => {
    if (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) out.push(n.text);
    else if (ts.isTemplateExpression(n)) {
      out.push([n.head.text, ...n.templateSpans.map((s) => s.literal.text)].join(""));
    }
    ts.forEachChild(n, visit);
  };
  visit(sf);
  return out;
}

/**
 * 〔全量抽表〕tab 层的界面文字搬进了文案表 ⇒ 「这 12 份说的话」= 它们的字面量 ＋ 它们经 `copyText("key")` 取的那些表条目。
 * 只数字面量的话，抽完之后本判据就对着一个空集零命中（正控那条会先红，提醒这里要跟着改）。
 */
const TABLE = loadTable();
function spokenIn(src: string): string[] {
  const viaTable = [...src.matchAll(/\bcopyText\(\s*"([^"]+)"/g)].map((m) => TABLE[m[1]]?.zh ?? `〔表里没有 ${m[1]}〕`);
  return [...stringLiterals(src), ...viaTable];
}

const terms = loadTerms();
const scanner = (word: string): RegExp => {
  const t = terms.find((x) => x.word === word);
  const rx = t ? scannerOf(t) : null;
  if (!rx) throw new Error(`术语表里没有「${word}」的扫描器 —— 表变了，回来看本判据还该不该这么取`);
  return rx;
};
const SCANNERS: ReadonlyArray<readonly [string, RegExp]> = [
  ["拉前", scanner("拉前")],
  ["@ccm_sid", scanner("@ccm_sid")],
  ["ccm 助手", /ccm\s*助手/],
  ["英文实现词", scanner("英文实现词")],
];

/** 一串文字命中了哪几个词。 */
const hitsOf = (text: string): string[] => SCANNERS.filter(([, rx]) => rx.test(text)).map(([w]) => w);

describe("〔U2〕tab 层的界面文字：拉前 / @ccm_sid / ccm 助手 /〔S4〕英文实现词 零命中", () => {
  it("★ 正控：改之前的五句原文喂同一套扫描器，四个词全命中", () => {
    const before = [
      "invoke 超时 5000ms（后端 Win32 调用可能卡住）",
      "拉前失败",
      "该 tmux 会话没有 @ccm_sid 标记，可能连到同目录里其它会话；建议在远端重装 ccm 助手以精确匹配。",
      "\n\n⚠ 未检测到会话身份标记（@ccm_sid）——「x」是按工作目录猜的，可能不是本 tab 的会话，甚至可能是同目录里另一个正在运行的 Claude。建议在远端重装 ccm 助手后再操作。",
    ];
    expect([...new Set(before.flatMap(hitsOf))].sort()).toEqual(["@ccm_sid", "ccm 助手", "拉前", "英文实现词"].sort());
  });

  it("★ 抽取器自检：12 份里抽得出字面量，且认得出改之后的新词", () => {
    const all = TAB_LAYER.flatMap((f) => spokenIn(readFileSync(resolve(REPO_ROOT, f), "utf8")));
    expect(all.length, "一个字面量都没抽到 —— 抽取器坏了").toBeGreaterThan(200);
    expect(all).toContain("切到终端窗口失败");
    expect(all.filter((s) => s === "切到终端窗口超时").length, "两处拉前超时的新正文").toBe(2);
    // 模板字面量也要抽得到（插值处留空）。
    expect(all).toContain("[tabs] 骨架未接（）：");
  });

  it("★ 零命中：12 份的字符串字面量里一处都没有", () => {
    const found: string[] = [];
    for (const f of TAB_LAYER) {
      for (const s of spokenIn(readFileSync(resolve(REPO_ROOT, f), "utf8"))) {
        const h = hitsOf(s);
        if (h.length > 0) found.push(`${f}：「${s.slice(0, 60)}」命中 ${h.join(" / ")}`);
      }
    }
    expect(found).toEqual([]);
  });
});
