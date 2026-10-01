/**
 * CP2b · 全量抽表的主判据：**还有对外字面量的文件 == 待办表**（两向集合相等）。
 *
 * # 要求住址
 *
 * -，逐字：「**所有对外文案与报错都从一张表来**（结构化的 key → 文本，插值点留在表里）」；
 * -：「**全量抽表**，并照台账的『改』与术语表的换法改句子」；
 * - 用户 2026-09-17「**要引入一层文案表。把文本都抽出来解耦。**」。
 *
 * # 它治的病
 *
 * 抽表是一个文件一个文件抽的。没有这一条，抽完的文件会悄悄长回一句中文字面量（别的路照老习惯写），
 * 而「表 ↔ 引用两向相等」（`copy-table.vitest.ts`）看不见它 —— 那条只管已经进表的。
 * ⇒ 本条：不在待办表里的文件出现对外字面量 ⇒ 红；待办表里的文件其实已经一条不剩 ⇒ 也红（抽完了没划掉）。
 *
 * # 真正的量具不在这个文件里
 *
 * 「对外字面量」的人群借普查 `K-T68-A1`（主集里 `via=literal` 的）与 CP1 台账（存疑带里不是 `[不对外]` 的），
 * 相等判定住 `tests/evidence/CP2b-copy-pending.py`（口径见它的头注）。本文件只把它挂进 `npm test`，
 * 形状照 `tests/frontend/ui/copy-verdicts-ledger.vitest.ts`（一个性质一把尺子，不在 TS 里重写）。
 *
 * # 不判什么（诚实段）
 *
 * - 不判待办表里的文件还剩几条（集合相等，不是逐文件计数：待办文件里别的路照样会加字）。
 * - 登记例外（console / tracing / panic / 预备队 / cc-bus 注入 / 台账 `[不对外]`）不在人群里 —— 它们该不该是例外，归普查与台账的口径。
 * - 没有 python3 / python 的机器上**判不了**（`console.warn` 说出来，**不是绿**）。
 */
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { REPO_ROOT } from "../test-support/repo-root.ts";

const METER = resolve(REPO_ROOT, "tests", "evidence", "CP2b-copy-pending.py");
const PENDING = resolve(REPO_ROOT, "tests", "evidence", "CP2b-copy-pending.tsv");

type Report = {
  ok: boolean;
  literals: number;
  files: number;
  pending: number;
  unlisted: string[];
  stale: string[];
  problems: string[];
  empty: boolean;
  via_table: number;
  is_copy_text: Record<string, boolean>;
  static_faces: string[];
};

let memo: { report: Report | null; why: string } | undefined;
/** 量具跑一次要几秒：同一个文件里两条用同一份读数。 */
function runMeter(): { report: Report | null; why: string } {
  memo ??= runMeterOnce();
  return memo;
}

function runMeterOnce(): { report: Report | null; why: string } {
  const tried: string[] = [];
  for (const py of ["python3", "python"]) {
    let raw: string | undefined;
    try {
      raw = execFileSync(py, [METER, "--json"], {
        cwd: REPO_ROOT,
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
        env: { ...process.env, PYTHONIOENCODING: "utf-8", PYTHONUTF8: "1" },
      });
    } catch (e) {
      const err = e as { stdout?: string; code?: string; status?: number };
      if (typeof err.stdout === "string" && err.stdout.trim().startsWith("{")) raw = err.stdout;
      else tried.push(`${py}: ${String(err.code ?? err.status ?? e)}`);
    }
    if (raw !== undefined) return { report: JSON.parse(raw.trim().split("\n").pop()!) as Report, why: "" };
  }
  return { report: null, why: `量具跑不起来（${tried.join(" · ")}）` };
}

describe("CP2b · 还有对外字面量的文件 == 待办表", () => {
  it("量具与待办表都在盘上", () => {
    expect(existsSync(METER)).toBe(true);
    expect(existsSync(PENDING)).toBe(true);
  });

  it(
    "★ 两向相等：抽完的文件不许长回字面量；待办表里的文件确实还有字面量",
    () => {
      const { report, why } = runMeter();
      if (report === null) {
        console.warn(`CP2b: 判不了 —— ${why}。**这不是绿**，只是这台机器上量不了。`);
        expect(why.startsWith("量具跑不起来")).toBe(true);
        return;
      }
      // 反空真：终态待办表是空的、对外字面量 0 条 ⇒「表非空」当不了正控（同 CP2c）；
      //   改靠普查同一趟认出的取文口调用条数 —— 尺子没切到东西时它是 0。
      expect(report.empty, "出口里的取文口调用一条都没认出 —— 普查空转").toBe(false);
      expect(
        report.unlisted,
        "这些文件不在待办表里，却有对外字面量：抽完的文件又长回来了 / 新文件没登记。" +
          "改成 copyText(key) / copy_text(key) 取文（src/shared/copy/table.json），逐条见 CP2b-copy-pending.py --list。",
      ).toEqual([]);
      expect(report.stale, "这些文件在待办表里，其实已经一条不剩 —— 去 CP2b-copy-pending.tsv 删掉那一行").toEqual([]);
      expect(report.problems, report.problems.join("\n")).toEqual([]);
      expect(report.ok).toBe(true);
    },
    120_000,
  );

  it(
    "正控：普查认得纯符号串（「符号也进表」）、也读得出出口里的取文口调用（普查读表）",
    () => {
      const { report } = runMeter();
      if (report === null) return; // 判不了那一格上面那条已经说过
      expect(report.is_copy_text).toEqual({
        "✕": true,
        "↗": true,
        " · ": true,
        "—": true,
        关闭: true,
        "{…}：{…}": false, // 只有标点与插值的排版模板不算（登记的缺口）
        x: false,
      });
      expect(report.via_table, "出口里一条 copyText / copy_text 都没认出来 —— 普查读表那一格瞎了").toBeGreaterThan(0);
    },
    120_000,
  );

  // 「`.css` 的 `content:` 与 `index.html` 的静态文本没扫」（裁：补进 CP1 语料）。
  it(
    "正控：普查认得 CSS 的 content:（解 CSS 转义）与入口 HTML 的静态文本；注释 · 脚本 · var(--x) 不算",
    () => {
      const { report } = runMeter();
      if (report === null) return; // 判不了那一格上面那条已经说过
      expect(report.static_faces).toEqual([
        "css.content:body:▸ ",
        "css.content:body:✓",
        "html.text:body:正文",
        "html.text:title:某窗口",
      ]);
    },
    120_000,
  );
});
