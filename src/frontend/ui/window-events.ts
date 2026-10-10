/**
 * 窗口之间的几件事（Tauri 事件，全局广播；谁接写在每一条上）。
 *
 * - [`SWITCH_TO_SESSION_EVENT`]：独立查看窗里点［切过去］⇒ 主窗口拉到前面、切到那个会话的标签页（查看窗留着）。主窗口接。
 * - [`AGENT_WINDOW_EVENT`]：一个子运行的窗口（agent 窗口）开了 / 关了。主窗口接（记「哪些子运行有窗口」）。
 * - [`AGENT_WINDOWS_ASK_EVENT`]：主窗口（重新载入之后）问「哪些 agent 窗口开着」⇒ 每个 agent 窗口再报一次 [`AGENT_WINDOW_EVENT`]。
 * - [`SHOW_RUN_CARD_EVENT`]：「回到派出它的地方」⇒ 派出它的那一方（主窗口 / 父 agent 的窗口）拉到前面、滚到派出它的那张卡、闪一下。
 * - [`PLAN_OPEN_EVENT`]：文件窗口「在计划里看」（壳 `chan/host.rs::plan_open` 发给主窗口，已拉到前面）⇒ 主窗口开计划页、选中那一格。主窗口接。
 */
export const SWITCH_TO_SESSION_EVENT = "switch-to-session";

export interface SwitchToSession {
  sessionId: string;
}

export const AGENT_WINDOW_EVENT = "agent-window";

export interface AgentWindowSaid {
  sessionId: string;
  run: string;
  open: boolean;
}

export const AGENT_WINDOWS_ASK_EVENT = "agent-windows-ask";

export const SHOW_RUN_CARD_EVENT = "show-run-card";

export interface ShowRunCard {
  sessionId: string;
  /** 派出它的那次工具调用（父侧 id）。 */
  tool: string;
  /** 卡在哪：主运行 ⇒ `null`（主窗口接）；某个子运行 ⇒ 它（那个 agent 窗口接）。 */
  in: string | null;
}

/** 名字与壳那一份（`chan/host.rs::PLAN_OPEN_EVENT`）逐字相同（`plan-signs.vitest.ts` 两边抠出来比）。 */
export const PLAN_OPEN_EVENT = "plan-open";

export interface PlanOpenSaid {
  /** 那台机器（文件窗口寻址的那一台）。 */
  origin: string;
  workspace: string;
  slice: string;
  id: string;
}
