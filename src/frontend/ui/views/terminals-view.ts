/**
 * 主窗口顶层「终端」视图：左边是各台的终端名单（按机器分组），右边是抽屉里同一个终端面部件（宿主交进来，看的是选中那一行的终端）。
 * 与历史同级的浮层：命令面板「终端」进，`Esc` · 返回 回原标签页。
 *
 * - 名单与状态全是本机后端 `terminals-board` 写好的（组 · 行 · 字 · 筛选四格的数）：这里只挑格摆、按 `tags` 筛、按 `find` 找。
 * - **实时只改行内字不挪行**：按 `key` 认行；已在的行原地改字、原位不动；新来的接在那一组末尾；没了的删掉。
 *   排在哪是「在看什么」那一份（界面自己的）：重新打开 · 换筛选才照后端的先后重排。
 * - 什么时候再问：打开时 · 宿主转来「会话状态变了 / 机器连上断开」· 结束之后。开着不轮询。
 * - 多选：Ctrl / Shift 点行；底下一条「已选 n · 取消选择 · 结束 n 个」，结束交宿主（确认与交哪条命令在宿主）。
 */
import { copyText } from "../copy-table";
import { dispatcher } from "../keybindings/registry";
import { el } from "../kit/el";
import { button } from "../kit/button";
import { emptyState } from "../kit/empty";
import { saidOf } from "../kit/detail";
import { skeletonRows } from "../kit/skeleton";
import { statusDot, setDot } from "../kit/status-dot";
import type { Origin } from "../ipc/origin";
import type { Watched } from "../terminal-page";
import type { BoardPick } from "../terminals-board-reads";
import type { TerminalsBoard } from "../generated/TerminalsBoard";
import type { BoardGroup } from "../generated/BoardGroup";
import type { BoardRow } from "../generated/BoardRow";
import type { BoardTag } from "../generated/BoardTag";
import s from "./terminals-view.module.css";

export type { BoardPick };

/** 视图要的读写与出口（缺省由 `main.ts` 接；测试换成假的）。 */
export interface TerminalsHost {
  /** 要问的那几台。 */
  machines(): Origin[];
  read(machines: readonly Origin[]): Promise<TerminalsBoard>;
  /** 终端面部件那个元素（挂进右边）。 */
  face(): HTMLElement;
  /** 终端面改看这一个（`null` ＝ 没选）。 */
  watch(w: Watched | null): void;
  /** 结束这几行（确认 · 交哪条命令都在宿主）。 */
  end(items: BoardPick[]): Promise<void>;
  reconnect(origin: Origin): void;
  goSession(origin: Origin, sid: string): void;
  openWindow(pick: BoardPick): void;
  newSession(origin: Origin | null): void;
}

/** 一行在页上的那几样（原地改字用）。 */
interface RowView {
  el: HTMLElement;
  dot: HTMLElement;
  title: HTMLElement;
  badge: HTMLElement;
  meta: HTMLElement;
  at: HTMLElement;
  pick: BoardPick;
}

/** 连着来的推送合成一问：等这么久。 */
const POKE_MS = 300;

/** 键 ⇒ 在哪台（多选结束要交）。 */
const keyOf = (origin: Origin, row: BoardRow): string => `${origin}\u0000${row.key}`;

export class TerminalsView {
  private readonly root: HTMLElement;
  private readonly listEl: HTMLElement;
  private readonly filtersEl: HTMLElement;
  private readonly faceEl: HTMLElement;
  private readonly actsEl: HTMLElement;
  private readonly selectBar: HTMLElement;
  private readonly selectCount: HTMLElement;
  private readonly endBtn: HTMLButtonElement;
  private readonly findBox: HTMLInputElement;
  private isOpen = false;
  private board: TerminalsBoard | null = null;
  /** 头一问就没问到 ⇒ 那一句（后端写好）；问到过 ⇒ 照上一份画。 */
  private failed: string | null = null;
  private filter: BoardTag = "all";
  /** 每台此刻摆的先后（行键）：重新打开 · 换筛选才照后端那份重排。 */
  private order = new Map<Origin, string[]>();
  private rows = new Map<string, RowView>();
  private selected = new Set<string>();
  private current: string | null = null;
  private anchor: string | null = null;
  private seq = 0;
  private faceView: HTMLElement | null = null;
  private pokeTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly pickEl = el("div", s.tvPick, copyText("terminalsView.face.pick"));

  constructor(private readonly host: TerminalsHost) {
    this.root = el("div", s.tvView);
    const bar = el("div", s.tvBar);
    bar.append(el("span", s.tvTitle, copyText("terminalsView.page.title")));
    this.filtersEl = el("div", s.tvFilters);
    this.findBox = el("input", s.tvFind);
    this.findBox.type = "search";
    this.findBox.dataset.find = "";
    this.findBox.placeholder = copyText("terminalsView.page.search");
    this.findBox.addEventListener("input", () => this.paint(false));
    const spacer = el("span", s.tvSpacer);
    bar.append(
      this.filtersEl,
      spacer,
      this.findBox,
      button({ label: copyText("terminalsView.page.new"), kind: "primary", icon: "plus", onClick: () => this.host.newSession(null) }),
      button({ label: copyText("terminalsView.page.back"), kind: "ghost", onClick: () => this.close() }),
    );
    const body = el("div", s.tvMain);
    const left = el("div", s.tvLeft);
    this.listEl = el("div", s.tvList);
    this.selectBar = el("div", s.tvSelectBar);
    this.selectBar.dataset.selectBar = "";
    this.selectCount = el("span", s.tvCount);
    this.endBtn = button({ label: copyText("terminalsView.select.end"), kind: "danger-text", onClick: () => void this.endSelected() });
    this.endBtn.dataset.end = "";
    const picked = el("span", s.tvSelectCount, copyText("terminalsView.select.count"));
    this.selectBar.append(
      picked,
      this.selectCount,
      el("span", s.tvSpacer),
      button({ label: copyText("terminalsView.select.clear"), kind: "ghost", onClick: () => this.clearSelection() }),
      this.endBtn,
    );
    left.append(this.listEl, this.selectBar);
    const right = el("div", s.tvRight);
    this.actsEl = el("div", s.tvActs);
    this.faceEl = el("div", s.tvFace);
    right.append(this.actsEl, this.faceEl);
    body.append(left, right);
    this.root.append(bar, body);
  }

  get element(): HTMLElement {
    return this.root;
  }

  isVisible(): boolean {
    return this.isOpen;
  }

  handleEsc(): void {
    this.close();
  }

  async open(): Promise<void> {
    if (this.isOpen) return;
    this.isOpen = true;
    this.board = null;
    this.failed = null;
    this.order.clear();
    this.rows.clear();
    document.body.appendChild(this.root);
    dispatcher.pushOverlay(this);
    this.paintFace();
    this.paint(true);
    await this.refresh();
  }

  close(): void {
    if (!this.isOpen) return;
    this.isOpen = false;
    this.root.remove();
    dispatcher.popOverlay(this);
    this.current = null;
    this.selected.clear();
    this.host.watch(null);
  }

  /** 宿主转来「会话状态变了 · 机器连上断开」：开着才问，连着来的合成一问。 */
  poke(): void {
    if (!this.isOpen || this.pokeTimer !== null) return;
    // 调度：合批 —— 连着来的会话状态变化 · 机器连上断开合成一问
    this.pokeTimer = setTimeout(() => {
      this.pokeTimer = null;
      void this.refresh();
    }, POKE_MS);
  }

  /** 再问一次（开着才问；叠着问只收最后一问）。 */
  async refresh(): Promise<void> {
    if (!this.isOpen) return;
    const mine = ++this.seq;
    try {
      const b = await this.host.read(this.host.machines());
      if (mine !== this.seq || !this.isOpen) return;
      this.board = b;
      this.failed = null;
    } catch (e) {
      if (mine !== this.seq || !this.isOpen) return;
      this.failed = this.board === null ? saidOf(e) : null;
    }
    this.paint(false);
  }

  /** 排在哪：`reorder` ⇒ 照后端那份；否则已在的原位、新来的接尾、没了的删掉。 */
  private arrange(g: BoardGroup, reorder: boolean): BoardRow[] {
    const by = new Map(g.rows.map((r) => [r.key, r]));
    const was = reorder ? [] : (this.order.get(g.machine) ?? []);
    const keys = was.filter((k) => by.has(k));
    for (const r of g.rows) if (!keys.includes(r.key)) keys.push(r.key);
    this.order.set(g.machine, keys);
    return keys.map((k) => by.get(k)!);
  }

  private visible(r: BoardRow): boolean {
    if (this.filter !== "all" && !r.tags.includes(this.filter)) return false;
    const q = this.findBox.value.trim().toLowerCase();
    return q === "" || r.find.includes(q);
  }

  private paint(reorder: boolean): void {
    this.paintFilters();
    if (this.failed !== null) {
      const e = emptyState({ text: this.failed, icon: "error" });
      e.dataset.failed = "";
      this.listEl.replaceChildren(e);
      this.rows.clear();
      return;
    }
    if (this.board === null) {
      this.listEl.replaceChildren(skeletonRows(6));
      return;
    }
    const all = this.board.groups.reduce((n, g) => n + g.rows.length, 0);
    if (all === 0 && this.board.groups.every((g) => !g.down)) {
      const e = emptyState({
        text: copyText("terminalsView.empty.title"),
        hint: copyText("terminalsView.empty.body"),
        icon: "terminal",
        action: button({ label: copyText("terminalsView.page.new"), kind: "secondary", icon: "plus", onClick: () => this.host.newSession(null) }),
      });
      e.dataset.boardEmpty = "";
      this.listEl.replaceChildren(e);
      this.rows.clear();
      return;
    }
    const seen = new Set<string>();
    const out: HTMLElement[] = [];
    let shown = 0;
    for (const g of this.board.groups) {
      const head = el("div", s.tvGroupHead);
      head.append(el("span", s.tvGroupName, g.name), el("span", s.tvGroupSub, g.sub), el("span", s.tvSpacer));
      head.append(button({ label: copyText("terminalsView.page.new"), kind: "ghost", size: "compact", icon: "plus", onClick: () => this.host.newSession(g.machine) }));
      out.push(head);
      if (g.down) {
        const d = el("div", s.tvDown);
        d.dataset.down = "";
        d.append(el("span", "", g.down.said), button({ label: copyText("terminalsView.group.reconnect"), kind: "secondary", size: "compact", onClick: () => this.host.reconnect(g.machine) }));
        out.push(d);
      }
      if (!g.complete) out.push(el("div", s.tvPartial, copyText("terminalsView.list.partial")));
      for (const r of this.arrange(g, reorder)) {
        const k = keyOf(g.machine, r);
        seen.add(k);
        if (!this.visible(r)) continue;
        shown++;
        out.push(this.rowOf(k, { origin: g.machine, row: r }, !!g.down));
      }
    }
    for (const k of [...this.rows.keys()]) if (!seen.has(k)) this.rows.delete(k);
    for (const k of [...this.selected]) if (!seen.has(k)) this.selected.delete(k);
    if (shown === 0 && all > 0) out.push(el("div", s.tvNoMatch, copyText("terminalsView.empty.noMatch")));
    this.listEl.replaceChildren(...out);
    this.paintSelection();
    this.paintActs();
  }

  private paintFilters(): void {
    const fs = this.board?.filters ?? [];
    this.filtersEl.replaceChildren(
      ...fs.map((f) => {
        const b = el("button", s.tvFilter);
        b.type = "button";
        b.dataset.filter = f.tag;
        b.setAttribute("aria-pressed", String(f.tag === this.filter));
        b.append(el("span", "", f.label), el("span", s.tvCount, String(f.count)));
        b.addEventListener("click", () => {
          if (this.filter === f.tag) return;
          this.filter = f.tag;
          this.paint(true);
        });
        return b;
      }),
    );
  }

  /** 那一行：已有 ⇒ 原地改字（同一个元素）；没有 ⇒ 建。 */
  private rowOf(k: string, pick: BoardPick, stale: boolean): HTMLElement {
    let v = this.rows.get(k);
    if (!v) {
      const e = el("div", s.tvRow);
      e.dataset.row = pick.row.key;
      e.tabIndex = 0;
      e.setAttribute("role", "option");
      const dot = statusDot(null, null, "compact");
      const title = el("span", `${s.tvRowTitle} one-line`);
      const badge = el("span", s.tvBadge);
      const at = el("span", s.tvAt);
      const top = el("div", s.tvRowTop);
      top.append(dot, title, badge, el("span", s.tvSpacer), at);
      const meta = el("div", s.tvMeta);
      e.append(top, meta);
      e.addEventListener("click", (ev) => this.onRowClick(k, ev));
      e.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter") this.onRowClick(k, ev);
      });
      v = { el: e, dot, title, badge, meta, at, pick };
      this.rows.set(k, v);
    }
    v.pick = pick;
    const r = pick.row;
    setDot(v.dot, r.dot, r.dot_name);
    v.title.textContent = r.title;
    v.title.toggleAttribute("data-mono", r.mono);
    v.badge.textContent = r.badge ?? "";
    v.at.textContent = r.at_text;
    v.meta.replaceChildren(el("span", "", r.meta), ...r.chips.map((c) => el("span", s.tvPill, c)));
    v.el.toggleAttribute("data-stale", stale);
    return v.el;
  }

  private onRowClick(k: string, ev: MouseEvent | KeyboardEvent): void {
    if (ev.ctrlKey || ev.metaKey) {
      if (this.selected.has(k)) this.selected.delete(k);
      else this.selected.add(k);
      this.anchor = k;
      this.paintSelection();
      return;
    }
    if (ev.shiftKey && this.anchor) {
      const flat = [...this.listEl.querySelectorAll<HTMLElement>("[data-row]")].map((e) => [...this.rows].find(([, v]) => v.el === e)?.[0]).filter((x): x is string => !!x);
      const [a, b] = [flat.indexOf(this.anchor), flat.indexOf(k)].sort((x, y) => x - y);
      if (a >= 0) for (const x of flat.slice(a, b + 1)) this.selected.add(x);
      this.paintSelection();
      return;
    }
    this.selected.clear();
    this.anchor = k;
    this.current = k;
    this.paintSelection();
    this.paintActs();
    this.paintFace();
    const p = this.rows.get(k)?.pick;
    if (p) this.host.watch({ kind: "slot", origin: p.origin, terminal: p.row.terminal, name: p.row.tmux_name, title: p.row.title });
  }

  private paintSelection(): void {
    for (const [k, v] of this.rows) {
      v.el.setAttribute("aria-selected", String(k === this.current));
      v.el.toggleAttribute("data-marked", this.selected.has(k));
    }
    const n = this.selected.size;
    this.selectBar.toggleAttribute("data-shown", n > 0);
    this.selectCount.textContent = String(n);
  }

  private clearSelection(): void {
    this.selected.clear();
    this.paintSelection();
  }

  private async endSelected(): Promise<void> {
    const items = [...this.listEl.querySelectorAll<HTMLElement>("[data-row]")]
      .map((e) => [...this.rows].find(([, v]) => v.el === e))
      .filter((x): x is [string, RowView] => !!x && this.selected.has(x[0]))
      .map(([, v]) => v.pick);
    if (items.length === 0) return;
    await this.host.end(items);
    this.selected.clear();
    await this.refresh();
  }

  /** 终端面上那一排：去它的会话 · 新窗口 · 结束（选中那一行的）。 */
  private paintActs(): void {
    const p = this.current ? this.rows.get(this.current)?.pick : undefined;
    if (!p) {
      this.actsEl.replaceChildren();
      return;
    }
    const acts: HTMLElement[] = [];
    const sid = p.row.sid;
    if (sid) acts.push(button({ label: copyText("terminalsView.face.goSession"), kind: "ghost", size: "compact", onClick: () => this.host.goSession(p.origin, sid) }));
    acts.push(button({ label: copyText("terminalsView.face.openWindow"), kind: "ghost", size: "compact", icon: "popOut", onClick: () => this.host.openWindow(p) }));
    if (p.row.can_end === true) {
      acts.push(
        button({
          label: copyText("terminalsView.face.end"),
          kind: "ghost",
          size: "compact",
          icon: "stop",
          onClick: () => void this.host.end([p]).then(() => this.refresh()),
        }),
      );
    }
    this.actsEl.replaceChildren(...acts);
  }

  /** 右边：没选 ⇒「选择终端查看」；选了 ⇒ 终端面。 */
  private paintFace(): void {
    if (!this.faceView) this.faceView = this.host.face();
    const want = this.current === null ? this.pickEl : this.faceView;
    if (want.parentElement !== this.faceEl) this.faceEl.replaceChildren(want);
  }
}
