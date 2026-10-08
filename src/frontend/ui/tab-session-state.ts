/**
 * **一个会话此刻是什么状态 —— 两个正交的轴。**
 *
 * | 轴 | 取值 | 谁说 |
 * |---|---|---|
 * | 活性 | 活 / 死 | 会话所在的那台机器（后端 pidfile ＋ 探活） |
 * | 可恢复性 | 能接回去（容器还在）/ 只能 resume / 连记录都没了 | 活着时由容器类型定（后端 `session_added.container`）；死的那一刻由 monitor 裁（`idle` 格 / `ended` 格）；记录在不在由 resume 一跳问那台后端 |
 *
 * 「死了」与「能不能接回去」是两件事，不许挤在一个词里（不然可重连的会话会被记成活的，claude 进程其实已经没了）。
 *
 * # 这份文件管四件事，别处不各判一遍
 *
 * 1. **形状**：`SessionState` 判别联合（为什么是联合、为什么没有 `gone`，见类型头注）。
 * 2. **转移**：五种事件怎么改它（`nextState`）—— 事件从哪来（只远端 / 只本机）那道门留在 `TabManager`。
 * 3. **行为谓词**：× 能不能关、↗ 能不能拉前、算不算活跃……都是这里的一个谓词。
 *
 * 4. **呈现**：CSS 类开关 · 状态名 · 提示句（`stateView`）—— 界面上说到会话状态的字只从两轴派生，文字住文案表
 *    `src/shared/copy/table.json` 的 `sessionState.*`（那张表：已结束 / 可重连）。
 *
 * 后端给的几格事实：
 * - 活着时可恢复性那一格来自容器事实（`container-hosted` / `container-none` / `container-other`）；
 * - 可恢复性第三值 `gone`（resume 一跳问过那台后端、记录不在）；
 * - 活性第三值 `unseen` = 说不清（固定复活、那台机器还没把活会话清单报完）。
 */
import type { SessionContainer } from "./generated/SessionContainer";
import { copyText } from "./copy-table";

/*
 * 活性（下面 `liveness` 那一格）：进程在不在。
 * - `unseen`：**说不清** —— 由固定复活（那台机器还没把它的活会话清单报完）与那台机器看不见了（连接断了）产出。
 *   它说的是「机器看不见」，**不是**「会话死了」：`Unseen` 不许被显示成已结束。
 */
/**
 * 可恢复性：死了之后怎么回去（活着时 = 它现在死了会落到哪一格）。
 * - `attachable`：容器（tmux）还在 / 在 tmux 里跑，接回去就行 —— 「可重连」。
 * - `resumable`：没有容器，只能 resume 一个新进程接着那份记录 —— 「已结束」。
 * - `gone`：连记录都没了（resume 一跳问过那台后端，`history-record` 答不在）—— 重开必失败。
 *   只挂在「死 ＋ 只能 resume」上：容器还在的（可重连）照样接得回去，记录没了不改它那一格。
 */
export type Recoverability = "attachable" | "resumable" | "gone";

/**
 * 活着时可恢复性的第四种说法：那台报来的容器是这边不认识的宿主（那台比这边新）⇒ 终端形式未知。
 * 不当可接回、也不当只能重开：说不清它死了会落到哪一格，等死的那一刻的裁决。
 */
export type UnknownHost = "unknown-host";

/**
 * 两轴合起来。**判别联合，不是两个独立字段**：
 * - 活着：可恢复性是容器事实（`attachable` / `resumable`），后端没报（旧后端 / 判不了）⇒ `null`；
 *   活着不会是 `gone`（记录在不在只在 resume 那一刻问，而活会话不走 resume）。
 * - 死了：每一条死亡事件都带着 monitor 的裁决 ⇒ 没有「死了 ＋ 没报」。
 * - 说不清：什么都说不出 ⇒ 可恢复性 `null`。
 * 类型上不许出现别的组合，免得有人拿一个默认值去填空格。
 */
export type SessionState =
  | { readonly liveness: "live"; readonly recoverability: "attachable" | "resumable" | UnknownHost | null }
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
/** 活着，容器是这边不认识的宿主 ⇒ 终端形式未知（不当可接回）。 */
export const LIVE_UNKNOWN_HOST: SessionState = Object.freeze({
  liveness: "live",
  recoverability: "unknown-host",
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
 * - `container-hosted` / `container-none` / `container-other`：`container` 格（后端 `session_added.container`：
 *   在认得的宿主里 · 不在任何宿主里 · 这边不认识的宿主）
 * - `record-gone` / `record-present`：resume 一跳问那台后端（`history-record`）的答案
 * - `seen-absent`：那台机器的活会话清单报完了、里面没有它（会话流的 `listed` 格）
 * - `unseen`：那台机器看不见了（`unseen` 格：到它的连接断了 / F5 时它还没报完清单）
 */
export type StateEvent =
  | "ended"
  | "idle"
  | "started"
  | "remote-line"
  | "activity"
  | "container-hosted"
  | "container-none"
  | "container-other"
  | "record-gone"
  | "record-present"
  | "seen-absent"
  | "unseen";

/** 容器那一格落成的状态事件。 */
export type ContainerEvent = "container-hosted" | "container-none" | "container-other";

/** 容器那一格 ⇒ 状态事件：按判别联合的 `form` 分，不写宿主字面量（认哪些宿主由生成的类型说）。 */
export function containerEvent(c: SessionContainer): ContainerEvent {
  switch (c.form) {
    case "hosted":
      return "container-hosted";
    case "none":
      return "container-none";
    case "other":
      return "container-other";
  }
}

/**
 * 转移表。返回值与入参是同一个对象 ⇔ 这次事件不改状态
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
    case "container-hosted":
      return s.liveness === "live" && s.recoverability !== "attachable" ? LIVE_ATTACHABLE : s;
    case "container-none":
      return s.liveness === "live" && s.recoverability !== "resumable" ? LIVE_RESUMABLE : s;
    case "container-other":
      return s.liveness === "live" && s.recoverability !== "unknown-host" ? LIVE_UNKNOWN_HOST : s;
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
 * 没有终端可去，只能 resume：已结束 · 记录没了 · 说不清。
 *
 * 用在：关得掉（右键菜单 · 批量）· 自动跟随不跟 · 后台物化不排 · 活动信号不收。
 * 可重连的会话不在其中：它的终端还在（↗ 拉前照样有效），也不许关。
 * 说不清在其中，但它不当已结束画、不给恢复（[`canResume`]）、× / 中键 / W 不关它（[`closesWithoutMenu`]）。
 *
 * 不能写成 `recoverability === "resumable"`：「活 ＋ 只能重开」也是 `resumable`，那样会把活着、只是不在 tmux 里的会话当成死的。
 */
export function isResumeOnly(s: SessionState): boolean {
  return s.liveness !== "live" && s.recoverability !== "attachable";
}

/** 死了、没有终端可去：已结束 · 记录没了。 */
function isEnded(s: SessionState): boolean {
  return s.liveness === "dead" && s.recoverability !== "attachable";
}

/**
 * 能恢复（Resume / 在 tmux 里起）：**死了**、只能 resume（已结束 · 记录没了）。
 * 说不清的不算 —— 那台暂时看不见，会话也许还在跑；本机那一形再起一份就是同一个会话两个 claude。
 */
export function canResume(s: SessionState): boolean {
  return isEnded(s);
}

/**
 * × · 中键 · W 关得掉：已结束 · 记录没了。说不清的只在右键菜单里关 —— 它也许还在跑，那台连上之后会回来。
 */
export function closesWithoutMenu(s: SessionState): boolean {
  return isEnded(s);
}

/** 还有一个终端可去：活着，或者容器还在。↗ 拉前 · 本机「杀死会话」那几格用它。 */
export function hasTerminal(s: SessionState): boolean {
  return !isResumeOnly(s);
}

/** 在 tmux 里（活着在 tmux 里，或死了 tmux 会话还在）：终端窗口关了也能「在终端里打开」。 */
export function inTmux(s: SessionState): boolean {
  return s.recoverability === "attachable";
}

/** 活性 == 活。状态栏「活跃 N」· 固定条目的「最后活动时刻」· 机器总览的活跃会话数用它。 */
export function isLive(s: SessionState): boolean {
  return s.liveness === "live";
}

/**
 * 一个状态在界面上长什么样。这是说会话状态的字的唯一出处：
 * tab 栏的类与 tooltip、机器总览的类 / 灯 / 「状态」一格都从这里取。
 */
export interface StateView {
  /**
   * `.ended`：已结束那一套外观（整颗变淡、标题斜体、灯 / ↗ / 📂 隐藏、× 露出来）。已结束 · 记录没了两态，字不同（`name` / `tooltip`）。
   */
  ended: boolean;
  /** `.unseen`：说不清（那台暂时看不见）—— 单独一个状态，不当已结束画（标题照常、不出 ×，灯换成暗色）。 */
  unseen: boolean;
  /** `.reconnectable`：可重连（灯换成暗色、停呼吸）。 */
  reconnectable: boolean;
  /** 状态名（「已结束」「可重连」「记录已不在」「说不清」）；活着 ⇒ `null`（活着不另说状态，灯自己说）。 */
  name: string | null;
  /** 提示句；活着且容器没报 ⇒ `null`。 */
  tooltip: string | null;
}

export function stateView(s: SessionState): StateView {
  // `switch` 穷尽：三个活性 × 各自的可恢复性，漏一格编译期就会要。
  switch (s.liveness) {
    case "live":
      switch (s.recoverability) {
        case null:
          return { ended: false, unseen: false, reconnectable: false, name: null, tooltip: null };
        case "attachable":
          return {
            ended: false,
            unseen: false,
            reconnectable: false,
            name: null,
            tooltip: copyText("sessionState.liveAttachable.tooltip"),
          };
        case "resumable":
          return {
            ended: false,
            unseen: false,
            reconnectable: false,
            name: null,
            tooltip: copyText("sessionState.liveResumable.tooltip"),
          };
        case "unknown-host":
          return {
            ended: false,
            unseen: false,
            reconnectable: false,
            name: null,
            tooltip: copyText("sessionState.liveUnknownHost.tooltip"),
          };
      }
      break;
    case "unseen":
      return {
        ended: false,
        unseen: true,
        reconnectable: false,
        name: copyText("sessionState.unseen.name"),
        tooltip: copyText("sessionState.unseen.tooltip"),
      };
    case "dead":
      switch (s.recoverability) {
        case "resumable":
          return {
            ended: true,
            unseen: false,
            reconnectable: false,
            name: copyText("sessionState.ended.name"),
            tooltip: copyText("sessionState.ended.tooltip"),
          };
        case "attachable":
          return {
            ended: false,
            unseen: false,
            reconnectable: true,
            name: copyText("sessionState.reconnectable.name"),
            tooltip: copyText("sessionState.reconnectable.tooltip"),
          };
        case "gone":
          return {
            ended: true,
            unseen: false,
            reconnectable: false,
            name: copyText("sessionState.gone.name"),
            tooltip: copyText("sessionState.gone.tooltip"),
          };
      }
  }
  // 类型上走不到（上面两个内层 switch 穷尽）；给 tsc 一个出口。
  return { ended: false, unseen: false, reconnectable: false, name: null, tooltip: null };
}
