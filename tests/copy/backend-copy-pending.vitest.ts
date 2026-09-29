/**
 * CP2c · 后端那一半抽表的主判据：**常驻后端与子 crate 里还有对外字面量的文件 == 待办表**（两向集合相等）。
 *
 * # 要求住址
 *
 * - `设计/01 §6.9`，逐字：「**所有对外文案与报错都从一张表来**（结构化的 key → 文本，插值点留在表里）」；
 * - `设计/91 §5.1` 决定 2，逐字：「**一份文件，两侧各读，零转换**」—— 后端也读同一份 `table.json`、自己出句子
 *   （选 A 不选「后端回码、monitor 取表」的理由：`调研/第四波记录/CP2c.md §2.1`）；
 * - `设计/91 §6` 第 7 条：后端 crate 没有表的读口 —— 本路定了：`src/common/copy-core`。
 *
 * # 它治的病
 *
 * 与 CP2b 那条（前端与 monitor crate）同一个病、同一个形状，射程换成 `src/backend/**` ∪ `src/common/**`：
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
 *
 * # 第二段（COPY · 09-27）：生产代码不许按原文认话
 *
 * 要求住址：`设计/91 §5.5`，逐字：「线上契约 | 不变：`Reply {code, message}`，`code` 仍是命令级粗码」——
 * 判「是哪一种失败」靠 `code` / 结构化字段；句子进了表就会被改写，谁按原文认它，改一个字就静默失灵
 *（CP2c 待办表的理由列：「refuse write:」等前缀被按原文认，先把判定换成码再抽）。
 * 判法：生产段（`src/backend` · `src/frontend/shell/src` · `src/common` 的 `.rs` ＋ `src` 的 `.ts`，剥注释）里
 * 字符串匹配调用的字面量参数带汉字或 `refuse write/delete` 的地方 == `RECOGNIZE_BY_TEXT`（两向，按「文件 · 串」比）。
 */
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { productionRsFiles, productionTsFiles } from "../test-support/production-sources.ts";
import { REPO_ROOT } from "../test-support/repo-root.ts";
import { stripComments } from "../test-support/strip-comments.ts";

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

/**
 * 登记的例外（都在 COPY 写区外，交主会话；`调研/第四波记录/COPY.md` §设计 ①）：
 * - cc_bus 三条认的是 cc-bus 脚本自己的输出行（`设计/91 §3.2`：cc-bus 的文本不归文案表），不是表里的句子；
 * - `accounts.ts::deriveUi` 按「过旧」「不支持账号」认 —— 两支结果相同（都是 needs-update、reason 都是 e），是死判断，可直接删；
 * - `tab-drop.ts::defaultGroupName` 认自己起的默认组名「组 N」来续号 —— 那句改写（`tabDrop.group.defaultName`）续号就断。
 */
const RECOGNIZE_BY_TEXT = [
  "src/backend/control/cc_bus.rs · 已杀会话",
  "src/backend/control/cc_bus.rs · 已摘掉",
  "src/backend/control/cc_bus.rs · 已 spawn:",
  "src/accounts.ts · 过旧",
  "src/accounts.ts · 不支持账号",
  "src/tab-drop.ts · ^组\\s*(\\d+)$",
];

const MATCH_CALL =
  /\.(?:contains|starts_with|ends_with|strip_prefix|strip_suffix|find|rfind|split_once|rsplit_once|matches|includes|startsWith|endsWith|indexOf|lastIndexOf)\(\s*b?(?:"((?:[^"\\\n]|\\.)*)"|'((?:[^'\\\n]|\\.)*)'|`([^`]*)`)/g;
const EQ_LIT = /(?:==|!=)\s*"((?:[^"\\\n]|\\.)*)"/g;
const RE_LIT = /\/((?:[^/\\\n]|\\.)+)\/[a-z]*\.(?:test|exec)\(/g;
const OUR_TEXT = /[\u4e00-\u9fff]|refuse (?:write|delete)/i;

/** 一段源码里按原文认话的串（剥过注释之后）。 */
function textRecognizers(src: string, lang: "rust" | "ts"): string[] {
  const code = stripComments(src, lang);
  const out: string[] = [];
  for (const rx of [MATCH_CALL, EQ_LIT, RE_LIT]) {
    for (const m of code.matchAll(rx)) {
      const lit = m[1] ?? m[2] ?? m[3] ?? "";
      if (OUR_TEXT.test(lit)) out.push(lit);
    }
  }
  return out;
}

describe("COPY · 生产代码不许按原文认话（判是哪种失败靠码，不靠句子）", () => {
  it("正控：现造的源码里认得出三种写法，注释里的不算", () => {
    const rs = 'fn f(e: &str) { if e.contains("超时") {} }\n// e.starts_with("指纹")\nlet x = m == "所有地址连接失败";';
    const ts = 'if (msg.startsWith("refuse write:")) {}\nif (/握手超时/.test(m)) {}';
    expect(textRecognizers(rs, "rust")).toEqual(["超时", "所有地址连接失败"]);
    expect(textRecognizers(ts, "ts")).toEqual(["refuse write:", "握手超时"]);
  });

  it("★ 生产段里按原文认话的地方 == 登记的例外（两向）", () => {
    const rs = [
      ...productionRsFiles("src/backend"),
      ...productionRsFiles("src/frontend/shell/src"),
      ...productionRsFiles("src/common"),
    ].map((f) => ({ ...f, lang: "rust" as const }));
    const ts = productionTsFiles("src").map((f) => ({ ...f, lang: "ts" as const }));
    expect(rs.length, "Rust 生产文件一份都没扫到").toBeGreaterThan(100);
    expect(ts.length, "TS 生产文件一份都没扫到").toBeGreaterThan(100);
    const found = [...rs, ...ts].flatMap((f) => textRecognizers(f.text, f.lang).map((lit) => `${f.file} · ${lit}`));
    expect(
      [...new Set(found)].sort(),
      "有生产代码按句子原文判断是哪种失败 —— 句子会被改写（文案表），改成认 code / 结构化字段",
    ).toEqual([...RECOGNIZE_BY_TEXT].sort());
  });
});
