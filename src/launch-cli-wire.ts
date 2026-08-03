/**
 * U8c-2c-2：`render_ccm_launch` 的上线形状（TS 侧）。
 *
 * Rust 对侧是 `src-tauri/src/launch_cli_cmd.rs` 的 `CliRenderRequest`/`CliRenderResponse`，
 * 那边带 `deny_unknown_fields` —— **多送一个字段会被拒**，不静默吞。
 *
 * ⚠ 这是一份**手写镜像**（不是 ts-rs 生成的）。保证它与 Rust 一致的是
 * `src/launch-cli-wire.vitest.ts`：它读 Rust 源码、逐字段比对。
 */
export type CliWireAction =
  | { kind: "new" }
  | { kind: "resume"; sid: string }
  | { kind: "attach"; name: string };

export type CliWireContainer =
  | { kind: "none" }
  | { kind: "tmux"; name: string; send_into: boolean };

/** `name` 缺失 = 只有 configDir 没有名字 ⇒ 说不出 `--account` ⇒ §35 短路。 */
export type CliWireAccount = { kind: "base" } | { kind: "account"; name: string | null };

export interface CliRenderRequest {
  isSsh: boolean;
  /** `null` = 未装 ccm。 */
  caps: string[] | null;
  action: CliWireAction;
  container: CliWireContainer;
  cwd: string | null;
  account: CliWireAccount;
  ccmSid: string | null;
  model: string | null;
  /** 已 sanitize 的 launcher（sanitize 仍在 TS）。 */
  launcher: string;
  defaultLauncher: string;
}

/** 与 TS `CliRenderResult` 同构：`ok:false` 带**降级理由**，不是错误。 */
export interface CliRenderResponse {
  ok: boolean;
  cmd: string | null;
  reason: string | null;
}
