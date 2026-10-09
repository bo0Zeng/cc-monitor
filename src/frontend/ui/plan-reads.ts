/**
 * **计划读面**（pb 工作区 · 片 · 一格）的界面口：`plan-list` · `plan-read` · `plan-cell-view`（计划住哪台就问哪台：`chan.call(origin, …)`）。
 *
 * 判定全在后端（`src/backend/plan/`）：状态、原因、接手对到哪个会话、边与文件倒过来的索引都是成品。
 * 这里只做两件事：按形状收（缺格 / 类型不对 ⇒ 抛；后端多给的格不挡）、把拒绝的码翻成界面要的那几种。
 */
import { chan, ChanError } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, refusalOf } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { PlanArchived } from "./generated/PlanArchived";
import type { PlanBlock } from "./generated/PlanBlock";
import type { PlanCell } from "./generated/PlanCell";
import type { PlanCheck } from "./generated/PlanCheck";
import type { PlanCmdReply } from "./generated/PlanCmdReply";
import type { PlanEdgeWords } from "./generated/PlanEdgeWords";
import type { PlanEdges } from "./generated/PlanEdges";
import type { PlanFile } from "./generated/PlanFile";
import type { PlanKind } from "./generated/PlanKind";
import type { PlanList } from "./generated/PlanList";
import type { PlanListSlice } from "./generated/PlanListSlice";
import type { PlanListWorkspace } from "./generated/PlanListWorkspace";
import type { PlanNeed } from "./generated/PlanNeed";
import type { PlanPb } from "./generated/PlanPb";
import type { PlanPhase } from "./generated/PlanPhase";
import type { PlanProgress } from "./generated/PlanProgress";
import type { PlanRead } from "./generated/PlanRead";
import type { PlanRed } from "./generated/PlanRed";
import type { PlanRef } from "./generated/PlanRef";
import type { PlanRefs } from "./generated/PlanRefs";
import type { PlanReturned } from "./generated/PlanReturned";
import type { PlanReturnedChild } from "./generated/PlanReturnedChild";
import type { PlanSessionBlock } from "./generated/PlanSessionBlock";
import type { PlanSign } from "./generated/PlanSign";
import type { PlanSlice } from "./generated/PlanSlice";
import type { PlanStale } from "./generated/PlanStale";
import type { PlanUndecidable } from "./generated/PlanUndecidable";
import type { PlanWho } from "./generated/PlanWho";
import type { PlanWhyCode } from "./generated/PlanWhyCode";

/** 起一次 pb（研究盘 252 格 1.1 秒；后端给 pb 的期限 20 秒）＋ 回程；远端再加一段。 */
const READ_BUDGET_MS = 25_000;

// 线上形状由后端 `plan/wire.rs` 经 ts-rs 生成（不手写）；这里只按它收、再交给画的那几处。
export type {
  PlanArchived,
  PlanBlock,
  PlanCell,
  PlanCheck,
  PlanCmdReply,
  PlanEdgeWords,
  PlanEdges,
  PlanFile,
  PlanKind,
  PlanList,
  PlanListSlice,
  PlanListWorkspace,
  PlanNeed,
  PlanPb,
  PlanPhase,
  PlanProgress,
  PlanRead,
  PlanRed,
  PlanRef,
  PlanRefs,
  PlanReturned,
  PlanReturnedChild,
  PlanSessionBlock,
  PlanSign,
  PlanSlice,
  PlanStale,
  PlanUndecidable,
  PlanWho,
  PlanWhyCode,
};

/** 四种边的名字（`PlanEdges` 的键，界面按这个次序排）。 */
export type PlanEdgeKind = keyof PlanEdges;
export const PLAN_EDGE_KINDS: readonly PlanEdgeKind[] = ["to", "with", "after", "replaces"];

/** 读不成的几种（后端的码）。 */
export type PlanMissCode = "no_pb" | "not_workspace" | "pb_unsupported" | "failed" | "no_view" | "offline" | "backend_old" | "other";

export class PlanMiss extends Error {
  constructor(
    readonly code: PlanMissCode,
    readonly said: string,
  ) {
    super(said);
    this.name = "PlanMiss";
  }
}

// ───────────────────────── 收 ─────────────────────────

function bad(what: string): never {
  throw new Error(`plan reply shape mismatch: ${what}`); // 程序员错误，刻意英文
}

function obj(v: unknown, what: string): Record<string, unknown> {
  return v !== null && typeof v === "object" && !Array.isArray(v) ? (v as Record<string, unknown>) : bad(`${what} is not an object`);
}

function arr(v: unknown, what: string): unknown[] {
  return Array.isArray(v) ? v : bad(`${what} is not an array`);
}

function str(v: unknown, what: string): string {
  return typeof v === "string" ? v : bad(`${what} is not a string`);
}

/** 后端没写这一格（`?:` 那几格）⇒ `undefined`。 */
function strOpt(v: unknown, what: string): string | undefined {
  return v === null || v === undefined ? undefined : str(v, what);
}

function strOrNull(v: unknown, what: string): string | null {
  if (v === null || v === undefined) return null;
  return str(v, what);
}

function bool(v: unknown, what: string): boolean {
  return typeof v === "boolean" ? v : bad(`${what} is not a boolean`);
}

function num(v: unknown, what: string): number {
  return typeof v === "number" ? v : bad(`${what} is not a number`);
}

function strs(v: unknown, what: string): string[] {
  return arr(v, what).map((x, i) => str(x, `${what}[${i}]`));
}

function staleOf(v: unknown, what: string): PlanStale | null {
  if (v === null || v === undefined) return null;
  const o = obj(v, what);
  return { said: strOrNull(o.said, `${what}.said`), raw: strOrNull(o.raw, `${what}.raw`), since: num(o.since, `${what}.since`), sinceText: strOpt(o.sinceText, `${what}.sinceText`) };
}

/** 缺 / `null` ⇒ 0（老后端不给数）。 */
function count(v: unknown, what: string): number {
  return v === null || v === undefined ? 0 : num(v, what);
}

function bySessionOf(v: unknown, what: string): Record<string, PlanSessionBlock> {
  if (v === null || v === undefined) return {};
  const o = obj(v, what);
  const out: Record<string, PlanSessionBlock> = {};
  for (const [sid, e] of Object.entries(o)) {
    const x = obj(e, `${what}.${sid}`);
    const via = str(x.via, `${what}.${sid}.via`);
    if (via !== "session" && via !== "subagent") bad(`${what}.${sid}.via ${via}`);
    out[sid] = {
      slice: str(x.slice, `${what}.${sid}.slice`),
      block: str(x.block, `${what}.${sid}.block`),
      title: strOrNull(x.title, `${what}.${sid}.title`),
      top: bool(x.top, `${what}.${sid}.top`),
      phase: strOrNull(x.phase, `${what}.${sid}.phase`),
      at: strOrNull(x.at, `${what}.${sid}.at`),
      atTitle: strOrNull(x.atTitle, `${what}.${sid}.atTitle`),
      via,
    };
  }
  return out;
}

const RETURNED_STATES = new Set(["returned", "unsure", "landed"]);
const LANDED_BY = new Set(["child", "body"]);

function returnedOf(v: unknown, what: string): PlanReturned | null {
  if (v === null || v === undefined) return null;
  const o = obj(v, what);
  const child = o.child === null || o.child === undefined ? null : obj(o.child, `${what}.child`);
  return {
    at: num(o.at, `${what}.at`),
    atText: strOpt(o.atText, `${what}.atText`),
    to: decodeWho(o.to, `${what}.to`),
    state: oneOf(o.state, RETURNED_STATES, `${what}.state`) as PlanReturned["state"],
    by: oneOf(o.by, LANDED_BY, `${what}.by`) as PlanReturned["by"],
    child: child && { id: str(child.id, `${what}.child.id`), title: strOrNull(child.title, `${what}.child.title`) },
  };
}

function progressOf(v: unknown, what: string): PlanProgress | null {
  if (v === null || v === undefined) return null;
  const o = obj(v, what);
  return { done: num(o.done, `${what}.done`), open: num(o.open, `${what}.open`), dropped: num(o.dropped, `${what}.dropped`) };
}

const WHO_KINDS = new Set(["session", "subagent", "unknown"]);
const ACTIVITIES = new Set(["working", "needs_you", "idle"]);

export function decodeWho(v: unknown, what: string): PlanWho | null {
  if (v === null || v === undefined) return null;
  const o = obj(v, what);
  const kind = str(o.kind, `${what}.kind`);
  if (!WHO_KINDS.has(kind)) bad(`${what}.kind ${kind}`);
  const activity = strOrNull(o.activity, `${what}.activity`);
  if (activity !== null && !ACTIVITIES.has(activity)) bad(`${what}.activity ${activity}`);
  return {
    id: str(o.id, `${what}.id`),
    kind: kind as PlanWho["kind"],
    sid: strOrNull(o.sid, `${what}.sid`),
    alive: bool(o.alive, `${what}.alive`),
    activity: activity as PlanWho["activity"],
    needs: strOrNull(o.needs, `${what}.needs`),
  };
}

function refsOf(v: unknown, what: string): PlanRef[] {
  return arr(v ?? [], what).map((r, i) => {
    const o = obj(r, `${what}[${i}]`);
    return { start: num(o.start, `${what}[${i}].start`), end: num(o.end, `${what}[${i}].end`), ids: strs(o.ids, `${what}[${i}].ids`) };
  });
}

function edgesOf(v: unknown, what: string): Record<PlanEdgeKind, string[]> {
  const o = obj(v, what);
  return { to: strs(o.to, `${what}.to`), with: strs(o.with, `${what}.with`), after: strs(o.after, `${what}.after`), replaces: strs(o.replaces, `${what}.replaces`) };
}

const STATUS_CODES = new Set(["done", "open", "dropped"]);
const NEED_KINDS = new Set(["top", "red", "ended", "ask"]);
const FILE_CODES = new Set(["ok", "missing", "empty", "broken"]);

/** 一个码：缺 / `null` ⇒ `null`；不在闭集里 ⇒ 抛。 */
function oneOf(v: unknown, set: ReadonlySet<string>, what: string): string | null {
  const x = strOrNull(v, what);
  if (x !== null && !set.has(x)) bad(`${what} ${x}`);
  return x;
}

function cellOf(v: unknown, what: string): PlanCell {
  const o = obj(v, what);
  const refs = obj(o.refs, `${what}.refs`);
  return {
    id: str(o.id, `${what}.id`),
    title: strOrNull(o.title, `${what}.title`),
    kind: strOrNull(o.kind, `${what}.kind`),
    body: strOrNull(o.body, `${what}.body`),
    parent: strOrNull(o.parent, `${what}.parent`),
    children: strs(o.children, `${what}.children`),
    edges: edgesOf(o.edges, `${what}.edges`),
    pointedBy: edgesOf(o.pointedBy, `${what}.pointedBy`),
    files: arr(o.files, `${what}.files`).map((f, i) => {
      const x = obj(f, `${what}.files[${i}]`);
      return {
        path: str(x.path, `${what}.files[${i}].path`),
        state: strOrNull(x.state, `${what}.files[${i}].state`),
        stateCode: oneOf(x.stateCode, FILE_CODES, `${what}.files[${i}].stateCode`) as PlanFile["stateCode"],
        note: strOrNull(x.note, `${what}.files[${i}].note`),
        alsoBy: strs(x.alsoBy, `${what}.files[${i}].alsoBy`),
      };
    }),
    status: strOrNull(o.status, `${what}.status`),
    statusCode: oneOf(o.statusCode, STATUS_CODES, `${what}.statusCode`) as PlanCell["statusCode"],
    why: strOrNull(o.why, `${what}.why`),
    signs: arr(o.signs, `${what}.signs`).map((g, i) => {
      const x = obj(g, `${what}.signs[${i}]`);
      return {
        at: strOrNull(x.at, `${what}.signs[${i}].at`),
        atText: strOpt(x.atText, `${what}.signs[${i}].atText`),
        by: decodeWho(x.by, `${what}.signs[${i}].by`),
        reason: strOrNull(x.reason, `${what}.signs[${i}].reason`),
        refs: refsOf(x.refs, `${what}.signs[${i}].refs`),
      };
    }),
    owner: decodeWho(o.owner, `${what}.owner`),
    refs: { title: refsOf(refs.title, `${what}.refs.title`), body: refsOf(refs.body, `${what}.refs.body`) },
    hasView: bool(o.hasView, `${what}.hasView`),
    whyCode: whyCodeOf(o.whyCode, `${what}.whyCode`),
    signer: decodeWho(o.signer, `${what}.signer`),
    returned: returnedOf(o.returned, `${what}.returned`),
  };
}

function whyCodeOf(v: unknown, what: string): PlanWhyCode | null {
  if (v === null || v === undefined) return null;
  const o = obj(v, what);
  const kind = str(o.kind, `${what}.kind`);
  if (kind === "nosign" || kind === "upper") return { kind };
  if (kind === "inside") return { kind, done: num(o.done, `${what}.done`), of: num(o.of, `${what}.of`) };
  return bad(`${what}.kind ${kind}`);
}

function needsOf(v: unknown, what: string): PlanNeed[] {
  return arr(v, what).map((n, i) => {
    const x = obj(n, `${what}[${i}]`);
    return {
      key: str(x.key, `${what}[${i}].key`),
      kind: oneOf(x.kind, NEED_KINDS, `${what}[${i}].kind`) as PlanNeed["kind"],
      cell: strOrNull(x.cell, `${what}[${i}].cell`),
      block: strOrNull(x.block, `${what}[${i}].block`),
      sid: strOrNull(x.sid, `${what}[${i}].sid`),
      red: x.red === null || x.red === undefined ? undefined : num(x.red, `${what}[${i}].red`),
      acked: bool(x.acked, `${what}[${i}].acked`),
    };
  });
}

export function decodeSlice(v: unknown, what: string): PlanSlice {
  const o = obj(v, what);
  const head = {
    name: str(o.name, `${what}.name`),
    domain: strOrNull(o.domain, `${what}.domain`),
    current: bool(o.current, `${what}.current`),
    error: strOrNull(o.error, `${what}.error`),
    stale: staleOf(o.stale, `${what}.stale`),
  };
  const bare = bool(o.bare, `${what}.bare`);
  if (bare && head.error === null) bad(`${what} is bare without an error`);
  const check = obj(o.check, `${what}.check`);
  return {
    ...head,
    kinds: arr(o.kinds, `${what}.kinds`).map((k, i) => {
      const x = obj(k, `${what}.kinds[${i}]`);
      const w = obj(x.edgeWords, `${what}.kinds[${i}].edgeWords`);
      return {
        name: strOrNull(x.name, `${what}.kinds[${i}].name`),
        edgeWords: {
          to: strOrNull(w.to, "edgeWords.to"),
          with: strOrNull(w.with, "edgeWords.with"),
          after: strOrNull(w.after, "edgeWords.after"),
          replaces: strOrNull(w.replaces, "edgeWords.replaces"),
        },
      };
    }),
    phases: arr(o.phases, `${what}.phases`).map((p, i) => {
      const x = obj(p, `${what}.phases[${i}]`);
      return { name: strOrNull(x.name, "phase.name"), does: strOrNull(x.does, "phase.does"), marks: strs(x.marks, "phase.marks") };
    }),
    done: bool(o.done, `${what}.done`),
    top: strs(o.top, `${what}.top`),
    progress: progressOf(o.progress, `${what}.progress`),
    blocks: arr(o.blocks, `${what}.blocks`).map((b, i) => {
      const x = obj(b, `${what}.blocks[${i}]`);
      return {
        id: str(x.id, `${what}.blocks[${i}].id`),
        cells: strs(x.cells, `${what}.blocks[${i}].cells`),
        dir: strOrNull(x.dir, "block.dir"),
        owner: decodeWho(x.owner, `${what}.blocks[${i}].owner`),
        phase: strOrNull(x.phase, "block.phase"),
        at: strOrNull(x.at, "block.at"),
        row: bool(x.row, "block.row"),
      };
    }),
    check: {
      unreadable: arr(check.unreadable, `${what}.check.unreadable`),
      red: arr(check.red, `${what}.check.red`).map((r, i) => {
        const x = obj(r, `${what}.check.red[${i}]`);
        return { rule: strOrNull(x.rule, "red.rule"), what: strOrNull(x.what, "red.what"), block: strOrNull(x.block, "red.block"), fix: strOrNull(x.fix, "red.fix") };
      }),
      undecidable: arr(check.undecidable, `${what}.check.undecidable`).map((r, i) => {
        const x = obj(r, `${what}.check.undecidable[${i}]`);
        return { rule: strOrNull(x.rule, "undecidable.rule"), why: strOrNull(x.why, "undecidable.why") };
      }),
    },
    cells: arr(o.cells, `${what}.cells`).map((c, i) => cellOf(c, `${what}.cells[${i}]`)),
    archived: arr(o.archived, `${what}.archived`).map((a, i) => {
      const x = obj(a, `${what}.archived[${i}]`);
      return { id: str(x.id, "archived.id"), title: strOrNull(x.title, "archived.title"), kind: strOrNull(x.kind, "archived.kind"), replacedBy: strOrNull(x.replacedBy, "archived.replacedBy") };
    }),
    bare,
    needs: needsOf(o.needs, `${what}.needs`),
    needCount: count(o.needCount, `${what}.needCount`),
  };
}

/** `plan-read` 的回包（形状不对 ⇒ 抛）。 */
export function decodePlanRead(v: unknown): PlanRead {
  const o = obj(v, "reply");
  return {
    pb: strOrNull(o.pb, "pb"),
    workspace: str(o.workspace, "workspace"),
    repo: strOrNull(o.repo, "repo"),
    auto: bool(o.auto, "auto"),
    slices: arr(o.slices, "slices").map((s, i) => decodeSlice(s, `slices[${i}]`)),
    rev: str(o.rev, "rev"),
    readAt: num(o.readAt, "readAt"),
    readAtText: strOpt(o.readAtText, "readAtText"),
    stale: staleOf(o.stale, "stale"),
    needCount: count(o.needCount, "needCount"),
    bySession: bySessionOf(o.bySession, "bySession"),
  };
}

const PB_STATES = new Set(["ok", "missing", "unsupported"]);

/** `plan-list` 的回包（形状不对 ⇒ 抛）。 */
export function decodePlanList(v: unknown): PlanList {
  const o = obj(v, "reply");
  const pb = obj(o.pb, "pb");
  const state = str(pb.state, "pb.state");
  if (!PB_STATES.has(state)) bad(`pb.state ${state}`);
  return {
    pb: { state: state as PlanList["pb"]["state"], said: strOrNull(pb.said, "pb.said"), version: strOpt(pb.version, "pb.version") },
    workspaces: arr(o.workspaces, "workspaces").map((w, i) => {
      const x = obj(w, `workspaces[${i}]`);
      return {
        workspace: str(x.workspace, `workspaces[${i}].workspace`),
        repo: strOrNull(x.repo, `workspaces[${i}].repo`),
        auto: bool(x.auto, `workspaces[${i}].auto`),
        rev: str(x.rev, `workspaces[${i}].rev`),
        stale: staleOf(x.stale, `workspaces[${i}].stale`),
        needCount: count(x.needCount, `workspaces[${i}].needCount`),
        bySession: bySessionOf(x.bySession, `workspaces[${i}].bySession`),
        slices: arr(x.slices, `workspaces[${i}].slices`).map((s, j) => {
          const y = obj(s, `workspaces[${i}].slices[${j}]`);
          const at = `workspaces[${i}].slices[${j}]`;
          return {
            name: str(y.name, `${at}.name`),
            domain: strOrNull(y.domain, `${at}.domain`),
            current: bool(y.current, `${at}.current`),
            progress: progressOf(y.progress, `${at}.progress`),
            error: strOrNull(y.error, `${at}.error`),
            stale: staleOf(y.stale, `${at}.stale`),
            needCount: count(y.needCount, `${at}.needCount`),
          };
        }),
      };
    }),
  };
}

// ───────────────────────── 问 ─────────────────────────

const MISS_CODES = new Set<PlanMissCode>(["no_pb", "not_workspace", "pb_unsupported", "failed", "no_view"]);

/** 拒绝 ⇒ `PlanMiss`（码 ＋ 后端那一句）；连不上 ⇒ `offline`。每一问各写一处 `chan.call`，操作名是字面量（通信层判据按字面量认是哪条帧命令）。 */
async function settle(call: Promise<Uint8Array>): Promise<unknown> {
  try {
    return readJson(await call);
  } catch (e) {
    if (e instanceof ChanError) {
      const err = e.error;
      if (err.layer === "peer" && err.why === "refused") {
        const r = refusalOf(err.body);
        throw new PlanMiss(r && MISS_CODES.has(r.code as PlanMissCode) ? (r.code as PlanMissCode) : "other", r?.message ?? e.message);
      }
      if (err.layer === "peer") throw new PlanMiss("backend_old", e.message);
      throw new PlanMiss(err.layer === "hop" ? "offline" : "other", e.message);
    }
    throw e;
  }
}

/** 这台的工作区与片（`fresh` ＝ 认过的目录也重问 pb）。 */
export async function fetchPlanList(origin: Origin, fresh: boolean, dirs: readonly string[] = []): Promise<PlanList> {
  const body = jsonBody(dirs.length > 0 ? { fresh, dirs } : { fresh });
  const budget = budgetWithin(READ_BUDGET_MS);
  return decodePlanList(await settle(chan.call(origin, "plan-list", body, budget)));
}

/** 一个工作区的成品。 */
export async function fetchPlanRead(origin: Origin, workspace: string): Promise<PlanRead> {
  const body = jsonBody({ workspace });
  const budget = budgetWithin(READ_BUDGET_MS);
  return decodePlanRead(await settle(chan.call(origin, "plan-read", body, budget)));
}

/** 一格的 agent 视角（pb 原样那一段）。 */
export async function fetchCellView(origin: Origin, workspace: string, slice: string, id: string): Promise<string> {
  const body = jsonBody({ workspace, slice, id });
  const budget = budgetWithin(READ_BUDGET_MS);
  const o = obj(await settle(chan.call(origin, "plan-cell-view", body, budget)), "reply");
  return str(o.view, "view");
}

/** 以人的身份代敲的三条用户命令。 */
export type PlanCmd = "continue" | "pause" | "view";

/** 代敲一条用户命令（pb 拒 ⇒ 后端回拒绝，`PlanMiss` 带 pb 那一句）。 */
export async function planCommand(origin: Origin, workspace: string, cmd: PlanCmd): Promise<PlanCmdReply> {
  const body = jsonBody({ workspace, cmd });
  const budget = budgetWithin(READ_BUDGET_MS);
  const o = obj(await settle(chan.call(origin, "plan-command", body, budget)), "reply");
  return { rc: num(o.rc, "rc"), said: strOrNull(o.said, "said"), path: strOrNull(o.path, "path") };
}
