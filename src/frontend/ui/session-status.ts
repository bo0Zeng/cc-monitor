/**
 * 会话活动态 ⇒ 点 / 灯（标签栏 · 状态点 · 总览共用这一张表）＋ 跨会话监控快照 DTO。
 * 活动态是后端翻好的（`SessionActivity`）；这里只排版，不认任何一家的状态词。
 * 零运行期 import（只有会被擦除的 `import type`），node 可测。
 */

import type { Origin } from "./generated/Origin";
import type { SessionState } from "./tab-session-state";
import type { RunState } from "./generated/RunState";
import type { SessionActivity } from "./generated/SessionActivity";

/** tab/cell 上叠的活动灯类名。空串 = 不叠类（默认绿点）。 */
export type ActivityLightClass = "" | "act-idle" | "act-waiting";

/** 活着的会话那颗点的颜色（`kit/status-dot` 的 `DotState` 里活着的那三态）。 */
export type ActivityDot = "running" | "needs-you" | "idle";

/** 活动态 ⇒ 点 · 灯。说不清（`null`）⇒ 默认：在运行的点、不叠灯。 */
const ACTIVITY_FACE: Record<SessionActivity, { dot: ActivityDot; light: ActivityLightClass }> = {
  working: { dot: "running", light: "" },
  needs_you: { dot: "needs-you", light: "act-waiting" },
  idle: { dot: "idle", light: "act-idle" },
};

/** 活动态 ⇒ 点 · 灯（标签栏 · 状态点 · 总览只从这里取）。 */
export function activityFace(a: SessionActivity | null): { dot: ActivityDot; light: ActivityLightClass } {
  return a === null ? { dot: "running", light: "" } : ACTIVITY_FACE[a];
}

/**
 * F91：跨会话监控快照的单条 DTO。`TabManager.snapshotSessions()` 产出，`GridMonitorView` 消费。
 * **纯派生值**——不含任何内部 DOM / Map 引用（防外部改到 TabManager 内部状态）。
 */
export interface GridSessionSnapshot {
  sessionId: string;
  /** Tab 标题（[项目] aiTitle > 项目名 > sid 前 8）。 */
  title: string;
  /** 哪台机器：本机 = `LOCAL_ORIGIN`；其余 = 远端主机 label。不再用 `null` 表示本机。 */
  origin: Origin;
  /** 会话的项目目录（`Tab.projectDir`，后端给的那一格）；null = 后端没给。 */
  cwd: string | null;
  /**
   * 会话状态的两个轴（活性 × 可恢复性），与 tab 栏**同一份**（`Tab.state` 原样交出）；
   * cell 的类 / 灯 / 排序 / 摘要都经 `tab-session-state.ts` 的谓词读，不在这里另判。
   * 原先是 `status: "live" | "archived"` ＋ `tmuxIdle: boolean`（可重连的会话在前者里是 live）。
   */
  state: SessionState;
  /** 此刻在干什么（后端翻好的）；null = 说不清。 */
  activity: SessionActivity | null;
  /** 在等人时那一家给的细分（原样）；否则 null。 */
  waitingFor: string | null;
  /** 本会话仍在跑的 subagent 数。 */
  runningAgents: number;
  /** 本会话 subagent 总数（含已结束）。 */
  totalAgents: number;
  /** context 占用近似%（最新一轮 prompt token ÷ 模型上限）；上限未知 / 无 usage → null。 */
  contextPct: number | null;
  /** 最新一轮用了多少 token（上限判不出、`contextPct` 为空时只写它）。 */
  contextTokens: number | null;
  /** 未读消息数（非活跃 tab 累积）。 */
  unread: number;
  /** 后台会话（⚙）。 */
  background: boolean;
  /** A3：该会话所属账号名（live 探测）；null = 本地会话 / 未知（不猜）。 */
  account: string | null;
}

/**
 * F91b（batch17）：监控板选中 cell 的「内容 peek」补充数据——比 `GridSessionSnapshot`（cell 面上那些）
 * 更细、只在选中一格时按需取的字段。全来自 TabManager 内存（纯读派生，无后端/无落盘，守 §1/§28）。
 */
export interface SessionPeek {
  /** 最新一轮 assistant 记录的 model 原串（供 peek 显示）；null = 尚无带 usage 记录。 */
  model: string | null;
  /** 本会话写类工具（Edit/Write/…）碰过的文件路径（首触序）——「谁跑偏」关键信号。 */
  recentFiles: string[];
  /** 本会话 subagent 名单（运行中优先），供 peek 显示「在跑什么」。 */
  agents: { label: string; status: RunState }[];
}
