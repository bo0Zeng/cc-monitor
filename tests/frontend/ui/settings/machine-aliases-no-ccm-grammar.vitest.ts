// @vitest-environment node
/**
 * 别名页里零处自己解析 ccm 参数（结构判据）。
 *
 * 要求：「表单 ↔ 参数的互转交后端，用后端 ccm 本来的解析器、同一份语法；界面删掉自己那两份函数，只调后端、只画」。
 * 从前 `machine-aliases.ts` 自己抄了一份 ccm 文法（表单 → 参数、参数 → 表单两个函数）：漏了七个旗标、把带空格的参数按空白拆开、
 * 把认不得的 ccm 旗标挪到 `--` 左边 —— 点「改」只改个名字，存下去别名就坏了。
 *
 * # 判法
 *
 * 旗标的全集取后端原文（`src/backend/control/ccm/argv.rs` 的 `mod flag` 里每一个常量，含 `--` 与 `new`）——
 * 与被扫的界面文件异源。别名页那两份生产文件（剥注释）里：
 * - 字符串字面量恰是某个旗标、或以「旗标=」打头 ⇒ 命中；
 * - 按空白切一串（`.split(/\s…/)`）⇒ 命中。
 * 两份都零命中。正控：同一把尺子量一段埋了旗标与按空白切的样本，数得出。
 *
 * ⚠ 射程如实写：把旗标拆成几段拼（`"--acc" + "ount"`）能从缝里过去；别的界面文件不在这里量。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { REPO_ROOT } from "../../../test-support/repo-root";

const FILES = ["src/frontend/ui/settings/machine-aliases.ts", "src/frontend/ui/alias-reads.ts", "src/frontend/ui/settings/profiles-list.ts", "src/frontend/ui/profiles-reads.ts"];

/**
 * 剥注释（块注释 · 整行与行尾的 `//`）。不用 `test-support/strip-comments`：那台状态机不认 TS 的正则字面量，
 * `machine-aliases.ts::describeArgs` 里的 `/[\s'"]/` 会让它把后面一大段读成字符串、注释漏剥（首跑逮到过）。
 * 行尾那一形只认「空白 ＋ //」，网址里的 `://` 不受影响。
 */
function stripTsComments(src: string): string {
  return src
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .split("\n")
    .map((l) => l.replace(/(^|\s)\/\/.*$/, ""))
    .join("\n");
}

/** 后端 `argv.rs::flag` 里每一个常量的值。 */
function ccmWords(): string[] {
  const src = readFileSync(resolve(REPO_ROOT, "src/backend/control/ccm/argv.rs"), "utf8");
  const at = src.indexOf("pub(crate) mod flag {");
  const body = src.slice(at, src.indexOf("\n}\n", at));
  return [...body.matchAll(/pub\(crate\) const [A-Z_]+: &str = "([^"]*)";/g)].map((m) => m[1]);
}

/** 一段（已剥注释的）代码里的命中。 */
function hits(code: string, words: readonly string[]): string[] {
  const out: string[] = [];
  for (const w of words) {
    for (const q of ['"', "'", "`"]) {
      if (code.includes(`${q}${w}${q}`)) out.push(`${q}${w}${q}`);
      if (w.startsWith("--") && w.length > 2 && code.includes(`${q}${w}=`)) out.push(`${q}${w}=`);
    }
  }
  for (const m of code.matchAll(/\.split\(\s*\/\\s/g)) out.push(m[0]);
  return out;
}

describe("别名页零处自己解析 ccm 参数", () => {
  it("旗标全集取自后端原文：恰是那几条常量，含分界 `--` 与位置词 `new`", () => {
    const words = ccmWords();
    const declared = (readFileSync(resolve(REPO_ROOT, "src/backend/control/ccm/argv.rs"), "utf8").split("pub(crate) mod flag {")[1] ?? "")
      .split("\n}\n")[0]
      .match(/pub\(crate\) const /g)!.length;
    expect(words.length, "抠出来的与声明的条数对不上 —— 抽取坏了").toBe(declared);
    expect(words).toContain("--");
    expect(words).toContain("new");
    expect(words).toContain("--account-dir");
  });

  it("正控：埋了旗标、旗标=值、按空白切的样本数得出；注释里提到的不算", () => {
    const planted = 'const a = ["--", "--account"]; // "--cwd"\nconst b = `--ccm-tmux=${n}`; /* "--base" */ s.trim().split(/\\s+/);';
    expect(hits(stripTsComments(planted), ccmWords()).sort()).toEqual(['"--"', '"--account"', "`--ccm-tmux=", ".split(/\\s"].sort());
  });

  it.each(FILES)("%s 零命中", (f) => {
    const code = stripTsComments(readFileSync(resolve(REPO_ROOT, f), "utf8"));
    expect(hits(code, ccmWords()), `${f} 里又在自己认 / 拼 ccm 参数 —— 交后端（aliases-to-form / aliases-from-form）`).toEqual([]);
  });
});
