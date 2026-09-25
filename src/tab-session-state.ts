/**
 * 〔U4 · `设计/01 §6.2` · `设计/30 §3.5.6`〕**一个会话此刻是什么状态 —— 两个正交的轴。**
 *
 * | 轴 | 取值 | 谁说 |
 * |---|---|---|
 * | 活性 | 活 / 死 | 会话所在的那台机器（后端 pidfile ＋ 探活） |
 * | 可恢复性 | 能接回去（容器还在）/ 只能 resume | 死的那一刻由 monitor 裁（`session-idle` / `session-ended`） |
 *
 * 「死了」与「能不能接回去」是两件事，不许挤在一个词里。改之前它们挤在 `Tab.status` ＋ `Tab.tmuxIdle`
 * 两个字段里：可重连的会话在活性那一格被记成 `live`（claude 进程其实已经没了）。
 *
 * # 这份文件管三件事，别处不再各判一遍
 *
 * 1. **形状**：`SessionState` 判别联合（为什么是联合、为什么没有 `gone`，见类型头注）。
 * 2. **转移**：五种事件怎么改它（`nextState`）—— 事件从哪来（只远端 / 只本机）那道门留在 `TabManager`。
 * 3. **行为谓词**：× 能不能关、↗ 能不能拉前、算不算活跃……每一处旧判断换成这里的一个谓词。
 *
 * 4. **呈现**：CSS 类开关 · 状态名 · 提示句（`stateView`）—— 界面上说到会话状态的字只从两轴派生，文字住文案表
 *    `src/shared/copy/table.json` 的 `sessionState.*`（`设计/30 §3.5.2` 那张表：已结束 / 可重连）。
 *
 * 缺的格（要后端补，不在前端猜）记在 `调研/第四波记录/U4.md §0.1`：G1 记录没了 · G2 本机容器还在 · G3 活会话的容器类型。
 */
import { copyText } from "./copy-table";

/** 活性：进程在不在。 */
export type Liveness = "live" | "dead";

/**
 * 可恢复性：死了之后怎么回去。
 * - `attachable`：容器（tmux）还在，接回去就行 —— 「可重连」。
 * - `resumable`：容器没了，只能 resume 一个新进程接着那份记录 —— 「已结束」。
 *
 * ⚠ `设计/30 §3.5.6` 还有第三格 `gone`（连记录都没了）。**今天零生产者**：没有任何事实告诉前端
 *   「对方那份记录没了」（`U4.md §0.1` G1）。写一个到不了的值就是写了没接 ⇒ 后端补上之后再加。
 */
export type Recoverability = "attachable" | "resumable";

/**
 * 两轴合起来。**判别联合，不是两个独立字段**：
 * - 「活着 ＋ 能接回去」今天没有任何事实能填（后端不回报活会话的容器类型，G3）⇒ 活着时可恢复性是 `null`（没报）；
 * - 「死了 ＋ 没报」今天也不存在：每一条死亡事件都带着 monitor 的裁决。
 * 类型上不许出现这两种，免得有人拿一个默认值去填空格。
 */
export type SessionState =
  | { readonly liveness: "live"; readonly recoverability: null }
  | { readonly liveness: "dead"; readonly recoverability: Recoverability };

export const LIVE: SessionState = Object.freeze({ liveness: "live", recoverability: null });
/** 死了，容器也没了 ⇒ 只能 resume。 */
export const ENDED: SessionState = Object.freeze({ liveness: "dead", recoverability: "resumable" });
/** 死了，容器还在 ⇒ 接回去。 */
export const RECONNECTABLE: SessionState = Object.freeze({
  liveness: "dead",
  recoverability: "attachable",
});

/**
 * 改变状态的五种事件（都是后端给的事实，前端不自己推）：
 * - `ended`：`session-ended`（monitor 裁 Archive：容器没了 / 被 `/branch` 顶替）
 * - `idle`：`session-idle`（monitor 裁 Idle：claude 没了，`@ccm_sid` 还在 tmux 里）
 * - `started`：本机 `session-started`（pidfile 新增且探活通过）
 * - `remote-line`：远端又见到这条会话的行 / 重宣告（后端只对活 pidfile 推行）
 * - `activity`：带 status 的活动信号（后端只对活着的 claude 推）
 */
export type StateEvent = "ended" | "idle" | "started" | "remote-line" | "activity";

/**
 * 转移表（`U4.md §1.1` 那张表逐格）。返回值与入参**是同一个对象** ⇔ 这次事件不改状态
 * （调用方靠它决定要不要重画、要不要打探针）。
 *
 * 两条「不变」是刻意的，理由各自住在旧代码的注释里、这里照旧：
 * - 已结束收到 `idle` 不变 —— 「归档优先，不回置灰」：容器真没了才裁 Archive，晚到的 Idle 是旧消息。
 * - 已结束收到 `activity` 不变 —— 心跳清掉死会话后磁盘上残留的 pidfile 被重扫会推陈旧活动。
 */
export function nextState(s: SessionState, ev: StateEvent): SessionState {
  switch (ev) {
    case "ended":
      return isResumeOnly(s) ? s : ENDED;
    case "idle":
      return s.liveness === "live" ? RECONNECTABLE : s;
    case "started":
    case "remote-line":
      return s.liveness === "dead" ? LIVE : s;
    case "activity":
      return s.recoverability === "attachable" ? LIVE : s;
  }
}

/**
 * 死了，而且只能 resume（旧 `status === "archived"`）。
 *
 * 用在：× / Ctrl+W / 中键关得掉 · 自动跟随不跟 · 后台物化不排 · 右键给 Resume · 活动信号不收。
 * 可重连的会话**不**在其中：它的终端还在（↗ 拉前照样有效），今天也不许关（与改之前逐条相同）。
 */
export function isResumeOnly(s: SessionState): boolean {
  return s.recoverability === "resumable";
}

/** 还有一个终端可去：活着，或者容器还在。↗ 拉前 · 本机「杀死会话」那几格用它。 */
export function hasTerminal(s: SessionState): boolean {
  return !isResumeOnly(s);
}

/** 活性 == 活。状态栏「活跃 N」· 固定条目的「最后活动时刻」· 机器总览的活跃会话数用它。 */
export function isLive(s: SessionState): boolean {
  return s.liveness === "live";
}

/**
 * 一个状态在界面上长什么样（`U4.md §1.2` 那张表）。**这是说会话状态的字的唯一出处**：
 * tab 栏的类与 tooltip、机器总览的类 / 灯 / 「状态」一格都从这里取。
 */
export interface StateView {
  /** `.ended`：已结束（整颗变淡、标题斜体、灯 / ↗ / 📂 隐藏、× 露出来）。 */
  ended: boolean;
  /** `.reconnectable`：可重连（灯换成暗色、停呼吸）。 */
  reconnectable: boolean;
  /** 状态名（「已结束」「可重连」）；活着 ⇒ `null`（活着不另说状态，灯自己说）。 */
  name: string | null;
  /** 提示句（`设计/30 §3.5.2`「说给用户的话」一列）；活着 ⇒ `null`。 */
  tooltip: string | null;
}

export function stateView(s: SessionState): StateView {
  if (s.liveness === "live") return { ended: false, reconnectable: false, name: null, tooltip: null };
  // 死了：按可恢复性分两格。`switch` 穷尽 —— 后端补上 `gone`（G1）之后这里编译期就会要第三格。
  switch (s.recoverability) {
    case "resumable":
      return {
        ended: true,
        reconnectable: false,
        name: copyText("sessionState.ended.name"),
        tooltip: copyText("sessionState.ended.tooltip"),
      };
    case "attachable":
      return {
        ended: false,
        reconnectable: true,
        name: copyText("sessionState.reconnectable.name"),
        tooltip: copyText("sessionState.reconnectable.tooltip"),
      };
  }
}
