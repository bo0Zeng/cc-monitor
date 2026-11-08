/**
 * **输入框上方那一排输入队**：会话那台那份名单 ＋ 这一端还没回执的那几句，每条一行：那一句此刻的样子 · 那一句开头 ·［撤回］。
 * 不写第几（先后就是上下顺序）、不写哪一端。名单那几条的那一句是会话那台写好的（`said`）；待发那几句的那一句由宿主交来
 * （在飞 ⇒「发送中 · 等确认」；交了没回执 ⇒ 那台离线照它成品的 `heldSaid`、连着说「排队 · 无应答」，那一行淡着；那台拒了 ⇒ 那台那一句，
 * 错误色同名单里没送进会话那一条）。这里不各自记、不按线上值挑字。
 * 最多露三行、多的框里滚（排版，不数条数）。那台离线 ⇒ 照手上那一份画、淡着，排头单独一行「名单为上次所知」（是哪台、为什么看机器条）。
 */
import { copyText } from "./copy-table";
import { firstLineOf } from "./format";
import { el } from "./kit/el";
import { button } from "./kit/button";
import type { QueueItem } from "./generated/QueueItem";
import type { QueueList } from "./generated/QueueList";
import s from "./queue-row.module.css";

export interface QueueRowHost {
  /** 撤名单里那一条（照编号请会话那台撤；`recall` 是撤成了回到输入框的字，编辑已发的那一句没有：它的字留在编辑框）。 */
  cancel(id: string, recall: string | null): void;
  /** 撤这一端还没回执的那一句（交那一句的编号与字）。 */
  cancelPending(id: string, text: string): void;
}

/** 待发里的一行：编号 · 那一句 · 此刻那一句样子（宿主写好的）· 交了没回执（淡着）· 那台拒了（错误色）。 */
export interface PendingLine {
  id: string;
  text: string;
  said: string;
  held: boolean;
  refused: boolean;
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

  /** 照此刻的名单与待发重画（名单还没读到 ⇒ `null`）。`stale` ＝ 那台此刻离线：那一排淡着、排头一行说名单是上次所知。 */
  paint(list: QueueList | null, pending: readonly PendingLine[], stale: boolean): void {
    const lines: HTMLElement[] = [];
    const items: readonly QueueItem[] = list?.items ?? [];
    for (const it of items) lines.push(this.line(it.said, it.text, it.state, () => this.host.cancel(it.id, it.rewind === undefined ? it.text : null)));
    // 回执还没到、名单里已经有了的那一句只画一遍（照名单）。
    for (const o of pending) {
      if (items.some((it) => it.id === o.id)) continue;
      // 拒了的那一行同名单里没送进会话那一条的样子（错误色）。
      const at = o.refused ? "undelivered" : o.held ? "held" : "sending";
      lines.push(this.line(o.said, o.text, at, () => this.host.cancelPending(o.id, o.text)));
    }
    this.lines.replaceChildren(...lines);
    if (stale && lines.length > 0) {
      const head = el("div", "", copyText("terminal.queue.stale"));
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

  private line(said: string, text: string, at: string, cancel: () => void): HTMLElement {
    const line = el("div");
    line.className = s.queueLine;
    line.dataset.role = "queue-line";
    line.dataset.state = at;
    const st = el("span", "", said);
    st.className = s.queueState;
    const head = el("span", "", firstLineOf(text, HEAD_CHARS).line);
    head.className = s.queueText;
    head.title = text;
    line.append(st, head, button({ label: copyText("mobChat.outbox.cancel"), kind: "ghost", size: "compact", onClick: cancel }));
    return line;
  }
}
