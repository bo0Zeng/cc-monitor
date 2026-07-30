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
import {
  posixQuote,
  sanitizeRemoteLauncher,
  buildEnvPrefix,
  UNSET_CONFIG_DIR_PREFIX,
} from "./shell-quote.ts";
import { SESSION_BACKEND, type TmuxTarget } from "./session-backend.ts";
import type { EnvOp, LaunchContainer, LaunchPlan, WrapSpec } from "./launch-plan.ts";

function renderEnvOps(ops: EnvOp[]): string {
  return ops
    .map((op) => {
      if (op.kind === "export-config-dir") return buildEnvPrefix(op.value); // "export CLAUDE_CONFIG_DIR='…'; "
      if (op.kind === "export-model") return `export ANTHROPIC_MODEL=${posixQuote(op.value)}; `; // F07
      // R04③：`unset` 侧收窄为无参变体后，键表由 kind 在这里查——不再由维度递自由字符串数组。
      // 输出逐字节不变（`unset CLAUDE_CONFIG_DIR; ` / `unset <嵌套env 全套>; `）。
      if (op.kind === "unset-config-dir") return UNSET_CONFIG_DIR_PREFIX;
      if (op.kind === "unset-nested-env") return `unset ${AGENT_PROFILE.nestedEnvVars.join(" ")}; `;
      // **穷尽性守卫（R04 Phase D 审计发现的真实回退，已修）**：收窄前的末支是
      // `return \`unset ${op.keys.join(" ")}; \``——它**读了 `op.keys`**，于是编译器被迫穷尽：
      // 加第 4 个 `EnvOp` 变体时 tsc 会在这里报 `Property 'keys' does not exist`。
      // 收窄后的末支若写成无条件 `return`（不读 `op` 任何字段），就把一切都兜住了——
      // 审计实测：加一个 `{kind:"unset-proxy"}` 变体，HEAD 快照 tsc **报错**，
      // 而收窄版 tsc **0 错**且把它静默渲染成嵌套 env 的 unset。
      // 那与 R04 自己的立意（把注释纪律变成类型上做不到）正好相反，故显式补回穷尽性。
      return ((_exhaustive: never): never => {
        throw new Error(`未处理的 EnvOp: ${JSON.stringify(_exhaustive)}`);
      })(op);
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

/**
 * order 升序由内向外折叠成 `( <prelude>; exec <inner> )`。今天 `plan.wrap` 恒空数组，
 * 折叠逻辑仍独立可测（给 F04 rbind 铺路）。`exec` 不可省——wrapper 用 `$BASHPID` 读
 * `sessions/$cpid.json`，不 exec 则 PID 对不上（审计 C1）。
 *
 * **`inner` 必须是「只有 argv」的那一段，不能带 env 前缀或 `cd`**（R04④ Phase D 审计发现，已修）：
 * `exec` 后面必须直接跟**可执行文件**。此前两个调用点把整条 payload（`envOps + cd + argv`）
 * 一起递进来，折叠出的是 `( __ccm_rbind; exec unset A B; claude … )`——
 * 实测 `bash -c '( echo RB; exec unset A B; echo REACHED )'` → `exec: unset: 未找到`、**rc=127**，
 * launcher 根本起不来。目标形态是 bashrc 原文的 `( __ccm_rbind; exec claude --resume S )`。
 * 这是 F03 就有的 call-site 缺陷（闭包版拿到的也是同一条拼好的串，同样错），
 * 但 R04④ 一度把它写成"已知唯一用例已钉死"并让测试断言了这个坏形态——那才是要害。
 * 因 `plan.wrap` 零生产者，此刻改 call-site 零风险；仍满足「sanitize 先于 wrap」
 * （sanitize 在 `renderArgv` 内部完成，wrap 在其外）。
 */
function applyWraps(inner: string, wraps: WrapSpec[]): string {
  return [...wraps]
    .sort((a, b) => a.order - b.order)
    .reduce((s, w) => `( ${w.prelude}; exec ${s} )`, inner);
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
    return renderEnvOps(plan.env) + cd + applyWraps(renderArgv(plan), plan.wrap);
  }

  const payload = renderEnvOps(plan.env) + applyWraps(renderArgv(plan), plan.wrap);
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
