/**
 * 状态栏最左那一枚「消息」：点开锚在它上方的浮层，列本次运行里最近 20 条提示（时刻 · 图标 · 一句 · 还能做的动作）；
 * 有没看过的出错提示时右上一个琥珀点。只在内存里（kit toast 的记录），不落盘。
 */
import { chip, setChipOpen } from "./kit/chip";
import { icon, type IconName } from "./kit/icon";
import { button } from "./kit/button";
import { openPopover } from "./kit/popover";
import { markRecordsSeen, onToastRecords, recentToasts, runRecordAction, unseenErrors, TOAST_RECORD_MAX, type ToastLevel } from "./kit/toast";
import { formatTimestampShort } from "./format";
import { copyText } from "./copy-table";
import s from "./status-messages.module.css";

const LEVEL_ICON: Record<ToastLevel, IconName> = { success: "success", info: "info", warn: "warning", error: "error" };

export class StatusMessages {
  readonly el: HTMLElement;
  private readonly dot: HTMLElement;
  private list: HTMLElement | null = null;

  private readonly trigger: HTMLElement;

  constructor() {
    this.trigger = chip({ text: copyText("statusBar.messages.label"), icon: "bell", onClick: () => this.toggle() });
    this.trigger.dataset.role = "status-messages";
    this.el = document.createElement("span");
    this.el.className = s.smChip;
    this.dot = document.createElement("span");
    this.dot.className = s.smDot;
    this.dot.hidden = true;
    this.el.append(this.trigger, this.dot);
    onToastRecords(() => this.refresh());
  }

  private refresh(): void {
    this.dot.hidden = !unseenErrors();
    if (this.list?.isConnected) this.fill(this.list);
  }

  private toggle(): void {
    const box = document.createElement("div");
    box.className = s.smBox;
    const head = document.createElement("div");
    head.className = s.smHead;
    const t = document.createElement("span");
    t.textContent = copyText("statusBar.messages.title");
    const keep = document.createElement("span");
    keep.className = s.smKeep;
    keep.textContent = copyText("statusBar.messages.keep", { n: TOAST_RECORD_MAX });
    head.append(t, keep);
    const list = document.createElement("div");
    list.className = s.smList;
    list.dataset.role = "messages-list";
    box.append(head, list);
    this.fill(list);
    const opened = openPopover(this.trigger, box, {
      label: copyText("statusBar.messages.title"),
      align: "start",
      onClose: () => {
        this.list = null;
        setChipOpen(this.trigger, false);
      },
    });
    if (!opened) return;
    this.list = list;
    setChipOpen(this.trigger, true);
    markRecordsSeen();
  }

  private fill(list: HTMLElement): void {
    list.replaceChildren();
    const recs = recentToasts();
    if (recs.length === 0) {
      const e = document.createElement("div");
      e.className = s.smEmpty;
      e.textContent = copyText("statusBar.messages.empty");
      list.appendChild(e);
      return;
    }
    for (const r of recs) {
      const row = document.createElement("div");
      row.className = s.smRow;
      row.dataset.level = r.level;
      const time = document.createElement("span");
      time.className = s.smTime;
      time.textContent = formatTimestampShort(r.at);
      const text = document.createElement("span");
      text.className = s.smText;
      text.textContent = r.detail ? copyText("statusBar.messages.line", { title: r.title, detail: r.detail.split("\n")[0] }) : r.title;
      text.title = r.detail ? `${r.title}\n${r.detail}` : r.title;
      row.append(time, icon(LEVEL_ICON[r.level], "compact"), text);
      if (r.count > 1) {
        const n = document.createElement("span");
        n.className = s.smTime;
        n.textContent = copyText("kit.toast.count", { n: r.count });
        row.appendChild(n);
      }
      for (const a of r.actions) row.appendChild(button({ label: a.label, kind: "ghost", size: "compact", onClick: () => runRecordAction(r, a) }));
      list.appendChild(row);
    }
  }
}
