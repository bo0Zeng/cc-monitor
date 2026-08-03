/**
 * U8c-2c-1：**ccm 调用行的跨语言黄金串来源**（同 `launch-payload-golden.ts` 的机制）。
 *
 * `tryRenderCli` 是**装了 ccm 的机器上真正在跑的那条路**（U8c-2b-0 摸底实测：
 * `renderLaunchCommand` 先试它，成功就直接返回，`renderFallback` 根本不执行）。
 * 它现在有了 Rust 对侧（`backend::control::ccm_invocation::render_ccm_invocation`），
 * 保证两者一致的必须是判据，不是注释。
 *
 * ⚠ **ok 与 refusal 两类都要覆盖**：只比 ok 的话，「该降级却渲染出来了」这一类抓不到 ——
 * 而那正是 `doc/INVARIANTS.md` §33 铁律要防的形态（CLI 渲染器对表达不了的东西必须放弃、
 * 不得近似）。夹具里因此**两类各占一半**。
 */
import { buildLaunchPlan } from "./launch-plan.ts";
import { tryRenderCli } from "./launch-render-cli.ts";
import { AGENT_PROFILE } from "./agent-profile.ts";
import type { LaunchContext } from "./launch-plan.ts";
import type { CcmProbeResult } from "./ccm-probe.ts";
import { buildCliRenderRequest } from "./remote-launch-run.ts";

/** 能力齐全的探测结果（`shared/ccm --ccm-probe` 今天真实吐出的那一串）。 */
const ALL_CAPS = [
  "new", "resume", "attach", "tmux", "account", "model", "cwd", "agent",
  "launcher", "ccm-sid", "print", "detach", "tmux-size",
];

export interface CliGoldenCase {
  name: string;
  /** 探测结果：`null` = 未装。 */
  caps: string[] | null;
  ctx: LaunchContext;
}

const ACCT = "/home/u/.claude-accts/z";
const base = (over: Partial<LaunchContext> = {}): LaunchContext => ({
  transport: { kind: "ssh" },
  action: { kind: "new" },
  container: { kind: "none" },
  cwd: null,
  account: { kind: "base" },
  launcherOverride: undefined,
  ccmSid: undefined,
  ...over,
});

/** 用例集 —— **这是对拍面的唯一定义处**。ok / refusal 两类都要有。 */
export const CLI_GOLDEN_CASES: readonly CliGoldenCase[] = [
  // ---- ok 类 ----
  { name: "new + base", caps: ALL_CAPS, ctx: base() },
  { name: "new + 具名账号", caps: ALL_CAPS, ctx: base({ account: { kind: "account", name: "z", configDir: ACCT } }) },
  { name: "resume + tmux + 具名账号", caps: ALL_CAPS, ctx: base({
      action: { kind: "resume", sid: "abc-123" },
      container: { kind: "tmux", name: "cc-abc123", nameQuoting: "raw", mode: "create-or-attach" },
      account: { kind: "account", name: "z", configDir: ACCT },
    }) },
  { name: "resume + cwd + model", caps: ALL_CAPS, ctx: base({
      action: { kind: "resume", sid: "s1" }, cwd: "/w", modelOverride: "opus",
    }) },
  { name: "identity（--ccm-sid）", caps: ALL_CAPS, ctx: base({ ccmSid: "sid-1" }) },
  { name: "自定义 launcher", caps: ALL_CAPS, ctx: base({ launcherOverride: "mycc" }) },
  { name: "launcher 等于默认 ⇒ 不吐 --launcher", caps: ALL_CAPS, ctx: base({ launcherOverride: AGENT_PROFILE.defaultLauncher }) },
  { name: "attach（分支在维度循环之前 return）", caps: ALL_CAPS, ctx: base({
      action: { kind: "attach", name: "cc-foo" },
      container: { kind: "tmux", name: "cc-foo", nameQuoting: "raw", mode: "attach-only" },
    }) },
  { name: "需要 quote 的 cwd", caps: ALL_CAPS, ctx: base({ cwd: "/home/用户/带 空格" }) },
  // ---- refusal 类（§33：表达不了就必须放弃） ----
  { name: "未装 ccm", caps: null, ctx: base() },
  { name: "本地 transport", caps: ALL_CAPS, ctx: base({ transport: { kind: "local" } }) },
  { name: "缺静态能力 tmux", caps: ALL_CAPS.filter((c) => c !== "tmux"), ctx: base() },
  { name: "#76 防线：send-into 无 CLI 等价语法", caps: ALL_CAPS, ctx: base({
      container: { kind: "tmux", name: "cc-x", nameQuoting: "raw", mode: "send-into" },
    }) },
  { name: "§35 安全网：只有 configDir 没有名字 ⇒ 说不出 --account", caps: ALL_CAPS, ctx: base({
      account: { kind: "account", configDir: ACCT },
    }) },
  { name: "已触发的 model 维度要的能力缺失", caps: ALL_CAPS.filter((c) => c !== "model"), ctx: base({ modelOverride: "opus" }) },
  { name: "已触发的 account 维度要的能力缺失", caps: ALL_CAPS.filter((c) => c !== "account"), ctx: base() },
];

function probeOf(caps: string[] | null): CcmProbeResult {
  return caps === null
    ? { installed: false, version: null, capabilities: new Set() }
    : { installed: true, version: "2", capabilities: new Set(caps) };
}

export function renderCliGoldenFixture(): string {
  return `${JSON.stringify(
    {
      _: "由 src/launch-cli-golden.ts 生成，勿手改。重生成：npm run gen:payload-golden",
      defaultLauncher: AGENT_PROFILE.defaultLauncher,
      cases: CLI_GOLDEN_CASES.map((c) => {
        const plan = buildLaunchPlan(c.ctx);
        const probe = probeOf(c.caps);
        const r = tryRenderCli(plan, c.ctx, probe);
        return {
          name: c.name,
          // ★ `req` 由**生产代码**构造（`buildCliRenderRequest`，`renderCliViaBackend` 用的同一个）。
          // Rust 侧拿**生产 wire 类型**反序列化它、跑**生产命令**，再与 `out` 比 ——
          // 于是「字段名对不对 / `deny_unknown_fields` 在不在 / 映射臂对不对 / 请求构造漏没漏字段」
          // 四件事一次覆盖，而且是**行为对拍不是文本对拍**。
          req: buildCliRenderRequest(c.ctx, plan, probe),
          ok: r.ok,
          out: r.ok ? r.cmd : r.reason,
        };
      }),
    },
    null,
    2,
  )}\n`;
}
