/**
 * 开一个子运行自己的窗口（agent 窗口）：面板那一行 · 派出卡的卡头 · agent 窗口里的小片与路径都经这里。
 * 一个子运行至多一个窗口（窗口名按「会话 ＋ 运行」定，已开着就拉到前面）；错开叠放由壳按「从哪扇窗开的」定。
 */
import { commands } from "./ipc/commands";
import type { Origin } from "./ipc/origin";
import type { RunInfo } from "./generated/RunInfo";
import { agentWindowTitle } from "./agent-window-text";

export interface OpenAgentWindow {
  origin: Origin;
  sid: string;
  run: RunInfo;
  /** 会话叫什么（系统标题那一段）。 */
  session: string;
  /** 远端的机器名；本机 ⇒ `null`。 */
  machine: string | null;
}

export async function openAgentWindow(o: OpenAgentWindow): Promise<void> {
  await commands.open_session_in_new_window({
    sessionId: o.sid,
    origin: o.origin,
    title: agentWindowTitle(o.run, o.session, o.machine),
    run: o.run.run,
  });
}
