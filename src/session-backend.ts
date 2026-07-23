/**
 * F90（#48）：**会话后端**前端座——把 `remote-launch.ts` 里硬编码的 `tmux …` 命令字面量收敛到一处
 * （照 F-MA `agent-profile.ts` 纯画像范式）。守 **doc/INVARIANTS.md §31（SS-12）**「一端起的会话
 * 另一端必须能接」的第①条：**前端绝不硬编码后端命令**，改问这一层要。
 *
 * ★ **两轴正交**（呼应 SS-11）：
 *   - `agent-profile.ts` = **哪个 AI**（claude / …）——resume flag / 嵌套 env / 工具名。
 *   - `session-backend.ts` = **哪个多路复用器**（tmux / abduco / dtach）——attach / create 命令语法。
 *   二者皆纯模块（零 bundler-import，node/tsx 可测），皆被 `remote-launch.ts` 消费。
 *
 * ★ **阶段①只做形状**（MASTERPLAN batch16 §133）：座抽出、命令语法归座，但**唯一后端 = tmux**，
 * `SESSION_BACKEND` 恒等于 `TMUX_BACKEND`、**无运行时后端选择/探测**。abduco/dtach 评估 + 后端能力
 * 探测 + daemon RPC = 阶段②（§9 轨道二，daemon 在场才补得了 `send-keys` 缺口）。
 *
 * ★ **阶段②不是「再加一个返回 shell 串的 const」**（SS-13 / §31）：abduco/dtach **没有 `send-keys`**，
 * 本接口 `createRunAttach({quotedPayload})` 的「打字载荷」模型是 tmux 特有的；纯 abduco/dtach 履约不了。
 * 阶段② daemon 在场后，取命令方式从「同步返回 shell 串的 builder」转成「问 daemon 的 RPC 句柄」
 * （异步、错误面变化）——**调用点届时要按 RPC 重塑，不是零改动换 const**。本座是阶段①的形状占位
 * （证明「命令可从调用方剥离」），不是阶段②接口的终态承诺。
 *
 * ★ **本座只搬命令语法，不搬安全校验**：`target`/`quotedPayload`/`quotedCwd` 由调用方（remote-launch）
 * 用 `posixQuote`/`isValidTmuxName` 预备好后传入；座只在这些**已安全**的片段外拼后端语法，不做校验/转义
 * （防注入面分散，安全边界仍集中在 remote-launch）。tmux 会话名约定（`cc-<sid8>` 派生/校验）暂留
 * remote-launch，其后端耦合移动延后阶段②。
 */

/** 一个会话后端（多路复用器）的命令构造契约。阶段②可加第二实现（abduco/dtach），靠能力探测选。 */
export interface SessionBackend {
  /**
   * 幂等「建 detached 会话 → 键入载荷 → attach」。会话已存在 → 建失败被吞、`&&` 短路跳过键入 →
   * 只 attach（不重复启动）。`target` = 后端目标 token（调用方决定裸校验名或 posixQuote 名）；
   * `quotedCwd` = 已 posixQuote 的工作目录，null 则不带目录标志；`quotedPayload` = 已 posixQuote 的
   * 键入载荷（含 unset 嵌套 env + 启动/resume 命令）。
   * `ccmSid`（#72，可选）= 完整会话 sid：**新建会话后在 create 分支里显式 set `@ccm_sid=<sid>`**，
   * 让本方自建的 resume 会话带身份标记（供 `findClaudeTmux` 精确匹配、不落 cwd 回退警告）。缺省不设
   * （新会话 sid 未知时不传）。**调用方须保证 `ccmSid` 为 `[A-Za-z0-9_-]`**（座不做校验、裸拼）。
   */
  createRunAttach(args: {
    target: string;
    quotedCwd: string | null;
    quotedPayload: string;
    ccmSid?: string;
  }): string;
  /** attach 一个已存在会话。`target` 由调用方预备（posixQuote 名或裸校验名）。 */
  attach(target: string): string;
}

/**
 * tmux 后端——独占所有 `tmux <动词>` 命令字面量。幂等结构（调研 03 §2b）：
 *   `tmux new-session -d -s <target>[ -c <cwd>] 2>/dev/null && tmux send-keys -t <target> <载荷> Enter; tmux attach -t <target>`
 * `new-session -d 2>/dev/null && send-keys`：会话已存在 → 建失败被吞、短路跳过 send-keys（不重复
 * resume/启动）→ 只 attach；不存在 → 建 → 键入 → attach。send-keys 只落**新建会话**的交互 shell。
 */
export const TMUX_BACKEND: SessionBackend = {
  createRunAttach: ({ target, quotedCwd, quotedPayload, ccmSid }): string => {
    const cflag = quotedCwd !== null ? ` -c ${quotedCwd}` : ""; // 契约是 string|null，按 null 判而非 truthy
    // #72：给自建 resume 会话打 `@ccm_sid=<完整 sid>`（session-scoped、`-t` 指名），放在 new-session
    // 成功的 create 分支里——会话已存在 → new-session 失败 → 短路跳过 set-option（它建时已带,不重设）。
    // 与 android-terminal §3 契约对齐（key `@ccm_sid` / 完整 sid / `-t` / create 分支）;ccmSid 由调用方
    // 保证 `[A-Za-z0-9_-]`（isValidSessionId），裸拼安全（同 target 裸名）。缺省不设。
    // **非阻断**（Phase D 审计 建议-1）:`(set-option 2>/dev/null || true)` 包起——身份标记是**次要**动作,
    // 绝不能阻断**主要**动作 resume（后续 `&& send-keys`）。古董 tmux(<1.8 无 `@`-option)等 set 失败 →
    // 降级到"无标记"(= #72 前行为、cc-monitor 回退 cwd 匹配),resume 照跑,而非把用户丢进空 shell。
    const setSid = ccmSid ? `(tmux set-option -t ${target} @ccm_sid ${ccmSid} 2>/dev/null || true) && ` : "";

    return (
      `tmux new-session -d -s ${target}${cflag} 2>/dev/null && ` +
      setSid +
      `tmux send-keys -t ${target} ${quotedPayload} Enter; ` +
      `tmux attach -t ${target}`
    );
  },
  attach: (target): string => `tmux attach -t ${target}`,
};

/**
 * 阶段①**唯一活跃会话后端**（= tmux）。同 `AGENT_PROFILE` 的「单画像、切换点集中」范式：
 * remote-launch 只认这个句柄。阶段②的重塑见顶注——不是把这个 const 换成另一个同型实现，
 * 而是把整条「取命令」路径改成 daemon RPC。
 */
export const SESSION_BACKEND: SessionBackend = TMUX_BACKEND;
