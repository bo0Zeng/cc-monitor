/**
 * 一台机器的状态点与问题行的语气：后端给的那一档（`MachineDot` · 条上那一行的 `Tone`，判定与字表在 `copy_core::machine`）
 * ⇒ 样子名，一档对一样，这里不另判。
 * 设置 › 机器卡、命令面板的「机器」条目与本机后端没起来那一格共用这两张表。
 */
import type { MachineDot } from "./generated/MachineDot";
import type { Tone } from "./generated/Tone";
import type { MachineDotState } from "./kit/status-dot";

const DOT: Record<MachineDot, MachineDotState> = {
  up: "up",
  pending: "unknown",
  warn: "needs-you",
  failed: "failed",
  off: "exited",
};

/** 那一档画哪种点。 */
export function machineDot(dot: MachineDot): MachineDotState {
  return DOT[dot];
}

/** 问题行的样子：出错 · 待办 · 正在做。 */
export type ProblemTone = "error" | "warn" | "busy";

const TONE: Record<Tone, ProblemTone> = { plain: "warn", now: "busy", busy: "busy", need: "warn", warn: "warn", offline: "error", fail: "error" };

/** 那一种语气画成问题行的哪种样子。 */
export function problemTone(tone: Tone): ProblemTone {
  return TONE[tone];
}
