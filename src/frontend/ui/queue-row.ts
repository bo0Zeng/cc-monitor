/**
 * **输入框上方那一排输入队**：会话那台那份名单（排第几 · 谁发的 · 那一句开头 ·［撤回］）＋ 这一端还没回执的那一句
 * （发送中 ·［撤回］）。只画宿主交来的：先后与每条那一句（`said` · `from`）都是会话那台写好的，这里不各自记、不按线上值挑字。
 * 最多露三行、多的框里滚（排版，不数条数）。那台离线 ⇒ 照手上那一份画、淡着，头上一行「{连接层那句离线} · 名单为上次所知」。
 */
import { copyText } from "./copy-table";
import { firstLineOf } from "./format";
import { el } from "./kit/el";
import { button } from "./kit/button";
import type { Outgoing } from "./session-queue";
import type { QueueItem } from "./generated/QueueItem";
import type { QueueList } from "./generated/QueueList";
import s from "./queue-row.module.css";

export interface QueueRowHost {
  /** 撤名单里那一条（照编号请会话那台撤）。 */
  cancel(id: string): void;
  /** 撤这一端还没回执的那一句（交那一句的编号）。 */
  cancelPending(id: string): void;
}

/** 一句开头最多几个字。 */
const HEAD_CHARS = 60;

export class QueueRow {
  /** 那一排（没有一条 ⇒ 收起）。 */
  readonly el: HTMLElement;
  /** 每条一行的那一框（限高、框里滚）。 */
  private readonly lines: HTMLElement;

  constructor(private readonly host: QueueRowHost) {
    this.el = el("div");
    this.el.className = s.queueRow;
    this.el.dataset.role = "queue-row";
    this.el.hidden = true;
    this.lines = el("div");
    this.lines.className = s.queueLines;
    this.lines.dataset.role = "queue-lines";
  }

  /**
   * 照此刻的名单与待发重画（名单还没读到 ⇒ `null`）。`offline` ＝ 那台此刻离线那一句（连接层写好的；连着 ⇒ `null`）：
   * 有 ⇒ 那一排淡着、头上一行说名单是上次所知。
   */
  paint(list: QueueList | null, pending: readonly Outgoing[], offline: string | null = null): void {
    const lines: HTMLElement[] = [];
    const items: readonly QueueItem[] = list?.items ?? [];
    for (const it of items) lines.push(this.line(it.said, it.from, it.text, it.state, () => this.host.cancel(it.id)));
    // 回执还没到、名单里已经有了的那一句只画一遍（照名单）。
    for (const o of pending) {
      if (items.some((it) => it.id === o.id)) continue;
      lines.push(this.line(copyText("mobChat.delivery.sending"), "", o.text, "sending", () => this.host.cancelPending(o.id)));
    }
    this.lines.replaceChildren(...lines);
    const stale = offline !== null && lines.length > 0;
    if (stale) {
      const head = el("div", "", [offline, copyText("terminal.queue.stale")].join(copyText("kit.text.sep")));
      head.className = s.queueHead;
      head.dataset.role = "queue-head";
      this.el.replaceChildren(head, this.lines);
      this.el.dataset.stale = "true";
    } else {
      this.el.replaceChildren(this.lines);
      delete this.el.dataset.stale;
    }
    this.el.hidden = lines.length === 0;
  }

  private line(state: string, from: string, text: string, at: string, cancel: () => void): HTMLElement {
    const line = el("div");
    line.className = s.queueLine;
    line.dataset.role = "queue-line";
    line.dataset.state = at;
    const st = el("span", "", state);
    st.className = s.queueState;
    const head = el("span", "", firstLineOf(text, HEAD_CHARS).line);
    head.className = s.queueText;
    head.title = text;
    line.append(st);
    if (from !== "") {
      const f = el("span", "", from);
      f.className = s.queueFrom;
      line.append(f);
    }
    line.append(head, button({ label: copyText("mobChat.outbox.cancel"), kind: "ghost", size: "compact", onClick: cancel }));
    return line;
  }
}
