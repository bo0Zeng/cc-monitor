/**
 * **运行表**（界面这一半）：一个会话 ＝ 主运行 ＋ 子运行。成品由那台后端给（`session_runs` ⇒ 会话流里的 `runs` 格）：
 * 标签 · 状态 · 最近一件事 · 派出它的那次工具调用。这里只排版，不认任何一家的目录、字段、工具名。
 *
 * - 主 tab 上每个**在跑**的子运行常驻一行「<标签> · 在跑 · 最近：<那件事>」；跑完那一行撤掉，状态收到派出它的那张工具卡上。
 * - 「最近」优先用它此刻在生成的那一块（流里的归一事件），没有就用记录给的最近一件事。
 */
import { copyText } from "./copy-table";
import type { RunInfo } from "./generated/RunInfo";
import type { RunState } from "./generated/RunState";
import type { LiveBlock } from "./live-card";

/** 主 tab 上的一行。 */
export interface RunRow {
  run: string;
  text: string;
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
  if (state === "running") return copyText("runs.state.running");
  if (state === "done") return copyText("runs.state.done");
  return copyText("runs.state.failed");
}

export function runLabel(r: RunInfo): string {
  return r.label && r.label.length > 0 ? r.label : copyText("runs.label.unnamed");
}

/** 「最近：…」那一截：在生成的那一块优先，其次记录给的最近一件事；都没有 ⇒ `null`。 */
export function runLastText(r: RunInfo, live: LiveBlock | null): string | null {
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

/** 主 tab 上那一行的字。 */
export function runRowText(r: RunInfo, live: LiveBlock | null): string {
  const label = runLabel(r);
  const state = runStateText(r.state);
  const last = runLastText(r, live);
  return last === null ? copyText("runs.row.bare", { label, state }) : copyText("runs.row.text", { label, state, last });
}
