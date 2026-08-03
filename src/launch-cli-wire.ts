/**
 * U8c-2c-2：`render_ccm_launch` 的上线形状（TS 侧）。
 *
 * Rust 对侧是 `src-tauri/src/backend/control/launch_wire.rs` 的 `CliRenderRequest`/`CliRenderResponse`，
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

/** U8a-2c-pre：兜底那支 `container:"none"` 的载荷渲染入参。 */
export type WireEnvOp =
  | { kind: "export-config-dir"; value: string }
  | { kind: "export-model"; value: string }
  | { kind: "unset-config-dir" }
  | { kind: "unset-nested-env" };

/** U8a-2c-1：`daemon_send_into` 的上线形状。Rust 对侧是
 *  `src-tauri/src/backend/control/daemon_launch.rs`。
 *  ⚠ **没有 `mode` 字段** —— 这条通道只会说 `send-into`（`create-or-attach` 会新建会话 =
 *  issue #76 的失管会话形态），mode 由 Rust 侧写死并有判据钉住。 */
export interface SendIntoRequest {
  origin: string;
  /** tmux 会话名（裸名；`=name:` 的精确匹配形态由 daemon 侧加）。 */
  name: string;
  /** 内层载荷（`env 前缀 → argv`），由 `render_launch_payload` 产出。 */
  payload: string;
}

/** `typed:false` 时 `reason` 必有值 —— 那是回落到「整串走终端」的唯一线索。 */
export interface SendIntoResponse {
  typed: boolean;
  reason: string | null;
}

export interface PayloadRenderRequest {
  env: WireEnvOp[];
  cwd: string | null;
  /** 已 sanitize 的 launcher。 */
  launcher: string;
  args: string[];
  /** 嵌套 env 键表（`AGENT_PROFILE.nestedEnvVars`）—— `unset-nested-env` 用。 */
  nestedEnv: string[];
  /** `( <prelude>; exec <inner> )` 包裹（§39 给 F04 rbind 留的槽）。今天恒空。
   *  ⚠ 复盘补的：初版 wire 没有它 ⇒ 后端静默丢。 */
  wrap: { order: number; prelude: string }[];
}
