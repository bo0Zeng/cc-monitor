/**
 * 活着的会话那颗点 / 灯（标签栏 · 状态点 · 监控板 · 轮换名单共用这一张表）＋ 跨会话监控快照 DTO。
 * 按核心写好的**语气**排（`activity_tone`：`now` 在跑 · `need` 需手动 · 其余常规），不认活动态的码、不认任何一家的状态词。
 * 零运行期 import（只有会被擦除的 `import type`），node 可测。
 */

import type { Origin } from "./generated/Origin";
import type { SessionState } from "./tab-session-state";
import type { RunState } from "./generated/RunState";
import type { SessionActivity } from "./generated/SessionActivity";
import type { DotState } from "./kit/status-dot";
import type { BackgroundWork } from "./session-reads";

/** tab/cell 根上活动灯那一档的钩子（`data-light`；没有样式、给判据与读屏读）。空串 = 不挂。 */
export type ActivityLightClass = "" | "idle" | "waiting";

/** 活着的会话那颗点的颜色（`kit/status-dot` 的 `DotState` 里活着的那四态）。 */
export type ActivityDot = "running" | "needs-you" | "background" | "idle";

/** 语气 ⇒ 点 · 灯（标签栏 · 状态点 · 监控板 · 轮换名单只从这里取）。还没收到那一格（`null`）⇒ 在运行的点、不叠灯。 */
export function activityFace(tone: string | null): { dot: ActivityDot; light: ActivityLightClass } {
  switch (tone) {
    case null:
    case "now":
      return { dot: "running", light: "" };
    case "need":
      return { dot: "needs-you", light: "waiting" };
    // 一轮停了、后台命令还在跑：单独一色（`--bgwork`），不呼吸。
    case "busy":
      return { dot: "background", light: "" };
    default:
      return { dot: "idle", light: "idle" };
  }
}

/** 状态点：颜色 ＝ 在干什么（核心的语气），形状 ＝ 进程还在不在（两轴）。标签栏 · 会话头 · 监控板同一份。 */
export function sessionDot(s: SessionState, tone: string | null): DotState {
  switch (s.liveness) {
    case "unseen":
      return "unknown";
    case "dead":
      return s.recoverability === "attachable" ? "exited" : s.recoverability === "gone" ? "gone" : "ended";
    case "live":
      return activityFace(tone).dot;
  }
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
  /** 此刻在干什么（后端翻好的）；null = 说不清。字与点按下面两格，排序按 `activityOrder`。 */
  activity: SessionActivity | null;
  /** 那一态核心写好的字（运行中 · 需手动 · 空闲 · 后台在跑 …）；还没收到 ⇒ null。 */
  activityText: string | null;
  /** 那一态的语气（点的颜色按它）；还没收到 ⇒ null。 */
  activityTone: string | null;
  /** 监控板组内的序（核心写的，小的在前：等人 · 在干活 · 后台在跑 · 闲着 · 说不清）；还没收到 ⇒ null。 */
  activityOrder: number | null;
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
