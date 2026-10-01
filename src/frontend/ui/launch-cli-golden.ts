/**
 * `ccm …` 调用行的入库夹具（`src/backend/control/launch_render/fixtures/cli-golden.json`）的用例表与落盘函数。
 *
 * # 它从「TS 渲染器的金串」换成了「生产请求 ＋ 手写期望」
 *
 * TS 那份 `ccm …` 渲染器已删（生产从 U8c-2c-2 起就走 Rust 的
 * `backend::control::ccm_invocation::render_ccm_invocation`）。这份夹具**没有**跟着删，
 * 因为它一直在钉两件事，只有第一件随渲染器走：
 *
 *  1. 「两种语言渲出同一行」—— 另一种语言没了，这一件没了（已知代价）。
 *  2. **生产的请求构造 → 线 → Rust 反序列化与映射 → 生产命令**这一整条。
 *     `req` 由生产的 `buildCliRenderRequest`（`renderCliViaBackend` 用的同一个）现产，
 *     Rust 侧拿生产 wire 类型反序列化、跑 `render_ccm_launch`、与 `out` 逐字节比。
 *     这一条买的东西与 TS 渲染器无关：没有它，请求构造与 wire 映射上的变异
 *     （`isSsh` 恒 false · 具名账号降成 base · 丢 cwd / model / ccmSid …）全部静默 ——
 *     它们的样子都是「降级到载荷那条，功能不变砖、门禁全绿」。
 *
 * ⇒ `out` 是**手写的期望**：前 16 条是 TS 渲染器最后一次跑出、与 Rust 逐字节对过的原样，
 *   `print-parity:` 那 4 条是 `tests/e2e/ccm-print-parity.sh` 喂给真 `ccm --print` 的四行
 *   （那套 e2e 从本夹具按名取行 —— 它验的是「生产渲染器那一行真 ccm 读得懂」）。
 *   改 Rust 渲染器的产出 ⇒ 回来改这里的期望，这一步必须是人做的。
 * `ccm new …` → `ccm …`、`resume <sid>` / `attach <名>` → `--resume <sid>` / `--attach <名>`（位置动作取消）。
 *
 * ⚠ **ok 与 refusal 两类都要覆盖**：只比 ok 的话，「该降级却渲染出来了」抓不到 ——
 * 而那正是 `src/doc/INVARIANTS.md` §33 铁律要防的形态。
 *
 * ⚠ 留在 `src/` 而不在 `tests/`：`tests/` 不在 `tsconfig.json` 的 `include` 里，
 * `LaunchContext` 改了形状 tsc 看不见这张表（另两份金样本发生器住 `src/` 是同一条理由）。
 */
import { buildLaunchPlan } from "./launch-plan.ts";
import { AGENT_PROFILE } from "./agent-profile.ts";
import type { LaunchContext } from "./launch-types.ts";
import { buildCliRenderRequest } from "./remote-launch-run.ts";

/** 能力齐全的探测结果（`ccm --ccm-probe` 今天真实吐出的那一串）。 */
const ALL_CAPS = [
  "new", "resume", "attach", "tmux", "account", "model", "cwd", "agent",
  "launcher", "ccm-sid", "print", "detach", "tmux-size",
];

export interface CliGoldenCase {
  name: string;
  /** 那台 `ccm` 的能力：`null` = 未装。不上线（渲染进了那台后端、能力问它自己），只当对拍那一侧的输入落进夹具。 */
  caps: string[] | null;
  ctx: LaunchContext;
  /** 期望：渲得出（`true`，`out` 是命令）还是诚实降级（`false`，`out` 是降级理由）。 */
  ok: boolean;
  /** **手写**的期望串（见文件头注）。 */
  out: string;
}

const ACCT = "/home/u/.claude-alt/z";
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
  { name: "new + base", caps: ALL_CAPS, ctx: base(), ok: true, out: "ccm -- new --base" },
  { name: "new + 具名账号", caps: ALL_CAPS, ctx: base({ account: { kind: "account", name: "z", configDir: ACCT } }),
    ok: true, out: "ccm -- new --account z" },
  { name: "resume + tmux + 具名账号", caps: ALL_CAPS, ctx: base({
      action: { kind: "resume", sid: "abc-123" },
      container: { kind: "tmux", name: "cc-abc123", nameQuoting: "raw", mode: "create" },
      account: { kind: "account", name: "z", configDir: ACCT },
    }), ok: true, out: "ccm --resume abc-123 -- --ccm-tmux=cc-abc123 --account z" },
  { name: "resume + cwd + model", caps: ALL_CAPS, ctx: base({
      action: { kind: "resume", sid: "s1" }, cwd: "/w", modelOverride: "opus",
    }), ok: true, out: "ccm --resume s1 --model opus -- --base --cwd /w" },
  { name: "identity（--ccm-sid）", caps: ALL_CAPS, ctx: base({ ccmSid: "sid-1" }),
    ok: true, out: "ccm -- new --ccm-sid=sid-1 --base" },
  { name: "自定义 launcher", caps: ALL_CAPS, ctx: base({ launcherOverride: "mycc" }),
    ok: true, out: "ccm -- new --base --launcher mycc" },
  { name: "launcher 等于默认 ⇒ 不吐 --launcher", caps: ALL_CAPS, ctx: base({ launcherOverride: AGENT_PROFILE.defaultLauncher }),
    ok: true, out: "ccm -- new --base" },
  { name: "attach（分支在维度循环之前 return）", caps: ALL_CAPS, ctx: base({
      action: { kind: "attach", name: "cc-foo" },
      container: { kind: "tmux", name: "cc-foo", nameQuoting: "raw", mode: "attach-only" },
    }), ok: true, out: "ccm -- --attach cc-foo" },
  { name: "需要 quote 的 cwd", caps: ALL_CAPS, ctx: base({ cwd: "/home/用户/带 空格" }),
    ok: true, out: "ccm -- new --base --cwd '/home/用户/带 空格'" },
  // ---- refusal 类（§33：表达不了就必须放弃） ----
  { name: "未装 ccm", caps: null, ctx: base(), ok: false, out: "远端还没装后端" },
  { name: "本地 transport", caps: ALL_CAPS, ctx: base({ transport: { kind: "local" } }),
    ok: false, out: "Windows 本机不用 ccm 命令起会话" },
  { name: "缺静态能力 tmux", caps: ALL_CAPS.filter((c) => c !== "tmux"), ctx: base(),
    ok: false, out: "远端的后端太旧，不支持这样起会话（缺 tmux）" },
  { name: "#76 防线：send-into 无 CLI 等价语法", caps: ALL_CAPS, ctx: base({
      container: { kind: "tmux", name: "cc-x", nameQuoting: "raw", mode: "send-into" },
    }), ok: false, out: "ccm 命令没法在已有的 tmux 会话里就地起新会话" },
  { name: "§35 安全网：只有 configDir 没有名字 ⇒ 说不出 --account", caps: ALL_CAPS, ctx: base({
      account: { kind: "account", configDir: ACCT },
    }), ok: false, out: "这一项设置（account）写不成 ccm 参数" },
  { name: "已触发的 model 维度要的能力缺失", caps: ALL_CAPS.filter((c) => c !== "model"), ctx: base({ modelOverride: "opus" }),
    ok: false, out: "远端的后端太旧，不认 model 这一项设置（缺 model）" },
  { name: "已触发的 account 维度要的能力缺失", caps: ALL_CAPS.filter((c) => c !== "account"), ctx: base(),
    ok: false, out: "远端的后端太旧，不认 account 这一项设置（缺 account）" },
  // ---- `ccm-print-parity` 的四个场景（那套 e2e 按名取 `out`，12 条断言钉着这几个值） ----
  { name: "print-parity:resumeTmuxWithIdentity", caps: ALL_CAPS, ctx: base({
      action: { kind: "resume", sid: "p1" },
      container: { kind: "tmux", name: "cc-p1", nameQuoting: "raw", mode: "create" },
      cwd: "/tmp", launcherOverride: "claude", ccmSid: "p1",
    }), ok: true, out: "ccm --resume p1 -- --ccm-tmux=cc-p1 --ccm-sid=p1 --base --cwd /tmp" },
  { name: "print-parity:newTmuxCustomLauncher", caps: ALL_CAPS, ctx: base({
      container: { kind: "tmux", name: "cc-proj", nameQuoting: "quoted", mode: "create" },
      cwd: "/home/pi/my proj", launcherOverride: "CCMPROBE",
    }), ok: true, out: "ccm -- new --ccm-tmux=cc-proj --base --cwd '/home/pi/my proj' --launcher CCMPROBE" },
  { name: "print-parity:attach", caps: ALL_CAPS, ctx: base({
      action: { kind: "attach", name: "cc-p1" },
      container: { kind: "tmux", name: "cc-p1", nameQuoting: "quoted", mode: "attach-only" },
    }), ok: true, out: "ccm -- --attach cc-p1" },
  // F08：真 ccm 收到 --model 后真的 export ANTHROPIC_MODEL。
  { name: "print-parity:resumeTmuxWithModel", caps: ALL_CAPS, ctx: base({
      action: { kind: "resume", sid: "p1" },
      container: { kind: "tmux", name: "cc-p1", nameQuoting: "raw", mode: "create" },
      cwd: "/tmp", launcherOverride: "claude", ccmSid: "p1", modelOverride: "opus",
    }), ok: true, out: "ccm --resume p1 --model opus -- --ccm-tmux=cc-p1 --ccm-sid=p1 --base --cwd /tmp" },
];

export function renderCliGoldenFixture(): string {
  return `${JSON.stringify(
    {
      _: "由 src/frontend/ui/launch-cli-golden.ts 生成，勿手改。重生成：npm run gen:payload-golden",
      defaultLauncher: AGENT_PROFILE.defaultLauncher,
      cases: CLI_GOLDEN_CASES.map((c) => ({
        name: c.name,
        // ★ `req` 由**生产代码**构造（`buildCliRenderRequest`，`renderCliViaBackend` 用的同一个）。
        // Rust 侧拿**生产 wire 类型**反序列化它、跑**生产命令**，再与 `out` 比 ——
        // 于是「字段名对不对 / `deny_unknown_fields` 在不在 / 映射臂对不对 / 请求构造漏没漏字段」
        // 四件事一次覆盖，而且是**行为对拍不是文本对拍**。
        caps: c.caps,
        req: buildCliRenderRequest(c.ctx, buildLaunchPlan(c.ctx)),
        ok: c.ok,
        out: c.out,
      })),
    },
    null,
    2,
  )}\n`;
}
