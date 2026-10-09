/**
 * 状态栏最左那一枚「消息」：点开锚在它上方的浮层，列本次运行里最近 20 条提示（时刻 · 图标 · 一句 · 还能做的动作）；
 * 有没看过的出错提示时右上一个琥珀点。只在内存里（kit toast 的记录），不落盘。
 */
import { chip, setChipOpen } from "./kit/chip";
import { icon, type IconName } from "./kit/icon";
import { button } from "./kit/button";
import { foldCaret } from "./kit/fold";
import { openPopover } from "./kit/popover";
import { markRecordsSeen, onToastRecords, recentToasts, recordDetail, runRecordAction, unseenErrors, TOAST_RECORD_MAX, type ToastLevel, type ToastRecord } from "./kit/toast";
import { copyDetailButton } from "./kit/detail";
import { copyText } from "./copy-table";
import s from "./status-messages.module.css";

const LEVEL_ICON: Record<ToastLevel, IconName> = { success: "success", info: "info", warn: "warning", error: "error" };

let shown: StatusMessages | null = null;

function adopt(m: StatusMessages): void {
  shown = m;
}

/** 打开「消息」并展开这一条（批量结果的［查看］用：逐条原因在那里看）。「消息」还没挂上 ⇒ 什么都不做。 */
export function showMessage(r: ToastRecord): void {
  shown?.reveal(r);
}

export class StatusMessages {
  readonly el: HTMLElement;
  private readonly dot: HTMLElement;
  private list: HTMLElement | null = null;
  /** 展开着的那几条（带逐条明细的才展得开）。 */
  private readonly expanded = new WeakSet<ToastRecord>();
  /** 展开了［详情］的那几条（复制详情那几行）。 */
  private readonly detailOpen = new WeakSet<ToastRecord>();

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
    adopt(this);
  }

  /** 打开（已开着就不关）、展开这一条、滚到它。 */
  reveal(r: ToastRecord): void {
    this.expanded.add(r);
    if (this.list?.isConnected) this.fill(this.list);
    else this.toggle();
    this.list?.querySelector<HTMLElement>("[data-more=open]")?.scrollIntoView?.({ block: "nearest" });
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
      time.textContent = toastClock(r.at);
      const text = document.createElement("span");
      text.className = s.smText;
      text.textContent = r.detail ? copyText("statusBar.messages.line", { title: r.title, detail: r.detail.split("\n")[0] }) : r.title;
      text.title = r.detail ? `${r.title}\n${r.detail}` : r.title;
      row.append(time, icon(LEVEL_ICON[r.level], "compact"), text);
      const open = r.more.length > 0 && this.expanded.has(r);
      if (r.more.length > 0) {
        // 带逐条明细的：点这一行展开 / 收起。
        row.dataset.more = open ? "open" : "shut";
        text.setAttribute("role", "button");
        text.tabIndex = 0;
        text.setAttribute("aria-expanded", String(open));
        const flip = (): void => {
          if (this.expanded.has(r)) this.expanded.delete(r);
          else this.expanded.add(r);
          this.fill(list);
        };
        text.addEventListener("click", flip);
        text.addEventListener("keydown", (ev) => {
          if (ev.key === "Enter" || ev.key === " ") {
            ev.preventDefault();
            flip();
          }
        });
        text.prepend(foldCaret());
      }
      if (r.count > 1) {
        const n = document.createElement("span");
        n.className = s.smTime;
        n.textContent = copyText("kit.toast.count", { n: r.count });
        row.appendChild(n);
      }
      for (const a of r.actions.filter((x) => !x.toastOnly)) row.appendChild(button({ label: a.label, kind: "ghost", size: "compact", onClick: () => runRecordAction(r, a) }));
      // 带复制详情的那条：［详情］展开 / 收起（只读可选中）＋ 同一颗［复制详情］（复制出全部段；本次运行内都在）。
      const showDetail = r.copy.length > 0 && this.detailOpen.has(r);
      if (r.copy.length > 0) {
        const flipDetail = button({
          label: copyText("messages.record.expand"),
          kind: "ghost",
          size: "compact",
          onClick: () => {
            if (this.detailOpen.has(r)) this.detailOpen.delete(r);
            else this.detailOpen.add(r);
            this.fill(list);
          },
        });
        flipDetail.setAttribute("aria-expanded", String(showDetail));
        row.appendChild(flipDetail);
        const copy = copyDetailButton(() => recordDetail(r), r.copy[0][1]);
        if (copy) row.appendChild(copy);
      }
      list.appendChild(row);
      if (showDetail) {
        // 合流 ×N：只显示最近一段 ＋「另 n 段」，复制出全部 N 段。
        const box = document.createElement("div");
        box.className = s.smDetail;
        box.dataset.role = "message-detail";
        box.dataset.detailHost = "";
        box.textContent = r.copy[r.copy.length - 1][1];
        if (r.copy.length > 1) {
          const rest = document.createElement("div");
          rest.className = s.smTime;
          rest.textContent = copyText("messages.record.moreSegments", { n: r.copy.length - 1 });
          box.appendChild(rest);
        }
        list.appendChild(box);
      }
      if (open) {
        const more = document.createElement("div");
        more.className = s.smMore;
        more.dataset.role = "message-more";
        for (const line of r.more) {
          const l = document.createElement("div");
          l.textContent = line;
          more.appendChild(l);
        }
        list.appendChild(more);
      }
    }
  }
}

/** 这条提示是几点出的：看的人这台的钟面 —— 提示是界面自己出的事，不来自哪台后端，没有「那台的本地钟」可照抄。 */
function toastClock(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}
