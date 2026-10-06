/**
 * **历史页**（设计稿「文件与历史」乙4）：主窗口上的全屏视图，列表在左、内容在右。
 *
 * - 列表：默认「按时间」平铺（今天 · 昨天 · 本周 · 按月分段），「按项目」是第二种看法；数据是每台一份平铺清单
 *   （后端 `history-list`：判活 · 标题 · 每行能做什么 · 分组都在那里定），哪台先答先画、一台没答不挡别的台。
 * - 一个搜索框：敲字 ⇒ 后端按标题 / 第一句 / 项目名过滤全部会话；回车 ⇒ 内容搜索（各台 `history-search`），结果按会话分块。
 * - 「筛选」浮层：机器 · 时间 · 排序 · 显示已隐藏 · 搜内容时谁说的 · 含工具输出与思考。选项记在本机（界面偏好）。
 * - 右边：点一行就地看（只读查看器），列表不动。
 * - 实时：鼠标在列表上或焦点在列表里时不重排，移开再排（`I6`）；选中的那一行按会话认，不因上面插行而移动。
 */
import { button } from "../kit/button";
import { icon } from "../kit/icon";
import { countBadge } from "../kit/badge";
import { tabs } from "../kit/tabs";
import { checkbox, radio } from "../kit/switch";
import { emptyState } from "../kit/empty";
import { skeletonRows } from "../kit/skeleton";
import { spinner } from "../kit/progress";
import { openPopover, closePopover } from "../kit/popover";
import { openMenu, type MenuItem } from "../kit/menu";
import { toast, undoToast } from "../kit/toast";
import { askText, confirmDialog } from "../kit/dialog";
import { dispatcher } from "../keybindings/registry";
import { copyText } from "../copy-table";
import { commands } from "../ipc/commands";
import { LOCAL_ORIGIN } from "../ipc/origin";
import { LS_KEYS, safeGetJson, safeSetJson } from "../local-storage";
import { annotate, forgetAnnotation, historyReasonOf } from "../history-reads";
import { fetchList, mergeByAt, mergeGroups, type HistoryGroup, type HistoryList, type HistoryRow } from "../history-list-reads";
import { searchAllMachines, type SearchResult, type SessionHits } from "./history-search";
import { SessionViewer } from "./session-viewer";
import { deleteSession } from "../session-writes";
import { resumeLocalSession } from "../local-resume";
import { runRemoteResume, runNewSessionRemote } from "../remote-launch-run";
import { resolveResumeCommand } from "../remote-config";
import { configuredLauncherFor } from "../launch-requests";
import { getBehavior } from "../behavior";
import { FOLLOW } from "../launch-account";
import { launchLocal } from "../launch-render";
import { arrivedBody, expectArrival } from "../launch-arrival";
import { revealInFolder } from "../reveal-in-folder";
import { groupHead, hitsBlock, labelOf, rowKey, sectionHead, sessionRow, strip, type RowHooks } from "./history-rows";
import { sectionKey, sectionLabel } from "./history-time";
import s from "./history.module.css";

type ViewMode = "time" | "project";
type Scope = "all" | "user" | "assistant" | "report";

/** 记在本机的界面偏好（不是数据）。 */
interface Prefs {
  view: ViewMode;
  /** 筛掉的机器（`""` = 本机）。 */
  off: string[];
  withinDays: 0 | 7 | 30;
  sort: "activity" | "created";
  hidden: boolean;
  scope: Scope;
  tools: boolean;
}

const DEFAULT_PREFS: Prefs = { view: "time", off: [], withinDays: 0, sort: "activity", hidden: false, scope: "all", tools: false };

function loadPrefs(): Prefs {
  const v = safeGetJson<Partial<Prefs>>(LS_KEYS.historyPrefs) ?? {};
  return { ...DEFAULT_PREFS, ...v, off: Array.isArray(v.off) ? v.off.filter((x) => typeof x === "string") : [] };
}

/** 一台此刻的样子。 */
type Machine =
  | { state: "loading"; prev: HistoryList | null }
  | { state: "ok"; list: HistoryList }
  | { state: "failed"; why: string };

/** 一台的键：本机 `""`、远端那台的名字。 */
const keyOf = (origin: string | undefined): string => origin ?? "";

/** 敲字之后停多久才问（乙5：150 ms）。 */
const QUERY_DEBOUNCE_MS = 150;
/** 方向键走行时停多久才读右边（快速划过不读）。 */
const PREVIEW_DEBOUNCE_MS = 200;

export class HistoryView {
  /** 删会话前问一句：主窗口里它此刻活着吗（列表拉下来那一刻的状态可能已旧）。 */
  liveInTabs: (sid: string) => boolean = () => false;
  /** 主窗口里这个会话此刻「需要你」的那个词；不在 / 不需要 ⇒ `null`。 */
  needsOf: (sid: string) => string | null = () => null;
  /** 切到主窗口里的这个会话。 */
  switchTo: (sid: string) => void = () => {};

  private readonly root: HTMLElement;
  private isOpen = false;
  private prefs: Prefs = loadPrefs();
  private machines: string[] = [""];
  private per = new Map<string, Machine>();
  private seq = 0;
  private query = "";
  private queryTimer: ReturnType<typeof setTimeout> | null = null;
  /** 内容搜索：`null` = 没在搜内容（列表是清单）。 */
  private content: { q: string; state: "searching" } | { q: string; state: "done"; r: SearchResult } | { q: string; state: "failed"; why: string } | null = null;
  private contentSeq = 0;
  private selected: string | null = null;
  private shown: string | null = null;
  private previewTimer: ReturnType<typeof setTimeout> | null = null;
  private openForks = new Set<string>(safeGetJson<string[]>(LS_KEYS.historyExpandedForks) ?? []);
  private openGroups = new Set<string>();
  /** 鼠标点一行时焦点进列表那一下：不让列表自己的「焦点落到选中行」接手。 */
  private quietFocus = false;
  /** 列表上有鼠标 / 焦点 ⇒ 新数据先不重排。 */
  private holding = false;
  private dirty = false;
  private rowsByKey = new Map<string, HistoryRow>();
  private order: string[] = [];

  private searchInput!: HTMLInputElement;
  private filterBtn!: HTMLButtonElement;
  private refreshBtn!: HTMLButtonElement;
  private listEl!: HTMLElement;
  private listHead!: HTMLElement;
  private stripsEl!: HTMLElement;
  private contentEl!: HTMLElement;
  private viewer: SessionViewer | null = null;
  private readonly layer = { handleEsc: () => this.handleEsc() };

  constructor() {
    this.root = this.build();
  }

  isVisible(): boolean {
    return this.isOpen;
  }

  async open(): Promise<void> {
    if (this.isOpen) return;
    document.body.appendChild(this.root);
    this.isOpen = true;
    this.query = "";
    this.searchInput.value = "";
    this.content = null;
    dispatcher.pushOverlay(this.layer);
    // 开页即给焦点（不等远端，R5W-H08）。
    this.searchInput.focus();
    this.machines = ["", ...(await commands.list_remote_mcp_origins().catch(() => [] as string[]))];
    // 开页那一问带 `fresh`：远端那台的清单后端记着、不按时间过期，开页与「刷新」才再问那台。
    this.refresh(true);
  }

  close(): void {
    if (!this.isOpen) return;
    this.seq++;
    this.contentSeq++;
    closePopover();
    this.disposeViewer();
    this.root.remove();
    this.isOpen = false;
    dispatcher.popOverlay(this.layer);
  }

  /** Ctrl+F 落在历史页：右边有会话 ⇒ 会话内查找；否则不接（交给主窗口）。 */
  openFind(): boolean {
    if (!this.isOpen || !this.viewer) return false;
    this.viewer.openFind();
    return true;
  }

  /** Esc 一次一层（乙5）：焦点在内容里 ⇒ 回列表 ＞ 搜索框有字 ⇒ 清空 ＞ 关历史页。菜单 / 浮层 / 对话框各自在栈上先接。 */
  private handleEsc(): boolean {
    if (this.contentEl.contains(document.activeElement)) {
      this.focusSelected();
      return true;
    }
    if (this.searchInput.value) {
      this.setQuery("");
      this.searchInput.focus();
      return true;
    }
    this.close();
    return true;
  }

  // ───────────────────────── 数据 ─────────────────────────

  private savePrefs(): void {
    safeSetJson(LS_KEYS.historyPrefs, this.prefs);
  }

  private wanted(): string[] {
    return this.machines.filter((m) => !this.prefs.off.includes(m));
  }

  /** 逐台问清单（`fresh` ＝ 远端不用后端记着的那份）。哪台先答先画。 */
  private refresh(fresh: boolean, only?: string): void {
    const seq = ++this.seq;
    const ask = {
      query: this.query || undefined,
      sort: this.prefs.sort,
      withinDays: this.prefs.withinDays || undefined,
      hidden: this.prefs.hidden,
      fresh,
    };
    const targets = only !== undefined ? [only] : this.wanted();
    for (const m of targets) {
      const was = this.per.get(m);
      this.per.set(m, { state: "loading", prev: was?.state === "ok" ? was.list : was?.state === "loading" ? was.prev : null });
      fetchList(m || undefined, ask).then(
        (list) => {
          if (seq !== this.seq && only === undefined) return;
          this.per.set(m, { state: "ok", list });
          this.scheduleRender();
        },
        (e: unknown) => {
          if (seq !== this.seq && only === undefined) return;
          this.per.set(m, { state: "failed", why: historyReasonOf(e) });
          this.scheduleRender();
        },
      );
    }
    this.refreshBtn.dataset.busy = "true";
    this.scheduleRender();
  }

  private setQuery(q: string): void {
    this.searchInput.value = q;
    this.onQueryInput();
  }

  private onQueryInput(): void {
    this.content = null;
    this.contentSeq++;
    if (this.queryTimer) clearTimeout(this.queryTimer);
    this.queryTimer = setTimeout(() => {
      this.queryTimer = null;
      const q = this.searchInput.value.trim();
      if (q === this.query) return this.renderNow();
      this.query = q;
      this.refresh(false);
    }, QUERY_DEBOUNCE_MS);
    this.renderNow();
  }

  /** 回车 ⇒ 在全部会话内容里搜。失败 ⇒ 清掉旧结果、换成错误条（R5W-H06）。 */
  private async runContentSearch(): Promise<void> {
    const q = this.searchInput.value.trim();
    if (!q) return;
    const seq = ++this.contentSeq;
    this.content = { q, state: "searching" };
    this.renderNow();
    try {
      const r = await searchAllMachines({
        query: q,
        includeTools: this.prefs.tools,
        scope: this.prefs.scope === "all" ? null : this.prefs.scope,
        afterMs: this.prefs.withinDays ? Date.now() - this.prefs.withinDays * 86_400_000 : null,
        limit: null,
      });
      if (seq !== this.contentSeq) return;
      this.content = { q, state: "done", r };
    } catch (e) {
      if (seq !== this.contentSeq) return;
      this.content = { q, state: "failed", why: historyReasonOf(e) };
    }
    this.renderNow();
  }

  // ───────────────────────── 外壳 ─────────────────────────

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = `${s.hv} history-view`;
    const top = document.createElement("div");
    top.className = s.hvTop;
    const back = button({ label: copyText("history.page.back"), kind: "ghost", icon: "back", onClick: () => this.close() });
    const views = tabs<ViewMode>({
      items: [
        { key: "time", label: copyText("history.page.byTime") },
        { key: "project", label: copyText("history.page.byProject") },
      ],
      current: this.prefs.view,
      label: copyText("history.page.views"),
      onChange: (k) => {
        this.prefs.view = k;
        this.savePrefs();
        this.renderNow();
      },
    });
    views.classList.add(s.hvViews);
    const search = document.createElement("label");
    search.className = s.hvSearch;
    search.appendChild(icon("search", "compact"));
    this.searchInput = document.createElement("input");
    this.searchInput.type = "search";
    this.searchInput.className = "history-search";
    this.searchInput.placeholder = copyText("history.page.placeholder");
    this.searchInput.setAttribute("aria-label", copyText("history.page.placeholder"));
    this.searchInput.addEventListener("input", () => this.onQueryInput());
    this.searchInput.addEventListener("keydown", (ev) => {
      if (ev.isComposing) return;
      if (ev.key === "Enter") {
        ev.preventDefault();
        void this.runContentSearch();
      } else if (ev.key === "ArrowDown") {
        ev.preventDefault();
        this.moveSelection(this.order[0] ?? null, true);
      }
    });
    search.appendChild(this.searchInput);
    this.filterBtn = button({ label: copyText("history.page.filter"), icon: "filter", onClick: () => this.openFilter() });
    this.filterBtn.setAttribute("aria-haspopup", "dialog");
    this.refreshBtn = button({
      label: copyText("history.page.refresh"),
      kind: "icon",
      icon: "refresh",
      hint: copyText("history.page.refresh"),
      onClick: () => this.refresh(true),
    });
    top.append(back, views, search, this.filterBtn, this.refreshBtn);
    root.appendChild(top);

    const split = document.createElement("div");
    split.className = s.hvSplit;
    const col = document.createElement("div");
    col.className = s.hvCol;
    this.stripsEl = document.createElement("div");
    this.listHead = document.createElement("div");
    this.listHead.className = s.hvHead;
    this.listEl = document.createElement("div");
    this.listEl.className = s.hvList;
    this.listEl.setAttribute("role", "listbox");
    this.listEl.setAttribute("aria-label", copyText("history.page.list"));
    this.listEl.tabIndex = 0;
    this.listEl.addEventListener("keydown", (ev) => this.onListKey(ev));
    // Tab 进列表 ⇒ 焦点落到选中的那一行（没有就第一行）；鼠标点进来的不在这里管（点的那一行自己选，见 `hooks().select`）。
    this.listEl.addEventListener("focus", (ev) => {
      if (ev.target === this.listEl && !this.quietFocus) this.moveSelection(this.selected ?? this.order[0] ?? null, true);
    });
    // 实时更新不拽人：鼠标在列表上 / 焦点在列表里 ⇒ 先不重排，移开再排（`I6`）。
    const hold = (on: boolean): void => {
      this.holding = on || this.listEl.matches(":hover") || this.listEl.contains(document.activeElement);
      if (!this.holding && this.dirty) this.renderNow();
    };
    this.listEl.addEventListener("pointerenter", () => hold(true));
    this.listEl.addEventListener("pointerleave", () => hold(false));
    this.listEl.addEventListener("focusin", () => hold(true));
    this.listEl.addEventListener("focusout", () => setTimeout(() => hold(false), 0));
    col.append(this.stripsEl, this.listHead, this.listEl);
    this.contentEl = document.createElement("div");
    this.contentEl.className = s.hvContent;
    split.append(col, this.contentEl);
    root.appendChild(split);
    this.showPlaceholder();
    return root;
  }

  private showPlaceholder(): void {
    this.contentEl.dataset.vacant = "true";
    this.contentEl.replaceChildren(emptyState({ icon: "chat", text: copyText("history.content.none") }));
  }

  // ───────────────────────── 筛选 ─────────────────────────

  private activeFilters(): number {
    const p = this.prefs;
    return (p.off.length > 0 ? 1 : 0) + (p.withinDays ? 1 : 0) + (p.sort !== "activity" ? 1 : 0) + (p.hidden ? 1 : 0) + (p.scope !== "all" ? 1 : 0) + (p.tools ? 1 : 0);
  }

  private markFilterBtn(): void {
    this.filterBtn.querySelector(`.${s.hvCount}`)?.remove();
    const b = countBadge(this.activeFilters());
    if (b) {
      b.classList.add(s.hvCount);
      this.filterBtn.appendChild(b);
    }
  }

  private openFilter(): void {
    const panel = document.createElement("div");
    panel.className = s.hvFilter;
    const group = (title: string, ...items: HTMLElement[]): HTMLElement => {
      const g = document.createElement("div");
      g.className = s.hvFilterGroup;
      const h = document.createElement("div");
      h.className = s.hvFilterHead;
      h.textContent = title;
      g.append(h, ...items);
      return g;
    };
    const changed = (refetch: boolean): void => {
      this.savePrefs();
      this.markFilterBtn();
      if (refetch) this.refresh(false);
      else this.renderNow();
    };
    const machines = this.machines.map((m) => {
      const c = checkbox(m ? m : copyText("history.filter.local"), !this.prefs.off.includes(m), (on) => {
        this.prefs.off = on ? this.prefs.off.filter((x) => x !== m) : [...this.prefs.off, m];
        if (on) this.refresh(false, m);
        changed(false);
      });
      if (this.per.get(m)?.state === "failed") {
        const w = document.createElement("span");
        w.className = s.hvFilterWarn;
        w.textContent = copyText("history.filter.offline");
        c.appendChild(w);
      }
      return c;
    });
    const radios = <T extends string | number>(name: string, cur: T, opts: [T, string][], set: (v: T) => void): HTMLElement[] =>
      opts.map(([v, label]) => radio(name, label, v === cur, () => set(v)));
    panel.append(
      group(copyText("history.filter.machines"), ...machines),
      group(
        copyText("history.filter.time"),
        ...radios<0 | 7 | 30>("hv-time", this.prefs.withinDays, [
          [0, copyText("history.filter.timeAll")],
          [7, copyText("history.filter.time7d")],
          [30, copyText("history.filter.time30d")],
        ], (v) => {
          this.prefs.withinDays = v;
          changed(true);
        }),
      ),
      group(
        copyText("history.filter.sort"),
        ...radios<Prefs["sort"]>("hv-sort", this.prefs.sort, [
          ["activity", copyText("history.filter.sortActivity")],
          ["created", copyText("history.filter.sortCreated")],
        ], (v) => {
          this.prefs.sort = v;
          changed(true);
        }),
      ),
      group(
        "",
        checkbox(copyText("history.filter.showHidden"), this.prefs.hidden, (on) => {
          this.prefs = { ...this.prefs, hidden: on };
          changed(true);
        }),
      ),
      group(
        copyText("history.filter.scope"),
        ...radios<Scope>("hv-scope", this.prefs.scope, [
          ["all", copyText("history.filter.scopeAll")],
          ["user", copyText("history.filter.scopeYou")],
          ["assistant", copyText("history.filter.scopeAgent")],
          ["report", copyText("history.filter.scopeReport")],
        ], (v) => {
          this.prefs.scope = v;
          changed(false);
          if (this.content) void this.runContentSearch();
        }),
        checkbox(copyText("history.filter.tools"), this.prefs.tools, (on) => {
          this.prefs.tools = on;
          changed(false);
          if (this.content) void this.runContentSearch();
        }),
      ),
    );
    openPopover(this.filterBtn, panel, { label: copyText("history.page.filter") });
  }

  private clearFilters(): void {
    this.prefs = { ...DEFAULT_PREFS, view: this.prefs.view };
    this.savePrefs();
    this.markFilterBtn();
    this.setQuery("");
    this.refresh(false);
  }

  // ───────────────────────── 画 ─────────────────────────

  private scheduleRender(): void {
    if (this.holding) {
      this.dirty = true;
      this.renderChrome();
      return;
    }
    this.renderNow();
  }

  /** 不动列表的那几块（刷新转圈 · 提示条 · 筛选数）。 */
  private renderChrome(): void {
    const loading = this.wanted().some((m) => this.per.get(m)?.state === "loading");
    this.refreshBtn.dataset.busy = String(loading);
    this.markFilterBtn();
    const strips: HTMLElement[] = [];
    for (const m of this.wanted()) {
      const st = this.per.get(m);
      if (st?.state === "failed") {
        strips.push(
          m
            ? strip("warn", copyText("history.list.machineDown", { machine: m }), { label: copyText("history.list.reconnect"), run: () => this.refresh(true, m) })
            : strip("error", copyText("history.list.localFailed"), { label: copyText("history.list.retry"), run: () => this.refresh(true, "") }),
        );
      } else if (st?.state === "ok" && st.list.notice) {
        strips.push(strip("warn", copyText("history.list.notice"), { label: copyText("history.list.retry"), run: () => this.refresh(true, m) }));
      }
    }
    const c = this.content;
    if (c?.state === "done") {
      for (const h of c.r.failedHosts) strips.push(strip("warn", copyText("history.search.machineDown", { machine: h }), { label: copyText("history.list.reconnect"), run: () => void this.runContentSearch() }));
      for (const a of c.r.skipped) strips.push(strip("info", copyText("history.search.skipped", { agent: a })));
      if (c.r.unreadable > 0) strips.push(strip("info", copyText("history.search.unreadable", { n: c.r.unreadable })));
    } else if (c?.state === "failed") {
      strips.push(strip("error", copyText("history.search.failed", { why: c.why }), { label: copyText("history.list.retry"), run: () => void this.runContentSearch() }));
    }
    this.stripsEl.replaceChildren(...strips);
  }

  private lists(): HistoryList[] {
    const out: HistoryList[] = [];
    for (const m of this.wanted()) {
      const st = this.per.get(m);
      if (st?.state === "ok") out.push(st.list);
      else if (st?.state === "loading" && st.prev) out.push(st.prev);
    }
    return out;
  }

  private renderNow(): void {
    this.dirty = false;
    this.renderChrome();
    const now = Date.now();
    const lists = this.lists();
    const rows = mergeByAt(lists.map((l) => l.rows));
    this.rowsByKey = new Map(rows.map((r) => [rowKey(r), r]));
    const anyLoading = this.wanted().some((m) => this.per.get(m)?.state === "loading");
    const body: HTMLElement[] = [];
    this.order = [];
    const hooks = this.hooks();
    if (this.content) {
      this.renderContent(body, hooks, now);
    } else if (rows.length === 0 && anyLoading) {
      this.listHead.replaceChildren(copyText("history.list.reading", { machines: this.wanted().map((m) => m || copyText("history.filter.local")).join(` ${copyText("cssMarks.sep.dot")} `) }));
      body.push(skeletonRows(6));
    } else if (rows.length === 0) {
      this.listHead.replaceChildren();
      const filtered = this.query !== "" || this.activeFilters() > 0;
      body.push(
        filtered
          ? emptyState({ icon: "search", text: copyText("history.list.noMatch"), action: button({ label: copyText("history.list.clearFilter"), onClick: () => this.clearFilters() }) })
          : emptyState({ text: copyText("history.list.empty"), hint: copyText("history.list.emptyHint") }),
      );
    } else if (this.prefs.view === "time") {
      this.renderByTime(body, rows, hooks, now, lists);
    } else {
      this.renderByProject(body, rows, hooks, now, lists);
    }
    if (lists.some((l) => l.truncated) && !this.content) {
      const more = document.createElement("div");
      more.className = s.hvFoot;
      more.textContent = copyText("history.list.truncated");
      body.push(more);
    }
    const top = this.listEl.scrollTop;
    this.listEl.replaceChildren(...body);
    this.listEl.scrollTop = top;
    this.markRows();
  }

  /** 每个父会话下挂着的分叉（同一台；只认清单里的父会话）。 */
  private forksOf(rows: HistoryRow[]): Map<string, HistoryRow[]> {
    const have = new Set(rows.map(rowKey));
    const kids = new Map<string, HistoryRow[]>();
    for (const r of rows) {
      if (!r.forkedFromSessionId) continue;
      const p = rowKey({ origin: r.origin, sessionId: r.forkedFromSessionId });
      if (!have.has(p)) continue;
      const a = kids.get(p) ?? [];
      a.push(r);
      kids.set(p, a);
    }
    return kids;
  }

  private pushRow(body: HTMLElement[], r: HistoryRow, hooks: RowHooks, kids: Map<string, HistoryRow[]>, compact: boolean, now: number, child: boolean): void {
    const k = rowKey(r);
    const mine = kids.get(k) ?? [];
    const open = this.openForks.has(k);
    const orphan = !child && r.forkedFromSessionId !== undefined && !this.rowsByKey.has(rowKey({ origin: r.origin, sessionId: r.forkedFromSessionId }));
    body.push(sessionRow(r, hooks, { compact, forks: mine.length, forksOpen: open, child, orphan, now }));
    this.order.push(k);
    if (open) for (const c of mine) this.pushRow(body, c, hooks, kids, compact, now, true);
  }

  private renderByTime(body: HTMLElement[], rows: HistoryRow[], hooks: RowHooks, now: number, lists: HistoryList[]): void {
    const total = lists.reduce((n, l) => n + l.total, 0);
    this.listHead.replaceChildren(copyText("history.list.count", { n: total }));
    const kids = this.forksOf(rows);
    let sec = "";
    for (const r of rows) {
      // 挂在父会话下的分叉不在顶层出。
      if (r.forkedFromSessionId && this.rowsByKey.has(rowKey({ origin: r.origin, sessionId: r.forkedFromSessionId }))) continue;
      const k = sectionKey(r.at, now);
      if (k !== sec) {
        sec = k;
        body.push(sectionHead(sectionLabel(k, now)));
      }
      this.pushRow(body, r, hooks, kids, false, now, false);
    }
  }

  private renderByProject(body: HTMLElement[], rows: HistoryRow[], hooks: RowHooks, now: number, lists: HistoryList[]): void {
    const groups = mergeGroups(lists.map((l) => l.groups));
    const sessions = groups.reduce((n, g) => n + g.count, 0);
    const head = document.createElement("span");
    head.textContent = copyText("history.list.countProjects", { p: groups.length, n: sessions });
    const end = document.createElement("span");
    end.className = s.hvHeadEnd;
    const all = (open: boolean): void => {
      this.openGroups = open ? new Set(groups.map(groupKey)) : new Set();
      this.renderNow();
    };
    end.append(
      button({ label: copyText("history.list.expandAll"), kind: "ghost", size: "compact", onClick: () => all(true) }),
      button({ label: copyText("history.list.collapseAll"), kind: "ghost", size: "compact", onClick: () => all(false) }),
    );
    this.listHead.replaceChildren(head, end);
    const byGroup = new Map<string, HistoryRow[]>();
    for (const r of rows) {
      const k = `${r.origin ?? ""}\u0000${r.group}`;
      const a = byGroup.get(k) ?? [];
      a.push(r);
      byGroup.set(k, a);
    }
    const kids = this.forksOf(rows);
    for (const g of groups) {
      const gk = groupKey(g);
      // 搜着字的时候组都展开（命中的就在眼前）。
      const open = this.query !== "" || this.openGroups.has(gk);
      body.push(
        groupHead(g, {
          open,
          onToggle: () => {
            if (this.openGroups.has(gk)) this.openGroups.delete(gk);
            else this.openGroups.add(gk);
            this.renderNow();
          },
          onNew: () => void this.newSessionIn(g.origin, g.projectPath, g.agent),
          onRetry: () => this.refresh(true, keyOf(g.origin)),
        }),
      );
      if (!open) continue;
      for (const r of byGroup.get(gk) ?? []) {
        if (r.forkedFromSessionId && this.rowsByKey.has(rowKey({ origin: r.origin, sessionId: r.forkedFromSessionId }))) continue;
        this.pushRow(body, r, hooks, kids, true, now, false);
      }
    }
  }

  /** 内容搜索的结果：每个会话一块（会话行 ＋ 前几处命中）。 */
  private renderContent(body: HTMLElement[], hooks: RowHooks, now: number): void {
    const c = this.content;
    if (!c) return;
    if (c.state === "searching") {
      const stop = button({ label: copyText("history.search.stop"), kind: "ghost", size: "compact", onClick: () => {
        this.contentSeq++;
        this.content = null;
        this.renderNow();
      } });
      this.listHead.replaceChildren(spinner(), copyText("history.search.searching"), stop);
      body.push(skeletonRows(4));
      return;
    }
    if (c.state === "failed") {
      this.listHead.replaceChildren();
      return;
    }
    const r = c.r;
    this.listHead.replaceChildren(
      r.truncated
        ? copyText("history.search.summaryTruncated", { q: c.q, n: r.totalHits, m: r.sessionCount })
        : copyText("history.search.summary", { q: c.q, n: r.totalHits, m: r.sessionCount }),
    );
    if (r.sessions.length === 0) {
      body.push(
        emptyState({
          icon: "search",
          text: copyText("history.search.noMatch", { q: c.q }),
          action: this.prefs.tools
            ? undefined
            : button({
                label: copyText("history.filter.tools"),
                onClick: () => {
                  this.prefs.tools = true;
                  this.savePrefs();
                  void this.runContentSearch();
                },
              }),
        }),
      );
      return;
    }
    for (const sh of r.sessions) {
      const row = this.rowsByKey.get(rowKey(sh)) ?? rowFromHits(sh);
      const el = sessionRow(row, hooks, { compact: false, forks: 0, forksOpen: false, child: false, orphan: false, now });
      this.order.push(rowKey(row));
      body.push(hitsBlock(el, sh, (uuid) => this.show(row, uuid ?? undefined)));
    }
  }

  /** 选中 / 正在显示的那一行（`aria-selected` 淡底 · `aria-current` 左边一道）。 */
  private markRows(): void {
    for (const el of this.listEl.querySelectorAll<HTMLElement>(`.${s.hvRow}`)) {
      const k = el.dataset.key ?? "";
      el.setAttribute("aria-selected", String(k === this.selected));
      if (k === this.shown) el.setAttribute("aria-current", "true");
      else el.removeAttribute("aria-current");
    }
  }

  private rowEl(k: string): HTMLElement | null {
    for (const el of this.listEl.querySelectorAll<HTMLElement>(`.${s.hvRow}`)) if (el.dataset.key === k) return el;
    return null;
  }

  private focusSelected(): void {
    const el = this.selected ? this.rowEl(this.selected) : null;
    (el ?? this.listEl).focus();
  }

  // ───────────────────────── 键盘 · 选中 ─────────────────────────

  private moveSelection(k: string | null, focus: boolean): void {
    if (k === null) return;
    this.selected = k;
    this.markRows();
    const el = this.rowEl(k);
    if (focus && el) {
      el.focus();
      el.scrollIntoView({ block: "nearest" });
    }
    if (this.previewTimer) clearTimeout(this.previewTimer);
    this.previewTimer = setTimeout(() => {
      this.previewTimer = null;
      const r = this.rowsByKey.get(k);
      if (r && this.shown !== k) this.show(r);
    }, PREVIEW_DEBOUNCE_MS);
  }

  private onListKey(ev: KeyboardEvent): void {
    if (ev.isComposing) return;
    const i = this.selected ? this.order.indexOf(this.selected) : -1;
    const at = (j: number): string | null => this.order[Math.max(0, Math.min(this.order.length - 1, j))] ?? null;
    const page = Math.max(1, Math.floor(this.listEl.clientHeight / 52));
    const r = this.selected ? this.rowsByKey.get(this.selected) : undefined;
    switch (ev.key) {
      case "ArrowDown":
        return this.consume(ev, () => this.moveSelection(at(i + 1), true));
      case "ArrowUp":
        if (i <= 0) return this.consume(ev, () => this.searchInput.focus());
        return this.consume(ev, () => this.moveSelection(at(i - 1), true));
      case "Home":
        return this.consume(ev, () => this.moveSelection(at(0), true));
      case "End":
        return this.consume(ev, () => this.moveSelection(at(this.order.length - 1), true));
      case "PageDown":
        return this.consume(ev, () => this.moveSelection(at(i + page), true));
      case "PageUp":
        return this.consume(ev, () => this.moveSelection(at(i - page), true));
      case "Enter":
        if (!r) return;
        if (ev.ctrlKey || ev.metaKey) return this.consume(ev, () => void this.resume(r));
        if (ev.shiftKey) return this.consume(ev, () => this.openWindow(r));
        return this.consume(ev, () => {
          this.show(r);
          this.viewer?.element.querySelector<HTMLElement>(".session-viewer-stream")?.focus();
        });
      case "F2":
        if (r) this.consume(ev, () => void this.rename(r));
        return;
      case "Delete":
        if (r) this.consume(ev, () => void this.remove(r));
        return;
      case "ContextMenu":
        if (r) this.consume(ev, () => this.menu(r, this.rowEl(rowKey(r)) ?? this.listEl));
        return;
      case "F10":
        if (r && ev.shiftKey) this.consume(ev, () => this.menu(r, this.rowEl(rowKey(r)) ?? this.listEl));
        return;
    }
  }

  private consume(ev: KeyboardEvent, f: () => void): void {
    ev.preventDefault();
    ev.stopPropagation();
    f();
  }

  // ───────────────────────── 右边 ─────────────────────────

  private disposeViewer(): void {
    this.viewer?.dispose();
    this.viewer = null;
    this.shown = null;
  }

  /** 右边就地看这一行（列表不动）。`jumpTo` = 内容命中那一句的 uuid。 */
  private show(r: HistoryRow, jumpTo?: string): void {
    const k = rowKey(r);
    this.selected = k;
    this.shown = k;
    this.markRows();
    if (!this.viewer) this.viewer = new SessionViewer();
    this.contentEl.dataset.vacant = "false";
    this.contentEl.replaceChildren(this.viewer.element);
    void this.viewer.load({
      jsonlPath: r.jsonlPath,
      displayTitle: labelOf(r),
      subtitle: `${r.projectName} · ${r.projectPath}`,
      origin: r.origin ?? LOCAL_ORIGIN,
      cwd: r.projectPath,
      scrollToUuid: jumpTo,
      suppressBranch: !r.can.fork,
    });
  }

  // ───────────────────────── 动作 ─────────────────────────

  private hooks(): RowHooks {
    return {
      // 鼠标点的：焦点进列表（方向键接着走），不画焦点环。
      select: (r) => {
        if (!this.listEl.contains(document.activeElement)) {
          this.quietFocus = true;
          this.listEl.focus({ preventScroll: true });
          this.quietFocus = false;
        }
        this.moveSelection(rowKey(r), false);
      },
      resume: (r) => void this.resume(r),
      menu: (r, at) => this.menu(r, at),
      openWindow: (r) => this.openWindow(r),
      toggleForks: (r) => {
        const k = rowKey(r);
        if (this.openForks.has(k)) this.openForks.delete(k);
        else this.openForks.add(k);
        safeSetJson(LS_KEYS.historyExpandedForks, [...this.openForks]);
        this.renderNow();
      },
      needs: (r) => (r.origin ? null : this.needsOf(r.sessionId)),
    };
  }

  /** 恢复（默认那一种：上次的号 · 设置里的方式）；在跑的 ⇒ 切过去。 */
  private async resume(r: HistoryRow): Promise<void> {
    if (r.can.resume === "bg") return;
    if (r.can.resume === "switch") {
      this.switchTo(r.sessionId);
      this.close();
      return;
    }
    try {
      if (r.origin) {
        const behavior = await getBehavior();
        const launcher = configuredLauncherFor(r.agent, await resolveResumeCommand(r.origin, behavior.resumeCommandRemote));
        await runRemoteResume(r.origin, r.agent, r.sessionId, r.projectPath, launcher, { account: FOLLOW });
      } else {
        await resumeLocalSession({ agent: r.agent, sid: r.sessionId, cwd: r.projectPath, account: FOLLOW });
      }
      this.close();
    } catch (e) {
      toast(copyText("history.resume.failed", { machine: r.origin ?? copyText("history.filter.local"), why: String(e) }), "");
    }
  }

  private openWindow(r: HistoryRow): void {
    void commands
      .open_session_in_new_window({ sessionId: r.sessionId, origin: r.origin ?? LOCAL_ORIGIN, title: labelOf(r) })
      .catch((e: unknown) => toast(copyText("tabSessionActions.openInWindow.failed"), String(e)));
  }

  private menu(r: HistoryRow, at: HTMLElement | { x: number; y: number }): void {
    const live = r.can.resume === "switch";
    const items: MenuItem[] = [
      { label: live ? copyText("history.row.switch") : copyText("history.row.resume"), enabled: r.can.resume !== "bg", title: r.can.resume === "bg" ? copyText("history.row.bgHint") : undefined, onClick: () => void this.resume(r) },
      { label: copyText("history.menu.openWindow"), icon: "front", onClick: () => this.openWindow(r) },
      { label: "", divider: true },
      { label: r.starred ? copyText("history.menu.unstar") : copyText("history.menu.star"), onClick: () => void this.star(r) },
      { label: copyText("history.menu.rename"), detail: "F2", onClick: () => void this.rename(r) },
      { label: r.hidden ? copyText("history.menu.unhide") : copyText("history.menu.hide"), onClick: () => void this.hide(r) },
      { label: "", divider: true },
      ...(r.origin ? [] : [{ label: copyText("history.menu.openDir"), icon: "folder" as const, onClick: () => void revealInFolder(r.projectPath) }]),
      { label: copyText("history.menu.newInDir"), onClick: () => void this.newSessionIn(r.origin, r.projectPath, r.agent) },
      { label: "", divider: true },
      {
        label: copyText("history.menu.delete"),
        danger: true,
        detail: "Delete",
        enabled: r.can.delete !== "live",
        title: r.can.delete === "live" ? copyText("history.delete.liveHint") : undefined,
        onClick: () => void this.remove(r),
      },
    ];
    openMenu("x" in at ? at : { el: at, align: "end" }, items, { label: copyText("history.row.more") });
  }

  private async star(r: HistoryRow): Promise<void> {
    try {
      await annotate(r.sessionId, { starred: !r.starred });
      this.refresh(false, keyOf(r.origin));
    } catch (e) {
      toast(copyText("history.star.failed", { why: String(e) }), "");
    }
  }

  private async hide(r: HistoryRow): Promise<void> {
    const to = !r.hidden;
    try {
      await annotate(r.sessionId, { hidden: to });
      this.refresh(false, keyOf(r.origin));
      if (to)
        undoToast(copyText("history.hide.done", { label: labelOf(r) }), () => void annotate(r.sessionId, { hidden: false }).then(() => this.refresh(false, keyOf(r.origin))), () => {});
    } catch (e) {
      toast(copyText("history.hide.failed", { why: String(e) }), "");
    }
  }

  private async rename(r: HistoryRow): Promise<void> {
    const next = await askText({
      title: copyText("history.rename.title"),
      label: copyText("history.rename.prompt"),
      action: copyText("history.rename.action"),
      initial: r.customTitle ?? r.label,
    });
    if (next === null) return;
    const before = r.customTitle;
    try {
      await annotate(r.sessionId, { customTitle: next.trim() });
      this.refresh(false, keyOf(r.origin));
      undoToast(copyText("history.rename.done"), () => void annotate(r.sessionId, { customTitle: before ?? "" }).then(() => this.refresh(false, keyOf(r.origin))), () => {});
    } catch (e) {
      toast(copyText("history.rename.failed", { why: String(e) }), "");
    }
  }

  /** 删除只问一次（R2-2-2）；在跑的不让删（后端判的 `can.delete`，主窗口此刻的状态再核一次）。 */
  private async remove(r: HistoryRow): Promise<void> {
    if (r.can.delete === "live" || (!r.origin && this.liveInTabs(r.sessionId))) {
      toast(copyText("history.delete.liveHint"), "");
      return;
    }
    const label = labelOf(r);
    const body = [
      r.origin ? copyText("history.delete.bodyRemote", { machine: r.origin }) : copyText("history.delete.body"),
      r.can.delete === "unsure" ? copyText("history.delete.unsure") : null,
    ].filter((l): l is string => l !== null);
    const ok = await confirmDialog({ title: copyText("history.delete.title", { label }), action: copyText("history.delete.action"), danger: true, body: body.join("\n") });
    if (!ok) return;
    try {
      await deleteSession(r.origin ?? LOCAL_ORIGIN, r.sessionId);
    } catch (e) {
      toast(copyText("history.delete.failed", { label, why: String(e) }), "");
      return;
    }
    void forgetAnnotation(r.sessionId);
    if (this.shown === rowKey(r)) {
      this.disposeViewer();
      this.showPlaceholder();
    }
    toast(copyText("history.delete.done", { label }), "", { level: "success" });
    this.refresh(false, keyOf(r.origin));
  }

  /** 在这个目录开一个新会话（主窗口稿那一个起会话的路；号跟随那台的默认号）。 */
  private async newSessionIn(origin: string | undefined, dir: string, agent: string): Promise<void> {
    const behavior = await getBehavior();
    try {
      if (origin) {
        await runNewSessionRemote(origin, agent, dir, await resolveResumeCommand(origin, behavior.resumeCommandRemote), { account: FOLLOW });
      } else {
        await launchLocal({ action: { kind: "new" }, agent, cwd: dir, launcher: behavior.resumeCommandLocal || null, account: FOLLOW, tmuxName: null }, dir);
        expectArrival({ origin: LOCAL_ORIGIN, match: { cwd: dir }, tmuxName: null, arrived: { title: copyText("history.newSession.started", { dir }), body: arrivedBody(LOCAL_ORIGIN) } });
      }
    } catch (e) {
      toast(copyText("history.newSession.failed", { why: String(e) }), "");
    }
  }
}

function groupKey(g: HistoryGroup): string {
  return `${g.origin ?? ""}\u0000${g.key}`;
}

/** 内容命中里那个会话不在清单里（被筛掉 / 那台的清单没答上）⇒ 用命中那一行拼一个只够画的行（不能做的都不给）。 */
function rowFromHits(sh: SessionHits): HistoryRow {
  return {
    agent: sh.agent,
    agentTag: null,
    sessionId: sh.sessionId,
    projectDir: "",
    projectPath: sh.projectPath,
    projectName: sh.projectName,
    group: "",
    aiTitle: null,
    firstUserExcerpt: "",
    title: sh.title,
    label: sh.title,
    untitled: false,
    startedAt: sh.updatedAt,
    updatedAt: sh.updatedAt,
    at: sh.updatedAt,
    jsonlPath: sh.jsonlPath,
    messageCountApprox: 0,
    isBg: false,
    starred: false,
    customTitle: null,
    hidden: false,
    status: "unknown",
    can: { resume: "yes", accounts: false, fork: false, delete: "unsure" },
    ...(sh.origin ? { origin: sh.origin } : {}),
  };
}
