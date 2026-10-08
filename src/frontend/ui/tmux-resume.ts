/**
 * **在 tmux 里恢复一个会话、起好了开一个终端接进去** —— 不依赖标签页对象（标签页「恢复 ▸ tmux」与历史页「恢复 ▾ 在 tmux 里」同一条）。
 *
 * 起法与批量「在 tmux 里后台起」同一条（`sessions-start`：那台判号 · 判在不在跑 · 空 tmux 就地键入 · 铸名交一行 ccm，建完不接进去），
 * 单个只多一步：起好之后（或本来就在跑）开一个终端接进去。能不能起、起到哪个 tmux 会话都是那台判的，这里只照它答的说。
 * 要的号选不了 ⇒ 不起、给显式选择（点了 ⇒ `again(那个号)`）。
 */
import { toast } from "./kit/toast";
import { copyText } from "./copy-table";
import { isLocalOrigin, type Origin } from "./ipc/origin";
import { refuseUnavailableAccount, type AccountAsk } from "./launch-account";
import { runRemoteAttach } from "./remote-launch-run";
import { saidOfControl } from "./tmux-control";
import { offerResyncRetry } from "./resync";
import { callStart, sayReply, type Reply, type StartItem } from "./tab-batch-run";
import { detailOf } from "./kit/detail";

/** 起哪一个：哪台 · 哪一家 · 哪个会话 · 在哪个目录。 */
export interface TmuxResumeTarget {
  origin: Origin;
  agent: string;
  sid: string;
  cwd: string;
}

export interface TmuxResumeHooks {
  /** 再起一次（选不了的号换了一个 / 对齐之后重试）。 */
  again(account?: AccountAsk): Promise<void>;
  /** 那台说了记录在不在（标签页据此画「记录已不在」；历史页不用）。 */
  onRecord?(present: boolean): void;
}

/** 起，起好了接进去。回 `false` = 没起（已经出过声）。 */
export async function startInTmuxThenAttach(t: TmuxResumeTarget, account: AccountAsk, hooks: TmuxResumeHooks): Promise<void | false> {
  const { origin } = t;
  const item: StartItem = { sid: t.sid, cwd: t.cwd, account };
  let r: Reply;
  try {
    [r] = await callStart(origin, "tmux", [item], t.agent);
  } catch (e) {
    toast(copyText("remoteLaunchRun.inPlace.notRun"), saidOfControl(e), { detail: detailOf(e) });
    return false;
  }
  if (r.unavailable) {
    refuseUnavailableAccount({ machine: origin, u: r.unavailable, choose: (a) => hooks.again(a) });
    return false;
  }
  if (r.why === "record_gone") {
    hooks.onRecord?.(false);
    toast(
      copyText("sessionState.recordGone.title"),
      copyText("sessionState.recordGone.body", {
        who: isLocalOrigin(origin) ? copyText("tabSessionActions.who.local") : copyText("tabSessionActions.who.remote", { machine: origin }),
        root: r.detail,
        sid: t.sid,
      }),
    );
    return false;
  }
  if (r.outcome === "failed" || r.session === null) {
    const said = sayReply(origin, "start", r);
    if (r.why === "wrong_owner") offerResyncRetry(origin, t.sid, copyText("remoteLaunchRun.inPlace.notRun"), said, () => hooks.again());
    else toast(copyText("remoteLaunchRun.inPlace.notRun"), said);
    return false;
  }
  if (r.outcome === "done") hooks.onRecord?.(true);
  // 在跑的不止一个 ⇒ 接第一个（接回可撤销），但说出来。
  if (r.why === "ambiguous") {
    toast(
      copyText("tabSessionActions.dupes.title"),
      copyText("tabSessionActions.dupes.body", { n: r.detail.split(", ").length, name: r.session }),
      { level: "info" },
    );
  }
  await runRemoteAttach(origin, t.agent, r.session);
}
