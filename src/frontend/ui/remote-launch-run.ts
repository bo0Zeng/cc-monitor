/**
 * 起会话执行器（UI 侧）：构造意图 → 问那台后端要那一行 `ccm …`（`launch-render-cli`）→ 开终端跑它
 * （`terminal-open.ts::openTerminal`：`ssh -t …` 那一行本机后端渲、monitor 开窗口）。resume 与接回走这里；起新会话走 `new-session.ts` 那一个请求。
 *
 * 起会话只有 ccm 一处：交给终端的永远是一行 `ccm …`（就地 resume 那一格外层包一层 tmux，包的也只是这一行）；
 * 环境、中转地址、身份标记由那台机器上的 `ccm` 自己做，本文件一句 shell 都不拼。
 *
 * 失败回退：复制命令 ＋ 提示（开不了终端窗口时用户仍拿得到可粘贴的那一行，功能永不变砖）。
 */
// 本机 origin（`"<local>"`，与 Rust `inbound_client::LOCAL_ORIGIN` 逐字节相同、有跨语言判据钉着）。
import { NoTerminalWindow, openTerminal } from "./terminal-open";
// 「是不是本机」只经 `ipc/origin.ts` 判。
import { isLocalOrigin } from "./ipc/origin";
import { planResumeDirect, planAttach } from "./launch-requests";
import type { LaunchContext, LaunchModifiers } from "./launch-types";
import type { AccountAsk } from "./generated/AccountAsk";
import {
  accountUnavailableOf,
  refuseUnavailableAccount,
} from "./launch-account";
import { machineModels } from "./account-prefs";
import { buildCliRenderRequest } from "./launch-cli-wire.ts";
import { renderCli } from "./launch-render";
import { toast } from "./kit/toast";
import { copyText } from "./copy-table";
import {
  arrivedBody,
  expectArrival,
  type ArrivalMatch,
} from "./launch-arrival";

/** 那台后端渲那一行；拒了 ⇒ 抛（带那台的原话，执行器那一格 catch 说出来），不换条路糊过去。 */
async function renderLaunchCommand(
  origin: string,
  ctx: LaunchContext,
): Promise<string> {
  return (await renderCli(origin, buildCliRenderRequest(ctx))).cmd;
}

/**
 * 渲那一行（那台判号）：要的号选不了 ⇒ 不起、给显式选择（点了 ⇒ `again(那个号)`）；别的失败 ⇒ 说成 `failedTitle` 那一句；
 * 开窗之前问 `mods.preflight`（收那台判出来的号的目录）。说不起 ⇒ `null`。
 */
async function renderOrRefuse(
  origin: string,
  ctx: LaunchContext,
  failedTitle: string,
  mods: LaunchModifiers,
  again: (account: AccountAsk) => unknown,
): Promise<string | null> {
  let r;
  try {
    const req = buildCliRenderRequest(ctx);
    if (mods.models === undefined && ctx.action.kind !== "attach")
      req.models = await machineModels(origin);
    r = await renderCli(origin, req);
  } catch (err) {
    const u = accountUnavailableOf(err);
    if (u)
      refuseUnavailableAccount({
        machine: origin,
        u,
        choose: (account) => again(account) as Promise<unknown>,
      });
    else toast(failedTitle, String(err));
    return null;
  }
  if (mods.preflight && !(await mods.preflight(r.account?.configDir)))
    return null;
  return r.cmd;
}

interface LaunchToasts {
  success: string;
  /** 只有 `claim`（接回那一格）用得着：窗口开了就说。 */
  successDetail?: string;
  failureCopied: string;
  failureNotCopied: string;
}

/**
 * 窗口开出来之后说什么：
 * `expect` = 这一趟起了 agent 进程 ⇒ 不当场说「起来了」，交 `launch-arrival.ts` 等那台报出它；
 * `silent` = 起会话那一跳已经交过「等它」了（就地 resume 先键入、再开窗接上）⇒ 窗口开了不再说话；
 * `claim` = 没起进程（接回）⇒ 窗口开了就说。
 */
type AfterOpen =
  | {
      kind: "expect";
      match: ArrivalMatch;
      tmuxName: string | null;
      slot: { cwd: string; agent: string };
    }
  | { kind: "silent" }
  | { kind: "claim" };

/** 窗口那一跳的结局：没真发出去 · 发出去了（等那台报出交给主窗口）。 */
type Opened = "unsent" | "sent";
const sent = (o: Opened): boolean => o !== "unsent";

/** 账本对 `remote-launch-run.ts` 的既定最终形态之一：「剪贴板回退集中一处」。
 *  6 个 executor 的 invoke→toast/剪贴板回退骨架逐字相同，只有文案与 `origin` 不同——收敛成
 *  这一个函数，返回「IPC 是否真的被接受」（true=拉起成功；false=已走剪贴板回退）。 */

async function invokeLaunchOrCopyFallback(
  origin: string,
  cmd: string,
  toasts: LaunchToasts,
  after: AfterOpen,
): Promise<Opened> {
  try {
    await openTerminal(origin, cmd);
    if (after.kind === "expect") {
      expectArrival({
        origin,
        match: after.match,
        tmuxName: after.tmuxName,
        arrived: { title: toasts.success, body: arrivedBody(origin) },
        slot: after.slot,
      });
    } else if (after.kind === "claim") {
      toast(toasts.success, toasts.successDetail ?? "", { level: "info" });
    }
    return "sent";
  } catch (err) {
    // 回退：复制命令让用户自己粘贴（保留 F09 语义）。
    let copied = true;
    try {
      await navigator.clipboard.writeText(cmd);
    } catch {
      copied = false; // 命令在 toast 里仍可见，可手动复制
    }
    // U8b：**POSIX 上不开终端窗口是既定设计，不是失败。**
    // 原来这里一律报「拉起失败」，在 Linux 上每次点 ↗ 都会读到 —— 那是把一个正常状态
    // 训练成「坏了」。标题按后端的声明分档；正文原样带上后端那句话（它自己会解释为什么）。
    // 按壳回的结局判（`NoTerminalWindow`），不按那句话里的字、也不按 `hostOs` 猜（那会把真失败也软化成「设计」）。
    const byDesign = err instanceof NoTerminalWindow;
    const headline = byDesign
      ? copied
        ? copyText("remoteLaunchRun.copyFallback.noWindowCopied")
        : copyText("remoteLaunchRun.copyFallback.noWindowManual")
      : copied
        ? toasts.failureCopied
        : toasts.failureNotCopied;
    // ★ 本机没有 ssh 那一跳，文案不能照抄远端那句。
    const where = isLocalOrigin(origin)
      ? copyText("remoteLaunchRun.copyFallback.runLocal")
      : copyText("remoteLaunchRun.copyFallback.runRemote", { machine: origin });
    toast(headline, `${String(err)}\n${where}\n${cmd}`, { level: "info" });
    return "unsent";
  }
}

/** 一键 resume 远端会话：拉起成功 toast 告知；失败回退复制命令。 */
export async function runRemoteResume(
  origin: string,
  agent: string,
  sid: string,
  cwd: string,
  launcher: string,
  mods: LaunchModifiers = {}, // 正交修饰（要哪个号 · 那台的模型偏好表），见 launch-types.ts
  // 回「真发出去了吗」（失败那一路自己出声）：别把「走到了最后一步」当成「已 resume」。
): Promise<boolean> {
  return sent(await resumeDirectCore(origin, agent, sid, cwd, launcher, mods));
}

async function resumeDirectCore(
  origin: string,
  agent: string,
  sid: string,
  cwd: string,
  launcher: string,
  mods: LaunchModifiers,
): Promise<Opened> {
  const ctx = planResumeDirect(agent, sid, cwd, launcher, mods);
  const cmd = await renderOrRefuse(
    origin,
    ctx,
    copyText("remoteLaunchRun.resume.buildFailed"),
    mods,
    (account) =>
      resumeDirectCore(origin, agent, sid, cwd, launcher, { ...mods, account }),
  );
  if (cmd === null) return "unsent";
  return invokeLaunchOrCopyFallback(
    origin,
    cmd,
    {
      success: copyText("remoteLaunchRun.resume.started"),
      failureCopied: copyText("remoteLaunchRun.resume.failedCopied"),
      failureNotCopied: copyText("remoteLaunchRun.copyFallback.failedManual"),
    },
    { kind: "expect", match: { sid }, tmuxName: null, slot: { cwd, agent } },
  );
}

/** F51：一键 attach 到远端 tmux 会话:拉起 `ssh -t … tmux attach -t <名>`;失败回退复制命令。
 *  ccm 已装且能力齐全时走 CLI 渲染器（`ccm attach <名>`，与兜底输出逐字同构，无 #76 歧义）。 */
export async function runRemoteAttach(
  origin: string,
  agent: string,
  name: string,
  opts: { quiet?: boolean } = {},
): Promise<void> {
  let cmd: string;
  try {
    cmd = await renderLaunchCommand(origin, planAttach(agent, name));
  } catch (err) {
    toast(copyText("remoteLaunchRun.attach.buildFailed"), String(err));
    return;
  }
  await invokeLaunchOrCopyFallback(
    origin,
    cmd,
    {
      success: copyText("remoteLaunchRun.attach.started"),
      successDetail: copyText("remoteLaunchRun.attach.startedBody", {
        machine: origin,
        name,
      }),
      failureCopied: copyText("remoteLaunchRun.attach.failedCopied"),
      failureNotCopied: copyText("remoteLaunchRun.copyFallback.failedManual"),
    },
    opts.quiet ? { kind: "silent" } : { kind: "claim" },
  );
}
