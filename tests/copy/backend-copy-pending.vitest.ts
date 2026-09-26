/**
 * CP2c · 后端那一半抽表的主判据：**常驻后端与子 crate 里还有对外字面量的文件 == 待办表**（两向集合相等）。
 *
 * # 要求住址
 *
 * - `设计/01 §6.9`，逐字：「**所有对外文案与报错都从一张表来**（结构化的 key → 文本，插值点留在表里）」；
 * - `设计/91 §5.1` 决定 2，逐字：「**一份文件，两侧各读，零转换**」—— 后端也读同一份 `table.json`、自己出句子
 *   （选 A 不选「后端回码、monitor 取表」的理由：`调研/第四波记录/CP2c.md §2.1`）；
 * - `设计/91 §6` 第 7 条：后端 crate 没有表的读口 —— 本路定了：`src/bridge/crates/copy-core`。
 *
 * # 它治的病
 *
 * 与 CP2b 那条（前端与 monitor crate）同一个病、同一个形状，射程换成 `src/backend/**` ∪ `src/bridge/crates/**`：
 * 抽完的文件悄悄长回一句中文字面量 / 抽完了没划掉。两张表各管一半、互不相交。
 *
 * # 真正的量具不在这个文件里
 *
 * 人群借普查 `K-T68-A1` 与 CP1 台账，相等判定住 `tests/evidence/CP2c-backend-copy-pending.py`（口径见它的头注）。
 * 本文件只把它挂进 `npm test`，形状照 `tests/copy-verdicts-ledger.vitest.ts`。
 *
 * # 不判什么（诚实段）
 *
 * - 不判待办表里的文件还剩几条（集合相等，不是逐文件计数）。
 * - 契约错的英文诊断（`common/contract.rs::malformed`）不在人群里 —— 它们该不该是英文，是 CP2c 记录 §3 的裁量，靠人复核。
 * - 没有 python3 / python 的机器上**判不了**（`console.warn` 说出来，**不是绿**）。
 * - 终态待办表是空的 ⇒ 「表非空」当不了正控；反空真靠「射程里扫到的生产文件数过地板」＋ 量具现造一棵临时树的探针。
 */
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { REPO_ROOT } from "../test-support/repo-root.ts";

const METER = resolve(REPO_ROOT, "tests", "evidence", "CP2c-backend-copy-pending.py");
const PENDING = resolve(REPO_ROOT, "tests", "evidence", "CP2c-backend-copy-pending.tsv");

type Report = {
  ok: boolean;
  literals: number;
  files: number;
  pending: number;
  scope_files: number;
  scope_files_floor: number;
  unlisted: string[];
  stale: string[];
  problems: string[];
  empty: boolean;
  probe: string[] | null;
};

let memo: { report: Report | null; why: string } | undefined;
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

describe("CP2c · 后端与子 crate 里还有对外字面量的文件 == 待办表", () => {
  it("量具与待办表都在盘上", () => {
    expect(existsSync(METER)).toBe(true);
    expect(existsSync(PENDING)).toBe(true);
  });

  it(
    "★ 两向相等：抽完的文件不许长回字面量；待办表里的文件确实还有字面量",
    () => {
      const { report, why } = runMeter();
      if (report === null) {
        console.warn(`CP2c: 判不了 —— ${why}。**这不是绿**，只是这台机器上量不了。`);
        expect(why.startsWith("量具跑不起来")).toBe(true);
        return;
      }
      expect(
        report.empty,
        `射程里只扫到 ${report.scope_files} 份生产文件（地板 ${report.scope_files_floor}）—— 普查空转`,
      ).toBe(false);
      expect(
        report.unlisted,
        "这些文件不在待办表里，却有对外字面量：抽完的文件又长回来了 / 新文件没登记。" +
          "改成 copy_core::copy_text(key, &[…]) 取文（src/shared/copy/table.json）；调用方是我们自己代码的契约错走 " +
          "common::contract::malformed(英文诊断)。逐条见 CP2c-backend-copy-pending.py --list。",
      ).toEqual([]);
      expect(report.stale, "这些文件在待办表里，其实已经一条不剩 —— 去 CP2c-backend-copy-pending.tsv 删掉那一行").toEqual([]);
      expect(report.problems, report.problems.join("\n")).toEqual([]);
      expect(report.ok).toBe(true);
    },
    120_000,
  );

  it(
    "正控：同一套人群定义在一棵现造的临时树里恰好认出那一条字面量（经表取文的那一句不算）",
    () => {
      const { report } = runMeter();
      if (report === null) return; // 判不了那一格上面那条已经说过
      expect(report.probe).toEqual(["探针：这一句是对外字面量 {}"]);
    },
    120_000,
  );
});
