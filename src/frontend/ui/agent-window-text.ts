/**
 * agent 窗口的几句字（只排版：状态 · 时刻 · 原因都是后端运行表给的，这里只按它拼）。
 *
 * 时长一律写到分钟（`fmtDur`：`45s` · `6m` · `1h50m`），窗口每分钟重画一次就够（`views/agent-window.ts`）。
 */
import { copyText } from "./copy-table";
import { fmtDur, spanNow } from "./duration-format";
import { runLabel, runStateText } from "./runs";
import type { RunInfo } from "./generated/RunInfo";

/** 系统标题：`{标签} · {状态} · {会话}`，远端再加 `· {机器}`（标题里不放符号）。 */
export function agentWindowTitle(r: RunInfo, session: string, machine: string | null): string {
  const label = runLabel(r);
  const state = runStateText(r.state);
  return machine ? copyText("agentWindow.title.remote", { label, state, session, machine }) : copyText("agentWindow.title.local", { label, state, session });
}

const since = (at: number | undefined, now: number): string | null => (at === undefined ? null : spanNow(at, now));

/** 标题区那一行小字：几点开始 · 已跑 / 用时 · 最近动静 · 调用工具几次 · 派出几个 · 后台派出。 */
export function factsOf(r: RunInfo, kids: number, now: number): string[] {
  const out: string[] = [];
  if (r.started_text !== undefined) out.push(copyText("agentWindow.facts.started", { time: r.started_text }));
  if (r.state === "running" && r.started_ms !== undefined) out.push(copyText("agentWindow.facts.running", { dur: spanNow(r.started_ms, now) }));
  if (r.state !== "running" && r.state !== "unknown" && r.started_ms !== undefined && r.ended_ms !== undefined) {
    out.push(copyText("agentWindow.facts.took", { dur: spanNow(r.started_ms, r.ended_ms) }));
  }
  if (r.state === "running" || r.state === "unknown") {
    const ago = since(r.active_ms, now);
    if (ago !== null) out.push(copyText("agentWindow.facts.active", { dur: ago }));
  }
  if (r.calls !== undefined && r.calls > 0) out.push(copyText("agentWindow.facts.calls", { n: r.calls }));
  if (kids > 0) out.push(copyText("agentWindow.facts.kids", { n: kids }));
  if (r.background) out.push(copyText("agentWindow.facts.background"));
  return out;
}

/** 说明那一块：在跑且没在等 ⇒ 不出（`null`）。 */
export interface WhyView {
  /** 打头那一格（状态不明那一种单独粗体写「状态不明」）。 */
  lead: string | null;
  text: string;
  /** 小一号的补一句。 */
  note: string | null;
  /** 报错原话（失败且说得出）。 */
  error: string | null;
  /** 带［刷新］（状态不明）。 */
  refresh: boolean;
}

export function whyOf(r: RunInfo, now: number): WhyView | null {
  const dur = since(r.active_ms, now) ?? fmtDur(0);
  switch (r.state) {
    case "running":
      return r.waiting === undefined
        ? null
        : { lead: null, text: copyText("agentWindow.why.waiting", { tool: r.waiting, dur }), note: copyText("agentWindow.why.waitingNote"), error: null, refresh: false };
    case "done":
      return { lead: null, text: copyText("agentWindow.why.done"), note: null, error: null, refresh: false };
    case "failed":
      return { lead: null, text: copyText("agentWindow.why.failed"), note: null, error: r.error ?? null, refresh: false };
    case "stopped":
      return { lead: null, text: copyText("agentWindow.why.stopped"), note: null, error: null, refresh: false };
    case "unknown":
      return {
        lead: runStateText("unknown"),
        text: r.why === "orphaned" ? copyText("agentWindow.why.orphaned") : copyText("agentWindow.why.quiet", { dur }),
        note: null,
        error: null,
        refresh: true,
      };
  }
}

/** 尾巴上的结束线（在跑 ⇒ `null`）：状态字（图标由界面在前面画）＋ 一句。 */
export function endOf(r: RunInfo, now: number): { head: string; sub: string } | null {
  if (r.state === "running") return null;
  const head = runStateText(r.state);
  const dur = since(r.active_ms, now) ?? fmtDur(0);
  const sub =
    r.state === "done"
      ? copyText("agentWindow.end.done")
      : r.state === "failed"
        ? copyText("agentWindow.end.failed")
        : r.state === "stopped"
          ? copyText("agentWindow.end.stopped")
          : r.why === "orphaned"
            ? copyText("agentWindow.end.orphaned")
            : copyText("agentWindow.end.quiet", { dur });
  return { head, sub };
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
