/**
 * **手机生产源码里不写死界面中文**：手机各模块 `src/main/` 下的 `.kt` 里每一个含汉字的字符串字面量，要么按位置放过，
 * 要么逐条登记在 `mobile-literal-exempt.json`（按 文件 · 那句 · 用途 认）；其余一律红 —— 改成 `copyText("键", …)` 取文。
 *
 * # 为什么
 *
 * 手机、桌面、后端读同一张 `src/shared/copy/table.json`。桌面那一半由普查 ＋ 待办表（`copy-ledgers.vitest.ts` 的 CP2b · CP2c）
 * 钉住「抽完的文件不许长回字面量」；手机抽完了（待办表直接是空的），就不另起待办表，直接判零。
 * 口径与桌面那两条一致：日志不对外（桌面放过 console / tracing）、前置条件的报错是程序员错（桌面放过 panic / assert），
 * 其余对外的字只从表里来。
 *
 * # 判什么
 *
 * 1. 人群：生产 Kotlin（`kotlin-literals.ts` 按词法读，注释不算）里含汉字的字面量；模板那一截记 `{…}`。
 * 2. 按位置放过（`PASS_CALLS`）：`Log.v/d/i/w/e/wtf(…)` 日志 · `testTag(…)` · `require/check(…) { … }` 前置条件。
 * 3. 其余每一处的（文件 · 字面量）都要在豁免表里、用途取自闭集 `USES`；否则红。
 * 4. 豁免表无死条：登记的那句在那份文件里真有一处命中、文件存在。
 * 5. 正控：同一个读法对着样本认出界面字、放过日志 / testTag / 前置条件 / 注释，模板记成 `{…}`。
 *
 * # 不判什么（诚实段）
 *
 * - 只认含汉字的：纯英文、纯符号（`⊕` `▸`）不在人群里（与桌面普查「含汉字」那一刀同）。
 * - 不判豁免登记得对不对：用途写成「内部」的那一句到底上不上屏，靠人复核（登记时写清去向）。
 * - 拼接绕过（`"连" + "接"` 各一个字也照样数得到；`String(charArrayOf(…))` 数不到）：防手滑，不防故意。
 * - `res/values/strings.xml` 不在这里：清单要求住在 xml 的那几条照留，见豁免表的说明。
 */
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { productionKtFiles } from "../test-support/production-sources.ts";
import { REPO_ROOT } from "../test-support/repo-root.ts";
import { hanLiteralsIn } from "./kotlin-literals.ts";

const EXEMPT_PATH = resolve(REPO_ROOT, "tests", "copy", "mobile-literal-exempt.json");

/** 按位置放过的调用：日志 · testTag · 前置条件（程序员错，同桌面放过 panic / assert）。 */
const PASS_CALLS = /(^|\.)(Log\.(v|d|i|w|e|wtf)|testTag|require|check|requireNotNull|checkNotNull)$/;

/** 豁免的用途闭集：这几类字面量不上屏，不该进表。 */
export const USES = {
  词表: "判据或过滤用的词表（禁词 · 认词），不显示",
  命令: "发给远端 shell / tmux 的命令串，字落在远端终端，不是手机界面上的字",
  内部: "内部诊断 · 断言 · 协议错的说明，只进日志 · 异常栈 · 复制详情的原话栏，不当句子上屏（登记时写清去向）",
} as const;
type Use = keyof typeof USES;

interface Exempt {
  text: string;
  use: Use;
  why: string;
}

interface Hit {
  file: string;
  line: number;
  text: string;
}

function hitsOf(files: { file: string; text: string }[]): Hit[] {
  return files.flatMap((f) =>
    hanLiteralsIn(f.text)
      .filter((l) => !PASS_CALLS.test(l.call))
      .map((l) => ({ file: f.file, line: l.line, text: l.text })),
  );
}

describe("手机生产源码不写死界面中文（文案只从 table.json 来）", () => {
  it("正控：认出界面字、模板记 {…}；放过日志 · testTag · 前置条件 · 注释", () => {
    const sample = [
      "// 注释里的「连接」不算",
      "/* 块注释 /* 嵌套 */ 也不算 */",
      'Text("连接失败：${e.message}")',
      'android.util.Log.w(TAG, "日志不算 $x")',
      'Modifier.testTag("标签不算")',
      'require(ok) { "前置条件不算" }',
      'val s = """原样串 $n 行"""',
      "val c = '\"' // 字符字面不吞后文",
      'title = "新建会话"',
    ].join("\n");
    const got = hitsOf([{ file: "x.kt", text: sample }]).map((h) => `${h.line}:${h.text}`);
    expect(got).toEqual(["3:连接失败：{…}", "7:原样串 {…} 行", "9:新建会话"]);
  });

  it("★ 零处写死的界面中文（豁免表里的除外）· 豁免表无死条", () => {
    const files = productionKtFiles("src/mobile");
    expect(files.length, "一份手机生产 .kt 都没扫到 —— 下面是空集对空集").toBeGreaterThan(100);
    const exempt = JSON.parse(readFileSync(EXEMPT_PATH, "utf8")).files as Record<string, Exempt[]>;
    const all = files.flatMap((f) => hanLiteralsIn(f.text).map((l) => ({ ...l, file: f.file })));
    expect(all.length, "人群为 0 —— 读法没接上（日志里明明有中文）").toBeGreaterThan(0);
    const bad: string[] = [];
    const used = new Set<string>();
    for (const h of hitsOf(files)) {
      const mine = (exempt[h.file] ?? []).find((e) => e.text === h.text);
      if (mine) used.add(`${h.file}\u0000${h.text}`);
      else bad.push(`${h.file}:${h.line} · ${h.text}`);
    }
    expect(
      bad,
      "这几处在手机界面上写死了中文 —— 改成 copyText(\"键\", \"名\" to 值) 取文（src/shared/copy/table.json）；" +
        "不上屏的（词表 · 远端命令 · 内部诊断）才进 tests/copy/mobile-literal-exempt.json",
    ).toEqual([]);
    const dead: string[] = [];
    const badUse: string[] = [];
    for (const [f, list] of Object.entries(exempt)) {
      if (!existsSync(resolve(REPO_ROOT, f))) dead.push(`${f}（文件不在）`);
      for (const e of list) {
        if (!(e.use in USES)) badUse.push(`${f} · ${e.text} · ${e.use}`);
        if (!e.why?.trim()) badUse.push(`${f} · ${e.text}（没写去向）`);
        if (!used.has(`${f}\u0000${e.text}`)) dead.push(`${f} · ${e.text}`);
      }
    }
    expect(badUse, `用途不在闭集里（${Object.keys(USES).join(" · ")}）或没写去向`).toEqual([]);
    expect(dead, "死豁免：那份文件里已没有这句 —— 从 mobile-literal-exempt.json 删掉").toEqual([]);
  });
});
