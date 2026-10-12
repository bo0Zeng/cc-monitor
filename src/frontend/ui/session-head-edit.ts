/**
 * **会话头按钮的编辑浮层**（ui3 · 10-11 稿「会话头按钮」§3.2）：锚在「⋯」下方，非模态（Esc · 点外面 · 再点「⋯」关）。
 *
 * 标题「会话头按钮」· 一行灰字 · 清单两段（上段 ＝ 会话头上的、按顺序；下段 ＝ 只在「⋯」里的）· 末行「更多 ⋯」灰勾固定 ·
 * 底部灰字 ＋［恢复默认］。每行：拖柄 · 复选框 · 图标 · 名字 · 右侧灰字（只在哪种会话才有）。
 * **立刻生效**：勾一下、拖一下、Alt+↑↓ 一下就交给宿主（宿主落盘、会话头当场跟着变），没有「保存」。
 * 键盘：↑↓ 走行 · 空格勾 / 不勾 · Alt+↑ / Alt+↓ 挪一格 · Esc 关（弹层栈）、焦点回「⋯」。
 *
 * 只排版：哪几颗能放、默认是哪几颗、怎么挪住 `session-head-buttons.ts`；落盘住 `session-head.ts`。
 */
import { copyText } from "./copy-table";
import { icon, type IconName } from "./kit/icon";
import { button } from "./kit/button";
import { dropHeadButton, headEditRows, nudgeHeadButton, toggleHeadButton, type HeadButtonId } from "./session-head-buttons";
import s from "./session-head.module.css";

export interface HeadEditHost {
  /** 会话头上此刻那几颗（按顺序）。 */
  buttons(): readonly HeadButtonId[];
  /** 改成这一排（宿主落盘、重画会话头）。 */
  set(next: HeadButtonId[]): void;
  /** ［恢复默认］。 */
  reset(): void;
}

/** 每一颗在清单里的名字 · 图标 · 右侧灰字（只在哪种会话才有）。 */
export const HEAD_BUTTON_FACE: Readonly<Record<HeadButtonId, { icon: IconName; name: () => string; note?: () => string }>> = {
  cwd: { icon: "folder", name: () => copyText("tabMenu.open.cwd"), note: () => copyText("sessionHead.edit.onlyLocal") },
  front: { icon: "front", name: () => copyText("tabMenu.open.front"), note: () => copyText("sessionHead.edit.onlyWindows") },
  terminal: { icon: "terminal", name: () => copyText("sessionHead.act.terminal") },
  popOut: { icon: "popOut", name: () => copyText("sessionHead.act.popOut") },
  find: { icon: "search", name: () => copyText("sessionHead.act.find") },
  account: { icon: "account", name: () => copyText("sessionHead.act.account") },
  display: { icon: "list", name: () => copyText("sessionHead.act.display"), note: () => copyText("sessionHead.edit.displayNote") },
  pin: { icon: "pin", name: () => copyText("tabMenu.pin.pin") },
  group: { icon: "list", name: () => copyText("tabMenu.collection.add") },
  resume: { icon: "history", name: () => copyText("tabMenu.item.resume"), note: () => copyText("sessionState.headEdit.onlyEnded") },
  close: { icon: "close", name: () => copyText("tabMenu.item.close") },
};

export class HeadEditor {
  readonly el: HTMLElement;
  private readonly list: HTMLElement;
  private readonly line: HTMLElement;

  constructor(private readonly host: HeadEditHost) {
    this.el = document.createElement("div");
    this.el.className = s.shEdit;
    const title = document.createElement("div");
    title.className = s.shEditTitle;
    title.textContent = copyText("sessionHead.edit.title");
    const hint = document.createElement("div");
    hint.className = s.shEditHint;
    hint.textContent = copyText("sessionHead.edit.hint");
    this.list = document.createElement("div");
    this.list.className = s.shEditList;
    this.list.setAttribute("role", "list");
    this.list.addEventListener("keydown", (e) => this.onKey(e));
    this.line = document.createElement("div");
    this.line.className = s.shEditLine;
    this.line.style.visibility = "hidden";
    const foot = document.createElement("div");
    foot.className = s.shEditFoot;
    const note = document.createElement("span");
    note.className = s.shEditHint;
    note.textContent = copyText("sessionHead.edit.overflow");
    foot.append(note, button({ label: copyText("sessionHead.edit.reset"), size: "compact", onClick: () => this.host.reset() }));
    this.el.append(title, hint, this.list, foot);
    this.update();
  }

  /** 照此刻那一排重画清单；焦点在哪一行就还在那一行（挪过之后也跟着它）。 */
  update(): void {
    const focused = (document.activeElement as HTMLElement | null)?.closest<HTMLElement>("[data-row]")?.dataset.row;
    const rows = headEditRows(this.host.buttons());
    const els: HTMLElement[] = [];
    let wasOn = true;
    for (const r of rows) {
      if (wasOn && !r.on) els.push(this.divider());
      wasOn = r.on;
      els.push(this.row(r.id, r.on));
    }
    els.push(this.divider(), this.moreRow(), this.line);
    this.list.replaceChildren(...els);
    const first = this.list.querySelector<HTMLElement>("[data-row]");
    if (first) first.tabIndex = 0;
    if (focused) this.rowEl(focused)?.focus();
  }

  /** 焦点进第一行（浮层开着时）。 */
  focusFirst(): void {
    this.list.querySelector<HTMLElement>("[data-row]")?.focus();
  }

  private divider(): HTMLElement {
    const d = document.createElement("div");
    d.className = s.shEditSep;
    d.setAttribute("role", "separator");
    return d;
  }

  private rowEl(id: string): HTMLElement | null {
    return this.list.querySelector<HTMLElement>(`[data-row="${id}"]`);
  }

  private row(id: HeadButtonId, on: boolean): HTMLElement {
    const face = HEAD_BUTTON_FACE[id];
    const r = document.createElement("div");
    r.className = s.shEditRow;
    r.dataset.row = id;
    r.tabIndex = -1;
    r.setAttribute("role", "listitem");
    const grip = document.createElement("span");
    grip.className = s.shEditGrip;
    grip.appendChild(icon("drag", "compact"));
    grip.addEventListener("mousedown", (e) => this.beginDrag(e, id));
    const box = document.createElement("input");
    box.type = "checkbox";
    box.tabIndex = -1;
    box.checked = on;
    box.setAttribute("aria-label", face.name());
    box.addEventListener("change", () => this.host.set(toggleHeadButton(this.host.buttons(), id)));
    const name = document.createElement("span");
    name.className = s.shEditName;
    name.textContent = face.name();
    r.append(grip, box, icon(face.icon, "compact"), name);
    if (face.note) {
      const n = document.createElement("span");
      n.className = s.shEditNote;
      n.textContent = face.note();
      r.appendChild(n);
    }
    return r;
  }

  /** 末行「更多 ⋯」：灰勾、不能取消、固定在最右。 */
  private moreRow(): HTMLElement {
    const r = document.createElement("div");
    r.className = `${s.shEditRow} ${s.shEditFixed}`;
    r.dataset.row = "more";
    r.tabIndex = -1;
    r.setAttribute("role", "listitem");
    const grip = document.createElement("span");
    grip.className = s.shEditGrip;
    const box = document.createElement("input");
    box.type = "checkbox";
    box.tabIndex = -1;
    box.checked = true;
    box.disabled = true;
    box.setAttribute("aria-label", copyText("tabBarView.tab.moreHint"));
    const name = document.createElement("span");
    name.className = s.shEditName;
    name.textContent = copyText("tabBarView.tab.moreHint");
    const n = document.createElement("span");
    n.className = s.shEditNote;
    n.textContent = copyText("sessionHead.edit.fixedMore");
    r.append(grip, box, icon("more", "compact"), name, n);
    return r;
  }

  private onKey(e: KeyboardEvent): void {
    if (e.isComposing) return;
    const row = (e.target as HTMLElement | null)?.closest<HTMLElement>("[data-row]");
    if (!row) return;
    const id = row.dataset.row!;
    const rows = [...this.list.querySelectorAll<HTMLElement>("[data-row]")];
    const i = rows.indexOf(row);
    if ((e.key === "ArrowDown" || e.key === "ArrowUp") && e.altKey) {
      e.preventDefault();
      if (id !== "more") this.host.set(nudgeHeadButton(this.host.buttons(), id as HeadButtonId, e.key === "ArrowUp" ? -1 : 1));
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      rows[Math.max(0, Math.min(rows.length - 1, i + (e.key === "ArrowUp" ? -1 : 1)))]?.focus();
      return;
    }
    if ((e.key === " " || e.code === "Space") && e.target === row) {
      e.preventDefault();
      if (id !== "more") this.host.set(toggleHeadButton(this.host.buttons(), id as HeadButtonId));
    }
  }

  /** 按住拖柄拖：插入线 2px accent 画在落点那一行的上沿；松手 ⇒ 交宿主（落在上段 ＝ 放到会话头那个位置，下段 ＝ 只在「⋯」）。 */
  private beginDrag(e: MouseEvent, id: HeadButtonId): void {
    if (e.button !== 0) return;
    e.preventDefault();
    const rows = (): HTMLElement[] => [...this.list.querySelectorAll<HTMLElement>("[data-row]")].filter((r) => r.dataset.row !== "more");
    let at: number | null = null;
    const src = this.rowEl(id);
    if (src) src.dataset.lifted = "";
    const move = (ev: MouseEvent): void => {
      const rs = rows();
      let k = rs.length;
      for (let j = 0; j < rs.length; j++) {
        const r = rs[j].getBoundingClientRect();
        if (ev.clientY < r.top + r.height / 2) {
          k = j;
          break;
        }
      }
      at = k;
      const ref = rs[k] ?? rs[rs.length - 1];
      if (!ref) return;
      const lr = this.list.getBoundingClientRect();
      const rr = ref.getBoundingClientRect();
      this.line.style.visibility = "";
      this.line.style.top = `${(k < rs.length ? rr.top : rr.bottom) - lr.top - 1}px`;
    };
    const up = (): void => {
      document.removeEventListener("mousemove", move);
      document.removeEventListener("mouseup", up);
      if (src) delete src.dataset.lifted;
      this.line.style.visibility = "hidden";
      if (at !== null) this.host.set(dropHeadButton(this.host.buttons(), id, at));
    };
    document.addEventListener("mousemove", move);
    document.addEventListener("mouseup", up);
  }
}
