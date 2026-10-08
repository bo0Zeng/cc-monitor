/**
 * 三条文案台账判据挂进 `npm test`：**CP1 存疑带裁决台账 · CP2b 前端待办表 · CP2c 后端待办表**，各自两向集合相等。
 *
 * # 为什么三条住一个文件
 *
 * 三条的人群都借普查 `tests/evidence/K-T68-A1-outward-copy-census.py`，各自的量具都要把整仓扫一遍。
 * 原先三个文件各起一个 python、并行各扫一遍：云端 windows runner（4 核）上三趟整仓扫挤在一起，
 * `backend-copy-pending` 撞 120 s 期限、同一时刻起的 S27 小量具被拖到 70 s（CI 37848630512 第一趟）。
 * ⇒ 现在一个进程、一趟普查（`tests/evidence/CP-copy-judges.py`）出三份读数；三份量具的口径、正控一条没动，
 *   各自的命令行照旧能单跑（人读 · `--list` · `--selftest`）。
 * ⚠ 别再拆回三个文件、也别在别的测试里另起这三份量具：每多一个文件就多一趟并行的整仓扫。
 *
 * # 判得了 / 判不了
 *
 * 没有 python3 / python 的机器上**判不了**（`console.warn` 说出来，**不是绿**），只认「量具跑不起来」这一条具名理由。
 */
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { REPO_ROOT } from "../test-support/repo-root.ts";

const EV = resolve(REPO_ROOT, "tests", "evidence");
const RUNNER = resolve(EV, "CP-copy-judges.py");
const CENSUS = resolve(EV, "K-T68-A1-outward-copy-census.py");
const CP1_METER = resolve(EV, "CP1-copy-verdicts.py");
const CP1_LEDGER = resolve(EV, "CP1-copy-verdicts.tsv");
const CP2B_METER = resolve(EV, "CP2b-copy-pending.py");
const CP2B_PENDING = resolve(EV, "CP2b-copy-pending.tsv");
const CP2C_METER = resolve(EV, "CP2c-backend-copy-pending.py");
const CP2C_PENDING = resolve(EV, "CP2c-backend-copy-pending.tsv");

type Cp1Report = {
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
type Cp2bReport = {
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
type Cp2cReport = {
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
type Reports = { CP1: Cp1Report; CP2b: Cp2bReport; CP2c: Cp2cReport };

let memo: { reports: Reports | null; why: string } | undefined;
/** 一趟普查出三份读数；本文件里每条都读这同一份。量具「不相等」退 1、空转退 3（照样吐 JSON）⇒ 非零退出码要接住。 */
function reports(): { reports: Reports | null; why: string } {
  memo ??= runOnce();
  return memo;
}

function runOnce(): { reports: Reports | null; why: string } {
  const tried: string[] = [];
  // Windows 的 runner 上常常只有 `python` 没有 `python3` ⇒ 两个名字都试。
  for (const py of ["python3", "python"]) {
    let raw: string | undefined;
    try {
      raw = execFileSync(py, [RUNNER, "--json"], {
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
    if (raw !== undefined) return { reports: JSON.parse(raw.trim().split("\n").pop()!) as Reports, why: "" };
  }
  return { reports: null, why: `量具跑不起来（${tried.join(" · ")}）` };
}

/** 判不了那一格：只认具名理由，**不是绿**。→ 判得了时的读数，判不了时 null。 */
function readOrWarn(tag: string): Reports | null {
  const { reports: r, why } = reports();
  if (r === null) {
    console.warn(`${tag}: 判不了 —— ${why}。**这不是绿**，只是这台机器上量不了。`);
    expect(why.startsWith("量具跑不起来")).toBe(true);
  }
  return r;
}

const RUN_MS = 120_000;

it("量具 · 台账 · 待办表 · 人群的定义都在盘上", () => {
  for (const f of [RUNNER, CENSUS, CP1_METER, CP1_LEDGER, CP2B_METER, CP2B_PENDING, CP2C_METER, CP2C_PENDING]) {
    expect(existsSync(f), f).toBe(true);
  }
});

/**
 * CP1（首条）：**存疑带逐条裁决台账 == 普查今天的存疑带**，两向集合相等。
 *
 * ## 它治的病
 *
 * 普查把对外文案分成「主集」与「存疑带」（先算成变量再交出去、或装进要人读才知道语义的字段里 —— 静态扫描分不出上不上界面）。
 * 存疑带「**对账开工前必须逐条裁完**」，裁完的结果住 `CP1-copy-verdicts.tsv`（一条一行：住址 · 原文 · 裁词 · 依据）。
 * 台账是一次性裁出来的，**源码会一直长**：有人往 `src/` 加一句 `let msg = format!("…")` 再 `Err(msg)`，
 * 它就进了存疑带、却没人裁。⇒ **普查有、台账没有** 红（新文案没裁）；**台账有、普查没有** 也红（删改了文案台账没跟）。
 * 人群定义住普查脚本（本条**不抄**它的正则），相等判定住 `CP1-copy-verdicts.py`。
 *
 * ## 不判什么（诚实段）
 *
 * - **不判裁得对不对**：只判人群相等、裁词在四档里、依据以 `[不对外]/[对外]/[可达存疑]` 开头（口径住量具头注四、五段，靠人复核）。
 * - **身份键不含行号**（文件 · 原文全文 · 同文件同文第几次）：只改行号不红；同一文件里把同一句话挪个位置也不红。
 * - **主集那 2 千条不归本条管**，本条只管存疑带。
 */
describe("CP1 · 存疑带裁决台账与普查两向相等", () => {
  it(
    "台账人群 == 普查存疑带：一条不多、一条不少，每行裁词都在四档里",
    () => {
      const r = readOrWarn("CP1");
      if (r === null) return;
      const report = r.CP1;
      // 反空真：正控在量具那边（临时树里现造恰好一句存疑带样本，普查必须恰好认出它，没过即退 3）。
      // 原来是地板 300：全量抽表之后存疑带本来就该变小（合并那一拍 241），地板把「抽对了」读成「尺子坏了」。
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
    RUN_MS,
  );
});

/**
 * CP2b · 全量抽表的主判据：**还有对外字面量的文件 == 待办表**（两向集合相等）。
 *
 * ## 要求住址
 *
 * - 「**所有对外文案与报错都从一张表来**（结构化的 key → 文本，插值点留在表里）」；
 * - 「**全量抽表**，并照台账的『改』与术语表的换法改句子」；
 * - 用户 2026-09-17「**要引入一层文案表。把文本都抽出来解耦。**」。
 *
 * ## 它治的病
 *
 * 抽表是一个文件一个文件抽的。没有这一条，抽完的文件会悄悄长回一句中文字面量（别的路照老习惯写），
 * 而「表 ↔ 引用两向相等」（`copy-table.vitest.ts`）看不见它 —— 那条只管已经进表的。
 * ⇒ 不在待办表里的文件出现对外字面量 ⇒ 红；待办表里的文件其实已经一条不剩 ⇒ 也红（抽完了没划掉）。
 * 人群借普查（主集里 `via=literal` 的）与 CP1 台账（存疑带里不是 `[不对外]` 的），相等判定住 `CP2b-copy-pending.py`。
 *
 * ## 不判什么（诚实段）
 *
 * - 不判待办表里的文件还剩几条（集合相等，不是逐文件计数：待办文件里别的路照样会加字）。
 * - 登记例外（console / tracing / panic / 预备队 / cc-bus 注入 / 台账 `[不对外]`）不在人群里 —— 它们该不该是例外，归普查与台账的口径。
 */
describe("CP2b · 还有对外字面量的文件 == 待办表", () => {
  it(
    "★ 两向相等：抽完的文件不许长回字面量；待办表里的文件确实还有字面量",
    () => {
      const r = readOrWarn("CP2b");
      if (r === null) return;
      const report = r.CP2b;
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
    RUN_MS,
  );

  it(
    "正控：普查认得纯符号串（「符号也进表」）、也读得出出口里的取文口调用（普查读表）",
    () => {
      const r = reports().reports;
      if (r === null) return; // 判不了那一格上面那条已经说过
      expect(r.CP2b.is_copy_text).toEqual({
        "✕": true,
        "↗": true,
        " · ": true,
        "—": true,
        关闭: true,
        "{…}：{…}": false, // 只有标点与插值的排版模板不算（登记的缺口）
        x: false,
      });
      expect(r.CP2b.via_table, "出口里一条 copyText / copy_text 都没认出来 —— 普查读表那一格瞎了").toBeGreaterThan(0);
    },
    RUN_MS,
  );

  // 「`.css` 的 `content:` 与 `index.html` 的静态文本没扫」（裁：补进 CP1 语料）。
  it(
    "正控：普查认得 CSS 的 content:（解 CSS 转义）与入口 HTML 的静态文本；注释 · 脚本 · var(--x) 不算",
    () => {
      const r = reports().reports;
      if (r === null) return; // 判不了那一格上面那条已经说过
      expect(r.CP2b.static_faces).toEqual([
        "css.content:body:▸ ",
        "css.content:body:✓",
        "html.text:body:正文",
        "html.text:title:某窗口",
      ]);
    },
    RUN_MS,
  );
});

/**
 * CP2c · 后端那一半抽表的主判据：**常驻后端与子 crate 里还有对外字面量的文件 == 待办表**（两向集合相等）。
 *
 * ## 要求住址
 *
 * - 「**所有对外文案与报错都从一张表来**（结构化的 key → 文本，插值点留在表里）」；
 * - 决定 2「**一份文件，两侧各读，零转换**」—— 后端也读同一份 `table.json`、自己出句子；
 * - 后端 crate 的表读口：`src/common/copy-core`。
 *
 * ## 它治的病
 *
 * 与 CP2b 同一个病、同一个形状，射程换成 `src/backend/**` ∪ `src/common/**`：
 * 抽完的文件悄悄长回一句中文字面量 / 抽完了没划掉。两张表各管一半、互不相交。相等判定住 `CP2c-backend-copy-pending.py`。
 *
 * ## 不判什么（诚实段）
 *
 * - 不判待办表里的文件还剩几条（集合相等，不是逐文件计数）。
 * - 契约错的英文诊断（`common/contract.rs::malformed`）不在人群里 —— 它们该不该是英文靠人复核。
 * - 终态待办表是空的 ⇒ 「表非空」当不了正控；反空真靠「射程里扫到的生产文件数过地板」＋ 量具现造一棵临时树的探针。
 */
describe("CP2c · 后端与子 crate 里还有对外字面量的文件 == 待办表", () => {
  it(
    "★ 两向相等：抽完的文件不许长回字面量；待办表里的文件确实还有字面量",
    () => {
      const r = readOrWarn("CP2c");
      if (r === null) return;
      const report = r.CP2c;
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
    RUN_MS,
  );

  it(
    "正控：同一套人群定义在一棵现造的临时树里恰好认出那一条字面量（经表取文的那一句不算）",
    () => {
      const r = reports().reports;
      if (r === null) return; // 判不了那一格上面那条已经说过
      expect(r.CP2c.probe).toEqual(["探针：这一句是对外字面量 {}"]);
    },
    RUN_MS,
  );
});
