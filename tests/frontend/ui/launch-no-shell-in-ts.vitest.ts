// @vitest-environment node
/**
 * `设计/90 §3` 条 1：**前端不许出现 shell 串拼接** —— 原文逐字：
 *
 * > 1. 前端不许出现 shell 串拼接 —— `src/**\/*.ts` 里零 `tmux ` / `&&` 命令串字面量；
 *
 * 住址：`设计/90 §3`（三条可机械执行的判据的第一条）· `设计/00 §2.5 ④`「同一条命令串只留 Rust 那两份
 * （CLI ＋ 载荷）」。〔LR2〕接替 `tests/session-backend-gate.vitest.ts`（`INVARIANTS §31` 最终形态第①条的
 * 机检）：那一条整份豁免了 `session-backend.ts`（「它就是那一层」），而那一层今天在 Rust 里 ——
 * TS 兜底一族（`launch-render-fallback.ts` · `session-backend.ts` · `remote-launch.ts` 五个 builder）
 * 零生产调用、LR2 按 `00 §2.5 ④` 删了 ⇒ 豁免没了，射程从「只认 `tmux <动词> -`」补上 `&&`。
 *
 * # 人群：`src/**\/*.ts` 全集，不开例外
 *
 * 照 `90 §3` 原文的射程：`src/` 下每一份生产 `.ts`（排掉 `*.test.ts` / `*.vitest.ts` —— 判据住那里，
 * 读到自己会恒绿；排掉 `src/frontend/ui/generated/` —— ts-rs 生成的线上类型，不是人写的，它们只有类型没有字面量）。
 * 共享遍历 `test-support/production-sources.ts` 按构造做这两件事。
 * 〔LR2〕两份夹具用例表（`launch-payload-golden.ts` · `launch-tmux-outer-golden.ts`）的手写期望逐字就是整条命令，
 * 原来住 `src/`；它们不是前端，挪进了 `tests/test-support/`（主会话 09-25 裁：射程取全集、不开例外表）。

 * # 命中的口径
 *
 * 剥注释之后，**字符串字面量**（`"…"` / `'…'` / 模板串的静态部分）里出现：
 *  - `&&`；或
 *  - `tmux <动词> -…`（动词后面紧跟旗标 —— 同 `session-backend-gate` 首跑逼出来的收窄：
 *    文案「已拉起 tmux attach」是给人看的话，不是命令）。
 * ⚠ 射程如实写：把命令拆成几段拼（`"tmux new-" + "session -d"`）能从缝里过去；
 *   代码里的 `a && b`（不在字面量里）按构造不算。
 */
import { describe, it, expect } from "vitest";

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { productionTsFiles } from "../../test-support/production-sources";
import { REPO_ROOT } from "../../test-support/repo-root";
import { stripComments } from "../../test-support/strip-comments";

const TMUX_VERBS = [
  "new-session",
  "send-keys",
  "attach",
  "attach-session",
  "kill-session",
  "rename-session",
  "set-option",
  "has-session",
  "capture-pane",
  "list-sessions",
];

/** 字符串字面量的文本（模板串只取静态部分，`${…}` 里的表达式跳过）。输入须已剥注释。 */
function stringLiterals(code: string): string[] {
  const out: string[] = [];
  const n = code.length;
  let i = 0;
  const skipQuoted = (start: number, q: string): number => {
    let k = start + 1;
    let buf = "";
    while (k < n && code[k] !== q) {
      if (code[k] === "\\") {
        buf += code.slice(k, k + 2);
        k += 2;
      } else {
        buf += code[k];
        k += 1;
      }
    }
    out.push(buf);
    return k + 1;
  };
  const skipTemplate = (start: number): number => {
    let k = start + 1;
    let buf = "";
    while (k < n && code[k] !== "`") {
      if (code[k] === "\\") {
        buf += code.slice(k, k + 2);
        k += 2;
      } else if (code[k] === "$" && code[k + 1] === "{") {
        buf += " ";
        k = skipExpr(k + 2);
      } else {
        buf += code[k];
        k += 1;
      }
    }
    out.push(buf);
    return k + 1;
  };
  /** 跳过 `${` 之后的表达式直到配对的 `}`；表达式里的字面量照样收。 */
  const skipExpr = (start: number): number => {
    let depth = 1;
    let k = start;
    while (k < n && depth > 0) {
      const c = code[k];
      if (c === '"' || c === "'") k = skipQuoted(k, c);
      else if (c === "`") k = skipTemplate(k);
      else {
        if (c === "{") depth += 1;
        if (c === "}") depth -= 1;
        k += 1;
      }
    }
    return k;
  };
  /** 正则字面量：`/` 出现在「能开始一个表达式」的位置才算（前一个非空字符是运算符 / 开括号 / `return`）。
   *  不认它的话，`/[\s'"]/` 里的引号会被读成字符串开头，一路吞到下一个同种引号（首跑逮到过）。 */
  const regexCanStart = (at: number): boolean => {
    let k = at - 1;
    while (k >= 0 && /\s/.test(code[k])) k -= 1;
    if (k < 0) return true;
    if ("(,=:[!&|?{};+-*%<>~^".includes(code[k])) return true;
    return /\b(?:return|typeof|case|in|of)$/.test(code.slice(Math.max(0, k - 6), k + 1));
  };
  const skipRegex = (start: number): number => {
    let k = start + 1;
    let inClass = false;
    while (k < n && code[k] !== "\n") {
      if (code[k] === "\\") k += 2;
      else if (code[k] === "[" || code[k] === "]") {
        inClass = code[k] === "[";
        k += 1;
      }
      else if (code[k] === "/" && !inClass) return k + 1;
      else k += 1;
    }
    return k;
  };
  while (i < n) {
    const c = code[i];
    if (c === '"' || c === "'") i = skipQuoted(i, c);
    else if (c === "`") i = skipTemplate(i);
    else if (c === "/" && regexCanStart(i)) i = skipRegex(i);
    else i += 1;
  }
  return out;
}

/** 一段字面量文本里的 shell 命令串命中（口径见头注）。 */
function shellHits(literal: string): string[] {
  const hits: string[] = [];
  if (literal.includes("&&")) hits.push("&&");
  for (const v of TMUX_VERBS) {
    if (new RegExp(`\\btmux\\s+${v}\\s+-`).test(literal)) hits.push(`tmux ${v} -`);
  }
  return hits;
}

function hitsIn(text: string): string[] {
  return stringLiterals(stripComments(text, "ts")).flatMap(shellHits);
}

describe("设计/90 §3 条 1：前端零 shell 串", () => {
  const all = new Map(productionTsFiles("src").map((s) => [s.file, s.text] as const));

  it("量具自检：抽字面量 ＋ 口径在一段已知语料上恰好命中该命中的（正控），代码里的 && 与注释不算", () => {
    const corpus = [
      "const a = `tmux attach -t ${t}`;",
      'const b = "cd x && exec y";',
      "const c = x && y;",
      "// tmux send-keys -t x Enter && y",
      'const d = "已拉起 tmux attach";',
      "const e = `${f(`tmux new-session -d -s ${n}`)}`;",
      // 正则里的引号不许被读成字符串开头：读错的话 `'"]/.test(a) && ` 会被当成字面量、多出一处 `&&`。
      "const r = /[\\s'\"]/.test(a) && 'x'; const g = 'plain';",
    ].join("\n");
    expect(hitsIn(corpus)).toEqual(["tmux attach -", "&&", "tmux new-session -"]);
  });

  it("量具自检：人群真的是 src/ 生产段全集（否则零命中是因为人群空了）", () => {
    // 起会话那条链上的几份必须在人群里 —— 它们正是本条要看的东西；测试文件与生成物必须不在。
    for (const f of ["src/frontend/ui/remote-launch-run.ts", "src/frontend/ui/launch-requests.ts", "src/frontend/ui/settings/panel.ts", "src/frontend/ui/entry-main.ts"]) {
      expect(all.has(f), `${f} 不在人群里 —— 遍历坏了`).toBe(true);
    }
    expect([...all.keys()].some((f) => f.includes(".vitest.") || f.includes(".test.") || f.startsWith("src/frontend/ui/generated/"))).toBe(false);
    expect(all.size).toBeGreaterThan(150); // 抽取器自检（不是判据）：09-25 现打 171
  });

  it("★★ src/**/*.ts 生产段里零 shell 命令串字面量（不开例外）", () => {
    const hits: string[] = [];
    for (const f of [...all.keys()].sort()) {
      for (const h of hitsIn(all.get(f) as string)) hits.push(`${f}  ${h}`);
    }
    expect(
      hits,
      "前端拼了 shell 串 —— `设计/90 §3` 条 1 逐字禁这件事（`00 §2.5 ④`：命令串只留 Rust 那两份）。\n" +
        "改成交结构化请求给后端渲染（`render_launch_payload` / `render_ccm_launch`）；\n" +
        "若它是规格 / 夹具（不是前端），它就不该住 `src/`。\n",
    ).toEqual([]);
  });

  it("★ 正控：挪出去的那两份夹具用例表确实满是命令串 —— 同一把尺子量得到它们（否则上一条的零是尺子瞎了）", () => {
    for (const f of ["tests/test-support/launch-payload-golden.ts", "tests/test-support/launch-tmux-outer-golden.ts"]) {
      const text = readFileSync(resolve(REPO_ROOT, f), "utf8");
      expect(hitsIn(text).length, `${f} 用同一把尺子量不出命中`).toBeGreaterThan(5);
    }
  });

  it("★ TS 兜底一族不许回来：文件不在盘上、导出名在 src/ 生产段零命中", () => {
    for (const f of ["src/launch-render-fallback.ts", "src/session-backend.ts"]) {
      expect(all.has(f), `${f} 回来了`).toBe(false);
    }
    const names = [
      "renderFallback",
      "SESSION_BACKEND",
      "TMUX_BACKEND",
      "buildResumeDirectCmd",
      "buildResumeTmuxCmd",
      "buildResumeIntoExistingTmuxCmd",
      "buildLauncherCmd",
      "buildAttachCmd",
      "buildOpenTerminalCmd",
      "buildEnvPrefix",
      "posixQuote",
    ];
    const found: string[] = [];
    for (const [f, text] of all) {
      const code = stripComments(text, "ts");
      for (const nm of names) if (new RegExp(`\\b${nm}\\b`).test(code)) found.push(`${f}  ${nm}`);
    }
    // 正控：同一把扫法在一段含这些名字的代码上必须命中（否则零命中是扫法坏了）。
    const probe = stripComments("export function renderFallback() {}\nconst x = posixQuote(y);", "ts");
    expect(names.filter((nm) => new RegExp(`\\b${nm}\\b`).test(probe))).toEqual(["renderFallback", "posixQuote"]);
    expect(found).toEqual([]);
  });
});
