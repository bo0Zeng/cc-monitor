/**
 * 帧命令 `launch-render-cli` 的上线形状（TS 侧）：那台机器要跑的那一行 `ccm …`（经 `launch-render.ts` 问那台后端）。
 *
 * Rust 对侧是 `src/backend/control/launch_render/wire.rs` 的 `CliRenderRequest`，带 `deny_unknown_fields`
 * —— **多送一个字段会被拒**。这是一份手写镜像，`tests/frontend/ui/launch-cli-wire.vitest.ts` 读 Rust 源码逐字段比对。
 */
export type CliWireAction =
  | { kind: "new" }
  | { kind: "resume"; sid: string }
  | { kind: "attach"; name: string };

/** `send_into: true` = 往已有的空 tmux 会话里就地 resume（外层包一层，键入直路那一行）。 */
export type CliWireContainer = { kind: "none" } | { kind: "tmux"; name: string; send_into: boolean };

/** 用哪个号：跟随 · 账号 0 · 用户点名（生成的类型，那台后端判）。 */
export type { AccountAsk as CliWireAccount } from "./generated/AccountAsk";
import type { AccountAsk } from "./generated/AccountAsk";
import type { LaunchedAccount } from "./generated/LaunchedAccount";
import type { LaunchContext } from "./launch-types.ts";
import { defaultLauncherOf } from "./agent-profile.ts";

export interface CliRenderRequest {
  agent: string;
  action: CliWireAction;
  container: CliWireContainer;
  cwd: string | null;
  account: AccountAsk;
  ccmSid: string | null;
  model: string | null;
  /** 那台的模型偏好表（原值，号 → 模型）：判出来的号在表里 ⇒ 用那一条。 */
  models: Record<string, string>;
  launcher: string;
  defaultLauncher: string;
}

/** `launch-render-cli` 的成品：那一行 ＋ 实际用的号（账号 0 / 不指定 ⇒ `null`）。 */
export interface CliRendered {
  cmd: string;
  account: LaunchedAccount | null;
}

/** 空白 ⇒ 那一家的默认启动器（没配就是没配，不是一个判定）。字符集只在后端判。 */
function launcherOrDefault(agent: string, launcher: string): string {
  return launcher.trim() || defaultLauncherOf(agent);
}

/**
 * 把意图摊成 `launch-render-cli` 的上线形状 —— 「起什么」交给那台后端的唯一住址。
 * 入库夹具 `cli-golden.json` 的 `req` 由同一个函数现产（`launch-cli-golden.ts`），Rust 侧拿生产 wire 类型反序列化、跑生产命令比 `out`。
 */
export function buildCliRenderRequest(ctx: LaunchContext): CliRenderRequest {
  return {
    agent: ctx.agent,
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
    account: ctx.account,
    ccmSid: ctx.ccmSid ?? null,
    model: null,
    models: ctx.models,
    launcher: launcherOrDefault(ctx.agent, ctx.launcherOverride ?? ""),
    defaultLauncher: defaultLauncherOf(ctx.agent),
  };
}
