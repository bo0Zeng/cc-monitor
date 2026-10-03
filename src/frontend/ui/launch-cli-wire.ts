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

/** 具名账号：`name` 说得出 ⇒ `--account`；只有 `configDir` ⇒ `--account-dir`。 */
export type CliWireAccount = { kind: "base" } | { kind: "account"; name: string | null; configDir: string | null };

export interface CliRenderRequest {
  agent: string;
  action: CliWireAction;
  container: CliWireContainer;
  cwd: string | null;
  account: CliWireAccount;
  ccmSid: string | null;
  model: string | null;
  launcher: string;
  defaultLauncher: string;
}
