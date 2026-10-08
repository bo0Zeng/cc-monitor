/**
 * **运行表**（界面这一半）：一个会话 ＝ 主运行 ＋ 子运行。成品由那台后端给（`session_runs` ⇒ 会话流里的 `runs` 格）：
 * 标签 · 状态 · 最近一件事 · 派出它的那次工具调用。这里只排版，不认任何一家的目录、字段、工具名，**也不判状态**（只读后端那一份）。
 *
 * - 子运行不进主 tab 的消息流：派出它的那张工具卡标状态（`cards/subagent.ts::markRunCard`），列表住 agent 面板（`agents-panel.ts`）。
 * - 面板列哪些（[`panelGroups`]）：在跑的全列 ＋ 最近结束的 [`RECENT_ENDED`] 个；其余（含状态不明的）收进「更早的」。
 * - 「最近」优先用它此刻在生成的那一块（流里的归一事件），没有就用记录给的最近一件事。
 */
import { copyText } from "./copy-table";
import type { RunInfo } from "./generated/RunInfo";
import type { RunState } from "./generated/RunState";
import type { BlockKind } from "./generated/BlockKind";
import type { RunEnded } from "./generated/RunEnded";
import type { RunDid } from "./generated/RunDid";
import type { RunWhy } from "./generated/RunWhy";
import type { SessionRunsPayload } from "./generated/SessionRunsPayload";
import { exactKeys, isObj, optionalKeys } from "./ipc/decode";

/** 一个子运行此刻在生成的那一块（活卡状态机给的，只取「最近：…」要用的两格）。 */
export interface LiveBlockView {
  kind: BlockKind;
  tool?: string;
}

/** 面板默认列出的「最近结束的」那一组至多几个（在跑的不限：同一时刻在跑的本来就有限）。 */
export const RECENT_ENDED = 5;

/** 面板里的三组（每组最近动过的在前）。 */
export interface PanelGroups {
  running: RunInfo[];
  recent: RunInfo[];
  older: RunInfo[];
}

/** 运行表（后端按最近一次动静排好，最早的在前）⇒ 面板的三组：在跑的全列 · 最近结束的 [`RECENT_ENDED`] 个 · 其余与状态不明的。 */
export function panelGroups(runs: readonly RunInfo[]): PanelGroups {
  const newest = [...runs].reverse();
  const ended = newest.filter((r) => r.state === "done" || r.state === "failed" || r.state === "stopped");
  const recent = ended.slice(0, RECENT_ENDED);
  return {
    running: newest.filter((r) => r.state === "running"),
    recent,
    older: newest.filter((r) => r.state === "unknown" || (r.state !== "running" && !recent.includes(r))),
  };
}

/** 每个会话的运行表（最新一份）。 */
export class RunBoard {
  private readonly bySid = new Map<string, RunInfo[]>();

  /** 换成新的一份。回：这一次从「在跑」变成收场的那几个。 */
  set(sid: string, runs: RunInfo[]): RunInfo[] {
    const before = new Map((this.bySid.get(sid) ?? []).map((r) => [r.run, r.state]));
    this.bySid.set(sid, runs);
    return runs.filter((r) => r.state !== "running" && before.get(r.run) === "running");
  }

  of(sid: string): RunInfo[] {
    return this.bySid.get(sid) ?? [];
  }

  running(sid: string): RunInfo[] {
    return this.of(sid).filter((r) => r.state === "running");
  }

  /** 派出它的那次工具调用 ⇒ 这个子运行（还没对上 ⇒ `undefined`）。 */
  byTool(sid: string, tool: string): RunInfo | undefined {
    return this.of(sid).find((r) => r.tool === tool);
  }

  drop(sid: string): void {
    this.bySid.delete(sid);
  }
}

export function runStateText(state: RunState): string {
  switch (state) {
    case "running":
      return copyText("runs.state.running");
    case "done":
      return copyText("runs.state.done");
    case "failed":
      return copyText("runs.state.failed");
    case "stopped":
      return copyText("runs.state.stopped");
    case "unknown":
      return copyText("runs.state.unknown");
  }
}

export function runStateIcon(state: RunState): string {
  switch (state) {
    case "running":
      return copyText("runs.icon.running");
    case "done":
      return copyText("runs.icon.done");
    case "failed":
      return copyText("runs.icon.failed");
    case "stopped":
      return copyText("runs.icon.stopped");
    case "unknown":
      return copyText("runs.icon.unknown");
  }
}

export function runLabel(r: RunInfo): string {
  return r.label && r.label.length > 0 ? r.label : copyText("runs.label.unnamed");
}

/** 「最近：…」那一截：在生成的那一块优先，其次记录给的最近一件事；都没有 ⇒ `null`。 */
export function runLastText(r: RunInfo, live: LiveBlockView | null): string | null {
  if (live) {
    if (live.kind === "tool") return copyText("liveCard.block.toolUse", { tool: live.tool ?? "?" });
    if (live.kind === "thinking") return copyText("runs.last.think");
    if (live.kind === "text") return copyText("runs.last.say");
  }
  const last = r.last;
  if (!last) return null;
  if (last.t === "tool") return copyText("runs.last.tool", { tool: last.name });
  if (last.t === "think") return copyText("runs.last.think");
  return copyText("runs.last.say");
}

const STATES: readonly RunState[] = ["running", "done", "failed", "stopped", "unknown"];
const WHYS: readonly RunWhy[] = ["reported", "own", "quiet", "orphaned"];
const RUN_REQUIRED = ["run", "state"] as const;
const RUN_TEXT = ["label", "kind", "tool", "parent", "waiting", "error", "started_text"] as const;
const RUN_TIMES = ["started_ms", "active_ms", "ended_ms"] as const;

const isStr = (v: unknown): v is string => typeof v === "string";
const isMs = (v: unknown): v is number => typeof v === "number" && Number.isSafeInteger(v) && v >= 0;
const isState = (v: unknown): v is RunState => STATES.includes(v as RunState);

function didOf(v: unknown): RunDid | null {
  if (!isObj(v)) return null;
  if ((v.t === "say" || v.t === "think") && exactKeys(v, ["t"])) return { t: v.t };
  if (v.t === "tool" && exactKeys(v, ["t", "name"]) && isStr(v.name)) return { t: "tool", name: v.name };
  return null;
}

function runOf(v: unknown): RunInfo | null {
  if (!isObj(v) || !isStr(v.run) || !isState(v.state)) return null;
  const out: RunInfo = { run: v.run, state: v.state };
  if (!optionalKeys(v, RUN_REQUIRED, [...RUN_TEXT, ...RUN_TIMES, "last", "why", "calls", "background"])) return null;
  for (const k of RUN_TEXT) {
    if (v[k] === undefined) continue;
    const x = v[k];
    if (!isStr(x)) return null;
    out[k] = x;
  }
  for (const k of RUN_TIMES) {
    if (v[k] === undefined) continue;
    const x = v[k];
    if (!isMs(x)) return null;
    out[k] = x;
  }
  if (v.last !== undefined) {
    const d = didOf(v.last);
    if (!d) return null;
    out.last = d;
  }
  if (v.calls !== undefined) {
    if (!isMs(v.calls)) return null;
    out.calls = v.calls;
  }
  if (v.background !== undefined) {
    if (typeof v.background !== "boolean") return null;
    out.background = v.background;
  }
  if (v.why !== undefined) {
    if (!WHYS.includes(v.why as RunWhy)) return null;
    out.why = v.why as RunWhy;
  }
  return out;
}

function endedOf(v: unknown): RunEnded | null {
  if (!isObj(v) || !exactKeys(v, ["run", "state", "tool"]) || !isStr(v.run) || !isStr(v.tool) || !isState(v.state)) return null;
  return { run: v.run, tool: v.tool, state: v.state };
}

/** 会话流里 `runs` 那一格 ⇒ 运行表（按形状严格收：多一格 / 缺一格 / 类型不对 ⇒ `null`，那一格不收）。 */
export function decodeRunsPayload(v: unknown): SessionRunsPayload | null {
  if (!isObj(v) || !exactKeys(v, ["ended", "runs", "session_id"]) || !isStr(v.session_id)) return null;
  if (!Array.isArray(v.runs) || !Array.isArray(v.ended)) return null;
  const runs = v.runs.map(runOf);
  const ended = v.ended.map(endedOf);
  if (runs.some((r) => r === null) || ended.some((e) => e === null)) return null;
  return { session_id: v.session_id, runs: runs as RunInfo[], ended: ended as RunEnded[] };
}
