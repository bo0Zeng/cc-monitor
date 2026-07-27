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
import { invoke } from "@tauri-apps/api/core";
import {
  planResumeDirect,
  planResumeTmux,
  planResumeIntoExistingTmux,
  planLauncher,
  planAttach,
} from "./launch-requests";
import { renderFallback } from "./launch-render-fallback";
import { canRenderCli, renderCli } from "./launch-render-cli";
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
    if (canRenderCli(plan, ctx, probe)) return renderCli(plan, ctx);
  }
  return renderFallback(plan);
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
async function invokeLaunchOrCopyFallback(
  origin: string,
  cmd: string,
  toasts: LaunchToasts,
): Promise<boolean> {
  try {
    await invoke("launch_remote_terminal", { origin, remoteCmd: cmd });
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
    showActionFailureToast(
      copied ? toasts.failureCopied : toasts.failureNotCopied,
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
  configDir?: string, // A4：非空 → 该会话用指定账号 resume（CLAUDE_CONFIG_DIR 注入）
  accountName?: string, // F05：与 configDir 成对——线通进 LaunchContext，供 CLI 渲染器吐 --account <名>
): Promise<void> {
  let cmd: string;
  try {
    const { ctx, plan } = planResumeDirect(sid, cwd, launcher, configDir, accountName);
    cmd = await renderLaunchCommand(origin, ctx, plan);
  } catch (err) {
    showActionFailureToast("无法构造 resume 命令", String(err));
    return;
  }
  await invokeLaunchOrCopyFallback(origin, cmd, {
    success: "已拉起远端 resume",
    successDetail: `新终端窗口正在连接 [${origin}] 并 resume 该会话。`,
    failureCopied: "拉起失败，已复制 resume 命令",
    failureNotCopied: "拉起失败，请手动复制以下命令",
  });
}

/** F52：tmux 版 resume——在远端 tmux 会话 `cc-<sid8>` 里幂等 resume Claude;失败回退复制命令。
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
  name?: string,
  configDir?: string, // A4：非空 → 该会话用指定账号 resume（CLAUDE_CONFIG_DIR 注入）
  accountName?: string, // F05：与 configDir 成对——线通进 LaunchContext，供 CLI 渲染器吐 --account <名>
): Promise<boolean> {
  let cmd: string;
  try {
    const { ctx, plan } = planResumeTmux(sid, cwd, launcher, name, configDir, accountName);
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

/** F03：往一个**已存在的空 tmux**（idle-tmux：claude 已退、只剩交互 shell 的 `cc-<sid8>`）就地
 *  resume——send-keys 载荷 + attach，复用原会话名（不产孤儿，治 #76）。签名/返回值与
 *  `runRemoteResumeTmux` 对齐：true=真拉起来了；false=命令构造失败/拉起失败（已回退剪贴板）。
 *  **`canRenderCli` 对这类 plan（`mode==="send-into"`）恒返回 false**——shared/ccm 没有就地
 *  复用能力，本函数因此恒走兜底渲染器（诚实放弃，见 F03 计划 §2「#76 防线」）。 */
export async function runRemoteResumeIntoExistingTmux(
  origin: string,
  sid: string,
  name: string,
  launcher: string,
  configDir?: string, // A4：非空 → 该会话用指定账号 resume；空 → 基座（builder 会 unset 残留 env）
  accountName?: string, // F05：与 configDir 成对——线通进 LaunchContext，供 CLI 渲染器吐 --account <名>
): Promise<boolean> {
  let cmd: string;
  try {
    const { ctx, plan } = planResumeIntoExistingTmux(sid, name, launcher, configDir, accountName);
    cmd = await renderLaunchCommand(origin, ctx, plan);
  } catch (err) {
    showActionFailureToast("无法构造就地 resume 命令", String(err));
    return false;
  }
  return invokeLaunchOrCopyFallback(origin, cmd, {
    success: "已在原 tmux 就地 resume",
    successDetail: `新终端窗口正在连接 [${origin}] 并在原 tmux 会话「${name}」里 resume 该会话（复用、不新建）。`,
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
  configDir?: string, // A4：非空 → 新会话用指定账号启动
  accountName?: string, // F05：与 configDir 成对——线通进 LaunchContext，供 CLI 渲染器吐 --account <名>
): Promise<void> {
  await runRemoteLauncher(
    origin,
    cwd,
    deriveTmuxName(cwd),
    command || AGENT_PROFILE.defaultLauncher,
    configDir,
    accountName,
  );
}

/** F53：「在这台机开新 Claude」——在远端 tmux 会话里启动全新 Claude;失败回退复制命令。 */
export async function runRemoteLauncher(
  origin: string,
  cwd: string,
  tmuxName: string,
  command: string,
  configDir?: string, // A4：非空 → 新会话用指定账号启动（CLAUDE_CONFIG_DIR 注入）
  accountName?: string, // F05：与 configDir 成对——线通进 LaunchContext，供 CLI 渲染器吐 --account <名>
): Promise<void> {
  let cmd: string;
  try {
    const { ctx, plan } = planLauncher(cwd, tmuxName, command, configDir, accountName);
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
