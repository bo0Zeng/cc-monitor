/**
 * **会话正文经通道直接问那台机器的后端、按形状收成品**：
 * 骨架按偏移取一段（`history-page`，主窗口 tab 与查看器都走它）· 按行号取一段（`history-lines`）· 一个子运行的记录（`history-run`，按运行读）·
 * 主线外清单（`history-branch`，冷读；实时那一路是会话流的 `branch` 格）· 那台后端的漂移账（`drift-report`）。
 *
 * # 它顶掉了什么
 *
 * 此前是 monitor 的四条 Tauri 命令（查看器整份读那一条 · `read_session_range` · `read_session_lines` ·
 * 按目录读子 agent 的那一条）：monitor 从那台后端取原文、自己解析记录（`messages.rs` / `parser.rs` / `codex_record.rs`）、
 * 编号、组载荷。记录解释搬进后端之后（`src/backend/agents/claudecode/`），后端的帧应答**就是成品** —— 每一行在渲染模型里
 * 是什么（通用记录 `LineRecord`，ts-rs 从后端导出）、行号、`cwd`、进不进界面，都是后端给的；monitor 那一跳只搬字节。
 * **本机与远端同一条路**（本机那台由 `<local>` 那条长连接答）。
 *
 * # 本文件做的只有三件（都是调用方那一侧的事）
 *
 * 1. **按类型收**：按偏移 · 按行号 · 子运行 · 主线外 · 漂移账这五条是 typed 命令，回包按后端生成的类型用（两端形状由握手指纹管）。
 *    记录本身**不解释**。
 * 2. **给载荷打上它从哪台来**（`origin`：本机不带、远端是那台的名字 —— 与实时流那一条同一个口径，这是地址不是解释）。
 * 3. **期限与翻页**：一件事一个期限，翻页只把后端交回的续点（`next` / `nextSeq`）原样交回去。
 *
 * 按偏移取一段 · 按行号取一段交「折起那一行」那份声明（`src/shared/views/folded-record.json`：省掉工具入参 · 结果正文 · 逐段改动 · 一步展开那一格；
 * 壳的旁路快照交同一份）；按记录 id 取回那一行不交（展开那一下要的就是全文）。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import type { JsonlLinePayload } from "./generated/JsonlLinePayload";
import type { LineRecord } from "./generated/LineRecord";
import type { RecordAt } from "./cards/index";
import type { Branch } from "./generated/Branch";
import type { HistoryLines } from "./generated/HistoryLines";
import type { HistoryPage } from "./generated/HistoryPage";
import type { HistoryRun } from "./generated/HistoryRun";
import type { RecordLine } from "./generated/RecordLine";
import type { DriftFaceReport } from "./generated/DriftFaceReport";
import { copyText } from "./copy-table";
import FOLDED from "../../shared/views/folded-record.json";
import { saidOf } from "./kit/detail";

/** 一页子运行记录（后端 `faces/history_face.rs` 的 `history-run`，类型从后端生成）。 */
export type RunPage = HistoryRun;

/** 按行号取回的那一段（后端 `history-lines`）：`[from, next)` 里进界面的那些（不进界面的照占号、不出现）。 */
export interface SessionLinesPage {
  from: number;
  next: number;
  eof: boolean;
  payloads: JsonlLinePayload[];
}

/** 主线外清单（后端 `history-branch`）：回退掉的那几条记录的 `id`（文件序）· 读到哪个字节（之后的由实时帧接着说）。 */
export type BranchOff = Branch;

/** 那台后端漂移账的一个面（各家的面并在一起；面名 · 后果那一句都是那一家写好的）。 */
export type RecordDriftFace = DriftFaceReport;

/** 按偏移取一段 · 子 agent 那一份：同它上一个住址（一次性远端那一趟的天花板，120 秒）。 */
const PIECE_BUDGET_MS = 120_000;
/** 漂移账：一问（30 秒，同会话读面那几问）。 */
const DRIFT_BUDGET_MS = 30_000;

/** 一条记录行（后端 `body::RecordLine`）⇒ 载荷；`origin` 由问的那一方打上（这是地址，不是解释）。 */
function payloadOf(origin: Origin, v: RecordLine): JsonlLinePayload {
  const p: JsonlLinePayload = { session_id: v.session_id, path: v.path, seq: v.seq, cwd: v.cwd, record: v.record };
  if (!isLocalOrigin(origin)) p.origin = origin;
  return p;
}

/** `history-page` 的一页 ⇒ `(载荷, 续点)`。 */
export function pageOf(origin: Origin, v: HistoryPage): { payloads: JsonlLinePayload[]; next: number; nextSeq: number; eof: boolean } {
  return { payloads: v.lines.map((l) => payloadOf(origin, l)), next: v.next, nextSeq: v.nextSeq, eof: v.eof };
}

/** `history-lines` 的一段 ⇒ [`SessionLinesPage`]。 */
export function linesOf(origin: Origin, v: HistoryLines): SessionLinesPage {
  return { from: v.from, next: v.next, eof: v.eof, payloads: v.lines.map((l) => payloadOf(origin, l)) };
}

/** typed 那几条的一问：回包按类型用（形状由握手指纹管）；失败 ⇒ 抛一个给人看的 `Error`（那一句 monitor 写好，`kit/detail.ts::saidOf` 取）。 */
async function asked<T>(reply: Promise<T>): Promise<T> {
  try {
    return await reply;
  } catch (e) {
    throw new Error(saidOf(e));
  }
}

/** **按偏移取一段** `[offset, until)`（两端、`seqBase` 取自骨架索引）：一段可能跨页，取到这一段的末尾。交折起那一行的声明。 */
export async function readRange(
  origin: Origin,
  jsonlPath: string,
  offset: number,
  until: number,
  seqBase: number,
): Promise<JsonlLinePayload[]> {
  return await readRangeAs(origin, jsonlPath, offset, until, seqBase, FOLDED);
}

/** [`readRange`] 的本体：`view` ＝ 交给后端的声明（`null` ＝ 全文）。 */
async function readRangeAs(
  origin: Origin,
  jsonlPath: string,
  offset: number,
  until: number,
  seqBase: number,
  view: object | null,
): Promise<JsonlLinePayload[]> {
  const budget = budgetWithin(PIECE_BUDGET_MS);
  const out: JsonlLinePayload[] = [];
  let at = offset;
  let seq = seqBase;
  for (;;) {
    const page = pageOf(origin, await asked(callTyped(origin, "history-page", { path: jsonlPath, offset: at, until, seq }, budget, view)));
    // 逐条接，不展开成实参：一页按字节算（1 MiB），短行能有几十万行，超过 V8（WebView2）一次调用的实参上限会抛 RangeError。
    for (const p of page.payloads) out.push(p);
    if (page.eof || page.next <= at) return out;
    at = page.next;
    seq = page.nextSeq;
  }
}

/** 骨架里认得一行在哪的那一件（`live-window.ts::SkeletonLedger.factsOf`）。 */
interface RowPlaces {
  factsOf(seq: number): { o: number; n: number } | undefined;
}

/**
 * **取回一条记录的全文**（出口省掉的正文展开那一下，`RenderContext.fullRecord`）：按它的行号找骨架里那一行的字节边界，
 * 取 `[o, o + n)`、不带声明，认 `id` 那一条。骨架还没接上 / 骨架里没有这一行 / 不知道行号 ⇒ 抛（说清为什么），下次展开再取。
 * 主窗口 tab 与查看器同一处。
 */
export async function readRecordById(
  origin: Origin,
  jsonlPath: string,
  places: RowPlaces | null,
  { id, seq }: RecordAt,
): Promise<LineRecord | null> {
  const at = seq === undefined ? undefined : places?.factsOf(seq);
  if (seq === undefined || !at) throw new Error(copyText("recordReads.byId.notIndexed"));
  const rows = await readRangeAs(origin, jsonlPath, at.o, at.o + at.n, seq, null);
  return rows.find((p) => p.record.id === id)?.record ?? null;
}

/** **按行号取一段** `[from, until)`（`until` 缺 ＝ 到末尾，一段 ≤ 1 MiB）。`leftMs` ＝ 那一件事还剩多少。交折起那一行的声明。 */
export async function readLines(
  origin: Origin,
  jsonlPath: string,
  from: number,
  until: number | undefined,
  leftMs: number,
): Promise<SessionLinesPage> {
  const budget = budgetWithin(leftMs);
  const req = until === undefined ? { path: jsonlPath, from } : { path: jsonlPath, from, until };
  return linesOf(origin, await asked(callTyped(origin, "history-lines", req, budget, FOLDED)));
}

/** 一个子运行要读哪一个：子运行本身（运行表里那一格）‖ 派出它的那次工具调用（后端在父记录里找派出链接）。 */
export type RunWhich = { run: string; tool?: string } | { run?: string; tool: string };

/** **一个子运行的记录，一页**（按运行读，通用命令 `history-run`）：父记录 ＋ 哪一个 ＋ 从哪个字节起。 */
export async function loadRunPage(origin: Origin, parentJsonlPath: string, which: RunWhich, from = 0): Promise<RunPage> {
  const budget = budgetWithin(PIECE_BUDGET_MS);
  return await asked(callTyped(origin, "history-run", { parent: parentJsonlPath, ...which, from }, budget));
}

/** **一份记录的主线外清单**（冷读一次；之后的变化由实时帧 `branch` 说）：那台按页给，照 `next` 接着问、拼成一份。 */
export async function readBranch(origin: Origin, jsonlPath: string): Promise<BranchOff> {
  const off: string[] = [];
  let at: number | null = 0;
  let end = 0;
  while (at !== null) {
    const page: Branch = await asked(callTyped(origin, "history-branch", { path: jsonlPath, from: at }, budgetWithin(PIECE_BUDGET_MS)));
    off.push(...page.off);
    end = page.end;
    at = page.next ?? null;
  }
  return { off, end, next: null };
}

/** **那台后端的漂移账**（看不懂的记录类型）。 */
export async function readRecordDrift(origin: Origin): Promise<RecordDriftFace[]> {
  const budget = budgetWithin(DRIFT_BUDGET_MS);
  const { faces } = await asked(callTyped(origin, "drift-report", {}, budget));
  return faces;
}
