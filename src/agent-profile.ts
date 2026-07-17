/**
 * F-MA:前端侧 agent 画像——把散落在 `cards/*`、`tabs.ts` 里的 **Claude Code 专属工具名/进程名**
 * 常量收敛到一处(对应 Rust `src-tauri/src/adapter`)。第一刀**值不变、行为零变化**;接第二个具体
 * agent 时这里按 agent 切换。
 *
 * ★ **第一刀只收敛"工具名字符串"**,不拆记录模型 / 不动 `renderMessage` 的 `switch(rec.type)`
 * 分发——那是第二刀(等第二个 wire 样本看清真·共性,守 SS-1)。见 `plan/features/MA-multi-agent-adapter.md`。
 */
export const AGENT_PROFILE = {
  /** 子 agent 工具(展开 = 子会话):Agent / Task。 */
  agentTools: new Set<string>(["Agent", "Task"]),
  /** 交互工具(Claude 在等用户决定):AskUserQuestion / ExitPlanMode。 */
  interactiveTools: new Set<string>(["AskUserQuestion", "ExitPlanMode"]),
  /** 写类工具(行级 diff):Edit / Write / MultiEdit。 */
  diffTools: new Set<string>(["Edit", "Write", "MultiEdit"]),
  /** 结果默认按 markdown 渲染的工具。 */
  mdTools: new Set<string>(["Read", "Grep", "WebFetch", "NotebookRead", "TodoWrite"]),
  /** tmux 前台命令算该 agent 的会话(`claude` / `node`——Claude 是 Node CLI,视启动路径也可能报解释器)。 */
  livenessProcessNames: new Set<string>(["claude", "node"]),
};
