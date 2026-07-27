/**
 * F03（unify-launch）：兜底渲染器——把 `LaunchPlan` 编译成裸 shell 命令串，逐字节等于今天
 * 7 个 builder 各自手拼的输出。维度效果（`plan.env`/`plan.args`/`plan.identity`）已在
 * `buildLaunchPlan` 阶段摊平完毕，本模块只做"结构 → 文本"的编译，不再向维度提问。
 *
 * **sanitize 必须先于 wrap**（MASTERPLAN 设计债 #2）：`renderArgv()` 内部先调用
 * `sanitizeRemoteLauncher`，其**返回值**才作为 `applyWraps()` 的输入——函数组合上的结构
 * 保证，不是注释纪律。F03 阶段 `plan.wrap` 恒空数组，折叠逻辑独立可测，给 F04 的 rbind
 * 包裹留好落点。
 */
import { AGENT_PROFILE } from "./agent-profile.ts";
import { posixQuote, sanitizeRemoteLauncher, buildEnvPrefix } from "./shell-quote.ts";
import { SESSION_BACKEND, type TmuxTarget } from "./session-backend.ts";
import type { EnvOp, LaunchContainer, LaunchPlan, WrapSpec } from "./launch-plan.ts";

function renderEnvOps(ops: EnvOp[]): string {
  return ops
    .map((op) => {
      if (op.kind === "export-config-dir") return buildEnvPrefix(op.value); // "export CLAUDE_CONFIG_DIR='…'; "
      if (op.kind === "export-model") return `export ANTHROPIC_MODEL=${posixQuote(op.value)}; `; // F07
      return `unset ${op.keys.join(" ")}; `;
    })
    .join("");
}

function renderArgv(plan: LaunchPlan): string {
  const launcher = sanitizeRemoteLauncher(plan.launcher); // sanitize 先于 wrap（设计债 #2）
  const parts = [launcher];
  if (plan.action.kind === "resume") parts.push(AGENT_PROFILE.resumeFlag, plan.action.sid);
  parts.push(...plan.args);
  return parts.join(" ");
}

function applyWraps(inner: string, wraps: WrapSpec[]): string {
  // order 升序由内向外折叠——F03 恒空数组，折叠逻辑仍独立可测（给 F04 铺路）。
  return [...wraps].sort((a, b) => a.order - b.order).reduce((s, w) => w.wrap(s), inner);
}

function tmuxTarget(container: Extract<LaunchContainer, { kind: "tmux" }>): TmuxTarget {
  return { kind: container.nameQuoting, value: container.name };
}

export function renderFallback(plan: LaunchPlan): string {
  if (plan.action.kind === "attach") {
    if (plan.container.kind !== "tmux") throw new Error("attach 必须是 tmux 容器");
    return SESSION_BACKEND.attach(tmuxTarget(plan.container));
  }

  if (plan.container.kind === "none") {
    // **顺序不是任意的**：今天 `buildResumeDirectCmd` 的 `cd '<cwd>' &&` 是**守卫式连接**，
    // 排在环境变量导出**之后**、启动命令**之前**——`<envOps>cd '<cwd>' && <argv>`。
    // 早期实现曾把 cd 错放到最前面（`cd && <envOps><argv>`），逐字节对拍时抓到——
    // 见 F03 计划 §5 步骤④「清零之前不进入下一步」。
    const cd = plan.cwd ? `cd ${posixQuote(plan.cwd)} && ` : "";
    return applyWraps(renderEnvOps(plan.env) + cd + renderArgv(plan), plan.wrap);
  }

  const payload = applyWraps(renderEnvOps(plan.env) + renderArgv(plan), plan.wrap);
  const target = tmuxTarget(plan.container);
  const quotedCwd = plan.cwd ? posixQuote(plan.cwd) : null;
  switch (plan.container.mode) {
    case "create-or-attach":
      return SESSION_BACKEND.createRunAttach({
        target,
        quotedCwd,
        quotedPayload: posixQuote(payload),
        ccmSid: plan.identity?.ccmSid,
      });
    case "send-into":
      return SESSION_BACKEND.runInExistingAttach({ target, quotedPayload: posixQuote(payload) });
    case "attach-only":
      throw new Error("不可达：attach-only 应由 action.kind==='attach' 分支处理");
  }
}
