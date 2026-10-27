/**
 * **运行表**（界面这一半）：一个会话 ＝ 主运行 ＋ 子运行。成品由那台后端给（`session_runs` ⇒ 会话流里的 `runs` 格）：
 * 标签 · 状态 · 最近一件事 · 派出它的那次工具调用，连同这几样写好的字（`status_text` · `label_text` · `last_text`）。
 * 这里只排版，不认任何一家的目录、字段、工具名，**也不判状态、不按状态挑字**（只读后端那一份）。
 *
 * - 子运行不进主 tab 的消息流：派出它的那张工具卡标状态（`cards/subagent.ts::markRunCard`），列表住 agent 面板（`agents-panel.ts`）。
 * - 面板列哪些（[`panelGroups`]）：在跑的全列 ＋ 最近结束的 [`RECENT_ENDED`] 个；其余（含状态不明的）收进「更早的」。
 * - 「最近」优先用它此刻在生成的那一块（流里的归一事件），没有就用记录给的最近一件事。
 */
import { copyText } from "./copy-table";
import { icon, type IconName } from "./kit/icon";
import { statusDot } from "./kit/status-dot";
import type { RunInfo } from "./generated/RunInfo";
import type { RunState } from "./generated/RunState";
import type { BlockKind } from "./generated/BlockKind";

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

/** 状态字：运行表那一项（或被挤出的那一项）后端写好的，照抄。 */
export function runStateText(r: Pick<RunInfo, "status_text">): string {
  return r.status_text;
}

const RUN_ICON: Record<Exclude<RunState, "running">, IconName> = { done: "check", failed: "failed", stopped: "stop", unknown: "question" };

/**
 * 状态字旁边那一枚：在跑 ＝ 呼吸的状态点，其余四态 ＝ Phosphor 图标。
 * 旁边总跟着状态字，所以这一枚读屏器不念。
 */
export function runStateMark(state: RunState): Element {
  if (state === "running") {
    const d = statusDot("running", null, "compact");
    d.setAttribute("aria-hidden", "true");
    d.removeAttribute("title");
    return d;
  }
  return icon(RUN_ICON[state], "compact");
}

/** 名字：后端写好的（没名字的那一份通称也是它写）。 */
export function runLabel(r: RunInfo): string {
  return r.label_text;
}

/** 「最近：…」那一截：在生成的那一块优先（流上的活卡，界面自己的态），其次记录给的最近一件事（后端写好）；都没有 ⇒ `null`。 */
export function runLastText(r: RunInfo, live: LiveBlockView | null): string | null {
  if (live) {
    if (live.kind === "tool") return copyText("liveCard.block.toolUse", { tool: live.tool ?? "?" });
    if (live.kind === "thinking") return copyText("runs.last.think");
    if (live.kind === "text") return copyText("runs.last.say");
  }
  return r.last_text ?? null;
}
