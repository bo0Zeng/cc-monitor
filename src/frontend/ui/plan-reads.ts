/**
 * **计划读面**（pb 工作区 · 片 · 一格）的界面口：`plan-list` · `plan-read` · `plan-cell-view`（计划住哪台就问哪台：`chan.call(origin, …)`）。
 *
 * 判定全在后端（`src/backend/plan/`）：状态、原因、接手对到哪个会话、边与文件倒过来的索引都是成品。
 * 这里只做两件事：按形状收（缺格 / 类型不对 ⇒ 抛；后端多给的格不挡）、把拒绝的码翻成界面要的那几种。
 */
import { chan, ChanError } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, refusalOf } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";

/** 起一次 pb（研究盘 252 格 1.1 秒）＋ 回程；远端再加一段。 */
const READ_BUDGET_MS = 20_000;

/** 一个 id（块的接手 · 签收的「由」· 在长它的）对到的会话。 */
export interface PlanWho {
  id: string;
  kind: "session" | "subagent" | "unknown";
  /** 主会话 ⇒ 它自己；子 agent ⇒ 父会话；认不出 ⇒ `null`。 */
  sid: string | null;
  alive: boolean;
  activity: "working" | "needs_you" | "idle" | null;
  needs: string | null;
}

export interface PlanFile {
  path: string;
  /** pb 的四种：在 · 缺 · 空 · 坏。 */
  state: string | null;
  /** 对账的码（后端从 pb 原话翻好）；认不出 ⇒ `null`。 */
  stateCode: "ok" | "missing" | "empty" | "broken" | null;
  note: string | null;
  /** 同一份文件还挂在哪几格（编号）。 */
  alsoBy: string[];
}

export interface PlanSign {
  at: string | null;
  by: PlanWho | null;
  reason: string | null;
  refs: PlanRef[];
}

/** 话里提到的格：`[start, end)` 按字数，`ids` 是那几格（简写已展开）。 */
export interface PlanRef {
  start: number;
  end: number;
  ids: string[];
}

export type PlanEdgeKind = "to" | "with" | "after" | "replaces";
export const PLAN_EDGE_KINDS: readonly PlanEdgeKind[] = ["to", "with", "after", "replaces"];

export interface PlanCell {
  id: string;
  title: string | null;
  kind: string | null;
  body: string | null;
  parent: string | null;
  children: string[];
  edges: Record<PlanEdgeKind, string[]>;
  pointedBy: Record<PlanEdgeKind, string[]>;
  files: PlanFile[];
  /** pb 的三种：做完了 · 没做完 · 不做了。 */
  status: string | null;
  /** 状态的码（后端从 pb 原话翻好）；pb 给了别的字 ⇒ `null`。 */
  statusCode: "done" | "open" | "dropped" | null;
  /** 没做完时的原因（pb 原话）。 */
  why: string | null;
  signs: PlanSign[];
  owner: PlanWho | null;
  refs: { title: PlanRef[]; body: PlanRef[] };
  hasView: boolean;
  /** 没做完时原因的结构化那一份（后端从 pb 的原话拆好）；后端没给 ⇒ `null`，界面照出 `why` 原话。 */
  whyCode: PlanWhyCode | null;
}

/** 没做完的三种原因：没签 · 里面 d/m 做完了 · 等上一级收下。 */
export type PlanWhyCode = { kind: "nosign" } | { kind: "inside"; done: number; of: number } | { kind: "upper" };

/** 要你看的一条（后端判的四种）。 */
export interface PlanNeed {
  key: string;
  kind: string;
  cell: string | null;
  block: string | null;
  acked: boolean;
}

export interface PlanBlock {
  id: string;
  cells: string[];
  dir: string | null;
  owner: PlanWho | null;
  phase: string | null;
  /** 站在哪一格（编号）。 */
  at: string | null;
  row: boolean;
}

export interface PlanKind {
  name: string | null;
  edgeWords: Record<PlanEdgeKind, string | null>;
}

export interface PlanPhase {
  name: string | null;
  does: string | null;
  marks: string[];
}

export interface PlanRed {
  rule: string | null;
  what: string | null;
  block: string | null;
  fix: string | null;
}

export interface PlanProgress {
  done: number;
  open: number;
  dropped: number;
}

/** 一片。读不成且没读好过 ⇒ 只有头几格（`error` 有字，`cells` 等为 `null`）。 */
export interface PlanSlice {
  name: string;
  domain: string | null;
  current: boolean;
  /** 这一刻读不成的那一句（pb 原样）。 */
  error: string | null;
  /** 这一刻读不成、给的是上一次那一份 ⇒ 那一句与那一份读到的时刻。 */
  stale: { said: string | null; since: number } | null;
  kinds: PlanKind[];
  phases: PlanPhase[];
  done: boolean;
  top: string[];
  progress: PlanProgress | null;
  blocks: PlanBlock[];
  check: { red: PlanRed[]; undecidable: { rule: string | null; why: string | null }[] };
  cells: PlanCell[];
  archived: { id: string; title: string | null; kind: string | null; replacedBy: string | null }[];
  /** 读不成又没读好过（只有头几格）。 */
  bare: boolean;
  /** 要你看的几条；后端还不判 ⇒ `null`（过滤「要你看」那一枚不出）。 */
  needs: PlanNeed[] | null;
}

export interface PlanRead {
  pb: string | null;
  workspace: string;
  repo: string | null;
  auto: boolean;
  slices: PlanSlice[];
  rev: string;
  readAt: number;
  /** 整次读不成、给的是上一次那一份。 */
  stale: { said: string | null; since: number } | null;
}

export interface PlanListSlice {
  name: string;
  domain: string | null;
  current: boolean;
  progress: PlanProgress | null;
  error: string | null;
  stale: { said: string | null; since: number } | null;
}

export interface PlanListWorkspace {
  workspace: string;
  repo: string | null;
  auto: boolean;
  rev: string;
  slices: PlanListSlice[];
}

export interface PlanList {
  /** pb 装没装、认不认得它的输出。 */
  pb: { state: "ok" | "missing" | "unsupported"; said: string | null };
  workspaces: PlanListWorkspace[];
}

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

function staleOf(v: unknown, what: string): { said: string | null; since: number } | null {
  if (v === null || v === undefined) return null;
  const o = obj(v, what);
  return { said: strOrNull(o.said, `${what}.said`), since: num(o.since, `${what}.since`) };
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
        by: decodeWho(x.by, `${what}.signs[${i}].by`),
        reason: strOrNull(x.reason, `${what}.signs[${i}].reason`),
        refs: refsOf(x.refs, `${what}.signs[${i}].refs`),
      };
    }),
    owner: decodeWho(o.owner, `${what}.owner`),
    refs: { title: refsOf(refs.title, `${what}.refs.title`), body: refsOf(refs.body, `${what}.refs.body`) },
    hasView: bool(o.hasView, `${what}.hasView`),
    whyCode: whyCodeOf(o.whyCode, `${what}.whyCode`),
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

function needsOf(v: unknown, what: string): PlanNeed[] | null {
  if (v === null || v === undefined) return null;
  return arr(v, what).map((n, i) => {
    const x = obj(n, `${what}[${i}]`);
    return {
      key: str(x.key, `${what}[${i}].key`),
      kind: str(x.kind, `${what}[${i}].kind`),
      cell: strOrNull(x.cell, `${what}[${i}].cell`),
      block: strOrNull(x.block, `${what}[${i}].block`),
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
  if (!("cells" in o)) {
    if (head.error === null) bad(`${what} has neither cells nor error`);
    return { ...head, kinds: [], phases: [], done: false, top: [], progress: null, blocks: [], check: { red: [], undecidable: [] }, cells: [], archived: [], bare: true, needs: null };
  }
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
    bare: false,
    needs: needsOf(o.needs, `${what}.needs`),
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
    stale: staleOf(o.stale, "stale"),
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
    pb: { state: state as PlanList["pb"]["state"], said: strOrNull(pb.said, "pb.said") },
    workspaces: arr(o.workspaces, "workspaces").map((w, i) => {
      const x = obj(w, `workspaces[${i}]`);
      return {
        workspace: str(x.workspace, `workspaces[${i}].workspace`),
        repo: strOrNull(x.repo, `workspaces[${i}].repo`),
        auto: bool(x.auto, `workspaces[${i}].auto`),
        rev: str(x.rev, `workspaces[${i}].rev`),
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
          };
        }),
      };
    }),
  };
}

// ───────────────────────── 问 ─────────────────────────

const MISS_CODES = new Set<PlanMissCode>(["no_pb", "not_workspace", "pb_unsupported", "failed", "no_view"]);

/** 拒绝 ⇒ `PlanMiss`（码 ＋ 后端那一句）；连不上 ⇒ `offline`。 */
async function ask(origin: Origin, cmd: string, args: Record<string, unknown>): Promise<unknown> {
  try {
    const body = await chan.call(origin, cmd, jsonBody(args), budgetWithin(READ_BUDGET_MS));
    return readJson(body);
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
  return decodePlanList(await ask(origin, "plan-list", dirs.length > 0 ? { fresh, dirs } : { fresh }));
}

/** 一个工作区的成品。 */
export async function fetchPlanRead(origin: Origin, workspace: string): Promise<PlanRead> {
  return decodePlanRead(await ask(origin, "plan-read", { workspace }));
}

/** 一格的 agent 视角（pb 原样那一段）。 */
export async function fetchCellView(origin: Origin, workspace: string, slice: string, id: string): Promise<string> {
  const o = obj(await ask(origin, "plan-cell-view", { workspace, slice, id }), "reply");
  return str(o.view, "view");
}

/** 以人的身份代敲的三条用户命令。 */
export type PlanCmd = "continue" | "pause" | "view";

/** `plan-command` 的回包：pb 的退出码 · 那一句（stderr 第一句）· `view` 给的 html 路径。 */
export interface PlanCmdReply {
  rc: number;
  said: string | null;
  path: string | null;
}

/** 代敲一条用户命令（pb 拒 ⇒ 后端回拒绝，`PlanMiss` 带 pb 那一句）。 */
export async function planCommand(origin: Origin, workspace: string, cmd: PlanCmd): Promise<PlanCmdReply> {
  const o = obj(await ask(origin, "plan-command", { workspace, cmd }), "reply");
  return { rc: num(o.rc, "rc"), said: strOrNull(o.said, "said"), path: strOrNull(o.path, "path") };
}
