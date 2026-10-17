/**
 * 帧命令 `launch-render-cli` 的入参（界面已不发这一问，起会话一律 `session-open`）：只给 `cli-golden.json` 的期望表与 e2e 驱动现产入参，
 * 随那条命令一起删。入参类型从 Rust 生成（`src/backend/control/launch_render/wire.rs` 的 `CliRenderRequest`，带 `deny_unknown_fields`）。
 */
import type { CliRenderRequest } from "../../src/frontend/ui/generated/CliRenderRequest";
import type { LaunchContext } from "./launch-types.ts";

export type { CliRenderRequest };

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
        ? { kind: "tmux", name: ctx.container.name, send_into: false }
        : { kind: "none" },
    cwd: ctx.cwd,
    account: ctx.account,
    ccmSid: ctx.ccmSid ?? null,
    model: null,
  };
}
