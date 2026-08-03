/**
 * U8c-1：**跨语言逐字节对拍的黄金串来源**。
 *
 * 载荷（`env 前缀 → cd → argv`）今天有四个产出点，其中一个刚被搬进 Rust 共享 crate
 * `launch-core`。两种语言各写一份实现，**保证它们一致的不能是注释，得是判据**：
 *
 * ```text
 *   本文件（真 renderFallback）  ──生成──▶  src/backend/control/fixtures/payload-golden.json
 *          ▲                                              │
 *          │ launch-payload-golden.vitest.ts               │ launch_payload_parity.rs
 *          │ 断言「入库的 == 现场渲染的」                    ▼ 断言「Rust 渲染 == 入库的」
 *          └────────────── 改 TS 不重生成 ⇒ 红          改 Rust ⇒ 红
 * ```
 *
 * ⚠ **不能让 Rust 侧去调 TS 现场生成**（那就成了自洽夹具 —— U7-4 的病根正是「写侧读侧
 * 同一个常量」）。夹具必须**入库**，两侧各自与它比。
 *
 * # 为什么用例全是 `container:{kind:"none"}` + `action:{kind:"new"}`
 *
 * - **容器那一层不在对拍范围内**：外层 tmux 命令 U8a-2b 起已由 `control/launch.rs` 用 argv
 *   直传取代，Rust 侧压根不再拼它。
 * - **`resume` 的 `--resume <sid>` 展开留在 TS**：`renderArgv` 会把 `AGENT_PROFILE.resumeFlag`
 *   与 sid 追加进 argv。Rust 侧 `PayloadSpec.args` 收的是**展开后**的 argv，
 *   所以用例把 resume flag 直接写进 `args` —— 这样 spec ↔ plan 的映射是 1:1 的，
 *   夹具里看得见，不需要在两边各写一遍展开规则。展开规则本身归 U8c-2。
 * - **launcher 的 sanitize 也留在 TS**（`sanitizeRemoteLauncher`）：Rust 侧收的是净化后的值。
 *   用例只用干净 launcher，净化规则由 `remote-launch.test.ts` 自己管。
 */
import { renderFallback } from "./launch-render-fallback.ts";
import { AGENT_PROFILE } from "./agent-profile.ts";
import { buildPayloadRenderRequest } from "./remote-launch-run.ts";
import type { EnvOp, LaunchPlan } from "./launch-plan.ts";

/** 一条用例：Rust `PayloadSpec` 的字段 + 它在 TS 侧渲染出来的串。 */
export interface GoldenCase {
  name: string;
  env: EnvOp[];
  cwd: string | null;
  launcher: string;
  args: string[];
  wrap: { order: number; prelude: string }[];
}

const ACCT = "/home/u/.claude-accts/z";

/**
 * 用例集。**每加一条 Rust 侧就多比一条** —— 这是对拍面的唯一定义处。
 *
 * 覆盖面：四种 `EnvOp` 各至少一次 · 有/无 cwd · 空 args · 多 args ·
 * 需要转义的路径与参数 · wrap 折叠（含乱序 `order`）。
 */
export const GOLDEN_CASES: readonly GoldenCase[] = [
  { name: "裸载荷（无修饰）", env: [], cwd: null, launcher: "claude", args: [], wrap: [] },
  {
    name: "只有 cwd",
    env: [],
    cwd: "/w",
    launcher: "claude",
    args: [],
    wrap: [],
  },
  {
    name: "具名账号 + 嵌套 env 清理 + cwd",
    env: [{ kind: "export-config-dir", value: ACCT }, { kind: "unset-nested-env" }],
    cwd: "/w",
    launcher: "claude",
    args: [],
    wrap: [],
  },
  {
    name: "账号 0（显式 unset）+ 嵌套 env 清理",
    env: [{ kind: "unset-config-dir" }, { kind: "unset-nested-env" }],
    cwd: "/w",
    launcher: "claude",
    args: [],
    wrap: [],
  },
  {
    name: "模型偏好",
    env: [{ kind: "export-model", value: "opus" }],
    cwd: null,
    launcher: "claude",
    args: [],
    wrap: [],
  },
  {
    name: "resume（flag 已展开进 args）",
    env: [],
    cwd: "/w",
    launcher: "claude",
    args: [AGENT_PROFILE.resumeFlag, "abc-123"],
    wrap: [],
  },
  {
    name: "四种 EnvOp 同时出现（顺序即契约）",
    env: [
      { kind: "export-config-dir", value: ACCT },
      { kind: "export-model", value: "sonnet" },
      { kind: "unset-config-dir" },
      { kind: "unset-nested-env" },
    ],
    cwd: "/w",
    launcher: "claude",
    args: [],
    wrap: [],
  },
  {
    name: "带空格与中文的 cwd（单引号包裹）",
    env: [],
    cwd: "/home/用户/带 空格/proj",
    launcher: "claude",
    args: [],
    wrap: [],
  },
  {
    name: "cwd 里有单引号（POSIX 断开转义）",
    env: [],
    cwd: "/tmp/it's here",
    launcher: "claude",
    args: [],
    wrap: [],
  },
  {
    name: "wrap 折叠（order 乱序给，必须按升序由内向外）",
    env: [{ kind: "unset-config-dir" }],
    cwd: "/w",
    launcher: "claude",
    args: [],
    wrap: [
      { order: 2, prelude: "outer_setup" },
      { order: 1, prelude: "inner_setup" },
    ],
  },
];

function planOf(c: GoldenCase): LaunchPlan {
  return {
    transport: { kind: "ssh" },
    action: { kind: "new" },
    container: { kind: "none" },
    cwd: c.cwd,
    env: c.env,
    launcher: c.launcher,
    args: c.args,
    wrap: c.wrap.map((w) => ({ id: w.prelude, order: w.order, prelude: w.prelude })),
  };
}

/**
 * 生成入库夹具的**全文**（含尾换行）。
 *
 * `nestedEnvKeys` 一并写进去：Rust 侧的 `EnvOp::UnsetNestedEnv` 需要键表，而键表是
 * per-agent 的画像数据（`AGENT_PROFILE.nestedEnvVars`），不该在 Rust 里再抄一份。
 */
export function renderGoldenFixture(): string {
  return `${JSON.stringify(
    {
      _: "由 src/launch-payload-golden.ts 生成，勿手改。重生成：npm run gen:payload-golden",
      nestedEnvKeys: AGENT_PROFILE.nestedEnvVars,
      cases: GOLDEN_CASES.map((c) => ({
        name: c.name,
        env: c.env,
        cwd: c.cwd,
        launcher: c.launcher,
        args: c.args,
        wrap: c.wrap,
        // ★ 同 cli 夹具：`req` 由生产代码（`buildPayloadRenderRequest`）构造。
        req: buildPayloadRenderRequest(planOf(c)),
        payload: renderFallback(planOf(c)),
      })),
    },
    null,
    2,
  )}\n`;
}
