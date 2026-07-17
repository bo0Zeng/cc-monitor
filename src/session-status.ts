/**
 * F91（#27）：会话活动状态的**共享纯逻辑** —— 红绿灯类名 + 跨会话监控快照 DTO。
 * 零 import，node 可测。
 *
 * **单一事实源**：红绿灯语义（`idle`/`shell`=红、`waiting`=黄、`busy`/未知=绿）此前内联在
 * `tabs.ts` 的 `updateTabButton`（tab-bar 灯）。F91 的 mission-control grid 也要同一套语义，
 * 故抽到这里让 tab-bar 与 grid **共用**、不各写一份（呼应 SS-9「同一套判定别写两遍」的精神）。
 * 抽取对 tab-bar 是**逐字节等价**重构：输出的类名与原三分支完全一致。
 */

/** tab/cell 上叠的活动灯类名。空串 = 不叠类（维持默认绿点：busy 或未知 activity）。 */
export type ActivityLightClass = "" | "act-idle" | "act-waiting";

/**
 * 由 Claude Code 的 `status` 字段映射到活动灯类名。
 * - `idle` / `shell`（都在等输入）→ `act-idle`（红）
 * - `waiting`（等对话框决策）→ `act-waiting`（黄）
 * - `busy`（运行中）/ `null`（旧版 CC 或远端 v1 无该字段）→ `""`（默认绿点）
 */
export function activityLightClass(status: string | null): ActivityLightClass {
  if (status === "idle" || status === "shell") return "act-idle";
  if (status === "waiting") return "act-waiting";
  return "";
}

/**
 * F91：跨会话监控快照的单条 DTO。`TabManager.snapshotSessions()` 产出，`GridMonitorView` 消费。
 * **纯派生值**——不含任何内部 DOM / Map 引用（防外部改到 TabManager 内部状态）。
 */
export interface GridSessionSnapshot {
  sessionId: string;
  /** Tab 标题（[项目] aiTitle > 项目名 > sid 前 8）。 */
  title: string;
  /** null = 本地；非空 = 远端主机 label。 */
  origin: string | null;
  /** 项目根 / 启动目录（最早记录的 cwd）；null = 尚未拿到。 */
  cwd: string | null;
  status: "live" | "archived";
  /** Claude 的 status 字段原值（busy/idle/shell/waiting）；null = 未知。 */
  activityStatus: string | null;
  /** waiting 时的子类（permission prompt / dialog open …）；否则 null。 */
  waitingFor: string | null;
  /** 本会话仍在跑的 subagent 数。 */
  runningAgents: number;
  /** 本会话 subagent 总数（含已结束）。 */
  totalAgents: number;
  /** context 占用近似%（最新一轮 prompt token ÷ 模型上限）；上限未知 / 无 usage → null。 */
  contextPct: number | null;
  /** 未读消息数（非活跃 tab 累积）。 */
  unread: number;
  /** 会话类型（"bg" → ⚙）；null / "interactive" = 交互。 */
  kind: string | null;
}
