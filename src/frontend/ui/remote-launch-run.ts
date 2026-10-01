/**
 * 起会话执行器（UI 侧）：构造意图 → 问那台后端要那一行 `ccm …`（`launch-render-cli`）→ 开终端跑它
 * （`terminal-open.ts::openTerminal`：`ssh -t …` 那一行本机后端渲、monitor 开窗口）。tabs / 历史页 / 换号重启 / 分叉共用这一个入口。
 *
 * 起会话只有 ccm 一处：交给终端的永远是一行 `ccm …`（就地 resume 那一格外层包一层 tmux，包的也只是这一行）；
 * 环境、中转地址、身份标记由那台机器上的 `ccm` 自己做，本文件一句 shell 都不拼。
 *
 * 失败回退：复制命令 ＋ 提示（开不了终端窗口时用户仍拿得到可粘贴的那一行，功能永不变砖）。
 */
// 本机 origin（`"<local>"`，与 Rust `inbound_client::LOCAL_ORIGIN` 逐字节相同、有跨语言判据钉着）。
import { LOCAL_ORIGIN } from "./backend-policy";
import { openTerminal } from "./terminal-open";
// 「是不是本机」只经 `ipc/origin.ts` 判。
import { isLocalOrigin } from "./ipc/origin";
import {
  planResumeDirect,
  planResumeTmux,
  planResumeIntoExistingTmux,
  planLauncher,
  planAttach,
} from "./launch-requests";
import type { LaunchContext, LaunchModifiers } from "./launch-types";
// 令牌的字母表与长度只有一份（共享 crate 那两个常量），这里读它现生成的那份、按它**造**。
import { RBIND_TOKEN_ALPHABET, RBIND_TOKEN_LEN } from "./generated/judgment-rules";
import type { CliRenderRequest } from "./launch-cli-wire.ts";
import { isRefusal, planLocalLaunch, renderCli } from "./launch-render";
import { saidOfControl } from "./control-said";
import { showActionFailureToast } from "./error-toast";
import { sendInto, type SendIntoOutcome } from "./tmux-control";
import { offerResyncRetry } from "./resync";
import { AGENT_PROFILE } from "./agent-profile";
// 起新会话的名字只从一个家取：`tmux-name-mint.ts`（列名单 ＋ 铸名 ＋ 「列不出 ⇒ 不起」）。
import { mintFreshTmuxName, refuseUnmintable } from "./tmux-name-mint";
import { copyText } from "./copy-table";
import { arrivedBody, awaitArrival, expectArrival, type ArrivalMatch, type LaunchWait } from "./launch-arrival";

/**
 * 铸一个启动期令牌：**按生成物造**（字母表 `RBIND_TOKEN_ALPHABET` × 长度 `RBIND_TOKEN_LEN`），
 * 每一位从平台 CSPRNG 取一个字节、按拒绝采样落到字母表里（今天 16 个字符 × 32 位 = 128 位熵）。
 * 令牌会进远端的 `/proc/<pid>/environ`、`cmdline` 与 shell 历史 ⇒ 它只是一个不可猜的关联 id，不许承载任何权限语义。
 * **只有一个出口**：全仓所有「起 agent 进程」的拉起都从这里取令牌。
 *
 * @throws 拿不到 CSPRNG（`crypto.getRandomValues` 不在）—— **不回落** `Math.random()`（种子可反推，「不可猜」会静默失效）。
 */
export function mintRbindToken(): string {
  const c: Crypto | undefined = globalThis.crypto;
  if (!c || typeof c.getRandomValues !== "function") {
    throw new Error(copyText("remoteLaunchRun.token.noRandom"));
  }
  const n = RBIND_TOKEN_ALPHABET.length;
  // 落在 [limit, 256) 的字节扔掉重取：否则 `b % n` 偏向前几个字符（n 不整除 256 时）。
  const limit = 256 - (256 % n);
  let token = "";
  while (token.length < RBIND_TOKEN_LEN) {
    const bytes = new Uint8Array(RBIND_TOKEN_LEN - token.length);
    c.getRandomValues(bytes);
    for (const b of bytes) if (b < limit) token += RBIND_TOKEN_ALPHABET[b % n];
  }
  return token;
}

/** 给一组修饰补上令牌。**已经有令牌就原样返回**（调用方显式传的优先）；`=== undefined` 不是 `??`：空串是坏数据，交给后端拒。 */
function withMintedRbindToken(mods: LaunchModifiers): LaunchModifiers {
  return mods.rbindToken === undefined ? { ...mods, rbindToken: mintRbindToken() } : mods;
}

/** 那台后端渲那一行；拒了 ⇒ 抛（带那台的原话，执行器那一格 catch 说出来），不换条路糊过去。 */
async function renderLaunchCommand(origin: string, ctx: LaunchContext): Promise<string> {
  return renderCli(origin, buildCliRenderRequest(ctx));
}

/** 空白 ⇒ 默认启动器（没配就是没配，不是一个判定）。字符集只在后端判。 */
function launcherOrDefault(launcher: string): string {
  return launcher.trim() || AGENT_PROFILE.defaultLauncher;
}

/**
 * 把意图摊成 `launch-render-cli` 的上线形状 —— 「起什么」交给那台后端的唯一住址。
 * 入库夹具 `cli-golden.json` 的 `req` 由同一个函数现产（`launch-cli-golden.ts`），Rust 侧拿生产 wire 类型反序列化、跑生产命令比 `out`。
 */
export function buildCliRenderRequest(ctx: LaunchContext): CliRenderRequest {
  return {
    action:
      ctx.action.kind === "resume"
        ? { kind: "resume", sid: ctx.action.sid }
        : ctx.action.kind === "attach"
          ? { kind: "attach", name: ctx.action.name }
          : { kind: "new" },
    container:
      ctx.container.kind === "tmux"
        ? { kind: "tmux", name: ctx.container.name, send_into: ctx.container.mode === "send-into" }
        : { kind: "none" },
    cwd: ctx.cwd,
    account:
      ctx.account.kind === "account"
        ? { kind: "account", name: ctx.account.name ?? null, configDir: ctx.account.configDir }
        : { kind: "base" },
    ccmSid: ctx.ccmSid ?? null,
    model: ctx.modelOverride ?? null,
    launcher: launcherOrDefault(ctx.launcherOverride ?? ""),
    defaultLauncher: AGENT_PROFILE.defaultLauncher,
    rbindToken: ctx.action.kind === "attach" ? null : (ctx.rbindToken ?? null),
  };
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
  | { kind: "expect"; match: ArrivalMatch; tmuxName: string | null }
  // 同 `expect`，但等主窗口回话、把「等到了没有」交回调用方（换号重启 · 分叉据它才说成了、才记账）。
  | { kind: "await"; match: ArrivalMatch; tmuxName: string | null }
  | { kind: "silent" }
  | { kind: "claim" };

/** 窗口那一跳的结局：没真发出去 · 发出去了（不等）· 等到了 / 没等到。 */
type Opened = LaunchWait | "sent";
const sent = (o: Opened): boolean => o !== "unsent";

/** 账本对 `remote-launch-run.ts` 的既定最终形态之一：「剪贴板回退集中一处」。
 *  6 个 executor 的 invoke→toast/剪贴板回退骨架逐字相同，只有文案与 `origin` 不同——收敛成
 *  这一个函数，返回「IPC 是否真的被接受」（true=拉起成功；false=已走剪贴板回退）。 */
/** U8b：后端在 POSIX 上回的那句话里的**稳定标记**。
 *
 *  它不是「随便找个子串」——后端 `platform/terminal.rs::POSIX_NO_TERMINAL_WINDOW` 是那句话的唯一出处，
 *  两边由 Rust 侧的 `the_posix_marker_is_the_one_the_frontend_matches_on`
 *  逐字对拍（`include_str!` 读本文件）。改一边不改另一边 ⇒ 红。
 *
 *  **为什么按错误文本判、而不是按 `hostOs` 判**：`hostOs !== "windows"` 会把
 *  **真失败**（配置缺失 / 命令被拒 / spawn 崩）也一起软化成「这是设计」——那是另一种撒谎。
 *  按后端自己的声明判，只软化后端明说「这是既定设计」的那一种。 */
export const POSIX_NO_WINDOW_MARKER = "刻意不替你挑终端模拟器";

async function invokeLaunchOrCopyFallback(
  origin: string,
  cmd: string,
  toasts: LaunchToasts,
  // 这次拉起的启动期令牌（交给 `ccm` 的那一个）；`attach` 那一格恒 `null`。
  //   交给本机后端，让新开的窗口以它为 marker 登记进本地表（前奏由后端接：`dial/terminal.rs::with_bind_prelude`）。
  rbindToken: string | null,
  after: AfterOpen,
): Promise<Opened> {
  try {
    await openTerminal(origin, cmd, rbindToken);
    if (after.kind === "expect") {
      expectArrival({
        origin,
        match: after.match,
        tmuxName: after.tmuxName,
        arrived: { title: toasts.success, body: arrivedBody(origin) },
      });
    } else if (after.kind === "await") {
      return (await awaitArrival({ origin, match: after.match, tmuxName: after.tmuxName, arrived: null })) ? "arrived" : "missed";
    } else if (after.kind === "claim") {
      showActionFailureToast(toasts.success, toasts.successDetail ?? "", { level: "info", durationMs: 6000 });
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
    const byDesign = String(err).includes(POSIX_NO_WINDOW_MARKER);
    const headline = byDesign
      ? copied
        ? copyText("remoteLaunchRun.copyFallback.noWindowCopied")
        : copyText("remoteLaunchRun.copyFallback.noWindowManual")
      : copied
        ? toasts.failureCopied
        : toasts.failureNotCopied;
    // ★ 本机没有 ssh 那一跳，文案不能照抄远端那句。
    const where =
      isLocalOrigin(origin) ? copyText("remoteLaunchRun.copyFallback.runLocal") : copyText("remoteLaunchRun.copyFallback.runRemote", { machine: origin });
    showActionFailureToast(
      headline,
      `${String(err)}\n${where}\n${cmd}`,
      { level: "info", durationMs: 10000 },
    );
    return "unsent";
  }
}

/** 一键 resume 远端会话：拉起成功 toast 告知；失败回退复制命令。 */
export async function runRemoteResume(
  origin: string,
  sid: string,
  cwd: string,
  launcher: string,
  mods: LaunchModifiers = {}, // 正交修饰（configDir/accountName/modelOverride/rbindToken），见 launch-types.ts
  // Phase G（branch-anywhere）：返回值从 `void` 改成 `boolean`，与 `runRemoteResumeTmux`
  // 对齐（那边的头注逐字记着为什么要有返回值：account-ux 那次把「走到了第⑤步」当成
  // 「已 resume」）。既有调用点忽略返回值 ⇒ 行为逐字不变。
): Promise<boolean> {
  return sent(await resumeDirectCore(origin, sid, cwd, launcher, mods, "expect"));
}

/** 同 [`runRemoteResume`]，但等那台报出会话 ⇒ 交回「等到了没有」（分叉据它才说「已分叉」）。 */
export async function runRemoteResumeAndWait(
  origin: string,
  sid: string,
  cwd: string,
  launcher: string,
  mods: LaunchModifiers = {},
): Promise<LaunchWait> {
  const o = await resumeDirectCore(origin, sid, cwd, launcher, mods, "await");
  return o === "sent" ? "missed" : o;
}

async function resumeDirectCore(
  origin: string,
  sid: string,
  cwd: string,
  launcher: string,
  mods: LaunchModifiers,
  wait: "expect" | "await",
): Promise<Opened> {
  let cmd: string;
  let token: string | null;
  try {
    const ctx = planResumeDirect(sid, cwd, launcher, withMintedRbindToken(mods));
    cmd = await renderLaunchCommand(origin, ctx);
    token = ctx.rbindToken ?? null;
  } catch (err) {
    showActionFailureToast(copyText("remoteLaunchRun.resume.buildFailed"), String(err));
    return "unsent";
  }
  return invokeLaunchOrCopyFallback(origin, cmd, {
    success: copyText("remoteLaunchRun.resume.started"),
    failureCopied: copyText("remoteLaunchRun.resume.failedCopied"),
    failureNotCopied: copyText("remoteLaunchRun.copyFallback.failedManual"),
  }, token, { kind: wait, match: { sid }, tmuxName: null });
}

/** F52：tmux 版 resume——在远端 tmux 会话 `<sid8>-cc` 里幂等 resume Claude;失败回退复制命令。
 *
 *  @returns 是否**真的把终端拉起来了**。false = 命令构造失败 / 开终端（`openTerminal`）失败
 *  （此时已走剪贴板回退，需用户手动粘贴）。
 *  account-ux Phase G 审计:此前返回 void 且两条失败路径都自己吞掉,于是 `restartWithAccount`
 *  把"走到了第⑤步"当成"已 resume"——会话被 kill、没起来,却照样记 pin、照样弹「已用新账号重启」、
 *  照样 return true,批量对齐还把它计成成功。而失败是**确定性**的（如 F34 launcher 含双引号被
 *  launch.rs 拒、tmux 名不合白名单、缺 OpenSSH），不是概率事件。 */
export async function runRemoteResumeTmux(
  origin: string,
  sid: string,
  cwd: string,
  launcher: string,
  // F13：`name` 改必填（原为 `name?`）。生产三个调用点本来都传，但类型允许省略 ——
  // 而省略就意味着走一个**不做撞名避让**的默认值。让 `tsc` 把「碰巧没人省略」变成「不可能省略」。
  name: string,
  mods: LaunchModifiers = {}, // 正交修饰（configDir/accountName/modelOverride/rbindToken），见 launch-types.ts
): Promise<boolean> {
  return sent(await resumeTmuxCore(origin, sid, cwd, launcher, name, mods, "expect"));
}

/** 同 [`runRemoteResumeTmux`]，但等那台报出会话 ⇒ 交回「等到了没有」（换号重启 · 分叉据它才说成了、才记账）。 */
export async function runRemoteResumeTmuxAndWait(
  origin: string,
  sid: string,
  cwd: string,
  launcher: string,
  name: string,
  mods: LaunchModifiers = {},
): Promise<LaunchWait> {
  const o = await resumeTmuxCore(origin, sid, cwd, launcher, name, mods, "await");
  return o === "sent" ? "missed" : o;
}

async function resumeTmuxCore(
  origin: string,
  sid: string,
  cwd: string,
  launcher: string,
  name: string,
  mods: LaunchModifiers,
  wait: "expect" | "await",
): Promise<Opened> {
  let cmd: string;
  let token: string | null;
  try {
    const ctx = planResumeTmux(sid, cwd, launcher, name, withMintedRbindToken(mods));
    cmd = await renderLaunchCommand(origin, ctx);
    token = ctx.rbindToken ?? null;
  } catch (err) {
    showActionFailureToast(copyText("remoteLaunchRun.resumeTmux.buildFailed"), String(err));
    return "unsent";
  }
  return invokeLaunchOrCopyFallback(origin, cmd, {
    success: copyText("remoteLaunchRun.resumeTmux.started"),
    failureCopied: copyText("remoteLaunchRun.resumeTmux.failedCopied"),
    failureNotCopied: copyText("remoteLaunchRun.copyFallback.failedManual"),
  }, token, { kind: wait, match: { sid }, tmuxName: name });
}

/**
 * 就地 resume 的键入那一半交给那台后端（`launch{mode:"send-into"}`）：键进那个 pane 的是**直路**那一行 `ccm …`
 * （pane 里已经有 shell，不再建容器）。三态：`typed` = 真的键入了 ⇒ 终端只需接进去；
 * `fallback` = **能证明什么都没发出去** ⇒ 可以照整串（外层包一层 tmux）走；`refused` = 后端说过话了 / 拿不准执行没有 ⇒ **绝不许回落**
 * （回落那条整串没有身份门：把「被门拒」或「已键入但应答超时」重做一遍，会把那一行第二次提交给正在跑的 agent）。
 * 「能不能回落」的判定住 `ipc/chan-caller.ts::provablyNotSent`（`tmux-control.ts::sendInto` 调它）。
 */
async function sendIntoViaBackend(origin: string, name: string, ctx: LaunchContext): Promise<SendIntoOutcome> {
  let line: string;
  try {
    line = await renderLaunchCommand(origin, { ...ctx, container: { kind: "none" } });
  } catch (e) {
    // 拒了 ⇒ 坏输入，重做只会被同一道闸再拒一次（不回落）；不是拒 ⇒ 还没到后端那一跳 ⇒ 可以回落。
    const raw = saidOfControl(e);
    if (isRefusal(e)) return { verdict: "refused", reason: raw };
    return { verdict: "fallback", reason: raw };
  }
  const sent = await sendInto(origin, name, line);
  if (sent.verdict === "fallback") console.debug(`[send-into] 回落到整串（证明没发出去）：${sent.reason}`);
  if (sent.verdict === "refused") console.debug(`[send-into] 被拒，**不回落**：${sent.reason}`);
  return sent;
}

/** 往一个**已存在的空 tmux**（agent 已退、只剩交互 shell 的那个会话）就地 resume：键入那一行 ＋ 接进去，复用原会话名（不产孤儿）。
 *  true = 真拉起来了；false = 构造失败 / 拉起失败（已回退剪贴板）/ 被拒。 */
export async function runRemoteResumeIntoExistingTmux(
  origin: string,
  sid: string,
  name: string,
  launcher: string,
  mods: LaunchModifiers = {}, // 正交修饰（configDir/accountName/modelOverride/rbindToken），见 launch-types.ts
): Promise<boolean> {
  let cmd: string;
  let viaBackend = false;
  let token: string | null;
  try {
    const ctx = planResumeIntoExistingTmux(sid, name, launcher, withMintedRbindToken(mods));
    // 两条出路都用这一个令牌：`typed` 那条开的窗口只跑接回，接上的正是刚被键入、环境里带着它的那个 agent。
    token = ctx.rbindToken ?? null;
    // 先让那台后端键入（`send-keys` 归那台，接回必须留在用户自己的终端）；证明没发出去 ⇒ 照整串走。
    const sent = await sendIntoViaBackend(origin, name, ctx);
    if (sent.verdict === "refused") {
      // 不许回落（理由见 `sendIntoViaBackend` 头注）⇒ 就地失败，并且要让用户看见。
      // 关卡 2 拒的 ⇒ 提示带「对齐后重试」（与结束会话那颗同一个动作）。
      const said = sent.reason ?? copyText("remoteLaunchRun.inPlace.refusedUnsure");
      if (sent.gate2) {
        offerResyncRetry(origin, sid, copyText("remoteLaunchRun.inPlace.notRun"), said, async () => {
          await runRemoteResumeIntoExistingTmux(origin, sid, name, launcher, mods);
        });
      } else {
        showActionFailureToast(copyText("remoteLaunchRun.inPlace.notRun"), said);
      }
      return false;
    }
    if (sent.verdict === "typed") {
      // 载荷已经键进那个 pane ⇒ 从这一刻起等那台报出它（接终端的窗口开不开得了不改变这件事）。
      expectArrival({
        origin,
        match: { sid },
        tmuxName: name,
        arrived: { title: copyText("remoteLaunchRun.inPlace.done"), body: arrivedBody(origin) },
      });
      cmd = await renderLaunchCommand(origin, planAttach(name));
      viaBackend = true;
    } else {
      cmd = await renderLaunchCommand(origin, ctx);
    }
  } catch (err) {
    showActionFailureToast(copyText("remoteLaunchRun.inPlace.buildFailed"), String(err));
    return false;
  }
  return sent(await invokeLaunchOrCopyFallback(origin, cmd, {
    success: copyText("remoteLaunchRun.inPlace.done"),
    failureCopied: copyText("remoteLaunchRun.inPlace.failedCopied"),
    failureNotCopied: copyText("remoteLaunchRun.copyFallback.failedManual"),
  }, token, viaBackend ? { kind: "silent" } : { kind: "expect", match: { sid }, tmuxName: name }));
}

/**
 * **本机**就地 resume —— 往一个已存在的空 tmux 键入那一行，不新建会话（不复用就会产 `<名>-2` 孤儿）。
 * 与远端那条共用 `planResumeIntoExistingTmux` ＋ `sendIntoViaBackend`；差别只有两处：
 * ① 本机没有回落那条路（造一条就是「给本地单写一套控制逻辑」）⇒ 不是 `typed` 就诚实失败；
 * ② 接回那一句也问本机后端（`launch-local` 的 attach 那一格），Linux 上不开终端窗口 ⇒ 交给用户自己跑。
 */
export async function runLocalResumeIntoExistingTmux(
  sid: string,
  name: string,
  launcher: string,
  mods: LaunchModifiers = {},
): Promise<boolean> {
  let ctx: LaunchContext;
  try {
    ctx = planResumeIntoExistingTmux(sid, name, launcher, withMintedRbindToken(mods));
  } catch (err) {
    showActionFailureToast(copyText("remoteLaunchRun.inPlace.buildFailed"), String(err));
    return false;
  }
  const sent = await sendIntoViaBackend(LOCAL_ORIGIN, name, ctx);
  if (sent.verdict !== "typed") {
    // `fallback` 与 `refused` 在本机是**同一种处置** —— 见头注：本机没有第二条路，
    // 而造一条就是 `C1` 排除的那件事。两者的 `reason` 都原样交给用户。
    const said = sent.reason ?? copyText("remoteLaunchRun.inPlaceLocal.noBackend");
    // 关卡 2 拒的 ⇒ 提示带「对齐后重试」。
    if (sent.verdict === "refused" && sent.gate2) {
      offerResyncRetry(LOCAL_ORIGIN, sid, copyText("remoteLaunchRun.inPlace.notRun"), said, async () => {
        await runLocalResumeIntoExistingTmux(sid, name, launcher, mods);
      });
    } else {
      showActionFailureToast(copyText("remoteLaunchRun.inPlace.notRun"), said);
    }
    return false;
  }
  // 载荷已经键进那个 pane ⇒ 从这一刻起等本机后端报出它。
  expectArrival({
    origin: LOCAL_ORIGIN,
    match: { sid },
    tmuxName: name,
    arrived: { title: copyText("remoteLaunchRun.inPlaceLocal.done"), body: arrivedBody(LOCAL_ORIGIN) },
  });
  // 接回那一句也问本机后端（`ccm -- --attach <名>`）；开窗那一跳与远端共用（Windows 开 PowerShell 窗口，Linux 复制给用户）。
  // 渲不出来就诚实失败，不在前端自己拼一条。
  let attachCmd: string;
  try {
    attachCmd = (await planLocalLaunch({ action: { kind: "attach" }, cwd: null, launcher: null, tmuxName: name })).cmd;
  } catch (err) {
    showActionFailureToast(
      copyText("remoteLaunchRun.inPlaceLocal.attachFailed"),
      copyText("remoteLaunchRun.inPlaceLocal.attachFailedBody", { err: String(err) }),
    );
    // ★ 与下面那条同一个道理：**就地 resume 已经成了**，attach 这一跳的成败不改变它。
    return true;
  }
  await invokeLaunchOrCopyFallback(LOCAL_ORIGIN, attachCmd, {
    success: copyText("remoteLaunchRun.inPlaceLocal.done"),
    failureCopied: copyText("remoteLaunchRun.inPlaceLocal.copied"),
    failureNotCopied: copyText("remoteLaunchRun.inPlaceLocal.manual"),
  }, ctx.rbindToken ?? null, { kind: "silent" });
  // ★ 就地 resume 本身已经成了（`typed`）——**attach 开不开得了窗口不改变这个结论**。
  //   返回 `false` 会让调用方以为这次 resume 没做成，那是把两件事混成一件。
  return true;
}

/**
 * F96：历史页「在该目录起新会话」——远端分支。tmux 会话名由 cwd 派生、默认拉起命令由
 * `AGENT_PROFILE` 兜底，**让调用方（history.ts）既不必知道底下用不用 tmux、也不必知道
 * 默认拉起是哪个 agent**（用户 2026-07-15 硬约束）——history.ts 只传 F34 配置命令（可空）。
 * 薄封装 F53 的 `runRemoteLauncher`，不写第二份拉起逻辑。
 * （`buildLauncherCmd` 只对 `undefined` 套默认、空串不触发，故默认在此显式兜。）
 */
export async function runNewSessionRemote(
  origin: string,
  cwd: string,
  command: string,
  mods: LaunchModifiers = {}, // 正交修饰（configDir/accountName/modelOverride/rbindToken），见 launch-types.ts
): Promise<void> {
  // ★★ **默认名必须过铸名口**（F13）：同一个 cwd 点两次「起新会话」派生出同一个名字 ⇒ 撞上远端
  // `create-or-attach` 的幂等闸 ⇒ **静默接进第一个会话，而用户以为开了新的**（issue #76 那一族）。
  //
  // 「列名单 → 铸名」收进 `tmux-name-mint.ts`（本机远端同一个家）。这里先前是
  // `settings/machine-card.ts` 那段的逐字副本，**列不出名单就拿空集铸名**（「诚实降级：列不出来就不避让」）——
  // 那正是 #76 的形状，而本机那一侧早就写着「绝不退化成空集」。⇒ 列不出 ⇒ 不起、说清。
  const minted = await mintFreshTmuxName(origin, cwd);
  if (!minted.ok) {
    refuseUnmintable(origin, minted.why);
    return;
  }
  await runRemoteLauncher(
    origin,
    cwd,
    minted.name,
    command || AGENT_PROFILE.defaultLauncher,
    mods,
  );
}

/** F53：「在这台机开新 Claude」——在远端 tmux 会话里启动全新 Claude;失败回退复制命令。 */
export async function runRemoteLauncher(
  origin: string,
  cwd: string,
  tmuxName: string,
  command: string,
  mods: LaunchModifiers = {}, // 正交修饰（configDir/accountName/modelOverride/rbindToken），见 launch-types.ts
): Promise<void> {
  let cmd: string;
  let token: string | null;
  try {
    const ctx = planLauncher(cwd, tmuxName, command, withMintedRbindToken(mods));
    cmd = await renderLaunchCommand(origin, ctx);
    token = ctx.rbindToken ?? null;
  } catch (err) {
    showActionFailureToast(copyText("remoteLaunchRun.launcher.buildFailed"), String(err));
    return;
  }
  // 新开的会话拉起那一刻没有 sid：认它靠这次铸进进程环境的启动期令牌（那台读回、随 `live` 报上来）。
  await invokeLaunchOrCopyFallback(origin, cmd, {
    success: copyText("remoteLaunchRun.launcher.started"),
    failureCopied: copyText("remoteLaunchRun.launcher.failedCopied"),
    failureNotCopied: copyText("remoteLaunchRun.copyFallback.failedManual"),
  }, token, { kind: "expect", match: token ? { token } : { cwd }, tmuxName });
}

/** F51：一键 attach 到远端 tmux 会话:拉起 `ssh -t … tmux attach -t <名>`;失败回退复制命令。
 *  ccm 已装且能力齐全时走 CLI 渲染器（`ccm attach <名>`，与兜底输出逐字同构，无 #76 歧义）。 */
export async function runRemoteAttach(origin: string, name: string): Promise<void> {
  let cmd: string;
  try {
    cmd = await renderLaunchCommand(origin, planAttach(name));
  } catch (err) {
    showActionFailureToast(copyText("remoteLaunchRun.attach.buildFailed"), String(err));
    return;
  }
  await invokeLaunchOrCopyFallback(origin, cmd, {
    success: copyText("remoteLaunchRun.attach.started"),
    successDetail: copyText("remoteLaunchRun.attach.startedBody", { machine: origin, name }),
    failureCopied: copyText("remoteLaunchRun.attach.failedCopied"),
    failureNotCopied: copyText("remoteLaunchRun.copyFallback.failedManual"),
  }, null, { kind: "claim" }); // `attach` 不铸币（`planAttach` 不收 `mods`）⇒ 新窗口不做令牌握手
}
