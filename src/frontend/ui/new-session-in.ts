/**
 * **在某个目录新建一个会话**（恢复菜单最底下「在此目录新建会话」：历史页与标签页同一条）。
 * 本机：本机后端出计划、开终端，等它出现再报；远端：那台起。账号跟随（那台判）。失败 ⇒ 一条 toast。
 */
import { toast } from "./kit/toast";
import { copyText } from "./copy-table";
import { LOCAL_ORIGIN } from "./ipc/origin";
import { runNewSessionRemote } from "./remote-launch-run";
import { resolveResumeCommand } from "./remote-config";
import { getBehavior } from "./behavior";
import { FOLLOW } from "./launch-account";
import { launchLocal } from "./launch-render";
import { arrivedBody, expectArrival } from "./launch-arrival";

/** `origin` 缺 = 本机。 */
export async function newSessionIn(origin: string | undefined, dir: string, agent: string): Promise<void> {
  const behavior = await getBehavior();
  try {
    if (origin) {
      await runNewSessionRemote(origin, agent, dir, await resolveResumeCommand(origin, behavior.resumeCommandRemote), { account: FOLLOW });
    } else {
      await launchLocal({ action: { kind: "new" }, agent, cwd: dir, launcher: behavior.resumeCommandLocal || null, account: FOLLOW, tmuxName: null }, dir);
      expectArrival({ origin: LOCAL_ORIGIN, match: { cwd: dir }, tmuxName: null, arrived: { title: copyText("history.newSession.started", { dir }), body: arrivedBody(LOCAL_ORIGIN) } });
    }
  } catch (e) {
    toast(copyText("history.newSession.failed", { why: String(e) }), "");
  }
}
