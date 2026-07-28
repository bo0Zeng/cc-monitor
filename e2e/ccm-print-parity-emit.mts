// F03「--print 平价预言机」的输入源：从真 CLI 渲染器取一批代表性场景的完整 `ccm …` 调用行。
// 与 e2e/tmux-target-emit.mts 同一模式（从真 builder/renderer 取生产串，不手搓等价命令）。
//
// R04① 后入口是 `tryRenderCli`（原 `canRenderCli`+`renderCli` 合成）。这里给一个"能力齐全"的
// 假探测结果——本预言机要验的是**渲染出的命令行与真 ccm 的解析是否对得上**，不是降级逻辑，
// 故必须让它走成功分支；真降级路径由 launch-render-cli.test.ts 覆盖。
//
// **注意（R04 实现期踩到）**：本目录不在 `tsconfig.json` 的 `include: ["src"]` 里，
// 所以改动生产侧导出签名时 **tsc 抓不到这里**——只有真跑 e2e 才会暴露（本次就是这样发现的：
// tsc 0 + npm test 701 全绿，而 ccm-print-parity 12 条全红）。这也是 R00 把这 7 套接进 CI 的理由。
import { buildLaunchPlan } from "../src/launch-plan.ts";
import { tryRenderCli } from "../src/launch-render-cli.ts";
import type { LaunchContext } from "../src/launch-plan.ts";
import type { CcmProbeResult } from "../src/ccm-probe.ts";

const FULL_CAPS: CcmProbeResult = {
  installed: true,
  version: "1",
  capabilities: new Set([
    "new", "resume", "attach", "tmux", "account", "model", "cwd", "agent", "launcher", "ccm-sid", "print",
  ]),
};

function line(ctx: LaunchContext): string {
  const r = tryRenderCli(buildLaunchPlan(ctx), ctx, FULL_CAPS);
  if (!r.ok) throw new Error(`平价预言机的场景必须可渲染，却降级了: ${r.reason}`);
  return r.cmd;
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
