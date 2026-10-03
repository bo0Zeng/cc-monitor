/**
 * 起会话的几种意图 → [`LaunchContext`]。每个函数对应一条起会话路径（远端 resume 直连 / tmux / 就地 · 开新 · 接回）。
 * 形状（sid · 会话名 · 账号名 · 目录）一个都不在这里判：规则只有后端一份，渲染那一跳判、判不过拒。
 * 每一条都说是哪一家（`agent`）：怎么 resume、能不能选号由那台后端按它渲；启动器缺省取那一家的默认启动器。
 */
import { DEFAULT_AGENT, defaultLauncherOf } from "./agent-profile.ts";
import type { LaunchAccount, LaunchContext, LaunchModifiers } from "./launch-types.ts";

/** 有目录 ⇒ 具名账号（名字说得出才带）；没有 ⇒ 账号 0。 */
function accountOf(configDir?: string, name?: string): LaunchAccount {
  return configDir ? { kind: "account", name, configDir } : { kind: "base" };
}

/**
 * 设置里配的 resume 命令（`cc` / `cct` 这类）是给默认那一家配的 ⇒ 只用在那一家的会话上；
 * 别的那一家用它自己的默认启动器（交回空串 ＝ 没配）。
 */
export function configuredLauncherFor(agent: string, configured: string): string {
  return agent === DEFAULT_AGENT ? configured : "";
}

/** 无容器（直连），resume 到当前登录 shell。 */
export function planResumeDirect(
  agent: string,
  sid: string,
  cwd: string,
  launcher = defaultLauncherOf(agent),
  mods: LaunchModifiers = {},
): LaunchContext {
  return {
    agent,
    action: { kind: "resume", sid },
    container: { kind: "none" },
    cwd: cwd.trim() || null,
    account: accountOf(mods.configDir, mods.accountName),
    launcherOverride: launcher,
    ccmSid: undefined,
    modelOverride: mods.modelOverride,
  };
}

/** 新建 tmux 会话，resume 进去。`name` 必填：省略就意味着一个不做撞名避让的默认值。 */
export function planResumeTmux(
  agent: string,
  sid: string,
  cwd: string,
  launcher: string | undefined,
  name: string,
  mods: LaunchModifiers = {},
): LaunchContext {
  return {
    agent,
    action: { kind: "resume", sid },
    container: { kind: "tmux", name, mode: "create" },
    cwd: cwd.trim() || null,
    account: accountOf(mods.configDir, mods.accountName),
    launcherOverride: launcher ?? defaultLauncherOf(agent),
    ccmSid: sid, // 自建 resume 会话打完整 sid，供精确找回那个会话
    modelOverride: mods.modelOverride,
  };
}

/** 往已存在的空 tmux 会话就地 resume（键入那一行，不新建会话）。 */
export function planResumeIntoExistingTmux(
  agent: string,
  sid: string,
  name: string,
  launcher = defaultLauncherOf(agent),
  mods: LaunchModifiers = {},
): LaunchContext {
  return {
    agent,
    action: { kind: "resume", sid },
    container: { kind: "tmux", name, mode: "send-into" },
    cwd: null,
    account: accountOf(mods.configDir, mods.accountName),
    launcherOverride: launcher,
    ccmSid: undefined, // 复用的会话建时已打过标，不重设
    modelOverride: mods.modelOverride,
  };
}

/** 「在这台机开新会话」—— 新建 tmux 会话，起全新会话。 */
export function planLauncher(
  agent: string,
  cwd: string,
  tmuxName: string,
  command = defaultLauncherOf(agent),
  mods: LaunchModifiers = {},
): LaunchContext {
  return {
    agent,
    action: { kind: "new" },
    container: { kind: "tmux", name: tmuxName.trim(), mode: "create" },
    cwd: cwd.trim() || null,
    account: accountOf(mods.configDir, mods.accountName),
    launcherOverride: command,
    ccmSid: undefined,
    modelOverride: mods.modelOverride,
  };
}

/** 接回一个已存在的 tmux 会话，不启动任何东西 ⇒ 不收修饰（不起 agent 进程，令牌没有读者）。`agent` 是那个会话的那一家。 */
export function planAttach(agent: string, name: string): LaunchContext {
  return {
    agent,
    action: { kind: "attach", name },
    container: { kind: "tmux", name, mode: "attach-only" },
    cwd: null,
    account: { kind: "base" },
    launcherOverride: undefined,
    ccmSid: undefined,
  };
}
