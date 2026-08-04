/**
 * F41：远端 resume 拉起执行器（UI 侧）。连接 remote-launch 纯函数与后端
 * `launch_remote_terminal`（wt.exe/PowerShell 拉起 `ssh -t …`），tabs.ts 与
 * views/history.ts 共用此单一入口，防两处行为漂移。
 *
 * 失败回退 = F09 旧行为：复制命令 + toast 说明（非 Windows dev / 配置缺失 /
 * wt+PowerShell 都 spawn 失败时，用户仍拿得到可粘贴命令，功能永不变砖）。
 *
 * F03：6 个 executor 收敛为「构造 {ctx,plan} → `renderLaunchCommand` 挑渲染器 → 执行」。
 * **对外签名/返回值语义逐字不变**——`account-restart.ts`/`tabs.ts`/`views/history.ts` 零改动
 * （`runRemoteResumeTmux` 的位置参数签名被 `e2e/restart-cmd-driver.ts` 经 `account-restart.ts`
 * 传递性锁死）。
 */
import { commands } from "./ipc/commands";
import {
  planResumeDirect,
  planResumeTmux,
  planResumeIntoExistingTmux,
  planLauncher,
  planAttach,
} from "./launch-requests";
import type { LaunchModifiers } from "./launch-plan";
import { renderFallback } from "./launch-render-fallback";
// U8c-2c-2：`tryRenderCli` **不再是生产渲染器**（那一支已切到 Rust）——
// 它降级为「只供 `launch-cli-golden.ts` 生成夹具」，删在 U8c-3。
import type { CliRenderResult } from "./launch-render-cli";
import type { CliRenderRequest, PayloadRenderRequest } from "./launch-cli-wire.ts";
import type { CcmProbeResult } from "./ccm-probe.ts";
import { sanitizeRemoteLauncher } from "./shell-quote.ts";
import { probeCcm } from "./ccm-probe";
import { getBehavior } from "./behavior";
import { showActionFailureToast } from "./error-toast";
import { AGENT_PROFILE } from "./agent-profile";
import { deriveTmuxName } from "./remote-launch";
import type { LaunchContext, LaunchPlan } from "./launch-plan";

/** 挑渲染器：`forceLegacyLaunchRenderer` 手动逃生口（MASTERPLAN R2）短路到兜底；否则探测到 ccm
 *  且该 plan 的全部维度都能表达成 CLI 语法 → 走 CLI；探测失败/未装/能力不足/含 CLI 表达不了的
 *  维度（如账号、idle-tmux 复用）→ 安全降级，绝不因为渲染器选择本身而让启动失败。 */
async function renderLaunchCommand(
  origin: string,
  ctx: LaunchContext,
  plan: LaunchPlan,
): Promise<string> {
  const behavior = await getBehavior();
  if (!behavior.forceLegacyLaunchRenderer && ctx.transport.kind === "ssh") {
    const probe = await probeCcm(origin);
    // R04①：一次调用同时回答"能不能"与"渲染成什么"。拿不到 `ok:true` 就走兜底——
    // 不存在"渲染出来了但悄悄丢了某个修饰"这个中间态（改造前 `renderCli` 对 `cliFlags` 返回
    // `null` 是静默跳过的，安全性全靠调用方记得先问 `canRenderCli`）。
    //
    // **U8c-2c-2：这一支已切到 Rust**（`backend::control::ccm_invocation::render_ccm_invocation`）。
    // 前端只发结构化请求，命令由后端渲染 —— 这是本工作区第一条真正切过去的渲染路径。
    // TS 的 `tryRenderCli` **没删**，降级为「只供夹具对拍」（删在 U8c-3）。
    //
    // ⚠ **兜底那支仍在 TS**：`container: tmux` 时它要外层 tmux 命令（`session-backend.ts`），
    // 而 §33b 写死了「搬它之前必须先回答三件事」。⇒ 那支归 U8c-3。
    const r = await renderCliViaBackend(ctx, plan, probe);
    if (r.ok) return r.cmd;
    // R04① 的第二条收益（Phase D 审计指出它此前"只活在测试里"，生产侧零消费者）：
    // 把**为什么**降级说出来。刻意用 `console.debug` 而非 toast/`console.warn`——
    // 走兜底渲染器是**正常且预期**的路径（没装 ccm 的用户每次拉起都会走它），
    // 弹 toast 或 warn 等于对着正常行为报警，是净噪音。要查"为什么这台机没走 CLI 路径"时，
    // 这一行是唯一线索；不查的时候它不打扰任何人。
    console.debug(`[launch] CLI 渲染器降级 → 兜底渲染器（origin=${origin}）: ${r.reason}`);
  }
  // U8a-2c-pre（账本 S28）：兜底那支的 **`container:"none"` 那一格**也切到 Rust 了
  // （`backend::control::payload::render_payload`）。**只有这一格** —— tmux 那两格还要外层容器命令
  // （`session-backend.ts`），而 §33b 写死了「搬它之前必须先回答三件事」⇒ U8c-3。
  if (plan.container.kind === "none" && plan.action.kind !== "attach") {
    try {
      return await commands.render_launch_payload({ req: buildPayloadRenderRequest(plan) });
    } catch (e) {
      // 后端拒了（非法 configDir / 会裂的 arg）⇒ **不静默用 TS 版糊过去**：
      // 那等于把一次 fail-closed 变成 fail-open。原样抛给调用方的 catch（它会 toast）。
      throw new Error(`后端拒绝渲染载荷：${String(e)}`);
    }
  }
  return renderFallback(plan);
}

/** U8c-2c-2：把 `{ctx, plan, probe}` 摊成上线形状，交给 Rust 渲染 `ccm …` 调用行。
 *
 *  **`ok:false` 不是错误，是诚实降级**（§33）—— 调用方拿着 `reason` 去走兜底渲染器，
 *  与切换前 `tryRenderCli` 的语义逐字相同。
 *
 *  ⚠ IPC 本身失败（后端崩/参数被拒）与「渲染器说渲染不出来」是**两件事**：
 *  前者 catch 成一条带 `IPC` 字样的 reason，照样降级 —— 拉起功能永不因为渲染器选择而变砖。 */
async function renderCliViaBackend(
  ctx: LaunchContext,
  plan: LaunchPlan,
  probe: CcmProbeResult,
): Promise<CliRenderResult> {
  const req = buildCliRenderRequest(ctx, plan, probe);
  try {
    const res = await commands.render_ccm_launch({ req });
    return res.ok && res.cmd !== null
      ? { ok: true, cmd: res.cmd }
      : { ok: false, reason: res.reason ?? "后端未给降级理由" };
  } catch (e) {
    return { ok: false, reason: `IPC 渲染失败，走兜底：${String(e)}` };
  }
}

/** U8a-2c-pre 复盘（判据体系审计）：**请求构造抽出来，好让夹具用同一份代码产它**。
 *
 *  ⚠ 抽之前，`renderCliViaBackend` 里这 22 行**零判据** —— 审计实测三个变异全绿：
 *  成功分支整个作废 · `isSsh` 恒 false（CLI 路径永久死掉）· `ccmSid`/`model` 恒 null
 *  （两个维度静默消失）。它们静默的形态都一样：**回落 TS 兜底渲染器，功能不变砖、门禁全绿**。
 *
 *  现在 `launch-cli-golden.ts` 用这同一个函数产夹具里的 `req`，Rust 侧拿**生产 wire 类型**
 *  反序列化它、跑**生产命令**、与 TS 渲染器的产物逐字节比 ⇒ 上面那三个变异各自会让
 *  `req` 变形 ⇒ Rust 产出变 ⇒ 与 `out` 不一致 ⇒ 红。 */
export function buildCliRenderRequest(
  ctx: LaunchContext,
  plan: LaunchPlan,
  probe: CcmProbeResult,
): CliRenderRequest {
  return {
    isSsh: plan.transport.kind === "ssh",
    caps: probe.installed ? [...probe.capabilities] : null,
    action:
      plan.action.kind === "resume"
        ? { kind: "resume", sid: plan.action.sid }
        : plan.action.kind === "attach"
          ? { kind: "attach", name: plan.action.name }
          : { kind: "new" },
    container:
      plan.container.kind === "tmux"
        ? { kind: "tmux", name: plan.container.name, send_into: plan.container.mode === "send-into" }
        : { kind: "none" },
    cwd: plan.cwd,
    account:
      ctx.account.kind === "account"
        ? { kind: "account", name: ctx.account.name ?? null }
        : { kind: "base" },
    ccmSid: ctx.ccmSid ?? null,
    model: ctx.modelOverride ?? null,
    launcher: sanitizeRemoteLauncher(plan.launcher),
    defaultLauncher: AGENT_PROFILE.defaultLauncher,
  };
}

/** 同 [`buildCliRenderRequest`]：抽出来好让夹具用同一份代码产 `req`。 */
export function buildPayloadRenderRequest(plan: LaunchPlan): PayloadRenderRequest {
  return {
    env: plan.env,
    cwd: plan.cwd,
    launcher: sanitizeRemoteLauncher(plan.launcher),
    args:
      plan.action.kind === "resume"
        ? [AGENT_PROFILE.resumeFlag, plan.action.sid, ...plan.args]
        : [...plan.args],
    nestedEnv: [...AGENT_PROFILE.nestedEnvVars],
    wrap: plan.wrap.map((w) => ({ order: w.order, prelude: w.prelude })),
  };
}

interface LaunchToasts {
  success: string;
  successDetail: string;
  failureCopied: string;
  failureNotCopied: string;
}

/** MASTERPLAN §3 账本对 `remote-launch-run.ts` 的既定最终形态之一：「剪贴板回退集中一处」。
 *  6 个 executor 的 invoke→toast/剪贴板回退骨架逐字相同，只有文案与 `origin` 不同——收敛成
 *  这一个函数，返回「IPC 是否真的被接受」（true=拉起成功；false=已走剪贴板回退）。 */
/** U8b：后端在 POSIX 上回的那句话里的**稳定标记**。
 *
 *  它不是「随便找个子串」——后端 `launch.rs::POSIX_NO_TERMINAL_WINDOW` 是那句话的唯一出处，
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
): Promise<boolean> {
  try {
    await commands.launch_remote_terminal({ origin, remoteCmd: cmd });
    showActionFailureToast(toasts.success, toasts.successDetail, { level: "info", durationMs: 6000 });
    return true;
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
        ? "本机不开终端窗口，命令已复制"
        : "本机不开终端窗口，请手动复制以下命令"
      : copied
        ? toasts.failureCopied
        : toasts.failureNotCopied;
    showActionFailureToast(
      headline,
      `${String(err)}\n到远端 [${origin}] 的 ssh 终端粘贴执行：\n${cmd}`,
      { level: "info", durationMs: 10000 },
    );
    return false;
  }
}

/** 一键 resume 远端会话：拉起成功 toast 告知；失败回退复制命令。 */
export async function runRemoteResume(
  origin: string,
  sid: string,
  cwd: string,
  launcher: string,
  mods: LaunchModifiers = {}, // R03：正交修饰 bag（configDir/accountName/modelOverride），见 launch-plan.ts
  // Phase G（branch-anywhere）：返回值从 `void` 改成 `boolean`，与 `runRemoteResumeTmux`
  // 对齐（那边的头注逐字记着为什么要有返回值：account-ux 那次把「走到了第⑤步」当成
  // 「已 resume」）。既有调用点忽略返回值 ⇒ 行为逐字不变。
): Promise<boolean> {
  let cmd: string;
  try {
    const { ctx, plan } = planResumeDirect(sid, cwd, launcher, mods);
    cmd = await renderLaunchCommand(origin, ctx, plan);
  } catch (err) {
    showActionFailureToast("无法构造 resume 命令", String(err));
    return false;
  }
  return invokeLaunchOrCopyFallback(origin, cmd, {
    success: "已拉起远端 resume",
    successDetail: `新终端窗口正在连接 [${origin}] 并 resume 该会话。`,
    failureCopied: "拉起失败，已复制 resume 命令",
    failureNotCopied: "拉起失败，请手动复制以下命令",
  });
}

/** F52：tmux 版 resume——在远端 tmux 会话 `<sid8>-cc` 里幂等 resume Claude;失败回退复制命令。
 *
 *  @returns 是否**真的把终端拉起来了**。false = 命令构造失败 / `launch_remote_terminal` 失败
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
  mods: LaunchModifiers = {}, // R03：正交修饰 bag（configDir/accountName/modelOverride），见 launch-plan.ts
): Promise<boolean> {
  let cmd: string;
  try {
    const { ctx, plan } = planResumeTmux(sid, cwd, launcher, name, mods);
    cmd = await renderLaunchCommand(origin, ctx, plan);
  } catch (err) {
    showActionFailureToast("无法构造 tmux resume 命令", String(err));
    return false;
  }
  return invokeLaunchOrCopyFallback(origin, cmd, {
    success: "已拉起 tmux resume",
    successDetail: `新终端窗口正在连接 [${origin}] 并在 tmux 会话里 resume 该会话。`,
    failureCopied: "拉起失败，已复制 tmux resume 命令",
    failureNotCopied: "拉起失败，请手动复制以下命令",
  });
}

/** U8a-2c-1：把 `send-keys` 那半边交给**远端 daemon**（`control/launch.rs`，`mode:"send-into"`）。
 *
 *  @returns `true` = 载荷**真的键入了** ⇒ 终端只需 `attach`；`false` = 回落到今天那条整串。
 *
 *  # 任何一步不顺都回落，而且回落后**行为逐字不变**
 *
 *  拿不到控制通道（daemon 不在场 / 长连接未握手）· daemon 回报未键入（会话已不存在）·
 *  载荷渲染被后端拒 · IPC 本身异常 —— 一律 `false`，调用方照今天那条
 *  `send-keys …; attach …` 整串走。**所以这条切换在没有 daemon 的远端上是零影响的。**
 *
 *  ⚠ **诚实登记一处 fail-open**：载荷渲染被 Rust 拒（非法 configDir / 会裂的 arg）时这里也回落，
 *  而兜底渲染器（TS）对同样的输入**未必拒**。那不是本件引入的 —— 这一格今天**根本不经
 *  Rust 渲染**（`renderLaunchCommand` 对 send-into 恒走 `renderFallback`），所以「回落=今天」
 *  才是零行为变化的那个选择。要把它收成 fail-closed 得连兜底渲染器一起收 ⇒ U8c-3。
 *
 *  ⚠ **绝不 toast** 回落 —— 用户看不到区别（两条路都把会话就地 resume 起来了），
 *  弹一个「降级了」的提示只会制造噪音。理由走 `console.debug`，同 `renderCliViaBackend`。 */
async function sendIntoViaDaemon(
  origin: string,
  name: string,
  plan: LaunchPlan,
): Promise<boolean> {
  try {
    const payload = await commands.render_launch_payload({
      req: buildPayloadRenderRequest(plan),
    });
    const res = await commands.daemon_send_into({ req: { origin, name, payload } });
    if (!res.typed) {
      console.debug(`[U8a-2c-1] send-into 回落到整串：${res.reason ?? "daemon 未给理由"}`);
    }
    return res.typed;
  } catch (e) {
    console.debug(`[U8a-2c-1] send-into 回落到整串（通道异常）：${String(e)}`);
    return false;
  }
}

/** F03：往一个**已存在的空 tmux**（idle-tmux：claude 已退、只剩交互 shell 的 `<sid8>-cc`）就地
 *  resume——send-keys 载荷 + attach，复用原会话名（不产孤儿，治 #76）。签名/返回值与
 *  `runRemoteResumeTmux` 对齐：true=真拉起来了；false=命令构造失败/拉起失败（已回退剪贴板）。
 *  **`tryRenderCli` 对这类 plan（`mode==="send-into"`）恒返回 `ok:false`**——shared/ccm 没有就地
 *  复用能力，本函数因此恒走兜底渲染器（诚实放弃，见 F03 计划 §2「#76 防线」）。 */
export async function runRemoteResumeIntoExistingTmux(
  origin: string,
  sid: string,
  name: string,
  launcher: string,
  mods: LaunchModifiers = {}, // R03：正交修饰 bag（configDir/accountName/modelOverride），见 launch-plan.ts
): Promise<boolean> {
  let cmd: string;
  let viaDaemon = false;
  try {
    const { ctx, plan } = planResumeIntoExistingTmux(sid, name, launcher, mods);
    // ★ U8a-2c-1：**先试 daemon**。这一格今天的整串是
    //   `tmux send-keys -t '=name:' '<载荷>' Enter; tmux attach -t '=name:'` —— 两半干干净净：
    //   `send-keys` 交给远端 `control/`，`attach` **必须**留在用户自己的终端（§1.3）。
    //   拿不到控制通道 / daemon 回报未键入 ⇒ 原样回落到整串（**行为逐字不变**），
    //   所以这条切换在没有 daemon 的远端上是零影响的。
    const sent = await sendIntoViaDaemon(origin, name, plan);
    if (sent) {
      const attach = planAttach(name);
      cmd = await renderLaunchCommand(origin, attach.ctx, attach.plan);
      viaDaemon = true;
    } else {
      cmd = await renderLaunchCommand(origin, ctx, plan);
    }
  } catch (err) {
    showActionFailureToast("无法构造就地 resume 命令", String(err));
    return false;
  }
  return invokeLaunchOrCopyFallback(origin, cmd, {
    success: "已在原 tmux 就地 resume",
    successDetail: viaDaemon
      ? `远端 daemon 已在 tmux 会话「${name}」里就地 resume（复用、不新建），新终端窗口正在连接 [${origin}] 接上它。`
      : `新终端窗口正在连接 [${origin}] 并在原 tmux 会话「${name}」里 resume 该会话（复用、不新建）。`,
    failureCopied: "拉起失败，已复制就地 resume 命令",
    failureNotCopied: "拉起失败，请手动复制以下命令",
  });
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
  mods: LaunchModifiers = {}, // R03：正交修饰 bag（configDir/accountName/modelOverride），见 launch-plan.ts
): Promise<void> {
  await runRemoteLauncher(
    origin,
    cwd,
    deriveTmuxName(cwd),
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
  mods: LaunchModifiers = {}, // R03：正交修饰 bag（configDir/accountName/modelOverride），见 launch-plan.ts
): Promise<void> {
  let cmd: string;
  try {
    const { ctx, plan } = planLauncher(cwd, tmuxName, command, mods);
    cmd = await renderLaunchCommand(origin, ctx, plan);
  } catch (err) {
    showActionFailureToast("无法构造 launcher 命令", String(err));
    return;
  }
  await invokeLaunchOrCopyFallback(origin, cmd, {
    success: "已拉起「开新 Claude」",
    successDetail: `新终端窗口正在连接 [${origin}] 并在 tmux 会话「${tmuxName}」里启动 Claude。`,
    failureCopied: "拉起失败，已复制命令",
    failureNotCopied: "拉起失败，请手动复制以下命令",
  });
}

/** F51：一键 attach 到远端 tmux 会话:拉起 `ssh -t … tmux attach -t <名>`;失败回退复制命令。
 *  ccm 已装且能力齐全时走 CLI 渲染器（`ccm attach <名>`，与兜底输出逐字同构，无 #76 歧义）。 */
export async function runRemoteAttach(origin: string, name: string): Promise<void> {
  let cmd: string;
  try {
    const { ctx, plan } = planAttach(name);
    cmd = await renderLaunchCommand(origin, ctx, plan);
  } catch (err) {
    showActionFailureToast("无法构造 attach 命令", String(err));
    return;
  }
  await invokeLaunchOrCopyFallback(origin, cmd, {
    success: "已拉起 tmux attach",
    successDetail: `新终端窗口正在连接 [${origin}] 并 attach 到 tmux 会话「${name}」。`,
    failureCopied: "拉起失败，已复制 attach 命令",
    failureNotCopied: "拉起失败，请手动复制以下命令",
  });
}
