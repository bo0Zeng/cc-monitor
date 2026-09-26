/**
 * U8c-2c-2：`render_ccm_launch` 的上线形状（TS 侧）。
 *
 * Rust 对侧是 `src/bridge/src/backend/control/launch_wire.rs` 的 `CliRenderRequest`/`CliRenderResponse`，
 * 那边带 `deny_unknown_fields` —— **多送一个字段会被拒**，不静默吞。
 *
 * ⚠ 这是一份**手写镜像**（不是 ts-rs 生成的）。保证它与 Rust 一致的是
 * `tests/launch-cli-wire.vitest.ts`：它读 Rust 源码、逐字段比对。
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

/** U8a-2c-pre：兜底那支 `container:"none"` 的载荷渲染入参。
 *
 *  ⚠ 与 TS `launch-plan.ts::EnvOp` **同名同序**（那边是 IR，这边是上线形状）。
 *  Rust 对侧 `launch_wire.rs::WireEnvOp` 带 `deny_unknown_fields` ⇒ 少一个变体
 *  就是一次「反序列化失败 → 静默走另一条渲染路」，所以两边必须一起加。 */
export type WireEnvOp =
  | { kind: "export-config-dir"; value: string }
  | { kind: "export-model"; value: string }
  /** `设计/80 §8` 步 1：启动期令牌。Rust 渲染侧对 `[0-9a-f]{32}` 之外的值 fail-closed 拒。 */
  | { kind: "export-rbind-token"; value: string }
  /** 〔RL1〕中转地址。Rust 渲染侧只收 `relay_base_url_in` 产得出的那一形，别的 fail-closed 拒。 */
  | { kind: "export-relay-base-url"; value: string }
  | { kind: "unset-config-dir" }
  | { kind: "unset-nested-env" };

// 〔C4e · 第四波 4C〕这里原来住着 `backend_send_into`〔散文墓碑〕的上线形状（`SendIntoRequest` / `SendIntoResponse`）：
//   就地 resume 那一次键入改由界面经通道直接问那台机器的后端（`src/tmux-control.ts::sendInto`，
//   `launch{mode:"send-into"}`），F14 那条「能不能回落」的三态也随之住在那里（`SendIntoOutcome`）。

/** `设计/90 §4 E`：**外层容器那一层**的上线形状。Rust 对侧是 `launch_wire.rs::WireTmuxOuter`
 *  （带 `deny_unknown_fields`）。
 *
 *  ⚠ `quoting` 不是「要不要加引号」，是「**这个名字过的是哪道校验**」——
 *  `"raw"` = 已证明只含 `[A-Za-z0-9_-]`；`"quoted"` = 校验时允许空格等自由字符。
 *  怎么拼由 Rust 那侧按它决定，前端不许替它决定（F03 在 TS 侧消灭掉的就是那种嗅探）。
 *
 *  ⚠ `attach` 那一格**不带载荷**：同一个请求里的 `env`/`args`/`launcher` 必须是空的，
 *  否则后端 fail-closed 拒（那说明调用方把格搞错了，不该静默把载荷丢掉）。 */
export type WireTmuxOuter =
  | {
      mode: "create";
      name: string;
      quoting: "raw" | "quoted";
      /** `new-session -c <目录>`。⚠ 与顶层 `cwd` **只许有一个非空** ——
       *  tmux 那两格的内层没有 `cd`，两个都送后端会拒。 */
      cwd: string | null;
      ccmSid: string | null;
    }
  | { mode: "send-into"; name: string; quoting: "raw" | "quoted" }
  | { mode: "attach"; name: string; quoting: "raw" | "quoted" };

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
  /** 缺席 = `container:"none"` 那一格（本命令原本的唯一形态，字节一个都没变）。
   *  Rust 侧是 `#[serde(default)] pub outer: Option<WireTmuxOuter>`。 */
  outer?: WireTmuxOuter;
}
