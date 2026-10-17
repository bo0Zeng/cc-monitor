/**
 * 起会话的几种意图 → [`LaunchContext`]。每个函数对应一条起会话路径（远端 resume 直连 / tmux / 就地 · 开新 · 接回）。
 * 形状（sid · 会话名 · 账号名 · 目录）一个都不在这里判：规则只有后端一份，渲染那一跳判、判不过拒。
 * 每一条都说是哪一家（`agent`）：怎么 resume、能不能选号由那台后端按它渲；启动命令由那台照自己的偏好定。
 * 界面已不用（起会话一律 `session-open`）：只给 `cli-golden.json` 的期望表与 e2e 驱动产 `launch-render-cli` 的入参，随那条命令一起删。
 */
import type { LaunchContext, LaunchModifiers } from "./launch-types.ts";

/** 无容器（直连），resume 到当前登录 shell。 */
export function planResumeDirect(
  agent: string,
  sid: string,
  cwd: string,
  mods: LaunchModifiers = {},
): LaunchContext {
  return {
    agent,
    action: { kind: "resume", sid },
    container: { kind: "none" },
    cwd: cwd.trim() || null,
    account: mods.account ?? { kind: "follow" },
    ccmSid: undefined,
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
    ccmSid: undefined,
  };
}
