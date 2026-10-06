/**
 * **恢复历史里的一个会话**（历史页［恢复 ▾］与独立查看窗［恢复 ▾］同一条）：按勾着的那一组起 ——
 * 在 tmux 里 ⇒ 那台在 tmux 里起再接进去（与标签页同一条，`tmux-resume.ts`）；不用 tmux ⇒ 远端那台起 / 本机开终端。
 * 号由那台判（跟随 / 点名 / 不指定；选不了 ⇒ 那台不起、给替代，点了 ⇒ `again`）。起不来 ⇒ 抛（说法归调用方）。
 */
import { LOCAL_ORIGIN } from "./ipc/origin";
import { startInTmuxThenAttach } from "./tmux-resume";
import { resumeLocalSession } from "./local-resume";
import { runRemoteResume } from "./remote-launch-run";
import { resolveResumeCommand } from "./remote-config";
import { configuredLauncherFor } from "./launch-requests";
import { getBehavior } from "./behavior";
import type { AccountAsk } from "./launch-account";
import type { HistoryRow } from "./history-list-reads";

export interface ResumeHow {
  tmux: boolean;
  account: AccountAsk;
}

/** 起；回 `false` = 没起（那台已经出过声：选不了的号 · 记录已不在 …）。 */
export async function resumeHistoryRow(r: HistoryRow, how: ResumeHow, again: (account?: AccountAsk) => Promise<void>): Promise<boolean> {
  const origin = r.origin ?? LOCAL_ORIGIN;
  if (how.tmux) {
    const started = await startInTmuxThenAttach({ origin, agent: r.agent, sid: r.sessionId, cwd: r.projectPath }, how.account, { again });
    return started !== false;
  }
  if (r.origin) {
    const behavior = await getBehavior();
    const launcher = configuredLauncherFor(r.agent, await resolveResumeCommand(r.origin, behavior.resumeCommandRemote));
    await runRemoteResume(r.origin, r.agent, r.sessionId, r.projectPath, launcher, { account: how.account });
  } else {
    await resumeLocalSession({ agent: r.agent, sid: r.sessionId, cwd: r.projectPath, account: how.account });
  }
  return true;
}
