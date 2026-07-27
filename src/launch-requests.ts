/**
 * F03（unify-launch）：「legacy 请求 → LaunchContext/LaunchPlan」翻译层。每个函数对应今天
 * `remote-launch.ts` 里一个 builder 的校验/意图构造逻辑，逐字保留其校验顺序与错误文案
 * （调用方 toast 依赖这些文案）。`remote-launch.ts` 的 builder 改造后只剩「调这里 + 交渲染器」。
 */
import { AGENT_PROFILE } from "./agent-profile.ts";
import { isValidSessionId, isValidTmuxName, isValidNewTmuxName } from "./shell-quote.ts";
import { buildLaunchPlan } from "./launch-plan.ts";
import type { LaunchAccount, LaunchAction, LaunchContext, LaunchPlan } from "./launch-plan.ts";

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

/** F06：本地（Windows）路径的 `LaunchContext` 构造——本地会话无账号隔离/tmux 概念，
 *  `container`/`account` 恒定；`launcher` 不经这层（本地路径的 launcher 是裸字符串直传 Rust，
 *  由 Rust 自己校验/拼接 PowerShell 命令，不是本 IR 的渲染对象）。
 *
 *  校验 `sid`（`action.kind==="resume"` 时）——同其余 4 个 `planXxx` 早就有的
 *  `isValidSessionId` 检查，本地路径此前唯一缺失这一层，只靠 Rust 侧 `build_local_ps_command`
 *  兜底校验。补齐后本地/远端 resume 在"sid 校验早于任何 IPC 往返"这条规矩上一致（见
 *  `features/F06-local-path-ir.md` §3.2 实现期修正）。
 *
 *  `plan.env` 会因 `NESTED_ENV_RESET_DIMENSION` 恒非空（action 是 new/resume 就触发）——
 *  **故意不消费**：本地场景的嵌套 env 污染保护已经在 `lib.rs::scrub_env_vars`（进程启动期
 *  一次性清洗）做完，调用方不应该、也不需要再从这里的 `plan.env` 拼 PowerShell env-unset 语句
 *  （见同一节 §0 的证据链）。 */
export function planLocal(action: LaunchAction, cwd: string | null): LaunchPlanBuild {
  if (action.kind === "resume" && !isValidSessionId(action.sid)) {
    throw new Error(`非法 sessionId（拒绝拼入命令）: ${JSON.stringify(action.sid)}`);
  }
  const ctx: LaunchContext = {
    transport: { kind: "local" },
    action,
    container: { kind: "none" },
    cwd,
    account: { kind: "base" },
    launcherOverride: undefined,
    ccmSid: undefined,
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
