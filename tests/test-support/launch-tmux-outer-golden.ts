/**
 * `设计/90 §4 E`：**外层 tmux 命令那三格的入库夹具**（同 `launch-payload-golden.ts`
 * 的机制，同一套纪律）。
 *
 * ```text
 *   本文件（用例表 ＋ 手写期望）──生成──▶ fixtures/tmux-outer-golden.json
 *          ▲                                                          │
 *          │ launch-tmux-outer-golden.vitest.ts                        │ launch_tmux_outer_parity.rs
 *          │ 断言「入库的 == 现场落盘的」                                ▼ 断言「Rust 生产命令渲染 == 入库的」
 *          └────────────── 改用例表不重生成 ⇒ 红                   改 Rust ⇒ 红
 * ```
 *
 * # 〔LR2〕左边从「TS 渲染器 ＋ 座」换成了「手写期望」
 *
 * 22b·B 起生产那三格走 Rust（`render_launch_payload` 带 `outer`）；TS 那份
 * （`launch-render-fallback.ts` ＋ `session-backend.ts`）零生产调用，照 `设计/00 §2.5 ④` 删了。
 * `cmd` 是**手写的期望**：值是 TS 那份最后一次跑出、与 Rust 逐字节对过的原样。
 * `req` 仍由生产的 `buildTmuxOuterRenderRequest` 现产 ⇒ 这份夹具钉的是
 * 「生产请求构造 → 线 → Rust 反序列化 → 生产命令」这一整条（例如「生产那一格少送 `outer`」
 * 会让 Rust 渲出一条没有 tmux 的串 ⇒ 与 `cmd` 不等 ⇒ 红）。
 *
 * # 三格 × 覆盖面
 *
 * - `create`：有/无 cwd · 有/无 `@ccm_sid`（那是 `setSid` + `setTitle` 两段的开关）·
 *   `raw` / `quoted` 两种名字校验路径 · 需要 POSIX 转义的 cwd 与会话名；
 * - `send-into`：没有 `new-session`、没有短路（治 #76 的那一格）；
 * - `attach`：不带载荷。
 *
 * 〔LR2〕住址从 `src/` 挪到 `tests/test-support/`，理由同 `launch-payload-golden.ts` 头注末段（`设计/90 §3` 条 1）。
 *
 * ⚠ **金标准盖不到的，如实写在这里**：非法输入（空会话名 / 越界 `@ccm_sid` / 空 cwd）
 * Rust 一律 `Err`，那一类进不了这份金标准，由 `payload_tests.rs` 外层那几条管。
 */
import { AGENT_PROFILE } from "../../src/frontend/ui/agent-profile.ts";
import { buildTmuxOuterRenderRequest } from "../../src/frontend/ui/remote-launch-run.ts";
import type { EnvOp, LaunchPlan, TmuxMode } from "../../src/frontend/ui/launch-types.ts";

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
  /** **手写**的期望串：生产 `render_launch_payload`（带 `outer`）对这一格该渲出的整条命令。 */
  cmd: string;
}

const ACCT = "/home/u/.claude-accts/z";
const SID = "0f1e2d3c";
/** `设计/80 §8` 步 1：形状合法的启动期令牌（`[0-9a-f]{32}`），夹具里是常量不是现场铸的。 */
const RBIND = "0f1e2d3c4b5a69788796a5b4c3d2e1f0";

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
  cmd: "",
  ...over,
});

/**
 * 用例集。**每加一条 Rust 侧就多比一条** —— 这是这三格对拍面的唯一定义处。
 */
export const TMUX_OUTER_CASES: readonly TmuxOuterCase[] = [
  base({ name: "create：裸的（无 cwd、无身份标记）", cmd: "tmux new-session -d -s cc-0f1e2d3c && tmux send-keys -t =cc-0f1e2d3c: 'claude' Enter && tmux attach -t =cc-0f1e2d3c:" }),
  base({ name: "create：带 cwd（-c 落在 new-session 上，内层没有 cd）",
    cmd: "tmux new-session -d -s cc-0f1e2d3c -c '/w' && tmux send-keys -t =cc-0f1e2d3c: 'claude' Enter && tmux attach -t =cc-0f1e2d3c:", cwd: "/w" }),
  base({
    name: "create：带 @ccm_sid（setSid + setTitle 两段一起出现）",
    cmd: "tmux new-session -d -s cc-0f1e2d3c && (tmux set-option -t =cc-0f1e2d3c: @ccm_sid 0f1e2d3c 2>/dev/null || true) && (tmux set-option -t =cc-0f1e2d3c: set-titles on 2>/dev/null || true) && (tmux set-option -t =cc-0f1e2d3c: set-titles-string ccm-rbind-#{@ccm_sid} 2>/dev/null || true) && tmux send-keys -t =cc-0f1e2d3c: 'claude' Enter && tmux attach -t =cc-0f1e2d3c:",
    ccmSid: SID,
  }),
  base({
    name: "create：cwd + @ccm_sid + 具名账号 + 嵌套 env 清理",
    cmd: "tmux new-session -d -s cc-0f1e2d3c -c '/w' && (tmux set-option -t =cc-0f1e2d3c: @ccm_sid 0f1e2d3c 2>/dev/null || true) && (tmux set-option -t =cc-0f1e2d3c: set-titles on 2>/dev/null || true) && (tmux set-option -t =cc-0f1e2d3c: set-titles-string ccm-rbind-#{@ccm_sid} 2>/dev/null || true) && tmux send-keys -t =cc-0f1e2d3c: 'export CLAUDE_CONFIG_DIR='\\''/home/u/.claude-accts/z'\\''; unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; claude' Enter && tmux attach -t =cc-0f1e2d3c:",
    cwd: "/w",
    ccmSid: SID,
    env: [{ kind: "export-config-dir", value: ACCT }, { kind: "unset-nested-env" }],
  }),
  base({
    name: "create：账号 0（显式 unset）+ 模型偏好",
    cmd: "tmux new-session -d -s cc-0f1e2d3c -c '/w' && tmux send-keys -t =cc-0f1e2d3c: 'unset CLAUDE_CONFIG_DIR; export ANTHROPIC_MODEL='\\''opus'\\''; claude' Enter && tmux attach -t =cc-0f1e2d3c:",
    cwd: "/w",
    env: [{ kind: "unset-config-dir" }, { kind: "export-model", value: "opus" }],
  }),
  base({
    name: "create：quoted 名字（=name: 整段被单引号包住）",
    cmd: "tmux new-session -d -s '开新 Claude' && (tmux set-option -t '=开新 Claude:' @ccm_sid 0f1e2d3c 2>/dev/null || true) && (tmux set-option -t '=开新 Claude:' set-titles on 2>/dev/null || true) && (tmux set-option -t '=开新 Claude:' set-titles-string ccm-rbind-#{@ccm_sid} 2>/dev/null || true) && tmux send-keys -t '=开新 Claude:' 'claude' Enter && tmux attach -t '=开新 Claude:'",
    tmuxName: "开新 Claude",
    nameQuoting: "quoted",
    ccmSid: SID,
  }),
  base({
    name: "create：cwd 带空格与中文（单引号包裹）",
    cmd: "tmux new-session -d -s cc-0f1e2d3c -c '/home/用户/带 空格/proj' && tmux send-keys -t =cc-0f1e2d3c: 'claude' Enter && tmux attach -t =cc-0f1e2d3c:",
    cwd: "/home/用户/带 空格/proj",
  }),
  base({
    name: "create：cwd 里有单引号（POSIX 断开转义）",
    cmd: "tmux new-session -d -s cc-0f1e2d3c -c '/tmp/it'\\''s here' && tmux send-keys -t =cc-0f1e2d3c: 'claude' Enter && tmux attach -t =cc-0f1e2d3c:",
    cwd: "/tmp/it's here",
  }),
  base({
    name: "create：resume（flag 已展开进 argv）",
    cmd: "tmux new-session -d -s cc-0f1e2d3c -c '/w' && (tmux set-option -t =cc-0f1e2d3c: @ccm_sid 0f1e2d3c 2>/dev/null || true) && (tmux set-option -t =cc-0f1e2d3c: set-titles on 2>/dev/null || true) && (tmux set-option -t =cc-0f1e2d3c: set-titles-string ccm-rbind-#{@ccm_sid} 2>/dev/null || true) && tmux send-keys -t =cc-0f1e2d3c: 'claude --resume abc-123' Enter && tmux attach -t =cc-0f1e2d3c:",
    cwd: "/w",
    ccmSid: SID,
    args: [AGENT_PROFILE.resumeFlag, "abc-123"],
  }),
  base({
    // 🔴 `设计/80 §8.4`：`EnvOp` **容器无关** —— 同一条令牌在 tmux 那一格
    //    也进**内层载荷**（被 `posixQuote` 一次塞进 `send-keys`），
    //    不需要在外层 tmux 命令上另开一个槽位。这一条与
    //    `payload-golden.json` 里那两条令牌用例合起来，就是「两条起法同一套机制」的
    //    逐字节读数：右边是同一个 `render_launch_payload`，只换了有没有 `outer`。
    name: "create：启动期令牌（容器无关 —— tmux 那一格也带，且排在 unset 之后）",
    cmd: "tmux new-session -d -s cc-0f1e2d3c -c '/w' && (tmux set-option -t =cc-0f1e2d3c: @ccm_sid 0f1e2d3c 2>/dev/null || true) && (tmux set-option -t =cc-0f1e2d3c: set-titles on 2>/dev/null || true) && (tmux set-option -t =cc-0f1e2d3c: set-titles-string ccm-rbind-#{@ccm_sid} 2>/dev/null || true) && tmux send-keys -t =cc-0f1e2d3c: 'unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; export CCM_RBIND_TOKEN='\\''0f1e2d3c4b5a69788796a5b4c3d2e1f0'\\''; claude' Enter && tmux attach -t =cc-0f1e2d3c:",
    cwd: "/w",
    ccmSid: SID,
    env: [{ kind: "unset-nested-env" }, { kind: "export-rbind-token", value: RBIND }],
  }),
  base({
    name: "send-into：无 new-session、无短路（治 #76 那一格）",
    cmd: "tmux send-keys -t =cc-0f1e2d3c: 'unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; claude --resume abc-123' Enter; tmux attach -t =cc-0f1e2d3c:",
    mode: "send-into",
    env: [{ kind: "unset-nested-env" }],
    args: [AGENT_PROFILE.resumeFlag, "abc-123"],
  }),
  base({
    name: "send-into：quoted 名字",
    cmd: "tmux send-keys -t '=开新 Claude:' 'claude' Enter; tmux attach -t '=开新 Claude:'",
    mode: "send-into",
    tmuxName: "开新 Claude",
    nameQuoting: "quoted",
  }),
  base({ name: "attach：raw 名字，不带载荷",
    cmd: "tmux attach -t =cc-0f1e2d3c:", mode: null }),
  base({
    name: "attach：quoted 名字，不带载荷",
    cmd: "tmux attach -t '=开新 Claude:'",
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
    // `attach` 那一格在 IR 里的容器 mode 是 `attach-only`（`buildTmuxOuterRenderRequest` 先按
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
      _: "由 tests/test-support/launch-tmux-outer-golden.ts 生成，勿手改。重生成：npm run gen:payload-golden",
      nestedEnvKeys: AGENT_PROFILE.nestedEnvVars,
      cases: TMUX_OUTER_CASES.map((c) => ({
        name: c.name,
        mode: c.mode ?? "attach",
        // ★ `req` 由**生产形状的构造口**（`buildTmuxOuterRenderRequest`）产出 ——
        //   Rust 侧跑的是真命令 `render_launch_payload`，不是自己重搭一个 spec。
        req: buildTmuxOuterRenderRequest(planOf(c)),
        cmd: c.cmd,
      })),
    },
    null,
    2,
  )}\n`;
}
