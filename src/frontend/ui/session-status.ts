/**
 * 会话那颗点上叠的灯（标签栏 · 监控板）＋ 跨会话监控快照 DTO。点本身是那台后端判好的（`SessionFace.dot`），这里只管画法。
 * 零运行期 import（只有会被擦除的 `import type`），node 可测。
 */

import type { Origin } from "./generated/Origin";
import type { SessionFace } from "./generated/SessionFace";
import type { RunState } from "./generated/RunState";
import type { DotState } from "./kit/status-dot";
import type { BackgroundWork } from "./session-reads";

/** tab/cell 根上活动灯那一档的钩子（`data-light`；没有样式、给判据与读屏读）。空串 = 不挂。 */
export type ActivityLightClass = "" | "idle" | "waiting";

/** 点 ⇒ 灯那一档：需手动呼吸、空闲暗着，别的不叠。 */
export function lightOf(dot: DotState): ActivityLightClass {
  return dot === "needs-you" ? "waiting" : dot === "idle" ? "idle" : "";
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
  /** 此刻画的那张脸（`face-can.ts::faceOf`，与 tab 栏同一份）：点 · 字 · 序 · 能做什么都照它。 */
  face: SessionFace;
  /** 在等人时等的是什么（核心写好的字：等批准 · 等回答 …，`needs.text`）；不在等 ⇒ null。 */
  needs: string | null;
  /** 本会话仍在跑的 subagent 数。 */
  runningAgents: number;
  /** 本会话 subagent 总数（含已结束）。 */
  totalAgents: number;
  /** 上下文那一格（核心写好的字 · 语气 · 百分比；百分比为 null ＝ 上限判不出）；还没有用量 ⇒ null。 */
  context: { text: string; tone: string; percent: number | null } | null;
  /** 未读消息数（非活跃 tab 累积）。 */
  unread: number;
  /** 后台会话（⚙）。 */
  background: boolean;
  /** 后台任务运行中那一句（`Tab.backgroundWork` 原样，核心写的）；不是这一态 ⇒ null。 */
  backgroundWork: BackgroundWork | null;
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
