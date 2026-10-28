/**
 * **输入框上方那一排输入队**（终端面板）：会话那台那份名单（排第几 · 谁发的 · 几点 · 那一句开头 ·［撤回］）＋ 这一端还没回执的那一句
 * （发送中 ·［撤回］）。只画宿主交来的：先后与每条那一句（`said` · `from`）都是会话那台写好的，这里不各自记、不按线上值挑字。
 */
import { copyText } from "./copy-table";
import { clockFaceHere } from "./clock-face";
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

  constructor(private readonly host: QueueRowHost) {
    this.el = el("div");
    this.el.className = s.queueRow;
    this.el.dataset.role = "queue-row";
    this.el.hidden = true;
  }

  /** 照此刻的名单与待发重画（名单还没读到 ⇒ `null`）。 */
  paint(list: QueueList | null, pending: readonly Outgoing[]): void {
    const lines: HTMLElement[] = [];
    const items: readonly QueueItem[] = list?.items ?? [];
    for (const it of items) lines.push(this.line(it.said, `${it.from} · ${clockFaceHere(it.atMs)}`, it.text, it.state, () => this.host.cancel(it.id)));
    // 回执还没到、名单里已经有了的那一句只画一遍（照名单）。
    for (const o of pending) {
      if (items.some((it) => it.id === o.id)) continue;
      lines.push(this.line(copyText("terminal.queue.sending"), "", o.text, "sending", () => this.host.cancelPending(o.id)));
    }
    this.el.replaceChildren(...lines);
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
    line.append(head, button({ label: copyText("terminal.queue.cancel"), kind: "ghost", size: "compact", onClick: cancel }));
    return line;
  }
}
