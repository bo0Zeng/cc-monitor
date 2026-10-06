/**
 * 窗口之间的几件事（Tauri 事件，全局广播；只有主窗口接）。
 *
 * - [`SWITCH_TO_SESSION_EVENT`]：独立查看窗里点［切过去］⇒ 主窗口拉到前面、切到那个会话的标签页（查看窗留着）。
 */
export const SWITCH_TO_SESSION_EVENT = "switch-to-session";

export interface SwitchToSession {
  sessionId: string;
}
