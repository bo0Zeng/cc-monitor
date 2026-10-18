/**
 * **历史页**：主窗口上的全屏视图，列表在左、内容在右。
 *
 * - 列表：默认「按时间」平铺（今天 · 昨天 · 本周 · 按月分段），「按项目」是第二种看法；几台在本机后端并成一列
 *   （`history-browse`：判活 · 标题 · 每行能做什么 · 分组 · 怎么并 · 分叉挂哪都在那里定），这里只问看得见的那一窗（虚拟滚动、滚到哪问到哪）、照项画。
 *   开页 · 刷新：一问照手上的马上答，每台远端另一问先同步它（慢的不挡别的）；一台读不到在列表顶说（有上一次的副本 ⇒ 行照副本出、说是几时读到的）。
 * - 一个搜索框：敲字 ⇒ 后端按标题 / 第一句 / 项目名过滤全部会话；回车 ⇒ 内容搜索（各台 `history-search`），结果按会话分块。
 * - 「筛选」浮层：机器 · 时间 · 排序 · 显示已隐藏 · 搜内容时谁说的 · 含工具输出与思考。选项记在本机（界面偏好）。
 * - 右边：点一行就地看（只读查看器），列表不动。
 * - 实时：鼠标在列表上或焦点在列表里时不重排，移开再排；选中的那一行按会话认，不因上面插行而移动。
 */
import { button, setBusy, setDisabled } from "../kit/button";
import { splitButton } from "../kit/split-button";
import { icon } from "../kit/icon";
import { countBadge } from "../kit/badge";
import { tabs } from "../kit/tabs";
import { checkbox, radio } from "../kit/switch";
import { emptyState } from "../kit/empty";
import { skeletonRows } from "../kit/skeleton";
import { spinner } from "../kit/progress";
import { openPopover, closePopover } from "../kit/popover";
import { openMenu, type MenuItem } from "../kit/menu";
import { toast, undoToast, failToast } from "../kit/toast";
import { askText, confirmDialog } from "../kit/dialog";
import { dispatcher } from "../keybindings/registry";
import { copyText } from "../copy-table";
import { commands } from "../ipc/commands";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import { LS_KEYS, safeGetJson, safeSetJson } from "../local-storage";
import { annotate, forgetAnnotation } from "../history-reads";
import { setHide } from "../session-hidden-cmds";
import { undoHide } from "../session-hidden";
import { fetchBrowse, type BrowseWindow, type HistoryGroup, type HistoryMachine, type HistoryRow } from "../history-list-reads";
import type { HistoryBrowseReq } from "../generated/HistoryBrowseReq";
import type { HistorySection } from "../generated/HistorySection";
import { searchAllMachines, type SearchResult, type SessionHits } from "./history-search";
import { SessionViewer, viewerPath, type ViewerHead } from "./session-viewer";
import { defaultPick, pickAsk, resumeAccounts, resumeHint, resumeMenuItems, type ResumeAccounts, type ResumePick } from "../resume-menu";
import { deleteSession } from "../session-writes";
import { revealInFolder } from "../reveal-in-folder";
import { openSession } from "../session-open";
import { standingOf } from "../sessions-where";
import { openNewSession } from "../new-session";
import { groupHead, hitsBlock, labelOf, machineTag, rowBadges, rowKey, sectionHead, sessionRow, strip, type RowHooks } from "./history-rows";
import s from "./history.module.css";
import { saidOf } from "../kit/detail";

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
  /** 列表拖到的宽（宽档；缺 = 420）。 */
  listWidth?: number;
}

const DEFAULT_PREFS: Prefs = { view: "time", off: [], withinDays: 0, sort: "activity", hidden: false, scope: "all", tools: false };

function loadPrefs(): Prefs {
  const v = safeGetJson<Partial<Prefs>>(LS_KEYS.historyPrefs) ?? {};
  return { ...DEFAULT_PREFS, ...v, off: Array.isArray(v.off) ? v.off.filter((x) => typeof x === "string") : [] };
}

/** 敲字之后停多久才问（乙5：150 ms）。 */
const QUERY_DEBOUNCE_MS = 150;
/** 列表可拖的宽（默认 420，320–640）；键盘一步 16。 */
const LIST_MIN = 320;
const LIST_MAX = 640;
const LIST_STEP = 16;
/** 窄档（乙4-⑧：< 800 单块）。jsdom 没有 `matchMedia` ⇒ 当宽档。 */
const isNarrow = (): boolean => window.matchMedia?.("(max-width: 799px)").matches ?? false;

/** 方向键走行时停多久才读右边（快速划过不读）。 */
const PREVIEW_DEBOUNCE_MS = 200;

/** 列表里每种项的高（与 `history.module.css` 里的同一组数）：按时间的会话行 · 段头；按项目的组头与紧凑行同高。 */
const ROW_H = 52;
const SEC_H = 32;
const COMPACT_H = 30;
/** 看得见的那几项上下各多要几项（滚一点不必再问）。一窗至多 2000 项（后端的上限）。 */
const PAD = 20;
const WINDOW_MAX = 2000;

export class HistoryView {
  /** 删会话前问一句：主窗口里它此刻活着吗（列表拉下来那一刻的状态可能已旧）。 */
  liveInTabs: (sid: string) => boolean = () => false;
  /** 主窗口里这个会话此刻「需手动」的那个词；不在 / 不需要 ⇒ `null`。 */
  needsOf: (sid: string) => string | null = () => null;
  /** 切到主窗口里的这个会话。 */
  switchTo: (sid: string) => void = () => {};

  private readonly root: HTMLElement;
  private isOpen = false;
  private prefs: Prefs = loadPrefs();
  private machines: string[] = [""];
  /** 这一列手上的那一窗（还没答过 ⇒ `null`）；本机后端整问没答上 ⇒ `failed` 那一句。 */
  private win: BrowseWindow | null = null;
  private failed: string | null = null;
  /** 要的那一窗（第几项起 · 几项）：滚到哪问到哪。 */
  private want = { from: 0, limit: 2 * PAD };
  /** 发问的先后号：比已画的那一问早发的答不画。开一次页换一个号（关了再开，上一回的答不画）。 */
  private issued = 0;
  private shownIssued = 0;
  private opens = 0;
  /** 没答回来的问几问（> 0 ⇒ 刷新转圈）· 正在同步的远端那几台。 */
  private inFlight = 0;
  private syncing = new Set<string>();
  /** 走到两头（Home / End）时那一头还不在手上那一窗里 ⇒ 等那一窗答回来再选。 */
  private pending: "first" | "last" | null = null;
  private query = "";
  private queryTimer: ReturnType<typeof setTimeout> | null = null;
  /** 内容搜索：`null` = 没在搜内容（列表是清单）。 */
  private content: { q: string; state: "searching" } | { q: string; state: "done"; r: SearchResult } | { q: string; state: "failed"; why: string } | null = null;
  private contentSeq = 0;
  private selected: string | null = null;
  private shown: string | null = null;
  /** 每行「恢复 ▾」里勾着的那一组（只在这一页开着时记）。 */
  private picks = new Map<string, ResumePick>();
  private previewTimer: ReturnType<typeof setTimeout> | null = null;
  private openForks = new Set<string>(safeGetJson<string[]>(LS_KEYS.historyExpandedForks) ?? []);
  /** 按项目看时哪几组开着：`open` 是全部的样子，`except` 里那几组（`台\u0000键`）反过来。 */
  private fold = { open: false, except: new Set<string>() };
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
  /** 按时间看时列表顶上粘着的段头。 */
  private readonly sticky = pinnedSection();
  private contentEl!: HTMLElement;
  private splitEl!: HTMLElement;
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
    // 开页即给焦点（不等远端）。
    this.searchInput.focus();
    this.machines = ["", ...(await commands.list_remote_mcp_origins().catch(() => [] as string[]))];
    // 开页：先答手上的，远端那几台各自同步一次（后端记着副本、不按时间过期，开页与「刷新」才再问那台）。
    this.restart();
    this.refresh(this.remotes());
  }

  close(): void {
    if (!this.isOpen) return;
    this.picks.clear();
    this.opens++;
    this.contentSeq++;
    closePopover();
    this.disposeViewer();
    this.root.remove();
    this.isOpen = false;
    dispatcher.popOverlay(this.layer);
  }

  /**
   * 那台推了「清单变了」（`changed/history_list`）：开着、那台在这一页上 ⇒ 再问看得见的那一窗（远端那台带它的 `fresh`，本机后端先同步副本）。
   * 改名 · 星标 · 隐藏 · 删了 · 会话起停之后的重问都走这一处（那台的推送叫回来），页里不另问。
   */
  onListMoved(origin: Origin): void {
    if (!this.isOpen) return;
    const m = isLocalOrigin(origin) ? "" : origin;
    if (!this.wanted().includes(m)) return;
    this.refresh(m ? [m] : []);
  }

  /** Ctrl+F 落在历史页：右边有会话 ⇒ 会话内查找；否则不接（交给主窗口）。 */
  openFind(): boolean {
    if (!this.isOpen || !this.viewer) return false;
    this.viewer.openFind();
    return true;
  }

  /** Esc 一次一层（乙5）：焦点在内容里 ⇒ 回列表 ＞ 搜索框有字 ⇒ 清空 ＞ 关历史页。菜单 / 浮层 / 对话框各自在栈上先接。 */
  private handleEsc(): boolean {
    // 窄档的内容 ⇒ 回列表（焦点在内容里也一样）。
    if (isNarrow() && this.splitEl.dataset.pane === "content") {
      this.toList();
      return true;
    }
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

  private remotes(): string[] {
    return this.wanted().filter((m) => m !== "");
  }

  /** 这一问（`fresh` ＝ 先同步的那几台远端）：要哪几台 · 筛排 · 怎么看 · 开合 · 窗口。 */
  private askOf(fresh: string[]): HistoryBrowseReq {
    const split = (k: string): [string, string] => {
      const i = k.indexOf("\u0000");
      return [k.slice(0, i), k.slice(i + 1)];
    };
    return {
      here: this.wanted().includes(""),
      remotes: this.remotes(),
      fresh,
      ...(this.query ? { query: this.query } : {}),
      sort: this.prefs.sort,
      ...(this.prefs.withinDays ? { within_days: this.prefs.withinDays } : {}),
      ...(this.prefs.hidden ? { hidden: true } : {}),
      by: this.prefs.view,
      groups: {
        open: this.fold.open,
        except: [...this.fold.except].map(split).map(([o, key]) => ({ ...(o ? { origin: o } : {}), key })),
      },
      forks: [...this.openForks].map(split).map(([o, sid]) => ({ ...(o ? { origin: o } : {}), sid })),
      range: { ...this.want },
    };
  }

  /** 这一问是哪一问（除了先同步哪几台，别的都算）。 */
  private genOf(req: HistoryBrowseReq): string {
    return JSON.stringify([this.opens, { ...req, fresh: [] }]);
  }

  /** 换了看什么（搜索词 · 筛选 · 看法）⇒ 回到顶、从头一窗问起。 */
  private restart(): void {
    this.listEl.scrollTop = 0;
    this.want = windowAround(0, Math.ceil(this.listEl.clientHeight / (this.prefs.view === "time" ? ROW_H : COMPACT_H)));
  }

  /** 问这一列：一问照手上的（马上答）＋ `sync` 里每台远端各一问（那一台同步完才答，慢的不挡别的）。 */
  private refresh(sync: readonly string[] = []): void {
    this.ask([], "data");
    for (const m of sync) {
      if (!this.remotes().includes(m)) continue;
      this.syncing.add(m);
      this.ask([m], "data");
    }
    this.renderChrome();
  }

  /** 发一问。`scroll` ＝ 滚出来的那一窗（答回来就画，鼠标在列表上也画）；`data` ＝ 数据变了（鼠标在列表上先不重排）。 */
  private ask(fresh: string[], why: "data" | "scroll"): void {
    const req = this.askOf(fresh);
    const gen = this.genOf(req);
    const n = ++this.issued;
    this.inFlight++;
    const land = (w: BrowseWindow | null, failed: string | null): void => {
      this.inFlight--;
      for (const m of fresh) this.syncing.delete(m);
      if (!this.isOpen) return;
      const current = gen === this.genOf(this.askOf([])) && n > this.shownIssued;
      // 同步那一问被后来的问顶掉了 · 自己没答上：副本已在本机后端换好（或记下了没成的那一句）⇒ 照现在的样子再问一次手上的。
      if (fresh.length > 0 && (!current || w === null)) return this.ask([], why);
      if (!current) return this.renderChrome();
      this.shownIssued = n;
      if (w) this.win = w;
      this.failed = failed;
      if (why === "scroll") this.renderNow();
      else this.scheduleRender();
    };
    fetchBrowse(req).then(
      (w) => land(w, null),
      (e: unknown) => land(null, saidOf(e)),
    );
  }

  private setQuery(q: string): void {
    this.searchInput.value = q;
    this.onQueryInput();
  }

  private onQueryInput(): void {
    this.content = null;
    this.contentSeq++;
    if (this.queryTimer) clearTimeout(this.queryTimer);
    // 调度：一次性 —— 敲字之后停 150ms 才问清单，再敲就重来
    this.queryTimer = setTimeout(() => {
      this.queryTimer = null;
      const q = this.searchInput.value.trim();
      if (q === this.query) return this.renderNow();
      this.query = q;
      this.restart();
      this.refresh();
    }, QUERY_DEBOUNCE_MS);
    this.renderNow();
  }

  /** 回车 ⇒ 在全部会话内容里搜。失败 ⇒ 清掉旧结果、换成错误条。 */
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
      this.content = { q, state: "failed", why: saidOf(e) };
    }
    this.renderNow();
  }

  // ───────────────────────── 外壳 ─────────────────────────

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = `${s.hv} history-view`;
    root.appendChild(this.buildTop());
    const split = document.createElement("div");
    split.className = s.hvSplit;
    // 窄档单块：`list` 只有列表 · `content` 内容盖满（宽档、中档不看它）。
    split.dataset.pane = "list";
    this.splitEl = split;
    this.contentEl = document.createElement("div");
    this.contentEl.className = s.hvContent;
    const col = this.buildList();
    split.append(col, this.sizer(root), this.contentEl);
    root.appendChild(split);
    if (this.prefs.listWidth) root.style.setProperty("--hv-list-w", `${this.prefs.listWidth}px`);
    this.showPlaceholder();
    return root;
  }

  /** 顶栏：返回 · 两种看法 · 搜索框 · 筛选 · 刷新。 */
  private buildTop(): HTMLElement {
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
        this.restart();
        this.refresh();
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
    this.filterBtn.classList.add(s.hvFilterBtn);
    this.filterBtn.setAttribute("aria-haspopup", "dialog");
    this.refreshBtn = button({
      label: copyText("history.page.refresh"),
      kind: "icon",
      icon: "refresh",
      hint: copyText("history.page.refresh"),
      onClick: () => this.refresh(this.remotes()),
    });
    top.append(back, views, search, this.filterBtn, this.refreshBtn);
    return top;
  }

  /** 左栏：提示条 · 列表头 · 列表（键盘走行 · Tab 进来落到选中行 · 鼠标 / 焦点在上面时不重排）。 */
  private buildList(): HTMLElement {
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
    this.listEl.addEventListener("scroll", () => this.onScroll());
    // Tab 进列表 ⇒ 焦点落到选中的那一行（没有就第一行）；鼠标点进来的不在这里管（点的那一行自己选，见 `hooks().select`）。
    this.listEl.addEventListener("focus", (ev) => {
      if (ev.target === this.listEl && !this.quietFocus) this.moveSelection(this.selected ?? this.order[0] ?? null, true);
    });
    // 实时更新不拽人：鼠标在列表上 / 焦点在列表里 ⇒ 先不重排，移开再排。
    const hold = (on: boolean): void => {
      this.holding = on || this.listEl.matches(":hover") || this.listEl.contains(document.activeElement);
      if (!this.holding && this.dirty) this.renderNow();
    };
    this.listEl.addEventListener("pointerenter", () => hold(true));
    this.listEl.addEventListener("pointerleave", () => hold(false));
    this.listEl.addEventListener("focusin", () => hold(true));
    // 调度：一次性 —— 焦点离开列表那一下推到下一拍再看焦点去了哪
    this.listEl.addEventListener("focusout", () => setTimeout(() => hold(false), 0));
    col.append(this.stripsEl, this.listHead, this.listEl);
    return col;
  }

  /** 列表与内容之间那一道（宽档）：拖改列表宽，松手记下。 */
  private sizer(root: HTMLElement): HTMLElement {
    return listSizer(root, this.prefs.listWidth ?? 420, () => this.listEl.parentElement!.getBoundingClientRect().left, (v) => {
      this.prefs.listWidth = v;
      this.savePrefs();
    });
  }

  /** 窄档：内容 ⇒ 回列表（焦点回选中的那一行）。 */
  private toList(): void {
    this.splitEl.dataset.pane = "list";
    this.focusSelected();
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
    const changed = (refetch: boolean): void => {
      this.savePrefs();
      this.markFilterBtn();
      if (refetch) {
        this.restart();
        this.refresh();
      } else this.renderNow();
    };
    const researchIfContent = (): void => {
      if (this.content) void this.runContentSearch();
    };
    const panel = filterPanel(this.prefs, this.machines.map((m) => this.machineCheck(m, changed)), {
      time: (v) => {
        this.prefs.withinDays = v;
        changed(true);
      },
      sort: (v) => {
        this.prefs.sort = v;
        changed(true);
      },
      hidden: (on) => {
        this.prefs = { ...this.prefs, hidden: on };
        changed(true);
      },
      scope: (v) => {
        this.prefs.scope = v;
        changed(false);
        researchIfContent();
      },
      tools: (on) => {
        this.prefs.tools = on;
        changed(false);
        researchIfContent();
      },
    });
    openPopover(this.filterBtn, panel, { label: copyText("history.page.filter") });
  }

  /** 筛选里一台机器那一格：勾掉 / 勾回 ⇒ 重问这一列（勾回的远端那台先同步一次）；那台上次没读到 ⇒ 标「离线」。 */
  private machineCheck(m: string, changed: (refetch: boolean) => void): HTMLElement {
    const c = checkbox(m ? m : copyText("history.filter.local"), !this.prefs.off.includes(m), (on) => {
      this.prefs.off = on ? this.prefs.off.filter((x) => x !== m) : [...this.prefs.off, m];
      changed(false);
      this.restart();
      this.refresh(on && m ? [m] : []);
    });
    const read = this.readOf(m)?.is;
    if (read === "failed" || read === "stale") {
      const w = document.createElement("span");
      w.className = s.hvFilterWarn;
      w.textContent = copyText("history.filter.offline");
      c.appendChild(w);
    }
    return c;
  }

  private clearFilters(): void {
    this.prefs = { ...DEFAULT_PREFS, view: this.prefs.view };
    this.savePrefs();
    this.markFilterBtn();
    this.setQuery("");
    this.restart();
    this.refresh();
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

  /** 一台在头里那一格（这一列还没答过 · 那台不在这一问里 ⇒ 缺）。 */
  private readOf(m: string): HistoryMachine["read"] | undefined {
    return this.win?.head.machines.find((x) => (x.origin ?? "") === m)?.read;
  }

  /** 不动列表的那几块（刷新转圈 · 提示条 · 筛选数）。 */
  private renderChrome(): void {
    this.refreshBtn.dataset.busy = String(this.inFlight > 0);
    this.markFilterBtn();
    this.stripsEl.replaceChildren(...this.machineStrips(), ...this.contentStrips());
  }

  /** 每台一条：没读到 ⇒［重新连接］（本机 ⇒［重试］）；读到了但说清单不全 ⇒［重试］。本机后端整问没答上 ⇒ 红条［重试］。 */
  private machineStrips(): HTMLElement[] {
    const strips: HTMLElement[] = [];
    if (this.failed !== null) {
      strips.push(strip("error", copyText("history.list.localFailed"), { label: copyText("history.list.retry"), run: () => this.refresh(this.remotes()) }));
    }
    for (const m of this.wanted()) {
      if (this.syncing.has(m)) continue;
      const st = this.readOf(m);
      const again = (): void => this.refresh(m ? [m] : []);
      if (st?.is === "failed") {
        strips.push(
          m
            ? strip("warn", copyText("history.list.machineDown", { machine: m }), { label: copyText("history.list.reconnect"), run: again })
            : strip("error", copyText("history.list.localFailed"), { label: copyText("history.list.retry"), run: again }),
        );
      } else if (st?.is === "stale") {
        // 读不到、行照那台上一次的副本出：说清是几时读到的，不冒充读到。
        strips.push(strip("warn", copyText("history.list.machineStale", { machine: m || copyText("history.filter.local"), at: st.at }), { label: copyText("history.list.reconnect"), run: again }));
      } else if (st?.is === "ok" && st.notice) {
        strips.push(strip("warn", copyText("history.list.notice"), { label: copyText("history.list.retry"), run: again }));
      }
    }
    return strips;
  }

  /** 内容搜索那几条：哪台没搜到 · 跳过的那一家 · 读不了的份数；整次失败 ⇒ 红条［重试］。 */
  private contentStrips(): HTMLElement[] {
    const strips: HTMLElement[] = [];
    const c = this.content;
    if (c?.state === "done") {
      for (const d of c.r.down) strips.push(strip("warn", copyText("history.search.machineDown", { machine: d.origin }), { label: copyText("history.list.reconnect"), run: () => void this.runContentSearch() }));
      for (const a of c.r.skipped) strips.push(strip("info", copyText("history.search.skipped", { agent: a })));
      if (c.r.unreadable > 0) strips.push(strip("info", copyText("history.search.unreadable", { n: c.r.unreadable })));
    } else if (c?.state === "failed") {
      strips.push(strip("error", copyText("history.search.failed", { why: c.why }), { label: copyText("history.list.retry"), run: () => void this.runContentSearch() }));
    }
    return strips;
  }

  /** 这一列的几何（只照项数 · 段头算：每种项一个定高）。 */
  private geom(): Geometry {
    return geometry(this.prefs.view, this.win?.total ?? 0, this.win?.head.sections ?? []);
  }

  private renderNow(): void {
    this.dirty = false;
    this.renderChrome();
    const w = this.win;
    this.rowsByKey = new Map();
    for (const l of w?.lines ?? []) if (l.kind === "row") this.rowsByKey.set(rowKey(l.row), l.row);
    const body: HTMLElement[] = [];
    this.order = [];
    const hooks = this.hooks();
    if (this.content) {
      this.renderContent(body, hooks);
    } else if (!w || w.total === 0) {
      body.push(this.emptyList(!w || this.inFlight > 0 || this.syncing.size > 0));
    } else {
      this.renderHead(w);
      this.renderLines(body, w, hooks);
    }
    const top = this.listEl.scrollTop;
    const refocus = this.listEl.contains(document.activeElement) && document.activeElement !== this.listEl;
    this.listEl.replaceChildren(...body);
    this.listEl.scrollTop = top;
    this.markRows();
    this.paintSticky();
    if (refocus) {
      const el = this.selected ? this.rowEl(this.selected) : null;
      (el ?? this.listEl).focus({ preventScroll: true });
    }
    if (this.pending) this.settlePending();
    // 答回来那一窗盖不住看得见的那几项（数据变了、整列变短了）⇒ 照现在的位置再问一窗。
    this.onScroll();
  }

  /** 一行都没有：还在读 ⇒ 骨架；带着字或筛选 ⇒「没有匹配」＋［清除筛选］；否则说还没有会话。 */
  private emptyList(loading: boolean): HTMLElement {
    if (loading) {
      this.listHead.replaceChildren(copyText("history.list.reading", { machines: this.wanted().map((m) => m || copyText("history.filter.local")).join(` ${copyText("cssMarks.sep.dot")} `) }));
      return skeletonRows(6);
    }
    this.listHead.replaceChildren();
    const filtered = this.query !== "" || this.activeFilters() > 0;
    return filtered
      ? emptyState({ icon: "search", text: copyText("history.list.noMatch"), action: button({ label: copyText("history.list.clearFilter"), onClick: () => this.clearFilters() }) })
      : emptyState({ text: copyText("history.list.empty"), hint: copyText("history.list.emptyHint") });
  }

  /** 列表头：按时间 ⇒ 几个会话（各台几行之和）；按项目 ⇒ 几个项目 · 几个会话 ＋ 全部展开 / 全部收起。 */
  private renderHead(w: BrowseWindow): void {
    if (this.prefs.view === "time") {
      this.listHead.replaceChildren(copyText("history.list.count", { n: w.head.machines.reduce((n, m) => n + m.total, 0) }));
      return;
    }
    const head = document.createElement("span");
    head.textContent = copyText("history.list.countProjects", { p: w.head.projects, n: w.head.grouped });
    const end = document.createElement("span");
    end.className = s.hvHeadEnd;
    const all = (open: boolean): void => {
      this.fold = { open, except: new Set() };
      this.refresh();
    };
    end.append(
      button({ label: copyText("history.list.expandAll"), kind: "ghost", size: "compact", onClick: () => all(true) }),
      button({ label: copyText("history.list.collapseAll"), kind: "ghost", size: "compact", onClick: () => all(false) }),
    );
    this.listHead.replaceChildren(head, end);
  }

  /**
   * 手上那一窗照项画：整列高的一块垫底，那一窗摆在它该在的高度上（虚拟滚动；项都是定高，段头落在第几项之前由头给）。
   * 按时间看时顶上另有一条粘着的段头，写看得见的第一项所在的那一段。
   */
  private renderLines(body: HTMLElement[], w: BrowseWindow, hooks: RowHooks): void {
    const g = this.geom();
    const spacer = document.createElement("div");
    spacer.className = s.hvSpacer;
    spacer.style.height = `${g.height}px`;
    const block = document.createElement("div");
    block.className = s.hvBlock;
    block.style.top = `${g.blockTop(w.from)}px`;
    const compact = this.prefs.view === "project";
    w.lines.forEach((line, j) => {
      for (const sec of w.head.sections) if (sec.at === w.from + j) block.appendChild(sectionHead(sec.text));
      if (line.kind === "group") {
        block.appendChild(this.groupLine(line.group, line.open));
        return;
      }
      block.appendChild(sessionRow(line.row, hooks, { compact, forks: line.forks, forksOpen: line.forksOpen, child: line.child, orphan: line.orphan }));
      this.order.push(rowKey(line.row));
    });
    spacer.appendChild(block);
    if (!compact) {
      this.sticky.textContent = "";
      body.push(this.sticky);
    }
    body.push(spacer);
  }

  /** 按项目看的组头：点了开合（开合交后端摊）· ［新建］在那个目录起新会话 · 读不了的那一组［重试］再同步那一台。 */
  private groupLine(g: HistoryGroup, open: boolean): HTMLElement {
    const gk = groupKey(g);
    return groupHead(g, {
      open,
      onToggle: () => {
        if (this.fold.except.has(gk)) this.fold.except.delete(gk);
        else this.fold.except.add(gk);
        this.refresh();
      },
      onNew: () => void openNewSession({ origin: g.origin ?? LOCAL_ORIGIN, cwd: g.projectPath, agent: g.agent }),
      onRetry: () => this.refresh(g.origin ? [g.origin] : []),
    });
  }

  /** 顶上粘着的段头：看得见的第一项落在哪一段。 */
  private paintSticky(): void {
    const w = this.win;
    if (!w || this.content || this.prefs.view !== "time" || !this.sticky.isConnected) return;
    const at = this.geom().lineAt(this.listEl.scrollTop);
    let text = "";
    for (const sec of w.head.sections) if (sec.at <= at) text = sec.text;
    // 没有段（空着）⇒ CSS 让它不占眼（`:empty`）。
    this.sticky.textContent = text;
  }

  /** 滚了：粘着的段头跟上；看得见的那几项不全在手上那一窗里 ⇒ 问看得见的那一窗（上下各多带一截）。 */
  private onScroll(): void {
    this.paintSticky();
    const w = this.win;
    if (!w || this.content || w.total === 0) return;
    const g = this.geom();
    const first = g.lineAt(this.listEl.scrollTop);
    const last = g.lineAt(this.listEl.scrollTop + this.listEl.clientHeight);
    if (w.from <= first && w.from + w.lines.length - 1 >= last) return;
    const want = windowAround(first, last);
    if (want.from === this.want.from && want.limit === this.want.limit) return;
    this.want = want;
    this.ask([], "scroll");
  }


  /** 内容搜索的结果：每个会话一块（会话行 ＋ 前几处命中）。 */
  private renderContent(body: HTMLElement[], hooks: RowHooks): void {
    const c = this.content;
    if (!c) return;
    if (c.state === "searching") {
      body.push(this.searchingHead());
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
      body.push(this.noHits(c.q));
      return;
    }
    for (const sh of r.sessions) {
      const row = rowFromHits(sh);
      const el = sessionRow(row, hooks, { compact: false, forks: 0, forksOpen: false, child: false, orphan: false });
      this.order.push(rowKey(row));
      body.push(hitsBlock(el, sh, (hit) => this.show(row, hit ?? undefined)));
    }
  }

  /** 搜着时：列表头转圈 ＋［停止］（停了回到清单，晚到的结果不画）；列表里给骨架。 */
  private searchingHead(): HTMLElement {
    const stop = button({ label: copyText("history.search.stop"), kind: "ghost", size: "compact", onClick: () => {
      this.contentSeq++;
      this.content = null;
      this.renderNow();
    } });
    this.listHead.replaceChildren(spinner(), copyText("history.search.searching"), stop);
    return skeletonRows(4);
  }

  /** 没搜到：没含工具输出与思考时给那颗（点了带上它再搜）。 */
  private noHits(q: string): HTMLElement {
    return emptyState({
      icon: "search",
      text: copyText("history.search.noMatch", { q }),
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
    });
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
    // 调度：一次性 —— 方向键走行时停 200ms 才读右边，快速划过不读
    this.previewTimer = setTimeout(() => {
      this.previewTimer = null;
      const r = this.rowsByKey.get(k);
      if (r && this.shown !== k) this.show(r);
    }, PREVIEW_DEBOUNCE_MS);
  }

  private onListKey(ev: KeyboardEvent): void {
    if (ev.isComposing) return;
    const i = this.selected ? this.order.indexOf(this.selected) : -1;
    const page = Math.max(1, Math.floor(this.listEl.clientHeight / ROW_H));
    // 走到两头：那一头不在手上那一窗里 ⇒ 滚过去、等那一窗答回来再选（[`settlePending`]）。
    const w = this.win;
    if (w && !this.content && ((ev.key === "Home" && w.from > 0) || (ev.key === "End" && w.from + w.lines.length < w.total))) {
      return this.consume(ev, () => {
        this.pending = ev.key === "Home" ? "first" : "last";
        this.listEl.scrollTop = ev.key === "Home" ? 0 : this.geom().height;
        this.onScroll();
      });
    }
    const to = navTarget(ev.key, i, this.order.length, page);
    if (to === "search") return this.consume(ev, () => this.searchInput.focus());
    if (to !== null) return this.consume(ev, () => this.moveSelection(this.order[Math.max(0, Math.min(this.order.length - 1, to))] ?? null, true));
    const r = this.selected ? this.rowsByKey.get(this.selected) : undefined;
    if (r) this.onRowKey(ev, r);
  }

  /** 选中那一行上的键：回车看 / Ctrl+回车恢复 / Shift+回车新窗口 · F2 改标题 · Delete 删 · 菜单键与 Shift+F10 开菜单。 */
  private onRowKey(ev: KeyboardEvent, r: HistoryRow): void {
    switch (ev.key) {
      case "Enter":
        if (ev.ctrlKey || ev.metaKey) return this.consume(ev, () => void this.resume(r));
        if (ev.shiftKey) return this.consume(ev, () => this.openWindow(r));
        return this.consume(ev, () => {
          this.show(r);
          this.splitEl.dataset.pane = "content";
          this.viewer?.element.querySelector<HTMLElement>(".session-viewer-stream")?.focus();
        });
      case "F2":
        return this.consume(ev, () => void this.rename(r));
      case "Delete":
        return this.consume(ev, () => void this.remove(r));
      case "ContextMenu":
        return this.consume(ev, () => this.menu(r, this.rowEl(rowKey(r)) ?? this.listEl));
      case "F10":
        if (ev.shiftKey) this.consume(ev, () => this.menu(r, this.rowEl(rowKey(r)) ?? this.listEl));
        return;
    }
  }

  /** Home / End 要的那一头答回来了 ⇒ 选它。 */
  private settlePending(): void {
    const w = this.win;
    if (!w) return;
    const at = this.pending === "first" ? w.from === 0 : w.from + w.lines.length >= w.total;
    if (!at) return;
    const k = this.pending === "first" ? this.order[0] : this.order.at(-1);
    this.pending = null;
    this.moveSelection(k ?? null, true);
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

  /** 右边就地看这一行（列表不动）。`jumpTo` = 内容命中的那一句（uuid · 行号 ＋ 后端说的落点 `tool`）。 */
  private show(r: HistoryRow, jumpTo?: { uuid: string; line: number; tool?: string }): void {
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
      head: this.headOf(r),
      origin: r.origin ?? LOCAL_ORIGIN,
      agent: r.agent,
      cwd: r.projectPath,
      ...(jumpTo !== undefined ? { scrollTo: { uuid: jumpTo.uuid, line: jumpTo.line } } : {}),
      ...(jumpTo?.tool !== undefined ? { scrollToTool: jumpTo.tool } : {}),
      suppressBranch: !r.can.fork,
      // 在跑的跟着长（与独立查看窗同一条订阅）；起了 / 结束了那台推「清单变了」（[`onListMoved`]），这里不另问。
      follow: r.face.live.is === "yes" ? { sid: r.sessionId, live: true } : undefined,
    });
  }

  /**
   * 内容头（乙4-④）：徽标（在跑 · 需手动 · 那一家 · 分身 · 已隐藏）｜［恢复 ▾］或［切过去］· 在新窗口打开 ·［⋯］；
   * 第二行项目 · 机器 · 路径 · 时间段（条数由查看器读完接上）。都按这一行的事实画，能做什么看 `can`。
   */
  private headOf(r: HistoryRow): ViewerHead {
    const needs = r.origin ? null : this.needsOf(r.sessionId);
    const acts = document.createElement("span");
    acts.className = s.hvHeadActs;
    acts.append(
      this.resumeControl(r),
      button({ label: copyText("history.menu.openWindow"), kind: "icon", icon: "front", hint: copyText("history.menu.openWindow"), onClick: () => this.openWindow(r) }),
    );
    const more = button({ label: copyText("history.row.more"), kind: "icon", icon: "more", hint: copyText("history.row.more"), onClick: () => this.menu(r, more) });
    acts.appendChild(more);
    const project = document.createElement("span");
    project.textContent = r.projectName || copyText("history.row.noDir");
    const machine = machineTag(r.origin) ?? Object.assign(document.createElement("span"), { textContent: copyText("history.filter.local") });
    const span = document.createElement("span");
    span.textContent = r.spanText;
    const toList = button({ label: copyText("history.content.toList"), kind: "ghost", icon: "back", size: "compact", onClick: () => this.toList() });
    toList.classList.add(s.hvToList);
    return {
      lead: toList,
      badges: rowBadges(r, needs),
      actions: acts,
      meta: [project, machine, ...(r.projectPath ? [viewerPath(r.projectPath)] : []), span],
    };
  }

  /**
   * 「恢复 ▾」：主按钮按勾着的那一组起（默认：上次的号 · 不用 tmux；悬停说清用哪个号、进不进 tmux）；
   * ▾ ＝ 与标签页「恢复 ▸」同一个组件（`resume-menu.ts`）＋ 在此目录新建会话。在跑的 ⇒［切过去］；分身会话 ⇒ 灰着说为什么。
   */
  private resumeControl(r: HistoryRow): HTMLElement {
    if (r.can.resume === "switch") {
      return button({ label: copyText("history.row.switch"), kind: "primary", hint: copyText("history.row.switchHint"), onClick: () => void this.resume(r) });
    }
    const sb = splitButton({
      label: copyText("history.row.resume"),
      hint: resumeHint(this.hintAccounts(r), this.pickOf(r)),
      onClick: () => void this.resume(r),
      moreLabel: copyText("history.resume.more"),
      items: () => this.resumeItems(r, sb.main),
    });
    sb.root.dataset.role = "resume";
    if (r.can.resume === "bg") {
      setDisabled(sb.main, copyText("history.row.bgHint"));
      setDisabled(sb.more, copyText("history.row.bgHint"));
    }
    return sb.root;
  }

  /** 这一行勾着的那一组（没动过 ⇒ 默认那一组）。 */
  private pickOf(r: HistoryRow): ResumePick {
    const k = rowKey(r);
    let p = this.picks.get(k);
    if (!p) this.picks.set(k, (p = defaultPick()));
    return p;
  }

  /** 菜单还没开过时悬停用的账号事实：这一家没有账号这一维 ⇒ 那一家的名字；有上次的号 ⇒ 它。 */
  private hintAccounts(r: HistoryRow): ResumeAccounts {
    if (!r.can.accounts) return { kind: "none", agentName: r.agentTag ?? r.agent };
    if (!r.lastAccount) return { kind: "off" };
    return { kind: "list", items: [{ name: r.lastAccount, label: r.lastAccount, quota: null, last: true }] };
  }

  /** ▾ 的项：账号（能选号的那一家才问那台）· 运行于（勾画在那台答的默认上）· 在此目录新建会话；勾一挪，主按钮的悬停跟着改。 */
  private async resumeItems(r: HistoryRow, main: HTMLButtonElement): Promise<MenuItem[]> {
    const origin = r.origin ?? LOCAL_ORIGIN;
    const [accounts, where] = await Promise.all([
      resumeAccounts(origin, r.agent, { hasAccounts: r.can.accounts, agentName: r.agentTag ?? r.agent, last: r.lastAccount }),
      standingOf(origin, r.sessionId),
    ]);
    const pick = this.pickOf(r);
    const machineRun = where?.run;
    main.title = resumeHint(accounts, pick, machineRun);
    return resumeMenuItems({
      accounts,
      pick,
      machineRun,
      onChange: (p) => {
        if (main.getAttribute("aria-disabled") !== "true") main.title = resumeHint(accounts, p, machineRun);
      },
      newInDir: () => void openNewSession({ origin: r.origin ?? LOCAL_ORIGIN, cwd: r.projectPath, agent: r.agent }),
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
        // 窄档：点开一行 ⇒ 内容盖满（方向键走行不翻，回车才翻）。
        this.splitEl.dataset.pane = "content";
      },
      resume: (r) => void this.resume(r),
      menu: (r, at) => this.menu(r, at),
      openWindow: (r) => this.openWindow(r),
      toggleForks: (r) => {
        const k = rowKey(r);
        if (this.openForks.has(k)) this.openForks.delete(k);
        else this.openForks.add(k);
        safeSetJson(LS_KEYS.historyExpandedForks, [...this.openForks]);
        this.refresh();
      },
      needs: (r) => (r.origin ? null : this.needsOf(r.sessionId)),
    };
  }

  /**
   * 恢复（按这一行勾着的那一组；没动过 ⇒ 跟随上次的号 · 照这台的设置，那台判）；在跑的 ⇒ 切过去。
   * 起了 ⇒ 关历史页（主窗口等它出现再切过去）；没起 ⇒ 那一句已经说过、页面留着。
   */
  private async resume(r: HistoryRow): Promise<void> {
    if (r.can.resume === "bg") return;
    if (r.can.resume === "switch") {
      this.switchTo(r.sessionId);
      this.close();
      return;
    }
    const main = this.shown === rowKey(r) ? this.viewer?.element.querySelector<HTMLButtonElement>(`[data-role="resume"] > button`) : null;
    if (main) setBusy(main, copyText("history.resume.busy"));
    const origin = r.origin ?? LOCAL_ORIGIN;
    try {
      const req = { what: { kind: "resume" as const, sid: r.sessionId }, ...pickAsk(this.pickOf(r)), local: isLocalOrigin(origin) };
      if (await openSession(origin, req)) this.close();
    } finally {
      if (main) setBusy(main, null);
    }
  }

  private openWindow(r: HistoryRow): void {
    void commands
      .open_session_in_new_window({ sessionId: r.sessionId, origin: r.origin ?? LOCAL_ORIGIN, title: labelOf(r) })
      .catch((e: unknown) => failToast(copyText("tabSessionActions.openInWindow.failed"), e));
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
      { label: copyText("history.menu.newInDir"), onClick: () => void openNewSession({ origin: r.origin ?? LOCAL_ORIGIN, cwd: r.projectPath, agent: r.agent }) },
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
      // 列表跟着那台的推送重问（[`onListMoved`]）。
      await annotate(r.origin ?? LOCAL_ORIGIN, r.sessionId, { starred: !r.starred });
    } catch (e) {
      failToast(copyText("history.star.failed"), e);
    }
  }

  private async hide(r: HistoryRow): Promise<void> {
    // 隐藏 · 取消隐藏 · 撤销与标签栏同一个口、同一套词：隐藏交 `on`、取消隐藏交 `off`（明说要显示）、
    //   撤销回到隐藏之前那一份单独设置（`undoHide`：原本没有 ⇒ `rule`）。
    const to = !r.hidden;
    const origin = r.origin ?? LOCAL_ORIGIN;
    try {
      const prev = await setHide(origin, r.sessionId, to ? "on" : "off");
      if (to) undoToast(copyText("history.hide.done", { label: labelOf(r) }), () => void setHide(origin, r.sessionId, undoHide(prev)), () => {});
    } catch (e) {
      failToast(copyText("history.hide.failed"), e);
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
      await annotate(r.origin ?? LOCAL_ORIGIN, r.sessionId, { customTitle: next.trim() });
      undoToast(copyText("history.rename.done"), () => void annotate(r.origin ?? LOCAL_ORIGIN, r.sessionId, { customTitle: before ?? "" }), () => {});
    } catch (e) {
      failToast(copyText("history.rename.failed"), e);
    }
  }

  /** 删除只问一次；在跑的不让删（后端判的 `can.delete`，主窗口此刻的状态再核一次）。 */
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
      failToast(copyText("history.delete.failed", { label }), e);
      return;
    }
    void forgetAnnotation(r.origin ?? LOCAL_ORIGIN, r.sessionId);
    if (this.shown === rowKey(r)) {
      this.disposeViewer();
      this.showPlaceholder();
    }
    toast(copyText("history.delete.done", { label }), "", { level: "success" });
  }
}

/** 列表顶上粘着的段头（字跟着滚改）。 */
function pinnedSection(): HTMLElement {
  const el = sectionHead("");
  el.dataset.pin = "true";
  return el;
}

/** 这一列的几何：项都是定高（按时间 52 · 段头 32；按项目 30），段头落在第几项之前由后端给 ⇒ 第几项在多高、多高是第几项都算得出。 */
interface Geometry {
  /** 整列多高。 */
  height: number;
  /** 第 `from` 项起的那一窗摆在多高（连它前面落着的段头）。 */
  blockTop(from: number): number;
  /** 这个高度上是第几项（夹在 0 与末项之间）。 */
  lineAt(y: number): number;
}

function geometry(by: ViewMode, total: number, sections: readonly HistorySection[]): Geometry {
  const r = by === "time" ? ROW_H : COMPACT_H;
  const h = by === "time" ? SEC_H : 0;
  const before = (i: number): number => sections.filter((x) => x.at < i).length;
  return {
    height: total * r + h * sections.length,
    blockTop: (from) => from * r + h * before(from),
    lineAt: (y) => {
      let i = 0;
      let top = 0;
      for (const x of sections) {
        const end = top + (x.at - i) * r;
        if (y < end) break;
        top = end + h;
        i = x.at;
      }
      return Math.max(0, Math.min(total - 1, i + Math.floor(Math.max(0, y - top) / r)));
    },
  };
}

/** 看得见的第 `first` 到第 `last` 项 ⇒ 要的那一窗（上下各多带 [`PAD`] 项）。 */
function windowAround(first: number, last: number): { from: number; limit: number } {
  const from = Math.max(0, first - PAD);
  return { from, limit: Math.min(WINDOW_MAX, last - from + 1 + PAD) };
}

/** 列表里走行的键 ⇒ 走到第几行（未夹到两头）；↑ 在第一行 ⇒ 回搜索框；不是走行的键 ⇒ `null`。 */
function navTarget(key: string, i: number, n: number, page: number): number | "search" | null {
  switch (key) {
    case "ArrowDown":
      return i + 1;
    case "ArrowUp":
      return i <= 0 ? "search" : i - 1;
    case "Home":
      return 0;
    case "End":
      return n - 1;
    case "PageDown":
      return i + page;
    case "PageUp":
      return i - page;
    default:
      return null;
  }
}

/** 筛选浮层里的一组：小标题 ＋ 几格。 */
function filterGroup(title: string, ...items: HTMLElement[]): HTMLElement {
  const g = document.createElement("div");
  g.className = s.hvFilterGroup;
  const h = document.createElement("div");
  h.className = s.hvFilterHead;
  h.textContent = title;
  g.append(h, ...items);
  return g;
}

function radioSet<T extends string | number>(name: string, cur: T, opts: [T, string][], set: (v: T) => void): HTMLElement[] {
  return opts.map(([v, label]) => radio(name, label, v === cur, () => set(v)));
}

/** 筛选浮层：机器 · 时间 · 排序 · 显示已隐藏 · 搜内容时谁说的 ＋ 含工具输出与思考。勾的是 `p` 此刻的值，改了交给 `on`。 */
function filterPanel(
  p: Prefs,
  machines: HTMLElement[],
  on: { time(v: Prefs["withinDays"]): void; sort(v: Prefs["sort"]): void; hidden(on: boolean): void; scope(v: Scope): void; tools(on: boolean): void },
): HTMLElement {
  const panel = document.createElement("div");
  panel.className = s.hvFilter;
  panel.append(
    filterGroup(copyText("history.filter.machines"), ...machines),
    filterGroup(
      copyText("history.filter.time"),
      ...radioSet<0 | 7 | 30>("hv-time", p.withinDays, [
        [0, copyText("history.filter.timeAll")],
        [7, copyText("history.filter.time7d")],
        [30, copyText("history.filter.time30d")],
      ], on.time),
    ),
    filterGroup(
      copyText("history.filter.sort"),
      ...radioSet<Prefs["sort"]>("hv-sort", p.sort, [
        ["activity", copyText("history.filter.sortActivity")],
        ["created", copyText("history.filter.sortCreated")],
      ], on.sort),
    ),
    filterGroup("", checkbox(copyText("history.filter.showHidden"), p.hidden, on.hidden)),
    filterGroup(
      copyText("history.filter.scope"),
      ...radioSet<Scope>("hv-scope", p.scope, [
        ["all", copyText("history.filter.scopeAll")],
        ["user", copyText("history.filter.scopeYou")],
        ["assistant", copyText("history.filter.scopeAgent")],
        ["report", copyText("history.filter.scopeReport")],
      ], on.scope),
      checkbox(copyText("history.filter.tools"), p.tools, on.tools),
    ),
  );
  return panel;
}

/** 列表与内容之间那一道：拖改列表宽（320–640），← → 一步 16；松手 / 按键之后交 `save`。`leftEdge` = 列表那一栏的左边。 */
function listSizer(root: HTMLElement, initial: number, leftEdge: () => number, save: (w: number) => void): HTMLElement {
  const el = document.createElement("div");
  el.className = s.hvSizer;
  el.tabIndex = 0;
  el.setAttribute("role", "separator");
  el.setAttribute("aria-orientation", "vertical");
  el.setAttribute("aria-label", copyText("history.page.resize"));
  el.setAttribute("aria-valuemin", String(LIST_MIN));
  el.setAttribute("aria-valuemax", String(LIST_MAX));
  const set = (w: number, keep: boolean): void => {
    const v = Math.round(Math.max(LIST_MIN, Math.min(LIST_MAX, w)));
    root.style.setProperty("--hv-list-w", `${v}px`);
    el.setAttribute("aria-valuenow", String(v));
    if (keep) save(v);
  };
  el.setAttribute("aria-valuenow", String(initial));
  el.addEventListener("pointerdown", (ev) => {
    ev.preventDefault();
    el.setPointerCapture?.(ev.pointerId);
    el.dataset.grabbed = "true";
    const left = leftEdge();
    const move = (e: PointerEvent): void => set(e.clientX - left, false);
    const up = (e: PointerEvent): void => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      delete el.dataset.grabbed;
      set(e.clientX - left, true);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  });
  el.addEventListener("keydown", (ev) => {
    if (ev.key !== "ArrowLeft" && ev.key !== "ArrowRight") return;
    ev.preventDefault();
    const now = Number(el.getAttribute("aria-valuenow"));
    set(now + (ev.key === "ArrowLeft" ? -LIST_STEP : LIST_STEP), true);
  });
  return el;
}

function groupKey(g: HistoryGroup): string {
  return `${g.origin ?? ""}\u0000${g.key}`;
}

/** 内容命中那一行 ⇒ 画它的那一行：名字 · 星标 · 隐藏 · 状态 · 能做什么都是那台在命中行上带来的（界面不拿清单去对）。 */
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
    label: sh.label,
    untitled: false,
    startedAt: sh.updatedAt,
    updatedAt: sh.updatedAt,
    at: sh.updatedAt,
    atText: sh.atText ?? "",
    sectionText: "",
    spanText: sh.spanText ?? "",
    jsonlPath: sh.jsonlPath,
    messageCountApprox: 0,
    isBg: sh.isBg,
    starred: sh.starred,
    customTitle: null,
    hidden: sh.hidden,
    face: sh.face,
    can: sh.can,
    ...(sh.origin ? { origin: sh.origin } : {}),
  };
}
