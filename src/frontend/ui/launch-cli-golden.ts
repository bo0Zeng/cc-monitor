/**
 * 那一行 `ccm …` 的入库夹具（`src/backend/control/launch_render/fixtures/cli-golden.json`）的用例表与落盘函数。
 *
 * `req` 由生产的 `buildCliRenderRequest` 现产，Rust 侧拿生产 wire 类型反序列化、跑生产命令、与 `out` 逐字节比
 * ⇒ 钉的是「生产请求构造 → 线 → Rust 映射 → 生产命令」这一整条。`out` 是**手写的期望**：改渲染器的产出 ⇒ 回来改这里，
 * 这一步必须是人做的。`print-parity:` 那 4 条是 `tests/e2e/ccm-print-parity.sh` 喂给真 `ccm --ccm-print` 的四行（按名取）。
 * ok 与 refusal 两类都要覆盖：只比 ok 的话，「该拒却渲染出来了」抓不到。
 *
 * ⚠ 留在 `src/` 而不在 `tests/`：`tests/` 不在 `tsconfig.json` 的 `include` 里，`LaunchContext` 改了形状 tsc 看不见这张表。
 */
import { AGENT_PROFILE } from "./agent-profile.ts";
import type { LaunchContext } from "./launch-types.ts";
import { buildCliRenderRequest } from "./remote-launch-run.ts";

/** 能力齐全的那一份（`ccm --ccm-probe` 在有 tmux 的机器上吐的那一串）。 */
const ALL_CAPS = [
  "new", "resume", "attach", "tmux", "account", "model", "cwd", "agent",
  "launcher", "ccm-sid", "print", "detach", "tmux-size",
];

export interface CliGoldenCase {
  name: string;
  /** 那台 `ccm` 的能力：不上线（渲染在那台后端里、能力问它自己），只当对拍那一侧的输入落进夹具。 */
  caps: string[];
  ctx: LaunchContext;
  /** 期望：渲得出（`true`，`out` 是命令）还是拒（`false`，`out` 是理由）。 */
  ok: boolean;
  /** **手写**的期望串（见文件头注）。 */
  out: string;
}

const ACCT = "/home/u/.claude-accts/z";
const TOKEN = "0123456789abcdef0123456789abcdef";
const base = (over: Partial<LaunchContext> = {}): LaunchContext => ({
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
  { name: "只有目录没有名字 ⇒ --account-dir", caps: ALL_CAPS, ctx: base({ account: { kind: "account", configDir: ACCT } }),
    ok: true, out: "ccm -- new --account-dir /home/u/.claude-accts/z" },
  { name: "resume + tmux + 具名账号", caps: ALL_CAPS, ctx: base({
      action: { kind: "resume", sid: "abc-123" },
      container: { kind: "tmux", name: "cc-abc123", mode: "create" },
      account: { kind: "account", name: "z", configDir: ACCT },
    }), ok: true, out: "ccm --resume abc-123 -- --ccm-tmux=cc-abc123 --account z" },
  { name: "resume + cwd + model", caps: ALL_CAPS, ctx: base({
      action: { kind: "resume", sid: "s1" }, cwd: "/w", modelOverride: "opus",
    }), ok: true, out: "ccm --resume s1 --model opus -- --base --cwd /w" },
  { name: "启动期令牌", caps: ALL_CAPS, ctx: base({ action: { kind: "resume", sid: "s1" }, rbindToken: TOKEN }),
    ok: true, out: `ccm --resume s1 -- --base --ccm-rbind-token ${TOKEN}` },
  { name: "就地 resume：外层只包那一行直路", caps: ALL_CAPS, ctx: base({
      action: { kind: "resume", sid: "s1" },
      container: { kind: "tmux", name: "cc-x", mode: "send-into" },
    }), ok: true, out: "tmux send-keys -t '=cc-x:' 'ccm --resume s1 -- --base' Enter; tmux attach -t '=cc-x:'" },
  { name: "identity（--ccm-sid）", caps: ALL_CAPS, ctx: base({ ccmSid: "sid-1" }),
    ok: true, out: "ccm -- new --ccm-sid=sid-1 --base" },
  { name: "自定义 launcher", caps: ALL_CAPS, ctx: base({ launcherOverride: "mycc" }),
    ok: true, out: "ccm -- new --base --launcher mycc" },
  { name: "launcher 等于默认 ⇒ 不吐 --launcher", caps: ALL_CAPS, ctx: base({ launcherOverride: AGENT_PROFILE.defaultLauncher }),
    ok: true, out: "ccm -- new --base" },
  { name: "attach（不起 agent，不收修饰）", caps: ALL_CAPS, ctx: base({
      action: { kind: "attach", name: "cc-foo" },
      container: { kind: "tmux", name: "cc-foo", mode: "attach-only" },
      rbindToken: TOKEN,
    }), ok: true, out: "ccm -- --attach cc-foo" },
  { name: "需要 quote 的 cwd", caps: ALL_CAPS, ctx: base({ cwd: "/home/用户/带 空格" }),
    ok: true, out: "ccm -- new --base --cwd '/home/用户/带 空格'" },
  // ---- refusal 类（表达不了就拒，不渲一条丢了修饰的命令） ----
  { name: "缺无条件能力 cwd", caps: ALL_CAPS.filter((c) => c !== "cwd"), ctx: base(),
    ok: false, out: "这台机器上的 ccm 做不到这样起会话（缺 cwd）" },
  { name: "建 tmux 会话而这台没有 tmux", caps: ALL_CAPS.filter((c) => c !== "tmux"), ctx: base({
      container: { kind: "tmux", name: "cc-x", mode: "create" },
    }), ok: false, out: "这台机器上的 ccm 做不到这样起会话（缺 tmux）" },
  { name: "已触发的 model 维度要的能力缺失", caps: ALL_CAPS.filter((c) => c !== "model"), ctx: base({ modelOverride: "opus" }),
    ok: false, out: "这台机器上的 ccm 不认 model 这一项设置（缺 model）" },
  { name: "已触发的 account 维度要的能力缺失", caps: ALL_CAPS.filter((c) => c !== "account"), ctx: base(),
    ok: false, out: "这台机器上的 ccm 不认 account 这一项设置（缺 account）" },
  { name: "坏 sid", caps: ALL_CAPS, ctx: base({ action: { kind: "resume", sid: "-x" } }),
    ok: false, out: "会话 ID \"-x\" 不合法（1 到 64 位，只许 A-Z a-z 0-9 与 -，不以 - 开头）" },
  { name: "坏令牌", caps: ALL_CAPS, ctx: base({ rbindToken: "XYZ" }),
    ok: false, out: "启动期令牌 \"XYZ\" 不合法（要 32 位小写十六进制）" },
  // ---- `ccm-print-parity` 的四个场景（那套 e2e 按名取 `out`） ----
  { name: "print-parity:resumeTmuxWithIdentity", caps: ALL_CAPS, ctx: base({
      action: { kind: "resume", sid: "p1" },
      container: { kind: "tmux", name: "cc-p1", mode: "create" },
      cwd: "/tmp", launcherOverride: "claude", ccmSid: "p1",
    }), ok: true, out: "ccm --resume p1 -- --ccm-tmux=cc-p1 --ccm-sid=p1 --base --cwd /tmp" },
  { name: "print-parity:newTmuxCustomLauncher", caps: ALL_CAPS, ctx: base({
      container: { kind: "tmux", name: "cc-proj", mode: "create" },
      cwd: "/home/pi/my proj", launcherOverride: "CCMPROBE",
    }), ok: true, out: "ccm -- new --ccm-tmux=cc-proj --base --cwd '/home/pi/my proj' --launcher CCMPROBE" },
  { name: "print-parity:attach", caps: ALL_CAPS, ctx: base({
      action: { kind: "attach", name: "cc-p1" },
      container: { kind: "tmux", name: "cc-p1", mode: "attach-only" },
    }), ok: true, out: "ccm -- --attach cc-p1" },
  // 真 ccm 收到 --model 后真的 export ANTHROPIC_MODEL。
  { name: "print-parity:resumeTmuxWithModel", caps: ALL_CAPS, ctx: base({
      action: { kind: "resume", sid: "p1" },
      container: { kind: "tmux", name: "cc-p1", mode: "create" },
      cwd: "/tmp", launcherOverride: "claude", ccmSid: "p1", modelOverride: "opus",
    }), ok: true, out: "ccm --resume p1 --model opus -- --ccm-tmux=cc-p1 --ccm-sid=p1 --base --cwd /tmp" },
];

export function renderCliGoldenFixture(): string {
  return `${JSON.stringify(
    {
      _: "由 src/frontend/ui/launch-cli-golden.ts 生成，勿手改。重生成：npm run gen:cli-golden",
      defaultLauncher: AGENT_PROFILE.defaultLauncher,
      cases: CLI_GOLDEN_CASES.map((c) => ({
        name: c.name,
        // `req` 由生产代码构造（`buildCliRenderRequest`）：字段名 · `deny_unknown_fields` · 映射臂 · 请求构造漏没漏字段一次覆盖。
        caps: c.caps,
        req: buildCliRenderRequest(c.ctx),
        ok: c.ok,
        out: c.out,
      })),
    },
    null,
    2,
  )}\n`;
}
