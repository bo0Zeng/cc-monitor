// F03「--print 平价预言机」的输入源：从真 renderCli 取一批代表性场景的完整 `ccm …` 调用行。
// 与 e2e/tmux-target-emit.mts 同一模式（从真 builder/renderer 取生产串，不手搓等价命令）。
import { buildLaunchPlan } from "../src/launch-plan.ts";
import { renderCli } from "../src/launch-render-cli.ts";
import type { LaunchContext } from "../src/launch-plan.ts";

function line(ctx: LaunchContext): string {
  return renderCli(buildLaunchPlan(ctx), ctx);
}

const scenarios: Record<string, LaunchContext> = {
  resumeTmuxWithIdentity: {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid: "p1" },
    container: { kind: "tmux", name: "cc-p1", nameQuoting: "raw", mode: "create-or-attach" },
    cwd: "/tmp",
    account: { kind: "base" },
    launcherOverride: "claude",
    ccmSid: "p1",
  },
  newTmuxCustomLauncher: {
    transport: { kind: "ssh" },
    action: { kind: "new" },
    container: { kind: "tmux", name: "cc-proj", nameQuoting: "quoted", mode: "create-or-attach" },
    cwd: "/home/pi/my proj",
    account: { kind: "base" },
    launcherOverride: "CCMPROBE",
    ccmSid: undefined,
  },
  attach: {
    transport: { kind: "ssh" },
    action: { kind: "attach", name: "cc-p1" },
    container: { kind: "tmux", name: "cc-p1", nameQuoting: "quoted", mode: "attach-only" },
    cwd: null,
    account: { kind: "base" },
    launcherOverride: undefined,
    ccmSid: undefined,
  },
  // F08：ccm 学会了 --model，闭合 R14①——验证真 ccm 收到 --model 后真的 export ANTHROPIC_MODEL。
  // account 用 base（同其余场景，避免这里额外牵扯账号 manifest 解析——组合测试见 ccm-cli.test.sh）。
  resumeTmuxWithModel: {
    transport: { kind: "ssh" },
    action: { kind: "resume", sid: "p1" },
    container: { kind: "tmux", name: "cc-p1", nameQuoting: "raw", mode: "create-or-attach" },
    cwd: "/tmp",
    account: { kind: "base" },
    launcherOverride: "claude",
    ccmSid: "p1",
    modelOverride: "opus",
  },
};

for (const [label, ctx] of Object.entries(scenarios)) {
  console.log(`${label}\t${line(ctx)}`);
}
