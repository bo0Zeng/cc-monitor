/**
 * **会话正文经通道直接问那台机器的后端、按形状收成品**：
 * 查看器整份读 · 骨架按偏移取一段（`history-page`）· 按行号取一段（`history-lines`）· 一个子运行的记录（`history-run`，按运行读）·
 * 主线外清单（`history-branch`，冷读；实时那一路是会话流的 `branch` 格）· 那台后端的漂移账（`drift-report`）。
 *
 * # 它顶掉了什么
 *
 * 此前是 monitor 的四条 Tauri 命令（`stream_read_session_jsonl` · `read_session_range` · `read_session_lines` ·
 * 按目录读子 agent 的那一条）：monitor 从那台后端取原文、自己解析记录（`messages.rs` / `parser.rs` / `codex_record.rs`）、
 * 编号、组载荷。记录解释搬进后端之后（`src/backend/agents/claudecode/`），后端的帧应答**就是成品** —— 每一行在渲染模型里
 * 是什么（通用记录 `LineRecord`，ts-rs 从后端导出）、行号、`cwd`、进不进界面，都是后端给的；monitor 那一跳只搬字节。
 * **本机与远端同一条路**（本机那台由 `<local>` 那条长连接答）。
 *
 * # 本文件做的只有三件（都是调用方那一侧的事）
 *
 * 1. **按形状收**：外层键集合恰好是后端出的那一形；记录本身**不解释**（只验「是对象」）。形状不对 ⇒ 抛。
 * 2. **给载荷打上它从哪台来**（`origin`：本机不带、远端是那台的名字 —— 与实时流那一条同一个口径，这是地址不是解释）。
 * 3. **期限与翻页**：一件事一个期限，翻页只把后端交回的续点（`next` / `nextSeq`）原样交回去。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, ReplyUnreadable, saidFrom } from "./ipc/chan-caller";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import type { JsonlLinePayload } from "./generated/JsonlLinePayload";
import type { LineRecord } from "./generated/LineRecord";
import { exactKeys, isObj } from "./ipc/decode";

/** 一个子运行记录里的一条：通用记录 ＋ 它的对账键（撤那个子运行的活卡用）。 */
export interface RunRecordRow {
  record: LineRecord;
  rid?: string;
}

/** 一页子运行记录（后端 `read_face.rs` 的 `history-run`，键名一字不差）。 */
export interface RunPage {
  /** 读的是哪个子运行。 */
  run: string;
  /** 那份记录（不透明，给查看器整份打开用）。 */
  path: string;
  rows: RunRecordRow[];
  /** 读到哪了（下次从这里续）。 */
  end: number;
  /** 这一页没读到头。 */
  more: boolean;
}

/** 按行号取回的那一段（后端 `history-lines`）：`[from, next)` 里进界面的那些（不进界面的照占号、不出现）。 */
export interface SessionLinesPage {
  from: number;
  next: number;
  eof: boolean;
  payloads: JsonlLinePayload[];
}

/** 主线外清单（后端 `history-branch`）：回退掉的那几条记录的 `id`（文件序）· 读到哪个字节（之后的由实时帧接着说）。 */
export interface BranchOff {
  off: string[];
  end: number;
}

/** 那台后端漂移账的一个面（后端 `agents/claudecode/drift.rs::DriftFaceReport`，键名一字不差）。 */
export interface RecordDriftFace {
  face: string;
  consequence: string;
  entries: Array<{ key: string; count: number; first_sample: string | null }>;
  overflowed: boolean;
}

/** 查看器整份读那一件的期限：同它上一个住址（monitor `frame_query::read_budget(256 MiB)` ＝ 60 ＋ 512 秒）。 */
const WHOLE_READ_BUDGET_MS = 572_000;
/** 按偏移取一段 · 子 agent 那一份：同它上一个住址（一次性远端那一趟的天花板，120 秒）。 */
const PIECE_BUDGET_MS = 120_000;
/** 漂移账：一问（30 秒，同会话读面那几问）。 */
const DRIFT_BUDGET_MS = 30_000;

const isNum = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v) && v >= 0;
const isStr = (v: unknown): v is string => typeof v === "string";


/** 应答形状不对：哪条命令只进细目（给人看的那句由 [`answered`] 按码取）。 */
function badShape(op: string): never {
  throw new ReplyUnreadable(`${op} reply shape`);
}

/** 一条记录行（后端 `record_page::record_lines`）⇒ 载荷；`origin` 由问的那一方打上。 */
function payloadOf(op: string, origin: Origin, v: unknown): JsonlLinePayload {
  if (
    !isObj(v) ||
    !exactKeys(v, ["session_id", "path", "seq", "cwd", "record"]) ||
    !isStr(v.session_id) ||
    !isStr(v.path) ||
    !isNum(v.seq) ||
    !(v.cwd === null || isStr(v.cwd)) ||
    !isObj(v.record)
  ) {
    return badShape(op);
  }
  const p: JsonlLinePayload = {
    session_id: v.session_id,
    path: v.path,
    seq: v.seq,
    cwd: v.cwd as string | null,
    record: v.record as unknown as LineRecord,
  };
  if (!isLocalOrigin(origin)) p.origin = origin;
  return p;
}

/** `history-page` 的一页 ⇒ `(载荷, 续点)`。 */
export function decodePage(
  origin: Origin,
  v: unknown,
): { payloads: JsonlLinePayload[]; next: number; nextSeq: number; eof: boolean } {
  if (!isObj(v) || !exactKeys(v, ["lines", "next", "nextSeq", "eof"]) || !Array.isArray(v.lines)) {
    return badShape("history-page");
  }
  if (!isNum(v.next) || !isNum(v.nextSeq) || typeof v.eof !== "boolean") return badShape("history-page");
  return { payloads: v.lines.map((l) => payloadOf("history-page", origin, l)), next: v.next, nextSeq: v.nextSeq, eof: v.eof };
}

/** `history-lines` 的一段 ⇒ [`SessionLinesPage`]。 */
export function decodeLines(origin: Origin, v: unknown): SessionLinesPage {
  if (!isObj(v) || !exactKeys(v, ["from", "next", "eof", "lines"]) || !Array.isArray(v.lines)) {
    return badShape("history-lines");
  }
  if (!isNum(v.from) || !isNum(v.next) || typeof v.eof !== "boolean") return badShape("history-lines");
  return { from: v.from, next: v.next, eof: v.eof, payloads: v.lines.map((l) => payloadOf("history-lines", origin, l)) };
}

/** `history-run` 的成品 ⇒ [`RunPage`]（记录本身不解释，只验是对象）。 */
export function decodeRun(v: unknown): RunPage {
  if (
    !isObj(v) ||
    !exactKeys(v, ["run", "path", "rows", "end", "more"]) ||
    !isStr(v.run) ||
    !isStr(v.path) ||
    typeof v.end !== "number" ||
    typeof v.more !== "boolean" ||
    !Array.isArray(v.rows)
  ) {
    return badShape("history-run");
  }
  const rows = v.rows.map((r): RunRecordRow => {
    if (!isObj(r) || !isObj(r.record) || !(r.rid === undefined || isStr(r.rid))) return badShape("history-run");
    return r.rid === undefined
      ? { record: r.record as unknown as LineRecord }
      : { record: r.record as unknown as LineRecord, rid: r.rid };
  });
  return { run: v.run, path: v.path, rows, end: v.end, more: v.more };
}

/** `history-branch` 的成品 ⇒ [`BranchOff`]。 */
export function decodeBranch(v: unknown): BranchOff {
  if (!isObj(v) || !exactKeys(v, ["off", "end"]) || !Array.isArray(v.off) || !isNum(v.end)) return badShape("history-branch");
  if (!v.off.every(isStr)) return badShape("history-branch");
  return { off: v.off, end: v.end };
}

/** `drift-report` 的成品 ⇒ 各面。 */
export function decodeDrift(v: unknown): RecordDriftFace[] {
  if (!isObj(v) || !exactKeys(v, ["faces"]) || !Array.isArray(v.faces)) return badShape("drift-report");
  return v.faces.map((f): RecordDriftFace => {
    if (
      !isObj(f) ||
      !exactKeys(f, ["face", "consequence", "entries", "overflowed"]) ||
      !isStr(f.face) ||
      !isStr(f.consequence) ||
      typeof f.overflowed !== "boolean" ||
      !Array.isArray(f.entries)
    ) {
      return badShape("drift-report");
    }
    const entries = f.entries.map((e) => {
      if (
        !isObj(e) ||
        !exactKeys(e, ["key", "count", "first_sample"]) ||
        !isStr(e.key) ||
        !isNum(e.count) ||
        !(e.first_sample === null || isStr(e.first_sample))
      ) {
        return badShape("drift-report");
      }
      return { key: e.key, count: e.count, first_sample: e.first_sample as string | null };
    });
    return { face: f.face, consequence: f.consequence, entries, overflowed: f.overflowed };
  });
}

/** 一问的结局 ⇒ 按形状收好的成品；失败 ⇒ 抛一个给人看的 `Error`（`ipc/chan-caller.ts::saidFrom`）。 */
async function answered<T>(origin: Origin, reply: Promise<Uint8Array>, decode: (v: unknown) => T): Promise<T> {
  try {
    return decode(readJson(await reply));
  } catch (e) {
    throw new Error(saidFrom(e, origin));
  }
}

/**
 * **查看器读一整份会话**：从头按页取，每页交 `onChunk`（边读边发，首屏不等整份）；`cancelled()` 为真就停。
 * 回交出去的条数。读过 256 MiB 那一件由后端明拒（那句话说读到了哪），这里原样抛出。
 */
export async function readWholeSession(
  origin: Origin,
  jsonlPath: string,
  onChunk: (payloads: JsonlLinePayload[]) => void,
  cancelled: () => boolean,
): Promise<number> {
  const budget = budgetWithin(WHOLE_READ_BUDGET_MS);
  let offset = 0;
  let seq = 0;
  let total = 0;
  for (;;) {
    const body = jsonBody({ path: jsonlPath, offset, seq, whole: true });
    const page = await answered(origin, chan.call(origin, "history-page", body, budget), (v) => decodePage(origin, v));
    if (cancelled()) return total;
    if (page.payloads.length > 0) onChunk(page.payloads);
    total += page.payloads.length;
    if (page.eof || page.next <= offset) return total;
    offset = page.next;
    seq = page.nextSeq;
  }
}

/** **按偏移取一段** `[offset, until)`（两端、`seqBase` 取自骨架索引）：一段可能跨页，取到这一段的末尾。 */
export async function readRange(
  origin: Origin,
  jsonlPath: string,
  offset: number,
  until: number,
  seqBase: number,
): Promise<JsonlLinePayload[]> {
  const budget = budgetWithin(PIECE_BUDGET_MS);
  const out: JsonlLinePayload[] = [];
  let at = offset;
  let seq = seqBase;
  for (;;) {
    const body = jsonBody({ path: jsonlPath, offset: at, until, seq });
    const page = await answered(origin, chan.call(origin, "history-page", body, budget), (v) => decodePage(origin, v));
    out.push(...page.payloads);
    if (page.eof || page.next <= at) return out;
    at = page.next;
    seq = page.nextSeq;
  }
}

/** **按行号取一段** `[from, until)`（`until` 缺 ＝ 到末尾，一段 ≤ 1 MiB）。`leftMs` ＝ 那一件事还剩多少。 */
export async function readLines(
  origin: Origin,
  jsonlPath: string,
  from: number,
  until: number | undefined,
  leftMs: number,
): Promise<SessionLinesPage> {
  const args: Record<string, unknown> = { path: jsonlPath, from };
  if (until !== undefined) args.until = until;
  const body = jsonBody(args);
  const budget = budgetWithin(leftMs);
  return await answered(origin, chan.call(origin, "history-lines", body, budget), (v) => decodeLines(origin, v));
}

/** 一个子运行要读哪一个：子运行本身（运行表里那一格）‖ 派出它的那次工具调用（后端在父记录里找派出链接）。 */
export type RunWhich = { run: string; tool?: string } | { run?: string; tool: string };

/** **一个子运行的记录，一页**（按运行读，通用命令 `history-run`）：父记录 ＋ 哪一个 ＋ 从哪个字节起。 */
export async function loadRunPage(origin: Origin, parentJsonlPath: string, which: RunWhich, from = 0): Promise<RunPage> {
  const body = jsonBody({ parent: parentJsonlPath, ...which, from });
  const budget = budgetWithin(PIECE_BUDGET_MS);
  return await answered(origin, chan.call(origin, "history-run", body, budget), decodeRun);
}

/** **一份记录的主线外清单**（冷读一次；之后的变化由实时帧 `branch` 说）。 */
export async function readBranch(origin: Origin, jsonlPath: string): Promise<BranchOff> {
  const body = jsonBody({ path: jsonlPath });
  const budget = budgetWithin(PIECE_BUDGET_MS);
  return await answered(origin, chan.call(origin, "history-branch", body, budget), decodeBranch);
}

/** **那台后端的漂移账**（看不懂的记录类型）。 */
export async function readRecordDrift(origin: Origin): Promise<RecordDriftFace[]> {
  const body = jsonBody({});
  const budget = budgetWithin(DRIFT_BUDGET_MS);
  return await answered(origin, chan.call(origin, "drift-report", body, budget), decodeDrift);
}
