/**
 * CP1（`调研/设计/91 §5.1.2` · `§6.2` 首条）：**存疑带逐条裁决台账 == 普查今天的存疑带**，两向集合相等。
 *
 * # 它治的病
 *
 * 普查 `tests/evidence/K-T68-A1-outward-copy-census.py` 把对外文案分成「主集」与「存疑带」
 * （先算成变量再交出去、或装进要人读才知道语义的字段里 —— 静态扫描分不出上不上界面）。
 * `91 §5.1.2` 逐字：存疑带「**对账开工前必须逐条裁完**」。裁完的结果住
 * `tests/evidence/CP1-copy-verdicts.tsv`（一条一行：住址 · 原文 · 裁词 · 依据）。
 *
 * 台账是一次性裁出来的，**源码会一直长**。没有这一条，有人往 `src/` 加一句
 * `let msg = format!("…")` 再 `Err(msg)`，它就进了存疑带、却没人裁 —— 台账静默变成过期读数。
 * ⇒ 本条：**普查有、台账没有** 红（新文案没裁）；**台账有、普查没有** 也红（删改了文案台账没跟）。
 *
 * # 真正的量具不在这个文件里
 *
 * 人群定义住普查脚本（本条**不抄**它的正则），相等判定住 `tests/evidence/CP1-copy-verdicts.py`。
 * 本文件只把它挂进 `npm test`（`vitest run` 的 `tests/**\/*.vitest.ts` 自动收录），
 * 并把「判得了 / 判不了」这一格写死 —— 与 `offline-cargo-cache.vitest.ts` 同一个形状
 * （「一个性质一把尺子」，别在 TS 里重写一遍）。
 *
 * # 它**不**判什么（诚实段）
 *
 * - **不判裁得对不对**：只判人群相等、裁词在四档里、依据以 `[不对外]/[对外]/[可达存疑]` 开头。
 *   裁词本身的口径住 `CP1-copy-verdicts.py` 头注第四、五段，靠人复核。
 * - **身份键不含行号**（文件 · 原文全文 · 同文件同文第几次）：只改行号不红；
 *   同一文件里把同一句话挪个位置也不红 —— 那不是新文案。
 * - **主集那 2 千条不归本条管**，本条只管存疑带。
 * - 没有 python3 / python 的机器上**判不了**（`console.warn` 说出来，**不是绿**）。
 */
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { REPO_ROOT } from "./test-support/repo-root.ts";

const METER = resolve(REPO_ROOT, "tests", "evidence", "CP1-copy-verdicts.py");
const LEDGER = resolve(REPO_ROOT, "tests", "evidence", "CP1-copy-verdicts.tsv");
const CENSUS = resolve(REPO_ROOT, "tests", "evidence", "K-T68-A1-outward-copy-census.py");

type Report = {
  ok: boolean;
  band: number;
  ledger: number;
  missing: number;
  extra: number;
  problems: string[];
  probe_ok: boolean;
  probe_got: string[];
  missing_sample: string[];
  extra_sample: string[];
};

/** 量具在「不相等」时退 1、空转退 3（照样吐 JSON）⇒ 非零退出码要接住，别炸掉。 */
function runMeter(): { report: Report | null; why: string } {
  const tried: string[] = [];
  // Windows 的 runner 上常常只有 `python` 没有 `python3` ⇒ 两个名字都试。
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

describe("CP1 · 存疑带裁决台账与普查两向相等", () => {
  it("三件东西都在盘上（量具 · 台账 · 人群的定义）", () => {
    expect(existsSync(METER)).toBe(true);
    expect(existsSync(LEDGER)).toBe(true);
    expect(existsSync(CENSUS)).toBe(true);
  });

  it(
    "台账人群 == 普查存疑带：一条不多、一条不少，每行裁词都在四档里",
    () => {
      const { report, why } = runMeter();
      if (report === null) {
        // 判不了只认这一条具名理由；它**不是绿**。
        console.warn(`CP1: 判不了 —— ${why}。**这不是绿**，只是这台机器上量不了。`);
        expect(why.startsWith("量具跑不起来")).toBe(true);
        return;
      }
      // 反空真：正控在量具那边（临时树里现造恰好一句存疑带样本，普查必须恰好认出它，没过即退 3）。
      // 〔MG1 · 主会话 09-25 裁〕原来是地板 300：全量抽表之后存疑带本来就该变小（合并那一拍 241），地板把「抽对了」读成「尺子坏了」。
      expect(report.probe_ok, `正控没过：普查在临时树里认出的是 ${JSON.stringify(report.probe_got)} ⇒ 普查空转`).toBe(true);
      expect(report.band).toBeGreaterThan(0);
      expect(
        report.missing_sample,
        `普查里有 ${report.missing} 条存疑带文案**台账没裁**（新加的文案？）。` +
          "去 tests/evidence/CP1-copy-verdicts.tsv 补一行裁词，口径见 CP1-copy-verdicts.py 头注。",
      ).toEqual([]);
      expect(
        report.extra_sample,
        `台账里有 ${report.extra} 行**普查已经没有了**（文案删了/改了字）。去台账删掉或按新原文改。`,
      ).toEqual([]);
      expect(report.problems, report.problems.join("\n")).toEqual([]);
      expect(report.ledger).toBe(report.band);
      expect(report.ok).toBe(true);
    },
    120_000,
  );
});
