/**
 * **活会话的记录文件被删 / 改名 / 截短 / 原地改写之后，那个 tab 说一句话。**
 *
 * 要求：「活会话的 jsonl 被改 / 删 / 改名：观察侧当它是
 * 『看的、不是管的』—— 删了 / 改名 ⇒ 出声（该 tab 说一句『记录文件不见了』），不崩、不误判结束；被截短 ⇒ 按截断重读」＋
 * 09-25 补「原地整份改写且变长 …… 从 0 重读并出声（与截短同一句话族）」。
 *
 * 来路：后端 `session_file_gone` / `session_file_reread` 帧 → monitor 会话内容流一格 `{"file_notice": …}`（与行同序）
 * → `events.ts` → `main.ts` 装的 `recordFileWiring`（查法是 `TabManager.streamElOf`）。这里只管那一句话画在哪、什么时候收：
 * - 画在流容器的第一个孩子（贴顶），不进记录那一层；同一个 tab 只有一句，新的盖旧的。
 * - 「不见了」那句：之后这个会话又来了一行（文件回来了）⇒ 收掉。「已从头重读」那句留着（它说的是已经发生的事）。
 * - ⚠ 不进留存：那句话没了（已知缺口）。
 * 字全住文案表 `sessionState.recordFile.*`（说到会话状态的字只住 `sessionState.*`，U4 判据 S5）。
 */
import { copyText } from "./copy-table";
import s from "./record-file-notice.module.css";

/** 线上三个字面量（monitor `stream_source::FileChange::as_wire`）。 */
export const RECORD_FILE_CHANGES = ["gone", "truncated", "rewritten"] as const;
export type RecordFileChange = (typeof RECORD_FILE_CHANGES)[number];

export function isRecordFileChange(c: string): c is RecordFileChange {
  return (RECORD_FILE_CHANGES as readonly string[]).includes(c);
}

/** 这个流容器顶上现在挂着的那一句（没有 ⇒ `null`）。 */
function noticeOf(streamEl: HTMLElement): HTMLElement | null {
  const first = streamEl.firstElementChild;
  return first instanceof HTMLElement && first.dataset.recordFileNotice !== undefined ? first : null;
}

/** 挂着「不见了」那一句的流容器（`onLine` 热路径上只查这一张表，不碰 DOM）。 */
const goneShown = new WeakSet<HTMLElement>();

/** 那一句的字（key 逐字写出：文案表判据只认字面 key）。 */
function sayOf(change: RecordFileChange): string {
  switch (change) {
    case "gone":
      return copyText("sessionState.recordFile.gone");
    case "truncated":
      return copyText("sessionState.recordFile.truncated");
    case "rewritten":
      return copyText("sessionState.recordFile.rewritten");
  }
}

/** 画 / 换那一句。 */
export function showRecordFileNotice(streamEl: HTMLElement, change: RecordFileChange): void {
  let el = noticeOf(streamEl);
  if (!el) {
    el = document.createElement("div");
    el.className = s.notice;
    el.dataset.recordFileNotice = "";
    streamEl.prepend(el);
  }
  el.dataset.change = change;
  el.textContent = sayOf(change);
  if (change === "gone") goneShown.add(streamEl);
  else goneShown.delete(streamEl);
}

/**
 * 主窗口的接线（`main.ts` 装进 `EventHandlers`）：只要一个「会话 → 它的流容器」的查法。
 * - `onSessionFileNotice`：认得的取值 ∧ 有这个 tab ⇒ 画；否则不画、不抛。
 * - `afterLine`：这个会话又来了一行（`TabManager.onLine` 之后调）⇒ 「不见了」那一句收掉。
 * ⚠ 只在主窗口接：本模块带一份 CSS Module，从 `tabs.ts`（主窗口与独立查看窗共用）引进来，样式会落进共用块、
 *   排到全局样式前面（`entry-graphs` 次序判据当场红过一次）。
 */
export function recordFileWiring(streamElOf: (sessionId: string) => HTMLElement | null): {
  onSessionFileNotice: (sessionId: string, change: string) => void;
  afterLine: (sessionId: string) => void;
} {
  return {
    onSessionFileNotice: (sessionId, change) => {
      if (!isRecordFileChange(change)) return;
      const el = streamElOf(sessionId);
      if (el) showRecordFileNotice(el, change);
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
