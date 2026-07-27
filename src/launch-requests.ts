/**
 * F03（unify-launch）：「legacy 请求 → LaunchContext/LaunchPlan」翻译层。每个函数对应今天
 * `remote-launch.ts` 里一个 builder 的校验/意图构造逻辑，逐字保留其校验顺序与错误文案
 * （调用方 toast 依赖这些文案）。`remote-launch.ts` 的 builder 改造后只剩「调这里 + 交渲染器」。
 */
import { AGENT_PROFILE } from "./agent-profile.ts";
import { isValidSessionId, isValidTmuxName, isValidNewTmuxName } from "./shell-quote.ts";
import { buildLaunchPlan } from "./launch-plan.ts";
import type { LaunchAccount, LaunchContext, LaunchPlan } from "./launch-plan.ts";

export interface LaunchPlanBuild {
  ctx: LaunchContext;
  plan: LaunchPlan;
}

/** F05：触发条件仍是 `configDir` 单独非空（同 F03 原行为，`remote-launch.test.ts` 的老式
 *  直调路径只传 `configDir` 不传名字，必须继续正确触发兜底渲染器的 env 注入）；`name` 是
 *  可选增强——传了就线通进 IR 供 CLI 渲染器用，没传时 `LaunchAccount.name` 是 `undefined`，
 *  `ACCOUNT_DIMENSION.cliFlags` 会诚实地对这种情形返回 `null`（强制走兜底），而不是把整个
 *  账号状态错误地降级成 `base`（那会连兜底渲染器的 env 注入也漏掉，是真回归）。 */
function accountOf(configDir?: string, name?: string): LaunchAccount {
  return configDir ? { kind: "account", name, configDir } : { kind: "base" };
}

/** 对应 `buildResumeDirectCmd`：无容器（直连），resume 到当前登录 shell。 */
export function planResumeDirect(
  sid: string,
  cwd: string,
  launcher = AGENT_PROFILE.defaultLauncher,
  configDir?: string,
  accountName?: string,
): LaunchPlanBuild {
  if (!isValidSessionId(sid)) {
    throw new Error(`非法 sessionId（拒绝拼入命令）: ${JSON.stringify(sid)}`);
  }
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid },
    container: { kind: "none" },
    cwd: cwd.trim() || null,
    account: accountOf(configDir, accountName),
    launcherOverride: launcher,
    ccmSid: undefined,
  };
  return { ctx, plan: buildLaunchPlan(ctx) };
}

/** 对应 `buildResumeTmuxCmd`：新建/幂等接回 tmux，resume 进去。 */
export function planResumeTmux(
  sid: string,
  cwd: string,
  launcher = AGENT_PROFILE.defaultLauncher,
  name?: string,
  configDir?: string,
  accountName?: string,
): LaunchPlanBuild {
  if (!isValidSessionId(sid)) {
    throw new Error(`非法 sessionId（拒绝拼入命令）: ${JSON.stringify(sid)}`);
  }
  const tmuxName = name ?? `cc-${sid.slice(0, 8)}`;
  if (!/^[A-Za-z0-9_][A-Za-z0-9_-]*$/.test(tmuxName)) {
    throw new Error(`非法 tmux 会话名（拒绝拼入命令）: ${JSON.stringify(tmuxName)}`);
  }
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid },
    container: { kind: "tmux", name: tmuxName, nameQuoting: "raw", mode: "create-or-attach" },
    cwd: cwd.trim() || null,
    account: accountOf(configDir, accountName),
    launcherOverride: launcher,
    ccmSid: sid, // #72：自建 resume 会话打完整 sid，供 findClaudeTmux 精确命中
  };
  return { ctx, plan: buildLaunchPlan(ctx) };
}

/** 对应 `buildResumeIntoExistingTmuxCmd`：往已存在的 idle tmux 就地送键，不 new-session。 */
export function planResumeIntoExistingTmux(
  sid: string,
  name: string,
  launcher = AGENT_PROFILE.defaultLauncher,
  configDir?: string,
  accountName?: string,
): LaunchPlanBuild {
  if (!isValidSessionId(sid)) {
    throw new Error(`非法 sessionId（拒绝拼入命令）: ${JSON.stringify(sid)}`);
  }
  if (!/^[A-Za-z0-9_][A-Za-z0-9_-]*$/.test(name)) {
    throw new Error(`非法 tmux 会话名（拒绝拼入命令）: ${JSON.stringify(name)}`);
  }
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid },
    container: { kind: "tmux", name, nameQuoting: "raw", mode: "send-into" },
    cwd: null,
    account: accountOf(configDir, accountName),
    launcherOverride: launcher,
    ccmSid: undefined, // 复用会话已在建时打过标，不重设（同今天行为）
  };
  return { ctx, plan: buildLaunchPlan(ctx) };
}

/** 对应 `buildLauncherCmd`：「在这台机开新 Claude」——新建/幂等接回 tmux，起全新会话。 */
export function planLauncher(
  cwd: string,
  tmuxName: string,
  command = AGENT_PROFILE.defaultLauncher,
  configDir?: string,
  accountName?: string,
): LaunchPlanBuild {
  const name = tmuxName.trim();
  if (!isValidNewTmuxName(name)) {
    throw new Error(`非法 tmux 会话名（拒绝拼入命令）: ${JSON.stringify(name)}`);
  }
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "new" },
    container: { kind: "tmux", name, nameQuoting: "quoted", mode: "create-or-attach" },
    cwd: cwd.trim() || null,
    account: accountOf(configDir, accountName),
    launcherOverride: command,
    ccmSid: undefined, // 今天就不设——已知 F04 缺口，本次原样保留、不顺手"修一半"
  };
  return { ctx, plan: buildLaunchPlan(ctx) };
}

/** 对应 `buildAttachCmd`：接回一个已存在的 tmux 会话，不启动任何东西。 */
export function planAttach(name: string): LaunchPlanBuild {
  if (!isValidTmuxName(name)) {
    throw new Error(`非法 tmux 会话名(拒绝拼入命令): ${JSON.stringify(name)}`);
  }
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "attach", name },
    container: { kind: "tmux", name, nameQuoting: "quoted", mode: "attach-only" },
    cwd: null,
    account: { kind: "base" },
    launcherOverride: undefined,
    ccmSid: undefined,
  };
  return { ctx, plan: buildLaunchPlan(ctx) };
}
