/**
 * U8c-1：**载荷（`container:"none"`）的入库夹具** —— 用例表与落盘函数。
 *
 * ```text
 *   本文件（用例表 ＋ 手写期望）──生成──▶  src/backend/control/launch_render/fixtures/payload-golden.json
 *          ▲                                              │
 *          │ launch-payload-golden.vitest.ts               │ launch_payload_parity.rs
 *          │ 断言「入库的 == 现场落盘的」                    ▼ 断言「Rust 生产命令渲染 == 入库的」
 *          └────────────── 改用例表不重生成 ⇒ 红          改 Rust ⇒ 红
 * ```
 *
 * # 左边从「TS 渲染器的产出」换成了「手写期望」
 *
 * TS 那份兜底渲染器（`launch-render-fallback.ts` ＋ `session-backend.ts`）零生产调用，
 * 照「TS 的只供对拍、排期删」删了（同 LR1 删 `ccm …` 那一份的办法）。
 * 夹具**没有**跟着删，因为它一直在钉两件事，只有第一件随渲染器走：
 *
 *  1. 「两种语言渲出同一串」—— 另一种语言没了，这一件没了（已知代价）。
 *  2. **生产的请求构造（`buildPayloadRenderRequest`）→ 线 → Rust 反序列化 → 生产命令**这一整条。
 *
 * ⇒ `payload` 是**手写的期望**：值是 TS 渲染器最后一次跑出、与 Rust 逐字节对过的原样，
 *   改 Rust 渲染器的产出 ⇒ 回来改这里的期望，这一步必须是人做的。
 *
 * ⚠ **不能让 Rust 侧去调 TS 现场生成**（那就成了自洽夹具 —— U7-4 的病根正是「写侧读侧
 * 同一个常量」）。夹具必须**入库**，两侧各自与它比。
 *
 * # 为什么用例全是 `container:{kind:"none"}` + `action:{kind:"new"}`
 *
 * - **容器那一层归另一份夹具**（`launch-tmux-outer-golden.ts` ⇒ `tmux-outer-golden.json`）。
 * - **`resume` 的 `--resume <sid>` 展开在 TS 的请求构造里**（`buildPayloadRenderRequest`）。
 *   Rust 侧 `PayloadSpec.args` 收的是**展开后**的 argv，所以用例把 resume flag 直接写进 `args`
 *   —— 这样 spec ↔ plan 的映射是 1:1 的，夹具里看得见。
 * - **launcher**：用例只用干净 launcher。原先这里写「sanitize 也在 TS（`sanitizeRemoteLauncher`〔散文墓碑〕）」——
 *   TS 那份删了，字符集只在 Rust 载荷渲染判（判不过拒），前端只把空白读成默认启动器。
 *
 * 住址从 `src/` 挪到 `tests/test-support/`：它的手写期望逐字就是整条 shell 命令，而条 1
 * 要 `src/**\/*.ts` 零 shell 串（判据 `tests/frontend/ui/launch-no-shell-in-ts.vitest.ts`，不开例外）。当年留在 `src/` 的理由
 * 「`tests/` 不在 `tsconfig.json` 的 include 里、`LaunchPlan` 改形状 tsc 看不见这张表」重组之后已不成立
 * （`include` 今天是 `["src", "tests"]`）⇒ 挪过来 tsc 照样看得见。
 */
import { AGENT_PROFILE } from "../../src/frontend/ui/agent-profile.ts";
import { buildPayloadRenderRequest } from "../../src/frontend/ui/remote-launch-run.ts";
import type { EnvOp, LaunchPlan } from "../../src/frontend/ui/launch-types.ts";

/** 一条用例：Rust `PayloadSpec` 的字段 + 手写的期望载荷。 */
export interface GoldenCase {
  name: string;
  env: EnvOp[];
  cwd: string | null;
  launcher: string;
  args: string[];
  wrap: { order: number; prelude: string }[];
  /** **手写**的期望串：生产 `render_launch_payload`（`container:"none"`）对这一格该渲出的载荷。 */
  payload: string;
}

const ACCT = "/home/u/.claude-accts/z";
/** 一个形状合法的启动期令牌（`[0-9a-f]{32}`）。
 *  ⚠ 夹具里的令牌是**常量**，不是现场铸的 —— 逐字节金标准里不许有随机值。 */
const RBIND = "0f1e2d3c4b5a69788796a5b4c3d2e1f0";

/**
 * 用例集。**每加一条 Rust 侧就多比一条** —— 这是对拍面的唯一定义处。
 *
 * 覆盖面：五种 `EnvOp` 各至少一次 · 有/无 cwd · 空 args · 多 args ·
 * 需要转义的路径与参数 · wrap 折叠（含乱序 `order`）。
 */
export const GOLDEN_CASES: readonly GoldenCase[] = [
  { name: "裸载荷（无修饰）",
    payload: "claude", env: [], cwd: null, launcher: "claude", args: [], wrap: [] },
  {
    name: "只有 cwd",
    payload: "cd '/w' && claude",
    env: [],
    cwd: "/w",
    launcher: "claude",
    args: [],
    wrap: [],
  },
  {
    name: "具名账号 + 嵌套 env 清理 + cwd",
    payload: "export CLAUDE_CONFIG_DIR='/home/u/.claude-accts/z'; unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; cd '/w' && claude",
    env: [{ kind: "export-config-dir", value: ACCT }, { kind: "unset-nested-env" }],
    cwd: "/w",
    launcher: "claude",
    args: [],
    wrap: [],
  },
  {
    name: "账号 0（显式 unset）+ 嵌套 env 清理",
    payload: "unset CLAUDE_CONFIG_DIR; unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; cd '/w' && claude",
    env: [{ kind: "unset-config-dir" }, { kind: "unset-nested-env" }],
    cwd: "/w",
    launcher: "claude",
    args: [],
    wrap: [],
  },
  {
    name: "模型偏好",
    payload: "export ANTHROPIC_MODEL='opus'; claude",
    env: [{ kind: "export-model", value: "opus" }],
    cwd: null,
    launcher: "claude",
    args: [],
    wrap: [],
  },
  {
    name: "resume（flag 已展开进 args）",
    payload: "cd '/w' && claude --resume abc-123",
    env: [],
    cwd: "/w",
    launcher: "claude",
    args: [AGENT_PROFILE.resumeFlag, "abc-123"],
    wrap: [],
  },
  {
    // **只有令牌**的那一格 —— 它是 `container:"none"`
    // （`planResumeDirect`，今天 ↗ 做不到的那一档）在步 1 之后的载荷形态。
    name: "只有启动期令牌（container:\"none\" 那一档也带）",
    payload: "export CCM_RBIND_TOKEN='0f1e2d3c4b5a69788796a5b4c3d2e1f0'; cd '/w' && claude",
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
    //   它钉的是「**给定这个顺序**，Rust 渲出来的字节就是 `payload` 那一串」。
    //   「维度真的按这个顺序发」由 `tests/frontend/ui/launch-dimensions.test.ts`
    //   （`buildLaunchPlan` 五种 EnvOp 数组相等 ＋ 模块加载即崩的顺序不变式）与
    //   `tests/frontend/ui/launch-requests.vitest.ts`（整条载荷逐字节相等）钉 —— M7b 实测那两处共红 5 条。
    name: "五种 EnvOp 同时出现（顺序即契约）",
    payload: "export CLAUDE_CONFIG_DIR='/home/u/.claude-accts/z'; export ANTHROPIC_MODEL='sonnet'; unset CLAUDE_CONFIG_DIR; unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; export CCM_RBIND_TOKEN='0f1e2d3c4b5a69788796a5b4c3d2e1f0'; cd '/w' && claude",
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
    payload: "cd '/home/用户/带 空格/proj' && claude",
    env: [],
    cwd: "/home/用户/带 空格/proj",
    launcher: "claude",
    args: [],
    wrap: [],
  },
  {
    name: "cwd 里有单引号（POSIX 断开转义）",
    payload: "cd '/tmp/it'\\''s here' && claude",
    env: [],
    cwd: "/tmp/it's here",
    launcher: "claude",
    args: [],
    wrap: [],
  },
  {
    name: "wrap 折叠（order 乱序给，必须按升序由内向外）",
    payload: "unset CLAUDE_CONFIG_DIR; cd '/w' && ( outer_setup; exec ( inner_setup; exec claude ) )",
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
      _: "由 tests/test-support/launch-payload-golden.ts 生成，勿手改。重生成：npm run gen:payload-golden",
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
        payload: c.payload,
      })),
    },
    null,
    2,
  )}\n`;
}
