/**
 * **运行表**（界面这一半）：一个会话 ＝ 主运行 ＋ 子运行。成品由那台后端给（`session_runs` ⇒ 会话流里的 `runs` 格）：
 * 标签 · 状态 · 最近一件事 · 派出它的那次工具调用，连同这几样写好的字（`status_text` · `label_text` · `last_text`）。
 * 这里只排版，不认任何一家的目录、字段、工具名，**也不判状态、不按状态挑字**（只读后端那一份）。
 *
 * - 子运行不进主 tab 的消息流：派出它的那张工具卡标状态（`cards/subagent.ts::markRunCard`），列表住 agent 面板（`agents-panel.ts`）。
 * - 面板的三组（在跑 · 最近结束 · 更早）与 chip 的字也是后端写好的（`groups` · `summary`），这里照着摆。
 * - agent 窗口的事实 · 说明 · 结束线后端写好（`facts` · `explain` · `end`）；随时钟变的那一截是会走的钟，经 [`runLineNow`] 按此刻填。
 * - 「最近」优先用它此刻在生成的那一块（流里的归一事件），没有就用记录给的最近一件事。
 */
import { copyText } from "./copy-table";
import { clockNow } from "./duration-format";
import { icon, type IconName } from "./kit/icon";
import { statusDot } from "./kit/status-dot";
import type { RunInfo } from "./generated/RunInfo";
import type { RunLine } from "./generated/RunLine";
import type { RunState } from "./generated/RunState";
import type { SessionRuns } from "./generated/SessionRuns";
import type { BlockKind } from "./generated/BlockKind";

/** 一个子运行此刻在生成的那一块（活卡状态机给的，只取「最近：…」要用的两格）。 */
export interface LiveBlockView {
  kind: BlockKind;
  tool?: string;
}

/** 一个会话的运行表那一份里界面要留的：表 · 三组 · chip 的字。 */
export type RunTable = Pick<SessionRuns, "runs" | "groups" | "summary">;

/** 还没有运行表的会话那一份。 */
export const NO_RUNS: RunTable = { runs: [], groups: { running: [], recent: [], older: [] } };

/** 每个会话的运行表（最新一份）。 */
export class RunBoard {
  private readonly bySid = new Map<string, RunTable>();

  /** 换成新的一份。回：这一次从「在跑」变成收场的那几个。 */
  set(sid: string, t: RunTable): RunInfo[] {
    const before = new Map(this.of(sid).map((r) => [r.run, r.state]));
    this.bySid.set(sid, t);
    return t.runs.filter((r) => r.state !== "running" && before.get(r.run) === "running");
  }

  of(sid: string): RunInfo[] {
    return this.table(sid).runs;
  }

  /** 整份（三组 · chip 的字一起）；还没到 ⇒ 空的那一份。 */
  table(sid: string): RunTable {
    return this.bySid.get(sid) ?? NO_RUNS;
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

/** 写好的一截此刻的字：会走的钟（带 `from`）按此刻填 `{dur}`，整句照抄。 */
export function runLineNow(l: RunLine, now: number): string {
  return l.from === undefined ? l.text : clockNow({ text: l.text, from: l.from }, now);
}

/** 系统标题：`{标签} · {状态} · {会话}`，远端再加 `· {机器}`（标题里不放符号）。 */
export function agentWindowTitle(r: RunInfo, session: string, machine: string | null): string {
  const label = runLabel(r);
  const state = runStateText(r);
  return machine ? copyText("agentWindow.title.remote", { label, state, session, machine }) : copyText("agentWindow.title.local", { label, state, session });
}

/** 它派出的那几个（运行表里 `parent` 是它的，按开始的先后）。 */
export function kidsOf(runs: readonly RunInfo[], run: string): RunInfo[] {
  return runs.filter((r) => r.parent === run).sort((a, b) => (a.started_ms ?? 0) - (b.started_ms ?? 0));
}

/** 从主运行到它的那条派出链（不含它自己；最早的在前）。表里断了 ⇒ 到断处为止。 */
export function chainOf(runs: readonly RunInfo[], r: RunInfo): RunInfo[] {
  const out: RunInfo[] = [];
  const seen = new Set<string>([r.run]);
  let p = r.parent;
  while (p !== undefined && !seen.has(p)) {
    seen.add(p);
    const up = runs.find((x) => x.run === p);
    if (!up) break;
    out.unshift(up);
    p = up.parent;
  }
  return out;
}
