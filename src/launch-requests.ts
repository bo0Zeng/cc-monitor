/**
 * F03（unify-launch）：「legacy 请求 → LaunchContext/LaunchPlan」翻译层。每个函数对应今天
 * `remote-launch.ts` 里一个 builder 的校验/意图构造逻辑，逐字保留其校验顺序与错误文案
 * （调用方 toast 依赖这些文案）。`remote-launch.ts` 的 builder 改造后只剩「调这里 + 交渲染器」。
 */
import { AGENT_PROFILE } from "./agent-profile.ts";
import { isValidSessionId, isValidTmuxName, isValidNewTmuxName } from "./shell-quote.ts";
import { buildLaunchPlan } from "./launch-plan.ts";
import type {
  LaunchAccount,
  LaunchAction,
  LaunchContext,
  LaunchModifiers,
  LaunchPlan,
} from "./launch-plan.ts";

export interface LaunchPlanBuild {
  ctx: LaunchContext;
  plan: LaunchPlan;
}

/** F05：触发条件仍是 `configDir` 单独非空（同 F03 原行为，`remote-launch.test.ts` 的老式
 *  直调路径只传 `configDir` 不传名字，必须继续正确触发兜底渲染器的 env 注入）；`name` 是
 *  可选增强——传了就线通进 IR 供 CLI 渲染器用，没传时 `LaunchAccount.name` 是 `undefined`，
 *  `ACCOUNT_DIMENSION.cliFlags` 会诚实地对这种情形返回 `null`（强制走兜底），而不是把整个
 *  账号状态错误地降级成 `base`（那会连兜底渲染器的 env 注入也漏掉，是真回归）。 */
function accountOf(configDir?: string, name?: string): LaunchAccount {
  return configDir ? { kind: "account", name, configDir } : { kind: "base" };
}

/** 对应 `buildResumeDirectCmd`：无容器（直连），resume 到当前登录 shell。 */
export function planResumeDirect(
  sid: string,
  cwd: string,
  launcher = AGENT_PROFILE.defaultLauncher,
  mods: LaunchModifiers = {},
): LaunchPlanBuild {
  const { configDir, accountName, modelOverride } = mods;
  if (!isValidSessionId(sid)) {
    throw new Error(`非法 sessionId（拒绝拼入命令）: ${JSON.stringify(sid)}`);
  }
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid },
    container: { kind: "none" },
    cwd: cwd.trim() || null,
    account: accountOf(configDir, accountName),
    launcherOverride: launcher,
    ccmSid: undefined,
    modelOverride,
  };
  return { ctx, plan: buildLaunchPlan(ctx) };
}

/** 对应 `buildResumeTmuxCmd`：新建/幂等接回 tmux，resume 进去。 */
export function planResumeTmux(
  sid: string,
  cwd: string,
  launcher = AGENT_PROFILE.defaultLauncher,
  name?: string,
  mods: LaunchModifiers = {},
): LaunchPlanBuild {
  const { configDir, accountName, modelOverride } = mods;
  if (!isValidSessionId(sid)) {
    throw new Error(`非法 sessionId（拒绝拼入命令）: ${JSON.stringify(sid)}`);
  }
  // S4b-3b（用户 2026-07-31）：会话名是 `<X>-cc` 后缀形，不是 `cc-<X>` 前缀形。
  // **这一处是那次反转漏掉的最后一个生产生成点**（2026-08-01 用户复查时揪出）：
  // 三条真实调用路径都传显式 name（`tabs.ts` 走 `pickFreshTmuxName`、`account-restart.ts`
  // 复用既有会话名、`fork-flow.ts` 有非空守卫），所以旧形态**当时没在线上冒出来** ——
  // 但它是个公开导出 API 的默认值，下一个省略 name 的调用方就会静默拿到旧前缀。
  // 与 `remote-launch.ts::pickFreshTmuxName` 的基名逐字相同，由 `remote-launch.test.ts`
  // 的对拍断言钉死（两处同源，别只改一边）。
  const tmuxName = name ?? `${sid.slice(0, 8)}-cc`;
  if (!/^[A-Za-z0-9_][A-Za-z0-9_-]*$/.test(tmuxName)) {
    throw new Error(`非法 tmux 会话名（拒绝拼入命令）: ${JSON.stringify(tmuxName)}`);
  }
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid },
    container: { kind: "tmux", name: tmuxName, nameQuoting: "raw", mode: "create-or-attach" },
    cwd: cwd.trim() || null,
    account: accountOf(configDir, accountName),
    launcherOverride: launcher,
    ccmSid: sid, // #72：自建 resume 会话打完整 sid，供 findClaudeTmux 精确命中
    modelOverride,
  };
  return { ctx, plan: buildLaunchPlan(ctx) };
}

/** 对应 `buildResumeIntoExistingTmuxCmd`：往已存在的 idle tmux 就地送键，不 new-session。 */
export function planResumeIntoExistingTmux(
  sid: string,
  name: string,
  launcher = AGENT_PROFILE.defaultLauncher,
  mods: LaunchModifiers = {},
): LaunchPlanBuild {
  const { configDir, accountName, modelOverride } = mods;
  if (!isValidSessionId(sid)) {
    throw new Error(`非法 sessionId（拒绝拼入命令）: ${JSON.stringify(sid)}`);
  }
  if (!/^[A-Za-z0-9_][A-Za-z0-9_-]*$/.test(name)) {
    throw new Error(`非法 tmux 会话名（拒绝拼入命令）: ${JSON.stringify(name)}`);
  }
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid },
    container: { kind: "tmux", name, nameQuoting: "raw", mode: "send-into" },
    cwd: null,
    account: accountOf(configDir, accountName),
    launcherOverride: launcher,
    ccmSid: undefined, // 复用会话已在建时打过标，不重设（同今天行为）
    modelOverride,
  };
  return { ctx, plan: buildLaunchPlan(ctx) };
}

/** 对应 `buildLauncherCmd`：「在这台机开新 Claude」——新建/幂等接回 tmux，起全新会话。 */
export function planLauncher(
  cwd: string,
  tmuxName: string,
  command = AGENT_PROFILE.defaultLauncher,
  mods: LaunchModifiers = {},
): LaunchPlanBuild {
  const { configDir, accountName, modelOverride } = mods;
  const name = tmuxName.trim();
  if (!isValidNewTmuxName(name)) {
    throw new Error(`非法 tmux 会话名（拒绝拼入命令）: ${JSON.stringify(name)}`);
  }
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "new" },
    container: { kind: "tmux", name, nameQuoting: "quoted", mode: "create-or-attach" },
    cwd: cwd.trim() || null,
    account: accountOf(configDir, accountName),
    launcherOverride: command,
    ccmSid: undefined, // 今天就不设——已知 F04 缺口，本次原样保留、不顺手"修一半"
    modelOverride,
  };
  return { ctx, plan: buildLaunchPlan(ctx) };
}

/**
 * 本地（Windows）路径在发起 IPC 之前的**前置校验**。**只做校验，不构造任何 IR。**
 *
 * **R07（原 `planLocal`，原返回 `LaunchPlanBuild`，原内部还跑一遍 `buildLaunchPlan`）。**
 * 原名 + 原返回值合起来暗示"本地路径也经这套 IR 产出命令"，而事实是：4 个生产调用点
 * （`views/history.ts` ×2、`views/session-viewer.ts`、`tabs.ts`）**全部把返回值当语句丢弃**，
 * 真命令由 Rust 独立构造（`invoke("resume_history_session")` / `invoke("new_local_session")`
 * → `history.rs::build_local_ps_command`，三个实参无一来自 plan）。
 *
 * **为什么连 `buildLaunchPlan` 那一遍也删掉**（Phase D 审计发现，初稿保留了它并声称是
 * "一道便宜的一致性检查"）：那个声称**零门禁守护**——审计实测把整段 ctx 构造 + 调用删掉、
 * 只留一句 `void cwd;`，`tsc` 与 `npm test` **705 全绿**（改造前同一变异红 5 条，
 * 因为那时返回类型让这次调用在**类型层**是承重的；改成 `void` 恰恰把类型层强制降级成了
 * 一句谁都能顺手删的裸语句）。而它想验的东西**别处已经在验**：
 * `launch-render-cli.test.ts` 有 `ctxOf({ transport: { kind: "local" } })` → `buildLaunchPlan` 的用例。
 * 生产侧它纯属浪费，且是 **fail-closed 风险**——将来任何对 `transport:local` 抛异常的新维度，
 * 都会让本地 resume 彻底拉不起来，而收益是零。
 *
 * **为什么不"真接上"**（R07 明确否决的选项，**理由经 Phase D 审计订正**）：
 * 初稿引的是 F06 的 `Get-Command` 论证——那条**真实存在**（`F06-local-path-ir.md:27-30`），
 * 但它排除的是"**TS 全量渲染好字符串、Rust 只管 exec**"这一形态，**并不排除**
 * "TS 构造 IR、Rust 只做 `Get-Command` 那一步补全"。真正支撑否决的是 F06 §3.2 实现期修正：
 * **`plan.action`/`plan.cwd` 在当前维度注册表下恒等于输入，取回来没有信息增量**
 * （`plan.launcher` 更是恒 `""`，因为本地不传 `launcherOverride`）。
 * 即"不接"是因为**接了也拿不到新东西**，不是因为技术上不可能。见 `doc/INVARIANTS.md` §36。
 */
export function validateLocalLaunch(action: LaunchAction, cwd: string | null): void {
  void cwd; // 保留在签名里：调用点按「动作 + 目录」成对传，未来若加 cwd 校验就落在这
  if (action.kind === "resume" && !isValidSessionId(action.sid)) {
    throw new Error(`非法 sessionId（拒绝拼入命令）: ${JSON.stringify(action.sid)}`);
  }
}

/** 对应 `buildAttachCmd`：接回一个已存在的 tmux 会话，不启动任何东西。 */
export function planAttach(name: string): LaunchPlanBuild {
  if (!isValidTmuxName(name)) {
    throw new Error(`非法 tmux 会话名(拒绝拼入命令): ${JSON.stringify(name)}`);
  }
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "attach", name },
    container: { kind: "tmux", name, nameQuoting: "quoted", mode: "attach-only" },
    cwd: null,
    account: { kind: "base" },
    launcherOverride: undefined,
    ccmSid: undefined,
  };
  return { ctx, plan: buildLaunchPlan(ctx) };
}
