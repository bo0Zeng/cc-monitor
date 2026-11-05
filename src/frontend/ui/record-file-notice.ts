/**
 * 活会话的记录文件被删 / 改名 / 截短 / 原地改写之后，那个 tab 说一句话（观察侧只看不管：不崩、不误判结束；截短与整份改写从 0 重读）。
 *
 * 来路：后端 `session_file_gone` / `session_file_reread` 帧 → monitor 会话内容流一格 `{"file_notice": …}`（与行同序）
 * → `events.ts` → `main.ts` 装的 `recordFileWiring`（查法是 `TabManager.streamElOf`）。这里只管那一句话画在哪、什么时候收：
 * - 画在流容器的第一个孩子（贴顶），不进记录那一层；同一个 tab 只有一句，新的盖旧的。
 * - 「不见了」那句：之后这个会话又来了一行（文件回来了）⇒ 收掉。「已从头重读」那句留着（它说的是已经发生的事）。
 * - 不进留存：那句话没了（已知缺口）。
 * 那一句是那台后端写好的（帧上的 `said`，格里原样带来），这里照摆；取值（生成类型 `SessionFileNoticePayload.change`）只用来定什么时候收：
 * 是「不见了」（`gone`）那一句 ⇒ 下一行收，别的留着。
 */
import s from "./record-file-notice.module.css";
import type { SessionFileNoticePayload } from "./generated/SessionFileNoticePayload";

type Change = SessionFileNoticePayload["change"];

/** 这个流容器顶上现在挂着的那一句（没有 ⇒ `null`）。 */
function noticeOf(streamEl: HTMLElement): HTMLElement | null {
  const first = streamEl.firstElementChild;
  return first instanceof HTMLElement && first.dataset.recordFileNotice !== undefined ? first : null;
}

/** 挂着「不见了」那一句的流容器（`onLine` 热路径上只查这一张表，不碰 DOM）。 */
const goneShown = new WeakSet<HTMLElement>();

/** 画 / 换那一句（`said` 照摆）。 */
export function showRecordFileNotice(streamEl: HTMLElement, change: Change, said: string): void {
  let el = noticeOf(streamEl);
  if (!el) {
    el = document.createElement("div");
    el.className = s.notice;
    el.dataset.recordFileNotice = "";
    streamEl.prepend(el);
  }
  el.dataset.change = change;
  el.textContent = said;
  if (change === "gone") goneShown.add(streamEl);
  else goneShown.delete(streamEl);
}

/**
 * 主窗口的接线（`main.ts` 装进 `EventHandlers`）：只要一个「会话 → 它的流容器」的查法。
 * - `onSessionFileNotice`：有这个 tab ⇒ 画那一句；没有 ⇒ 不画、不抛。
 * - `afterLine`：这个会话又来了一行（`TabManager.onLine` 之后调）⇒ 「不见了」那一句收掉。
 * 只在主窗口接：本模块带一份 CSS Module，从主窗口与独立查看窗共用的 `tabs.ts` 引进来的话，样式会落进共用块、排到全局样式前面。
 */
export function recordFileWiring(streamElOf: (sessionId: string) => HTMLElement | null): {
  onSessionFileNotice: (sessionId: string, change: Change, said: string) => void;
  afterLine: (sessionId: string) => void;
} {
  return {
    onSessionFileNotice: (sessionId, change, said) => {
      const el = streamElOf(sessionId);
      if (el) showRecordFileNotice(el, change, said);
    },
    afterLine: (sessionId) => {
      const el = streamElOf(sessionId);
      if (el) clearGoneNotice(el);
    },
  };
}

/** 这个会话又来了一行 ⇒ 「不见了」那一句收掉（别的那两句不动）。 */
export function clearGoneNotice(streamEl: HTMLElement): void {
  if (!goneShown.has(streamEl)) return;
  goneShown.delete(streamEl);
  noticeOf(streamEl)?.remove();
}
