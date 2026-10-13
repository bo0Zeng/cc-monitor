/**
 * 帧命令 `launch-render-cli` 的上线形状（TS 侧）：那台机器要跑的那一行 `ccm …`（经 `launch-render.ts` 问那台后端）。
 *
 * 入参 · 回包的类型都从 Rust 生成（`src/backend/control/launch_render/wire.rs` 的 `CliRenderRequest`，带 `deny_unknown_fields`；
 * 回包 `Rendered`），两端形状由握手指纹核过。本文件只留「把意图摊成那一份入参」。
 */
import type { CliRenderRequest } from "./generated/CliRenderRequest";
import type { LaunchContext } from "./launch-types.ts";

export type { CliRenderRequest };
/** `launch-render-cli` 的成品：那一行 ＋ 实际用的号（账号 0 / 不指定 ⇒ `null`）。 */
export type { Rendered as CliRendered } from "./generated/Rendered";

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
