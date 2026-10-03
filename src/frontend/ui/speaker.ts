/**
 * 用户角色记录「谁说的」在界面这一侧只管**画不画**：判定在后端（随记录成品带来的 `userText.speaker`），
 * 这里不看正文、不认标记。消息流建卡（`cards/index.ts::renderMessage`）与骨架估高（`height-estimate.ts`）读同一张表。
 */
import type { Speaker } from "./generated/Speaker";

export type SpeakerKind = Speaker["kind"];

/** 建卡的那几种来源；其余（系统注入 · agent 来话 · 后台通知 · 输出回显 · 中断标记）不建卡。 */
const DRAWN: ReadonlySet<string> = new Set<SpeakerKind>([
  "human",
  "slashCommand",
  "bashInput",
  "bashOutput",
  "compactSummary",
  "agentTask",
  "toolResult",
]);

export function drawsCard(kind: string): boolean {
  return DRAWN.has(kind);
}

/** 人在这条里说了话（切 tab 那一道认它）。 */
const SAID_BY_HUMAN: ReadonlySet<string> = new Set<SpeakerKind>(["human", "slashCommand", "bashInput"]);

export function saidByHuman(kind: string): boolean {
  return SAID_BY_HUMAN.has(kind);
}
