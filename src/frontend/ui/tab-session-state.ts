/**
 * **一个会话此刻是什么状态 —— 两个正交的轴。**
 *
 * | 轴 | 取值 | 谁说 |
 * |---|---|---|
 * | 活性 | 活 / 死 | 会话所在的那台机器（后端 pidfile ＋ 探活） |
 * | 可恢复性 | 能接回去（容器还在）/ 只能 resume / 连记录都没了 | 活着时由容器类型定（后端 `session_added.container`）；死的那一刻由 monitor 裁（`idle` 格 / `ended` 格）；记录在不在由 resume 一跳问那台后端 |
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
 *    `src/shared/copy/table.json` 的 `sessionState.*`（那张表：已结束 / 可重连）。
 *
 * 缺的格（要后端补，不在前端猜）记在：G1 记录没了 · G2 本机容器还在 · G3 活会话的容器类型。
 * 三格都补上了，外加「说不清」：
 * - G3：活着时可恢复性那一格来自后端的容器事实（`container-tmux` / `container-none` 两个事件）；
 * - G1：可恢复性第三值 `gone`（resume 一跳问过那台后端、记录不在）；
 * - 说不清：活性第三值 `unseen`（固定复活、那台机器还没把活会话清单报完）。
 */
import { copyText } from "./copy-table";

/**
 * 活性：进程在不在。
 * - `unseen`：**说不清** —— 由固定复活（那台机器还没把它的活会话清单报完）与那台机器看不见了（连接断了）产出。
 *   它说的是「机器看不见」，**不是**「会话死了」：`Unseen` 不许被显示成已结束。
 */
export type Liveness = "live" | "dead" | "unseen";

/**
 * 可恢复性：死了之后怎么回去（活着时 = 它现在死了会落到哪一格）。
 * - `attachable`：容器（tmux）还在 / 在 tmux 里跑，接回去就行 —— 「可重连」。
 * - `resumable`：没有容器，只能 resume 一个新进程接着那份记录 —— 「已结束」。
 * - `gone`：连记录都没了（resume 一跳问过那台后端，`history-record` 答不在）—— 重开必失败。
 *   只挂在「死 ＋ 只能 resume」上：容器还在的（可重连）照样接得回去，记录没了不改它那一格。
 */
export type Recoverability = "attachable" | "resumable" | "gone";

/**
 * 两轴合起来。**判别联合，不是两个独立字段**：
 * - 活着：可恢复性是容器事实（`attachable` / `resumable`），后端没报（旧后端 / 判不了）⇒ `null`；
 *   活着不会是 `gone`（记录在不在只在 resume 那一刻问，而活会话不走 resume）。
 * - 死了：每一条死亡事件都带着 monitor 的裁决 ⇒ 没有「死了 ＋ 没报」。
 * - 说不清：什么都说不出 ⇒ 可恢复性 `null`。
 * 类型上不许出现别的组合，免得有人拿一个默认值去填空格。
 */
export type SessionState =
  | { readonly liveness: "live"; readonly recoverability: "attachable" | "resumable" | null }
  | { readonly liveness: "dead"; readonly recoverability: Recoverability }
  | { readonly liveness: "unseen"; readonly recoverability: null };

/** 活着，容器后端没报。 */
export const LIVE: SessionState = Object.freeze({ liveness: "live", recoverability: null });
/** 活着，在 tmux 里 ⇒ 退了也能接回去。 */
export const LIVE_ATTACHABLE: SessionState = Object.freeze({
  liveness: "live",
  recoverability: "attachable",
});
/** 活着，不在任何 tmux 里 ⇒ 退了只能 resume。 */
export const LIVE_RESUMABLE: SessionState = Object.freeze({
  liveness: "live",
  recoverability: "resumable",
});
/** 死了，容器也没了 ⇒ 只能 resume。 */
export const ENDED: SessionState = Object.freeze({ liveness: "dead", recoverability: "resumable" });
/** 死了，容器还在 ⇒ 接回去。 */
export const RECONNECTABLE: SessionState = Object.freeze({
  liveness: "dead",
  recoverability: "attachable",
});
/** 死了，连记录都没了 ⇒ 重开必失败。 */
export const GONE: SessionState = Object.freeze({ liveness: "dead", recoverability: "gone" });
/** 〔说不清〕固定复活、那台机器还没把活会话清单报完；或那台机器看不见了。 */
export const UNSEEN: SessionState = Object.freeze({ liveness: "unseen", recoverability: null });

/**
 * 改变状态的事件（都是后端给的事实，前端不自己推）：
 * - `ended`：`ended` 格（monitor 裁 Archive：容器没了 / 被 `/branch` 顶替）
 * - `idle`：`idle` 格（monitor 裁 Idle：claude 没了，`@ccm_sid` 还在 tmux 里）
 * - `started`：本机 `live` 格（pidfile 新增且探活通过）/ 本机清单里有它
 * - `remote-line`：远端又见到这条会话的行 / 重宣告（后端只对活 pidfile 推行）
 * - `activity`：带 status 的活动信号（后端只对活着的 claude 推）
 * - `container-tmux` / `container-none`：`container` 格（后端 `session_added.container`）
 * - `record-gone` / `record-present`：resume 一跳问那台后端（`history-record`）的答案
 * - `seen-absent`：那台机器的活会话清单报完了、里面没有它（`origin-sessions-listed` / 本机 `list_active_sessions`〔散文墓碑〕）
 * - `unseen`：那台机器看不见了（`unseen` 格：到它的连接断了 / F5 时它还没报完清单）
 */
export type StateEvent =
  | "ended"
  | "idle"
  | "started"
  | "remote-line"
  | "activity"
  | "container-tmux"
  | "container-none"
  | "record-gone"
  | "record-present"
  | "seen-absent"
  | "unseen";

/**
 * 转移表（`U4b.md §1.3` 那张表逐格）。返回值与入参**是同一个对象** ⇔ 这次事件不改状态
 * （调用方靠它决定要不要重画、要不要打探针）。
 *
 * 几条「不变」是刻意的：
 * - 已结束 / 记录没了收到 `idle` 不变 —— 「归档优先，不回置灰」：容器真没了才裁 Archive，晚到的 Idle 是旧消息。
 * - 已结束收到 `activity` 不变 —— 心跳清掉死会话后磁盘上残留的 pidfile 被重扫会推陈旧活动。
 * - 记录没了收到 `ended` 不变 —— 它本来就死了，而「记录不在」这件事 `ended` 推翻不了。
 * - 复活成活（`started` / `remote-line`）时容器一格回到 `null`：那是一个新进程，旧的容器类型不沿用，
 *   等它自己的 `container` 格。
 * - 容器事实只落在活着的会话上：死了的那一格由死的那一刻的裁决说了算。
 * - `unseen` 只改「还有终端可去」的两态（活 · 可重连）：机器看不见了，它们此刻是死是活都说不清；
 *   已结束 / 记录没了不动 —— 它们的死是那台机器看得见时亲口说的，看不见了推翻不了。
 */
export function nextState(s: SessionState, ev: StateEvent): SessionState {
  switch (ev) {
    case "ended":
      return s.liveness === "dead" && s.recoverability !== "attachable" ? s : ENDED;
    case "idle":
      return s.liveness === "dead" ? s : RECONNECTABLE;
    case "started":
    case "remote-line":
      return s.liveness === "live" ? s : LIVE;
    case "activity":
      return s.liveness === "dead" && s.recoverability === "attachable" ? LIVE : s;
    case "container-tmux":
      return s.liveness === "live" && s.recoverability !== "attachable" ? LIVE_ATTACHABLE : s;
    case "container-none":
      return s.liveness === "live" && s.recoverability !== "resumable" ? LIVE_RESUMABLE : s;
    case "record-gone":
      return s.liveness === "dead" && s.recoverability === "resumable" ? GONE : s;
    case "record-present":
      return s.liveness === "dead" && s.recoverability === "gone" ? ENDED : s;
    case "seen-absent":
      return s.liveness === "unseen" ? ENDED : s;
    case "unseen":
      return s.liveness === "live" || s.recoverability === "attachable" ? UNSEEN : s;
  }
}

/**
 * 没有终端可去，只能 resume（旧 `status === "archived"`）：已结束 · 记录没了 · 说不清。
 *
 * 用在：× / Ctrl+W / 中键关得掉 · 自动跟随不跟 · 后台物化不排 · 右键给 Resume · 活动信号不收。
 * 可重连的会话**不**在其中：它的终端还在（↗ 拉前照样有效），今天也不许关（与改之前逐条相同）。
 * 说不清在其中：改之前固定复活的 tab 就是已结束，这些行为逐条照旧（变的只是它说的话）。
 *
 * ⚠ 不能再写成 `recoverability === "resumable"`：G3 之后「活 ＋ 只能重开」也是 `resumable`，
 *   那样写会把一条活着、只是不在 tmux 里的会话当成死的（能关、不跟随、菜单给 Resume）。
 */
export function isResumeOnly(s: SessionState): boolean {
  return s.liveness !== "live" && s.recoverability !== "attachable";
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
 * 一个状态在界面上长什么样（`U4b.md §1.3` 呈现表）。**这是说会话状态的字的唯一出处**：
 * tab 栏的类与 tooltip、机器总览的类 / 灯 / 「状态」一格都从这里取。
 */
export interface StateView {
  /**
   * `.ended`：「没有终端可去」那一套外观（整颗变淡、标题斜体、灯 / ↗ / 📂 隐藏、× 露出来）。
   * 已结束 · 记录没了 · 说不清三态外观相同、**字不同**（`name` / `tooltip`）—— 不新增 CSS 规则。
   * 说不清在改之前就是这副外观（固定复活置已结束），改的只是它说的话。
   */
  ended: boolean;
  /** `.reconnectable`：可重连（灯换成暗色、停呼吸）。 */
  reconnectable: boolean;
  /** 状态名（「已结束」「可重连」「记录已不在」「说不清」）；活着 ⇒ `null`（活着不另说状态，灯自己说）。 */
  name: string | null;
  /** 提示句（「说给用户的话」一列 ＋ U4b 的四句）；活着且容器没报 ⇒ `null`。 */
  tooltip: string | null;
}

export function stateView(s: SessionState): StateView {
  // `switch` 穷尽：三个活性 × 各自的可恢复性，漏一格编译期就会要。
  switch (s.liveness) {
    case "live":
      switch (s.recoverability) {
        case null:
          return { ended: false, reconnectable: false, name: null, tooltip: null };
        case "attachable":
          return {
            ended: false,
            reconnectable: false,
            name: null,
            tooltip: copyText("sessionState.liveAttachable.tooltip"),
          };
        case "resumable":
          return {
            ended: false,
            reconnectable: false,
            name: null,
            tooltip: copyText("sessionState.liveResumable.tooltip"),
          };
      }
      break;
    case "unseen":
      return {
        ended: true,
        reconnectable: false,
        name: copyText("sessionState.unseen.name"),
        tooltip: copyText("sessionState.unseen.tooltip"),
      };
    case "dead":
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
        case "gone":
          return {
            ended: true,
            reconnectable: false,
            name: copyText("sessionState.gone.name"),
            tooltip: copyText("sessionState.gone.tooltip"),
          };
      }
  }
  // 类型上走不到（上面两个内层 switch 穷尽）；给 tsc 一个出口。
  return { ended: false, reconnectable: false, name: null, tooltip: null };
}
