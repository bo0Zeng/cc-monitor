/**
 * U8c-1：**跨语言逐字节对拍的黄金串来源**。
 *
 * 载荷（`env 前缀 → cd → argv`）今天有四个产出点，其中一个刚被搬进 Rust 共享 crate
 * Rust 侧的 `backend/control/payload.rs`。两种语言各写一份实现，**保证它们一致的不能是注释，得是判据**：
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

const ACCT = "/home/u/.claude-alt/z";
/** `设计/80 §8` 步 1：一个形状合法的启动期令牌（`[0-9a-f]{32}`）。
 *  ⚠ 夹具里的令牌是**常量**，不是现场铸的 —— 逐字节金标准里不许有随机值。 */
const RBIND = "0f1e2d3c4b5a69788796a5b4c3d2e1f0";

/**
 * 用例集。**每加一条 Rust 侧就多比一条** —— 这是对拍面的唯一定义处。
 *
 * 覆盖面：五种 `EnvOp` 各至少一次 · 有/无 cwd · 空 args · 多 args ·
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
    // `设计/80 §8` 步 1：**只有令牌**的那一格 —— 它是 `container:"none"`
    // （`planResumeDirect`，今天 ↗ 做不到的那一档）在步 1 之后的载荷形态。
    name: "只有启动期令牌（container:\"none\" 那一档也带）",
    env: [{ kind: "export-rbind-token", value: RBIND }],
    cwd: "/w",
    launcher: "claude",
    args: [],
    wrap: [],
  },
  {
    // ⚠ **别把这条读成「维度 order 的判据」**（死值验 `M7b` 现打订正的一句半真话）：
    //   本文件的用例**手写** `env` 数组、**从不跑维度注册表** ⇒ 把
    //   `RBIND_TOKEN_DIMENSION.order` 从 40 改成 15，本条金标准**照绿**。
    //   它钉的是「**给定这个顺序**，TS 与 Rust 渲出来的字节一样」。
    //   「维度真的按这个顺序发」由 `tests/launch-dimensions.test.ts`
    //   （`buildLaunchPlan` 五种 EnvOp 数组相等 ＋ 模块加载即崩的顺序不变式）与
    //   `tests/launch-requests.vitest.ts`（整条载荷逐字节相等）钉 —— M7b 实测那两处共红 5 条。
    name: "五种 EnvOp 同时出现（顺序即契约）",
    env: [
      { kind: "export-config-dir", value: ACCT },
      { kind: "export-model", value: "sonnet" },
      { kind: "unset-config-dir" },
      { kind: "unset-nested-env" },
      { kind: "export-rbind-token", value: RBIND },
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
