/**
 * F03（unify-launch）：「legacy 请求 → LaunchContext/LaunchPlan」翻译层。每个函数对应今天
 * `remote-launch.ts` 里一个 builder 的校验/意图构造逻辑，逐字保留其校验顺序与错误文案
 * （调用方 toast 依赖这些文案）。`remote-launch.ts` 的 builder 改造后只剩「调这里 + 交渲染器」。
 */
import { AGENT_PROFILE } from "./agent-profile.ts";
import { buildLaunchPlan } from "./launch-plan.ts";
import type {
  LaunchAccount,
  LaunchContext,
  LaunchModifiers,
  LaunchPlan,
} from "./launch-types.ts";

export interface LaunchPlanBuild {
  ctx: LaunchContext;
  plan: LaunchPlan;
}

/** F05：触发条件仍是 `configDir` 单独非空（同 F03 原行为，`remote-launch.test.ts` 的老式
 *  直调路径只传 `configDir` 不传名字，必须继续正确触发兜底渲染器的 env 注入）；`name` 是
 *  可选增强——传了就线通进 IR 供 CLI 渲染器用，没传时 `LaunchAccount.name` 是 `undefined`，
 *  CLI 渲染器（Rust `ccm_invocation.rs` 的 `account` 维度）会诚实地对这种情形放弃（强制走兜底），而不是把整个
 *  账号状态错误地降级成 `base`（那会连兜底渲染器的 env 注入也漏掉，是真回归）。 */
function accountOf(configDir?: string, name?: string): LaunchAccount {
  return configDir ? { kind: "account", name, configDir } : { kind: "base" };
}

/** 原先对应 `remote-launch.ts` 的 `buildResumeDirectCmd`（那五个 builder 已删，生产走 `buildLaunchRenderRequest` → Rust）：无容器（直连），resume 到当前登录 shell。
 *
 *  🔴 ** 那张表里「今天做不到 ↗ 的那一档」就是这一格**（`container:{kind:"none"}`，
 *  `§6.1`/`§5 方案 A` 明确不覆盖它，因为它没有 tmux 可以挂 `@ccm_sid`）。
 *  步 1 之后它**自动**带上启动期令牌（步 3 起真的有人铸了，铸币口是
 *  `remote-launch-run.ts::mintRbindToken`）—— 本函数为此**一行特殊处理都没有**：
 *  `rbindToken` 和其余修饰一样只是进 `ctx`，`RBIND_TOKEN_DIMENSION` 往 `plan.env` 推一条
 *  `EnvOp`，而 `EnvOp` 是容器无关的。**「自动」这件事本身有判据**
 *  （`tests/frontend/ui/launch-requests.vitest.ts` 的「容器无关」那一组：两条起法各渲一次、
 *  断言同一条 `export CCM_RBIND_TOKEN=` 都在）。 */
export function planResumeDirect(
  sid: string,
  cwd: string,
  launcher = AGENT_PROFILE.defaultLauncher,
  mods: LaunchModifiers = {},
): LaunchPlanBuild {
  const { configDir, accountName, modelOverride, rbindToken } = mods;
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid },
    container: { kind: "none" },
    cwd: cwd.trim() || null,
    account: accountOf(configDir, accountName),
    launcherOverride: launcher,
    ccmSid: undefined,
    modelOverride,
    rbindToken,
  };
  return { ctx, plan: buildLaunchPlan(ctx) };
}

/** 原先对应 `remote-launch.ts` 的 `buildResumeTmuxCmd`（那五个 builder 已删，生产走 `buildLaunchRenderRequest` → Rust）：新建/幂等接回 tmux，resume 进去。 */
export function planResumeTmux(
  sid: string,
  cwd: string,
  launcher = AGENT_PROFILE.defaultLauncher,
  name: string,
  mods: LaunchModifiers = {},
): LaunchPlanBuild {
  const { configDir, accountName, modelOverride, rbindToken } = mods;
  // F13（用户 2026-08-03：「要撞名检查」）：**这里原本有一个产名的默认值，已删。**
  //
  // 它是撞名的根因之一：它产的 `<sid8>-cc` 与那个铸名口（`K-R96` 之前叫 `pickFreshTmuxName`，
  // 今天是 `mintSessionTmuxName`）的基名**逐字相同**，
  // 而它**不做撞名避让** —— 也就是说另一处精心让出 `<sid8>-cc-2`，这里直接产
  // `<sid8>-cc` 撞上去。旧注释只钉了「两处同源，别只改一边」，没钉「避让也要相同」。
  //
  // 删它是**零行为改动**，但理由要说准（我第一版说错了，`tsc` 当场证伪）：
  // **生产上三个真实调用点都传了 name**（`tabs.ts` 传 `mintSessionTmuxName(cwd, existing)`、
  // `account-restart.ts` 传复用的既有名、`fork-flow.ts` 有 `a.tmuxName` 非空守卫）——
  // 但两个 **wrapper 的类型**（`buildResumeTmuxCmd` / `runRemoteResumeTmux`）当时写的是
  // `name?: string`，所以「省略 name」在**类型上是允许的**，只是碰巧没人这么调。
  // ⇒ 把这一路的 `name` 全改成必填，让 `tsc` 把「碰巧」变成「不可能」。
  // 会话名的形状不在这里判：规则只有一份（后端 `control/gate_rules.rs`），渲染那一跳（`payload.rs` 外层 · `ccm_invocation.rs`）判、
  //   判不过带 `REFUSE:` 标拒。这里原来的内联式子（首字符不许 `-`，`raw` 那一支唯一挡前导 `-` 的一道）今天由后端 `gate_rules` 的新建那一条接住。
  const tmuxName = name;
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid },
    container: { kind: "tmux", name: tmuxName, nameQuoting: "raw", mode: "create" },
    cwd: cwd.trim() || null,
    account: accountOf(configDir, accountName),
    launcherOverride: launcher,
    ccmSid: sid, // #72：自建 resume 会话打完整 sid，供 findClaudeTmux 精确命中
    modelOverride,
    rbindToken,
  };
  return { ctx, plan: buildLaunchPlan(ctx) };
}

/** 原先对应 `remote-launch.ts` 的 `buildResumeIntoExistingTmuxCmd`（那五个 builder 已删，生产走 `buildLaunchRenderRequest` → Rust）：往已存在的 idle tmux 就地送键，不 new-session。 */
export function planResumeIntoExistingTmux(
  sid: string,
  name: string,
  launcher = AGENT_PROFILE.defaultLauncher,
  mods: LaunchModifiers = {},
): LaunchPlanBuild {
  const { configDir, accountName, modelOverride, rbindToken } = mods;
  // 名字原样进请求（送进一个已在的会话 ⇒ 渲染侧按「已有会话」那一条判；`raw` 那一支另有裸拼的渲染前提）。
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid },
    container: { kind: "tmux", name, nameQuoting: "raw", mode: "send-into" },
    cwd: null,
    account: accountOf(configDir, accountName),
    launcherOverride: launcher,
    ccmSid: undefined, // 复用会话已在建时打过标，不重设（同今天行为）
    modelOverride,
    rbindToken,
  };
  return { ctx, plan: buildLaunchPlan(ctx) };
}

/** 原先对应 `remote-launch.ts` 的 `buildLauncherCmd`（那五个 builder 已删，生产走 `buildLaunchRenderRequest` → Rust）：「在这台机开新 Claude」——新建/幂等接回 tmux，起全新会话。 */
export function planLauncher(
  cwd: string,
  tmuxName: string,
  command = AGENT_PROFILE.defaultLauncher,
  mods: LaunchModifiers = {},
): LaunchPlanBuild {
  const { configDir, accountName, modelOverride, rbindToken } = mods;
  const name = tmuxName.trim();
  // 新建那一条（非空 · 不以 `-` 开头 · 无 `*?.:=` · 无控制符与欺骗字符 · ≤128）由渲染侧调后端 `gate_rules` 判。
  const ctx: LaunchContext = {
    transport: { kind: "ssh" },
    action: { kind: "new" },
    container: { kind: "tmux", name, nameQuoting: "quoted", mode: "create" },
    cwd: cwd.trim() || null,
    account: accountOf(configDir, accountName),
    launcherOverride: command,
    ccmSid: undefined, // 今天就不设——已知 F04 缺口，本次原样保留、不顺手"修一半"
    modelOverride,
    rbindToken,
  };
  return { ctx, plan: buildLaunchPlan(ctx) };
}

// 这里原来有 `validateLocalLaunch`〔散文墓碑〕—— 本机路径在发起 IPC 之前的「前置校验」，
// 它唯一的一格是 sid 字符集（TS `isValidSessionId`〔散文墓碑〕）。本机拉起那条路上 Rust 侧自己判同一件事
// （`history.rs` 本机决策那一处，今天调共享那一份 `shell_quote_core::session_id_ok`）⇒ 前端这份删了，四个调用点一起去掉。
// R07 那段「为什么不真接上本地 IR」的论证原文住 `src/doc/INVARIANTS.md` §36（那一条只绑 Windows 分支）。

/** 原先对应 `remote-launch.ts` 的 `buildAttachCmd`（那五个 builder 已删，生产走 `buildLaunchRenderRequest` → Rust）：接回一个已存在的 tmux 会话，不启动任何东西。
 *
 *  ⚠ **刻意不收 `mods`**（原状），于是也**不带启动期令牌** —— 不是漏了：
 *  attach 一个 agent 进程都不起，而令牌的唯一消费者是 agent 进程的 `environ`。
 *  `RBIND_TOKEN_DIMENSION.applies` 那条 `action.kind` 判断是第二道
 *  同向的闸（万一将来这里开始收 `mods`，它也不会往 attach 里注一个没人读的敏感值）。 */
export function planAttach(name: string): LaunchPlanBuild {
  // 已有会话那一条（②：拒绝集 ＋ 非空）由渲染侧调后端 `gate_rules` 判；寻址恒是 `=<名>:`。
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
