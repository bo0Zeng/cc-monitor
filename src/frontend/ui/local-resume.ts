/**
 * **本机 resume 一条会话 —— 编排只有这一份。**
 *
 * 入口：tab 栏右键 Resume · 历史页 ↺（账号跟随）· 分叉之后起新会话 · 本机换号重启的 resume 那一跳（用户点名的号）。
 * 用哪个号由本机后端判（`launch-local` 的 `account`）；要的号选不了 ⇒ 不起、说清、给显式选择（点了以点名再起一次）。
 *
 * # 失败怎么说
 *
 * 本函数**自己出声**并回 `false`，调用方据此收尾（换号重启说「旧会话已结束、新会话没起来」；
 * 分叉回 `failed`，不再叠成功提示）。
 */
// 计划与渲染问本机后端（`launch-local`），monitor 只开终端窗口（`open_local_terminal`）。
import { launchLocal } from "./launch-render";
import { LOCAL_ORIGIN } from "./ipc/origin";
import { accountUnavailableOf, refuseUnavailableAccount, type AccountAsk } from "./launch-account";
import { getBehavior } from "./behavior";
import { configuredLauncherFor } from "./launch-requests";
import { mintFreshTmuxName } from "./terminal-name-mint";
import { showActionFailureToast } from "./error-toast";
import { copyText } from "./copy-table";
import { arrivedBody, awaitArrival, expectArrival, type LaunchWait } from "./launch-arrival";

export interface LocalResumeRequest {
  /** 这个会话是哪一家（线上的 kind）。没有账号这一维的那一家不跟随上次的号。 */
  agent: string;
  sid: string;
  cwd: string;
  /** 跟随 / 账号 0 / 用户点名（`launch-account.ts`）。 */
  account: AccountAsk;
  /** 给了就用它（换号重启：复用被 kill 让出来的旧名）；缺席 ⇒ 问本机 tmux 名单现铸一个。 */
  tmuxName?: string;
  /** 设置里的本机 resume 命令（调用方已经读过就给；空串 = 没配 ⇒ 后端用默认）。缺席 ⇒ 现读（只用在默认那一家的会话上）。 */
  launcher?: string;
  /** 拉起失败那句提示的标题；缺席 ⇒ 「恢复失败」。 */
  failureTitle?: string;
  /**
   * 本机后端判完号、开窗之前问一句（tab 栏那条：「记录还在不在」要问**这次要用的那个账号根**）。
   * 收到的是判出来的那个号的目录（`undefined` = 账号 0 / 不指定）；回 `false` ⇒ 不起（它自己已经说过了）。
   */
  preflight?: (configDir: string | undefined) => Promise<boolean>;
}

/**
 * 在本机把一条会话 resume 起来。
 *
 * @returns 真拉起来了才 `true`；失败时已经出过声。
 */
export async function resumeLocalSession(req: LocalResumeRequest): Promise<boolean> {
  return (await resumeLocalCore(req, "expect")) !== "unsent";
}

/** 同 [`resumeLocalSession`]，但等本机后端报出会话 ⇒ 交回「等到了没有」（换号重启 · 分叉据它才说成了、才记账）。 */
export async function resumeLocalSessionAndWait(req: LocalResumeRequest): Promise<LaunchWait> {
  const o = await resumeLocalCore(req, "await");
  return o === "sent" ? "missed" : o;
}

async function resumeLocalCore(req: LocalResumeRequest, wait: "expect" | "await"): Promise<LaunchWait | "sent"> {
  try {
    const launcher = req.launcher ?? configuredLauncherFor(req.agent, (await getBehavior()).resumeCommandLocal);
    // 名字问本机后端铸（`terminal-name-mint`）；问不到 ⇒ `null`（不拿空集去避让）。
    const minted = req.tmuxName == null ? await mintFreshTmuxName(LOCAL_ORIGIN, req.cwd) : null;
    const tmuxName = req.tmuxName ?? (minted?.ok ? minted.name : null);
    const plan = await launchLocal(
      {
        action: { kind: "resume", sid: req.sid },
        agent: req.agent,
        cwd: req.cwd,
        launcher: launcher.trim() === "" ? null : launcher,
        tmuxName,
        account: req.account,
      },
      req.cwd,
      req.preflight,
    );
    if (plan === null) return "unsent";
    // 窗口开了不等于起来了：等本机后端报出这条会话再说。`await` ⇒ 交回等到了没有。
    const spec = { origin: LOCAL_ORIGIN, match: { sid: req.sid }, tmuxName };
    if (wait === "await") return (await awaitArrival({ ...spec, arrived: null })) ? "arrived" : "missed";
    expectArrival({ ...spec, arrived: { title: copyText("localResume.launch.arrived"), body: arrivedBody(LOCAL_ORIGIN) } });
    return "sent";
  } catch (err) {
    const u = accountUnavailableOf(err);
    if (u) {
      refuseUnavailableAccount({
        machine: LOCAL_ORIGIN,
        u,
        choose: (account) => resumeLocalCore({ ...req, account }, wait),
      });
      return "unsent";
    }
    showActionFailureToast(req.failureTitle ?? copyText("localResume.launch.failed"), String(err), {
      level: "error",
      durationMs: 10000,
    });
    return "unsent";
  }
}
