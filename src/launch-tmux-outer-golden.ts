/**
 * `设计/90 §4 E`：**外层 tmux 命令那三格的跨语言逐字节金标准**（同 `launch-payload-golden.ts`
 * 的机制，同一套纪律）。
 *
 * ```text
 *   本文件（真 renderFallback → 真 SESSION_BACKEND）──生成──▶ fixtures/tmux-outer-golden.json
 *          ▲                                                          │
 *          │ launch-tmux-outer-golden.vitest.ts                        │ launch_tmux_outer_parity.rs
 *          │ 断言「入库的 == 现场渲染的」                                ▼ 断言「Rust 渲染 == 入库的」
 *          └────────────── 改 TS 不重生成 ⇒ 红                     改 Rust ⇒ 红
 * ```
 *
 * # 🔴 它为什么必须先于「动刀」存在
 *
 * launch 的渲染结果是**要落到用户 shell 里执行的字符串** —— 一个字节的差异就是行为差异，
 * 而既有判据在这一维上是**瞎的**：`remote-launch.test.ts` 与 `session-backend` 那套测的是
 * 「TS 自己产的串对不对」，`payload-golden.json` 只覆盖**内层**（`container:"none"`）。
 * 把外层那三格搬进 Rust 而不先立这份金标准，「搬过去了但少了一个 `&&`」这一类改动
 * **一条判据都不会红**。⇒ 先立标准，再动刀。
 *
 * ⚠ **左边是「今天线上真在跑的那一支」**，不是一份手写期望值：
 * `renderFallback(plan)` 就是 `remote-launch-run.ts::renderLaunchCommand` 最后那一行调的
 * 同一个函数，它内部再去问座 `session-backend.ts` 要 `tmux …` 那几句。
 *
 * # 三格 × 覆盖面
 *
 * - `create`：有/无 cwd · 有/无 `@ccm_sid`（那是 `setSid` + `setTitle` 两段的开关）·
 *   `raw` / `quoted` 两种名字校验路径 · 需要 POSIX 转义的 cwd 与会话名；
 * - `send-into`：没有 `new-session`、没有短路（治 #76 的那一格）；
 * - `attach`：不带载荷。
 *
 * ⚠ **金标准盖不到的，如实写在这里**：
 * 1. **非法输入两侧姿态不同** —— Rust 侧对空会话名 / 越界 `@ccm_sid` / 空 cwd 一律 `Err`，
 *    TS 座逐字「不做校验/转义」照拼。这一类**结构上进不了金标准**（左边产得出、右边拒），
 *    与 `payload.rs` 头注记的那条「TS 生成夹具这个机制抓不到安全姿态差异」是同一件事。
 * 2. **两侧一致地错，金标准照样绿** —— 它挡的是**单侧静默漂移**。
 *    真正挡「两侧同错」的是各侧自己的语义判据（TS：`session-backend.test.ts`；
 *    Rust：`payload_tests.rs` 里外层那几条）。
 */
import { renderFallback } from "./launch-render-fallback.ts";
import { AGENT_PROFILE } from "./agent-profile.ts";
import { buildTmuxOuterRenderRequest } from "./remote-launch-run.ts";
import type { EnvOp, LaunchPlan, TmuxMode } from "./launch-plan.ts";

/** 一条用例：`LaunchPlan` 里与这三格有关的那几个字段。 */
export interface TmuxOuterCase {
  name: string;
  /** `null` = `action:attach`（那一格没有载荷）；否则是 `create` / `send-into`。 */
  mode: Exclude<TmuxMode, "attach-only"> | null;
  tmuxName: string;
  nameQuoting: "raw" | "quoted";
  cwd: string | null;
  ccmSid: string | null;
  env: EnvOp[];
  launcher: string;
  args: string[];
}

const ACCT = "/home/u/.claude-alt/z";
const SID = "0f1e2d3c";

const base = (over: Partial<TmuxOuterCase> = {}): TmuxOuterCase => ({
  name: "",
  mode: "create",
  tmuxName: "cc-0f1e2d3c",
  nameQuoting: "raw",
  cwd: null,
  ccmSid: null,
  env: [],
  launcher: "claude",
  args: [],
  ...over,
});

/**
 * 用例集。**每加一条 Rust 侧就多比一条** —— 这是这三格对拍面的唯一定义处。
 */
export const TMUX_OUTER_CASES: readonly TmuxOuterCase[] = [
  base({ name: "create：裸的（无 cwd、无身份标记）" }),
  base({ name: "create：带 cwd（-c 落在 new-session 上，内层没有 cd）", cwd: "/w" }),
  base({
    name: "create：带 @ccm_sid（setSid + setTitle 两段一起出现）",
    ccmSid: SID,
  }),
  base({
    name: "create：cwd + @ccm_sid + 具名账号 + 嵌套 env 清理",
    cwd: "/w",
    ccmSid: SID,
    env: [{ kind: "export-config-dir", value: ACCT }, { kind: "unset-nested-env" }],
  }),
  base({
    name: "create：账号 0（显式 unset）+ 模型偏好",
    cwd: "/w",
    env: [{ kind: "unset-config-dir" }, { kind: "export-model", value: "opus" }],
  }),
  base({
    name: "create：quoted 名字（=name: 整段被单引号包住）",
    tmuxName: "开新 Claude",
    nameQuoting: "quoted",
    ccmSid: SID,
  }),
  base({
    name: "create：cwd 带空格与中文（单引号包裹）",
    cwd: "/home/用户/带 空格/proj",
  }),
  base({
    name: "create：cwd 里有单引号（POSIX 断开转义）",
    cwd: "/tmp/it's here",
  }),
  base({
    name: "create：resume（flag 已展开进 argv）",
    cwd: "/w",
    ccmSid: SID,
    args: [AGENT_PROFILE.resumeFlag, "abc-123"],
  }),
  base({
    name: "send-into：无 new-session、无短路（治 #76 那一格）",
    mode: "send-into",
    env: [{ kind: "unset-nested-env" }],
    args: [AGENT_PROFILE.resumeFlag, "abc-123"],
  }),
  base({
    name: "send-into：quoted 名字",
    mode: "send-into",
    tmuxName: "开新 Claude",
    nameQuoting: "quoted",
  }),
  base({ name: "attach：raw 名字，不带载荷", mode: null }),
  base({
    name: "attach：quoted 名字，不带载荷",
    mode: null,
    tmuxName: "开新 Claude",
    nameQuoting: "quoted",
  }),
];

function planOf(c: TmuxOuterCase): LaunchPlan {
  const container = {
    kind: "tmux" as const,
    name: c.tmuxName,
    nameQuoting: c.nameQuoting,
    // `attach` 那一格在 IR 里的容器 mode 是 `attach-only`（`renderFallback` 先按
    // `action.kind === "attach"` 分流，根本不看 mode）—— 照 `launch-requests.ts::planAttach` 的原样。
    mode: (c.mode ?? "attach-only") as TmuxMode,
  };
  if (c.mode === null) {
    return {
      transport: { kind: "ssh" },
      action: { kind: "attach", name: c.tmuxName },
      container,
      cwd: null,
      env: [],
      launcher: "",
      args: [],
      wrap: [],
    };
  }
  const resumeSid = c.args[0] === AGENT_PROFILE.resumeFlag ? c.args[1] : null;
  return {
    transport: { kind: "ssh" },
    // ★ resume 的 `--resume <sid>` 展开留在 TS（同 `launch-payload-golden.ts` 那条口径）：
    //   用例把 flag 直接写进 `args`，于是 plan ↔ req 的映射是 1:1 的，夹具里看得见。
    action: resumeSid !== null ? { kind: "resume", sid: resumeSid } : { kind: "new" },
    container,
    cwd: c.cwd,
    env: c.env,
    launcher: c.launcher,
    args: resumeSid !== null ? c.args.slice(2) : c.args,
    ...(c.ccmSid !== null ? { identity: { ccmSid: c.ccmSid } } : {}),
    wrap: [],
  };
}

/** 生成入库夹具的**全文**（含尾换行）。 */
export function renderTmuxOuterFixture(): string {
  return `${JSON.stringify(
    {
      _: "由 src/launch-tmux-outer-golden.ts 生成，勿手改。重生成：npm run gen:payload-golden",
      nestedEnvKeys: AGENT_PROFILE.nestedEnvVars,
      cases: TMUX_OUTER_CASES.map((c) => ({
        name: c.name,
        mode: c.mode ?? "attach",
        // ★ `req` 由**生产形状的构造口**（`buildTmuxOuterRenderRequest`）产出 ——
        //   Rust 侧跑的是真命令 `render_launch_payload`，不是自己重搭一个 spec。
        req: buildTmuxOuterRenderRequest(planOf(c)),
        cmd: renderFallback(planOf(c)),
      })),
    },
    null,
    2,
  )}\n`;
}
