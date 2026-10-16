/**
 * 标签页栏视图：栏顶一排（全局入口 · 刷新）· 「需手动 N」· 机器离线条 · 列表（分组 ＋ 每个会话一行）。
 *
 * 一行：状态点 · 窄窗两字母 · 机器徽标（远端）· 项目名 ＋ 标题（后台多一个齿轮）· 行尾（等批准 / 未读数 · 停发 ·
 * 与默认不同的账号头像 · 图钉）· 悬停时盖在行尾的动作（目录 · ↗ · 更多 / 关闭）。悬停卡（kit 悬停提示的卡式）锚在行右侧。
 * 一句话怎么写全从 `session-face.ts` 取（状态点 · 状态句 · peek · 需手动）；这里只排。
 *
 * 只画、只把用户手势转交出去：点按钮切 tab、按下起拖、右键开菜单、子动作按钮 —— 做事的都经
 * `TabBarViewHost` 交给宿主（路由 / 拖拽 / 菜单 / 会话动作），本文件不 import 它们。
 * 读的状态：`TabStore`（tab 集合 · 顺序 · 当前 tab · 账号快照）与 `TabBarPrefs`（集合）。
 */
import { holdOf } from "./rot-words";
import { accountLabel } from "./acct-words";
import qs from "./tab-quota.module.css";
import { sessionBadge, shouldShowAccountBadge, detectAccountMismatch } from "./accounts";
import { appStore } from "./app-store";
import { accountAvatarEl, accountColorOf } from "./account-color";
import type { TabCollection } from "./tab-collections";
import { barRows, type BarRow, type RowRect } from "./tab-drop";
import { activityFace } from "./session-status";
import { terminalFrontAvailable } from "./terminal-front";
import { isRemoteOrigin } from "./ipc/origin";
import { closesWithoutMenu, hasTerminal, isLive, stateView } from "./tab-session-state";
import type { Tab } from "./tab-model";
import type { TabStore } from "./tab-store";
import type { TabBarPrefs } from "./tab-bar-prefs";
import { dispatcher, KeybindingDispatcher, type OverlayHandle } from "./keybindings/registry";
import rs from "./tab-group-rename.module.css";
import { copyText } from "./copy-table";
import { icon } from "./kit/icon";
import { statusDot } from "./kit/status-dot";
import { countBadge, tag, kbd } from "./kit/badge";
import { attachTooltip, delegateTooltip } from "./kit/tooltip";
import { openMenu, type MenuAnchor, type MenuItem } from "./kit/menu";
import { closePopover, openPopover, popoverOpenOn, swapPopoverContent } from "./kit/popover";
import { renderNeedsPanel, type PanelFacts, type PanelPlan } from "./needs-panel";
import type { ManualBook } from "./manual-book";
import type { ManualItem } from "./login-reads";
import type { Origin } from "./ipc/origin";
import { foldCaret } from "./kit/fold";
import { fmtDur } from "./duration-format";
import { abbrOf, dotOf, fullTitle, groupSummary, machineOf, needsOf, nextNeedsStep, peekLine, stateLine, stateWord, titleParts, sinceText } from "./session-face";
import { orderNeeds } from "./needs-order";

/** TabButton 的 DOM 引用：refreshTabBar 局部更新依赖这些 ref 避免重新创建 button */
export interface TabButtonRefs {
  root: HTMLButtonElement;
  /** 状态点（kit 的 status-dot）。 */
  dot: HTMLSpanElement;
  /** 窄窗那一格的两个字母。 */
  abbr: HTMLSpanElement;
  /** 远端的机器徽标（本机藏着）。 */
  machine: HTMLSpanElement;
  /** 标题前的项目名（`--text-2`）。 */
  proj: HTMLSpanElement;
  /** 后台会话（分身）的齿轮。 */
  bg: HTMLSpanElement;
  /** 标题正文那一格。 */
  label: HTMLSpanElement;
  /** 行尾「等批准 / 等回答 / 需手动」（琥珀字，换掉未读数）。 */
  needs: HTMLSpanElement;
  badge: HTMLSpanElement;
  /** 额度：被卡住的会话标题后那一格红字 `✕ 5h`（能发时藏着）。 */
  /** 账号徽章（该会话属于哪个账号；本机会话不显示，未知显 —）。 */
  acctBadge: HTMLSpanElement;
  /** 停发的会话标题后琥珀字 `停发`（与「等批准」同位）。 */
  holdBadge: HTMLSpanElement;
  cwdBtn: HTMLSpanElement;
  /**
   * 📌 角标。**纯展示，不可点** —— 与 `.tab-badge`（未读数）同族。
   * 固定是右键菜单那一项的事；这里再放一个能点的东西就是同一个动作两个入口。
   */
  pinBadge: HTMLSpanElement;
  /**
   * 这颗按钮上次**画出去的样子**（class 开关 ＋ title ＋ 标题 ＋ 未读数拼成一串）。
   * 一样 ⇒ `updateTabButton` 一个 DOM 都不写。按**输出**比而不是按输入比：输入散在 store 的好几张表里，
   * 漏一个就是静默不刷；输出只有这几项。`null` = 还没画过。
   */
  drawn: string | null;
  /**
   * 账号徽章上次画出去的样子：`""` = 藏着（`createTabButton` 建出来就是藏着的，所以初值是 `""`
   * 而不是 `null` —— 新 tab 没有账号时一个 DOM 都不写）；否则是「账号名 · 幽灵态 · 提示」拼成的一串。
   */
  acctDrawn: string;
}

/** tab 栏上的手势要交给谁（全是回调）。 */
export interface TabBarViewHost {
  /** 整刷 tab 栏的入口（`TabManager.refreshTabBar`：先过拖拽守卫再调本类 `refresh`）。 */
  refreshTabBar(): void;
  /** 组的菜单「关闭组里已结束的」：一次关掉、给一条可撤的 toast。 */
  closeEndedIn(gid: string): void;
  /** 📂：打开工作目录。 */
  openTabCwd(sid: string): Promise<void>;
  /** ↗：本机 tab 切到终端窗口。 */
  bringTerminalToFront(sid: string): Promise<void>;
  /** ↗：远端 tab 切到终端窗口。 */
  bringRemoteTerminalToFront(sid: string): Promise<void>;
  /** × / 中键：关（只关得掉已结束的）。 */
  closeTab(sid: string): void;
  /** 点按钮：手动切 tab。 */
  switchTo(sid: string): void;
  /** 从「需手动」的入口（Ctrl+J · 栏顶那一条与它的菜单 · 标签上那颗需手动的点）跳过去：切 tab、焦点进答题块。 */
  answerAt(sid: string): void;
  /**
   * 点按钮时的多选（`tab-selection.ts`）：`plain` 单击（清多选、随后切 tab）· `toggle` Ctrl 单击 · `range` Shift 单击 ·
   * `clear` 单击条上空白。
   */
  pick(sid: string | null, how: "plain" | "toggle" | "range" | "clear"): void;
  /** 这个 tab 在不在多选里（画 `.selected`）。 */
  isSelected(sid: string): boolean;
  /** 左键按下：候选拖拽。 */
  beginDrag(e: MouseEvent, sid: string, root: HTMLElement): void;
  /** 拖完那一下的 click 不切 tab（一次性消费）。 */
  takeSuppressedClick(sid: string): boolean;
  /** 按住组头：候选拖整个组。 */
  beginGroupDrag(e: MouseEvent, gid: string, head: HTMLElement): void;
  /** 拖整组完那一下的 click 不收展、不改名（一次性消费）。 */
  takeSuppressedHeadClick(gid: string): boolean;
  /** 右键：开这个 tab 的菜单。 */
  openMenu(e: MouseEvent, sid: string): void;
  /** 栏里键盘 Space：选中 / 取消（同 Ctrl 点）。 */
  toggleSelect(sid: string): void;
  /** 栏顶「刷新」：有打开 tab 的每台对齐 ＋ 补读一次；做完才 resolve。 */
  rereadAll(): Promise<void>;
  /** 「需手动」那几个会话的先后：问后端（各台排好的、跨台本机合；`needs-order.ts::askNeedsOrder`）。 */
  needsOrder(origins: readonly Origin[]): Promise<string[]>;
  /** 行尾「更多」：开这个 tab 的菜单（锚在那颗按钮上）。 */
  openMenuAt(anchor: HTMLElement, sid: string): void;
  /** 离线条的［重新连接］：那台断着在退避里等 ⇒ 立刻重拨。 */
  reconnect(origin: string): void;
}

/** 一个组在栏里的那几块（组头各格 ＋ 组员列表）；`drawn` ＝ 组头上次画的样子（没变就不写）。 */
interface GroupEls {
  wrap: HTMLElement;
  head: HTMLElement;
  list: HTMLElement;
  caret: HTMLElement;
  name: HTMLElement;
  count: HTMLElement;
  sum: HTMLElement;
  mini: HTMLElement;
  drawn: string;
}

/** 栏里的一行（键盘焦点落在哪）：标签页 · 组头。 */
/** 计划那一侧的「需手动」：几条 · 此刻站在第几条（计划页开着且选着那一条；否则 `null`）· 去第几条 · 按片列出（浮层用）。 */
export interface PlanNeeds {
  count: () => number;
  at: () => number | null;
  open: (i: number) => void;
  slices: () => PanelPlan[];
}

export type FocusRow = { kind: "tab"; sid: string } | { kind: "head"; gid: string };

/** 这个事件落在会打字的框里（组头改名的输入框）⇒ 列表键不管。 */
function isTyping(t: EventTarget | null): boolean {
  return t instanceof HTMLElement && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable);
}

/** 栏收成 44px 时（`tab-bar-fold.ts`）悬停卡宽 260。 */
const folded = (): boolean => document.body.dataset.tabBar === "folded";

export class TabBarView {
  /** 起步那一批已经画出去了（之后新出现的行淡入）。 */
  private painted = false;
  /** sessionId → button DOM refs，避免 refreshTabBar 每次重建整个 bar */
  readonly tabButtons = new Map<string, TabButtonRefs>();
  /** 每个集合在主栏里的容器（组头 + 成员列表）。 */
  private readonly groupEls = new Map<string, GroupEls>();
  /** 组头 → 组 id（按下组头起拖时从事件找回是哪个组）。 */
  private readonly headGid = new WeakMap<Element, string>();
  // 没有归档抽屉：已结束的 tab（`isResumeOnly(tab.state)`）留在原位变淡（`.tab.ended`）。

  /**
   * 按钮根 → sid。事件委托靠它从 `closest(".tab")` 找回 sid（不往 DOM 上加属性）。
   * `WeakMap`：按钮被摘掉之后这一条跟着它一起被回收，不用另外清。
   */
  private readonly sidOf = new WeakMap<Element, string>();

  /** 栏顶一排：全局入口（宿主经 [`mountHeadActions`] 交进来）· 刷新。 */
  private readonly headEl: HTMLDivElement;
  private readonly headActs: HTMLSpanElement;
  /** 栏顶右端「刷新」：点击走下面那个委托的 click；在飞时 `disabled`，不重入。 */
  private readonly rereadBtn: HTMLButtonElement;
  /** 「需手动 N」：有人在等你才出。 */
  private readonly needsEl: HTMLButtonElement;
  private readonly needsCount: HTMLSpanElement;
  /** 「需手动」那一条右端的键帽（按当前键位现拼，出现时才拼）。 */
  private readonly needsKbd: HTMLSpanElement;
  /** 机器离线条（一台一条）。 */
  private readonly downEl: HTMLDivElement;
  /** 列表：分组 ＋ 每个会话一行（滚动的是它）。 */
  readonly listEl: HTMLDivElement;
  /** 起新会话之后的占位标签页那几行（`launch-slot.ts` 往里放）：恒在列表最末。 */
  readonly slotTail: HTMLDivElement;
  /** 机器 → 它看不见是从何时起（`markOriginDown`）；连上了摘掉。 */
  private readonly downSince = new Map<string, number>();

  constructor(
    private readonly store: TabStore,
    private readonly prefs: TabBarPrefs,
    barEl: HTMLElement,
    private readonly host: TabBarViewHost,
  ) {
    // 事件委托：整条栏只在 `barEl` 上挂三个监听器。子按钮上：
    // ① 按下不起拖、不触发中键关 ⇒ mousedown 先看目标在不在子按钮里；
    // ② click / mousedown 不往上冒到 `document` ⇒ 这里 `stopPropagation()`（`barEl` 与按钮之间只隔组容器，组容器上没有这两个监听）。
    barEl.addEventListener("click", (e) => this.onBarClick(e));
    barEl.addEventListener("mousedown", (e) => this.onBarMouseDown(e));
    barEl.addEventListener("contextmenu", (e) => this.onBarContextMenu(e));
    barEl.addEventListener("keydown", (e) => this.onBarKeyDown(e));
    // 按钮上的 Space 在松开时点它（会切过去）：栏里 Space 是「选中」，松开那一下也吞掉。
    barEl.addEventListener("keyup", (e) => {
      if (e.key === " " && this.rowOf(e.target) !== null && !isTyping(e.target)) e.preventDefault();
    });
    this.headEl = document.createElement("div");
    this.headEl.className = "tab-bar-head";
    this.headActs = document.createElement("span");
    this.headActs.className = "tab-bar-head-acts";
    const sp = document.createElement("span");
    sp.className = "tab-bar-head-sp";
    this.rereadBtn = document.createElement("button");
    this.rereadBtn.type = "button";
    this.rereadBtn.className = "tab-bar-reread tab-bar-icon";
    this.rereadBtn.setAttribute("aria-label", copyText("tabBar.head.refresh"));
    this.rereadBtn.appendChild(icon("refresh"));
    attachTooltip(this.rereadBtn, () => copyText("tabBar.reread.hint"));
    this.headEl.append(this.headActs, sp, this.rereadBtn);

    this.needsEl = document.createElement("button");
    this.needsEl.type = "button";
    this.needsEl.className = "tab-needs";
    this.needsEl.style.display = "none";
    const needsDot = statusDot("needs-you", copyText("sessionFace.dot.needs"), "compact");
    const needsText = document.createElement("span");
    needsText.className = "tab-needs-text";
    needsText.textContent = copyText("tabBar.needs.label");
    this.needsCount = document.createElement("span");
    this.needsCount.className = "tab-needs-count";
    const needsSp = document.createElement("span");
    needsSp.className = "tab-bar-head-sp";
    this.needsKbd = document.createElement("span");
    this.needsKbd.className = "tab-needs-kbd";
    this.needsEl.append(needsDot, needsText, this.needsCount, needsSp, this.needsKbd);
    this.needsEl.setAttribute("aria-haspopup", "dialog");
    this.needsEl.setAttribute("aria-expanded", "false");

    this.downEl = document.createElement("div");
    this.downEl.className = "tab-machine-down";

    this.listEl = document.createElement("div");
    this.listEl.className = "tab-list";
    this.slotTail = document.createElement("div");
    this.listEl.appendChild(this.slotTail);
    barEl.prepend(this.headEl, this.needsEl, this.downEl, this.listEl);
    // 悬停卡与行尾动作的悬停提示都委托在列表上：一行零个监听器（与点击 / 拖拽同一条规矩）。
    delegateTooltip(this.listEl, ".tab", (el) => {
      const sid = this.sidOf.get(el);
      return sid === undefined ? null : this.hoverCard(sid);
    }, { placement: "right", hold: true, width: () => (folded() ? 260 : 300) });
    delegateTooltip(this.listEl, ".tab-cwd, .tab-focus, .tab-more, .tab-close", (el) => actHint(el));
  }

  /** 栏顶左边那几颗全局入口（会话总览 · 文件 · 历史 · 设置）：宿主建好交进来，这里只摆。 */
  mountHeadActions(buttons: HTMLElement[]): void {
    this.headActs.replaceChildren(...buttons);
  }

  /** 那台看不见了（会话流 `unseen` 格）⇒ 记下从何时起；连上了（报完清单）⇒ 摘掉。 */
  markOriginDown(origin: string, down: boolean, at: number = Date.now()): void {
    if (down) {
      if (!this.downSince.has(origin)) this.downSince.set(origin, at);
    } else this.downSince.delete(origin);
  }

  /**
   * 计划那一侧的「需手动」（计划页挂进来）：几条 · 此刻站在第几条（计划页开着且选着那一条；否则 `null`）· 去第几条。
   * 会话在前、计划项在后；计划项的数进「需手动 N」与窗口标题，不发系统通知。
   */
  plan: PlanNeeds = { count: () => 0, at: () => null, open: () => {}, slices: () => [] };

  /**
   * 会话外的「需手动」（宿主挂进来）：每台 `manual-list` 的账 · 点那一行开登录抽屉。条数进「需手动 N」与窗口标题，不进 `Ctrl+J`、不发系统通知。
   */
  manual: { book: ManualBook | null; login: (origin: Origin, item: ManualItem) => void } = { book: null, login: () => {} };

  /** 「需手动」的下一站（`Ctrl+J` · 点「需手动」那一条）：会话在前（先后先问后端）、计划项在后。没有 ⇒ 什么都不做。 */
  async nextNeeds(): Promise<void> {
    const order = await this.rankNeeds();
    const at = this.plan.at();
    const step = nextNeedsStep(order, at === null ? this.store.activeId : null, at, this.plan.count());
    if (step === null) return;
    if ("sid" in step) this.host.answerAt(step.sid);
    else this.plan.open(step.plan);
  }

  /** 后端上一回排好的「需手动」先后（`Ctrl+J` · 浮层开的那一下重问；问不到 ⇒ 照上一回的）。 */
  private needsRanked: string[] = [];

  /** 条上在等的那几个，按条上的顺序。 */
  private needingTabs(): Tab[] {
    return this.visibleOrder()
      .map((sid) => this.store.tabs.get(sid))
      .filter((t): t is Tab => t !== undefined && needsOf(t) !== null);
  }

  /** 问一回后端那份先后（只问有人在等的那几台），回条上在等的那几个照它排好的。 */
  private async rankNeeds(): Promise<string[]> {
    const origins = [...new Set(this.needingTabs().map((t) => t.origin))];
    if (origins.length > 0) {
      try {
        this.needsRanked = await this.host.needsOrder(origins);
      } catch (e) {
        console.warn("[needs] 需手动的先后问不到，照上一回的：", e);
      }
    }
    return orderNeeds(this.needingTabs(), this.needsRanked);
  }

  /** 此刻「需手动」的数：需要处理的会话 ＋ 计划项 ＋ 会话外的每一项（窗口标题用，与入口同一个数）。 */
  needsCountNow(): number {
    let n = this.plan.count() + (this.manual.book?.count() ?? 0);
    for (const t of this.store.tabs.values()) if (needsOf(t)) n++;
    return n;
  }

  /** 计划那一侧 · 会话外那一份的数变了 ⇒ 重画「需手动 N」（浮层开着 ⇒ 原地换内容）。 */
  planNeedsChanged(): void {
    this.updateNeedsStrip();
    this.repaintNeedsPanel();
  }

  /** 点「需手动」：开 / 关浮层。开的那一下各台的会话外清单重问一次（问回来之前照上一份画）。 */
  toggleNeedsPanel(): void {
    if (popoverOpenOn(this.needsEl)) {
      closePopover();
      return;
    }
    const content = this.needsPanel(0);
    openPopover(this.needsEl, content, {
      label: copyText("tabBar.needs.label"),
      align: "start",
      onClose: () => {
        this.needsEl.classList.remove("is-open");
        this.updateNeedsStrip();
      },
    });
    this.needsEl.classList.add("is-open");
    content.focus();
    void this.manual.book?.again();
    void this.rankNeeds().then(() => this.repaintNeedsPanel());
  }

  /** 浮层那一份内容（`at` ＝ 键盘选着第几行）。 */
  private needsPanel(at: number): HTMLElement {
    const facts = this.needsFacts();
    this.needsDrawn = JSON.stringify(facts);
    const root = this.buildNeedsPanel(facts, at);
    this.needsContent = root;
    return root;
  }

  /** 浮层此刻那一份内容（原地换时从它取选着的那一行）· 画它用的那一份事实（一字不差 ⇒ 不重画）。 */
  private needsContent: HTMLElement | null = null;
  private needsDrawn = "";

  private needsFacts(): PanelFacts {
    const now = Date.now();
    const sessions = orderNeeds(this.needingTabs(), this.needsRanked).map((sid) => {
      const t = this.store.tabs.get(sid)!;
      return { sid, origin: t.origin, title: fullTitle(t), state: stateLine(t, now).text, what: needsOf(t)?.what ?? null };
    });
    return { sessions, plans: this.plan.slices(), manual: this.manual.book?.states() ?? [] };
  }

  private buildNeedsPanel(facts: PanelFacts, at: number): HTMLElement {
    const close = (): void => closePopover();
    return renderNeedsPanel(
      facts,
      {
        answer: (sid) => {
          close();
          this.host.answerAt(sid);
        },
        plan: (i) => {
          close();
          this.plan.open(i);
        },
        login: (origin, item) => {
          close();
          this.manual.login(origin, item);
        },
      },
      at,
    ).root;
  }

  /** 浮层开着、数据变了 ⇒ 原地换内容（选着的那一行下标照留；焦点留在浮层里）。 */
  private repaintNeedsPanel(): void {
    if (!popoverOpenOn(this.needsEl)) return;
    if (JSON.stringify(this.needsFacts()) === this.needsDrawn) return;
    const old = this.needsContent;
    const at = Number(old?.dataset.at ?? 0);
    const focused = old !== null && old.contains(document.activeElement);
    const next = this.needsPanel(Number.isFinite(at) && at >= 0 ? at : 0);
    swapPopoverContent(this.needsEl, next);
    if (focused) next.focus();
  }

  private reread(): void {
    const btn = this.rereadBtn;
    if (btn.disabled) return;
    btn.disabled = true;
    void this.host.rereadAll().finally(() => {
      btn.disabled = false;
    });
  }

  /** 这个事件落在哪颗 tab 按钮上、是不是落在它的某颗子按钮（📂 / ↗ / ×）上。不是 tab 按钮 ⇒ `null`。 */
  private hitOf(e: Event): { sid: string; root: HTMLElement; sub: Element | null } | null {
    const t = e.target as Element | null;
    if (!t || typeof t.closest !== "function") return null;
    const root = t.closest(".tab");
    if (!root) return null;
    const sid = this.sidOf.get(root);
    if (sid === undefined) return null;
    const sub = t.closest(".tab-cwd, .tab-focus, .tab-close, .tab-more");
    return { sid, root: root as HTMLElement, sub: sub && root.contains(sub) ? sub : null };
  }

  /** 这颗 tab 行尾的 ↗（结果浮层锚在它上面）；不在条上 / 不是 Windows ⇒ `null`。 */
  frontButton(sid: string): HTMLElement | null {
    return this.tabButtons.get(sid)?.root.querySelector<HTMLElement>(".tab-focus") ?? null;
  }

  /** ↗ 在飞超过一会儿 ⇒ 这颗 tab 的 ↗ 进「进行中」（转圈、不可再点由 `frontOnce` 挡）。 */
  setFrontPending(sid: string, on: boolean): void {
    this.tabButtons.get(sid)?.root.querySelector(".tab-focus")?.classList.toggle("in-flight", on);
  }

  private onBarClick(e: MouseEvent): void {
    if (e.target instanceof Node && this.rereadBtn.contains(e.target)) {
      this.reread();
      return;
    }
    if (e.target instanceof Node && this.needsEl.contains(e.target)) {
      this.toggleNeedsPanel();
      return;
    }
    if (e.target instanceof Element && typeof e.target.closest === "function") {
      const re = e.target.closest<HTMLElement>("[data-reconnect]");
      if (re && this.downEl.contains(re)) {
        this.host.reconnect(re.dataset.reconnect ?? "");
        return;
      }
    }
    // 栏顶那一排、需手动、离线条之外的才算「条上」：点在列表外不清多选。
    if (!(e.target instanceof Node && this.listEl.contains(e.target))) return;
    const hit = this.hitOf(e);
    if (!hit) {
      // 条上空白（不是 tab、不是组头）⇒ 清掉多选。
      const t = e.target as Element | null;
      if (!(t && typeof t.closest === "function" && t.closest(".tab-group-head"))) this.host.pick(null, "clear");
      return;
    }
    const { sid, sub } = hit;
    if (sub) {
      e.stopPropagation();
      if (sub.classList.contains("tab-cwd")) {
        // 📂 打开工作目录（cwd）—— 系统默认文件管理器
        void this.host.openTabCwd(sid);
      } else if (sub.classList.contains("tab-more")) {
        this.host.openMenuAt(sub as HTMLElement, sid);
      } else if (sub.classList.contains("tab-focus")) {
        // ↗ 拉对应终端窗口。非 Windows 不渲（`terminal-front.ts`）——不渲就点不到。
        const t = this.store.tabs.get(sid);
        if (!t || !hasTerminal(t.state)) return; // 活着，或可重连（ssh 窗还在）才拉
        // 远端 Tab → 点那一刻现查此刻显示它的本机终端；
        // 本地 Tab → 走原 sid_hwnd_cache 路径。
        if (isRemoteOrigin(t.origin)) {
          void this.host.bringRemoteTerminalToFront(sid);
        } else {
          void this.host.bringTerminalToFront(sid);
        }
      } else {
        this.host.closeTab(sid);
      }
      return;
    }
    // 拖拽刚结束的那次 click 不切 Tab（drag-then-release ≠ 选中）。一次性消费。
    if (this.host.takeSuppressedClick(sid)) return;
    if (e.ctrlKey || e.metaKey) {
      this.host.pick(sid, "toggle");
      return;
    }
    if (e.shiftKey) {
      this.host.pick(sid, "range");
      return;
    }
    this.host.pick(sid, "plain");
    const t = this.store.tabs.get(sid);
    const onDot = e.target instanceof Element && typeof e.target.closest === "function" && e.target.closest(".tab-dot") !== null;
    if (onDot && t && needsOf(t)) this.host.answerAt(sid);
    else this.host.switchTo(sid);
  }

  /** 条上的顺序（组与散的混排、组员聚在第一个组员那一格；整刷就按它摆）。含收着的组里的。 */
  visibleOrder(): string[] {
    return this.store.visibleOrder(this.prefs.collections.map((c) => c.id));
  }

  /** 「已隐藏」那一行展开着没有（`tabBar.hiddenOpen`；缺省收着）。 */
  private hiddenOpen(): boolean {
    return this.prefs.hiddenOpen === true;
  }

  /** 数字键 · `]` `[` 走的顺序：同上，但跳过收着的组里的（看不见的不走）。 */
  navOrder(): string[] {
    const folded = new Set(this.prefs.collections.filter((c) => c.collapsed).map((c) => c.id));
    return this.store.visibleOrder(this.prefs.collections.map((c) => c.id), folded, !this.hiddenOpen());
  }

  /** 栏里从上到下的行（组头 · 标签页；收着的组里的标 hidden）。键盘挪位按它算（`tab-drop.ts::keyMoveTarget`）。 */
  rows(): BarRow[] {
    return barRows(
      this.store.barIds(),
      (sid) => this.store.tabs.get(sid)?.group ?? null,
      this.prefs.collections.map((c) => c.id),
      new Set(this.prefs.collections.filter((c) => c.collapsed).map((c) => c.id)),
    );
  }

  /** 事件落在栏里哪一行上（标签页 · 组头）；别处 ⇒ `null`。 */
  private rowOf(target: EventTarget | null): FocusRow | null {
    const t = target as Element | null;
    if (!t || typeof t.closest !== "function" || !this.listEl.contains(t)) return null;
    const tab = t.closest(".tab");
    const sid = tab ? this.sidOf.get(tab) : undefined;
    if (sid !== undefined) return { kind: "tab", sid };
    const head = t.closest(".tab-group-head");
    const gid = head ? this.headGid.get(head) : undefined;
    return gid !== undefined ? { kind: "head", gid } : null;
  }

  /** 焦点此刻在栏里哪一行上；不在栏里 ⇒ `null`。 */
  focusedRow(): FocusRow | null {
    return this.rowOf(document.activeElement);
  }

  /** 把焦点放到这一行上（标签页那颗按钮 · 组头）；那一行不在栏里 / 看不见 ⇒ `false`。 */
  focusRow(row: FocusRow): boolean {
    const el = row.kind === "tab" ? this.tabButtons.get(row.sid)?.root : this.groupEls.get(row.gid)?.head;
    if (!el || !this.listEl.contains(el) || (row.kind === "tab" && this.isHiddenTab(row.sid))) return false;
    el.focus();
    return document.activeElement === el;
  }

  /** 焦点进栏（F6）：落在当前标签页那一行；它在收着的组里 ⇒ 组头；都没有 ⇒ 第一行。 */
  focusBar(): boolean {
    const rows = this.rows();
    const sid = this.store.activeId;
    const cur = rows.find((r) => r.kind === "tab" && r.sid === sid);
    if (cur?.kind === "tab") {
      if (!cur.hidden && this.focusRow({ kind: "tab", sid: cur.sid })) return true;
      if (cur.gid !== null && this.focusRow({ kind: "head", gid: cur.gid })) return true;
    }
    const first = rows.find((r) => !(r.kind === "tab" && r.hidden));
    return first !== undefined && this.focusRow(first.kind === "tab" ? { kind: "tab", sid: first.sid } : { kind: "head", gid: first.gid });
  }

  private isHiddenTab(sid: string): boolean {
    const gid = this.store.tabs.get(sid)?.group ?? null;
    return gid !== null && this.prefs.collections.some((c) => c.id === gid && c.collapsed === true);
  }

  /**
   * 栏里的列表键（焦点在某一行上；改名的输入框里不管）：↑↓ 走行（组头也是一行；收着的组只走组头）·
   * Enter 切过去 / 收展 · ← → 收展（组员上 ← 跳到组头）· Space 选中。挪位 · 改名 · 菜单键走快捷键表（`tabBar.*` · `tab.context-menu`）。
   */
  private onBarKeyDown(e: KeyboardEvent): void {
    if (e.ctrlKey || e.altKey || e.metaKey || isTyping(e.target) || e.isComposing) return;
    const row = this.rowOf(e.target);
    if (row === null) return;
    const vis = this.rows().filter((r) => !(r.kind === "tab" && r.hidden));
    const at = vis.findIndex((r) => (r.kind === "tab" ? row.kind === "tab" && r.sid === row.sid : row.kind === "head" && r.gid === row.gid));
    const to = (r: BarRow | undefined): void => {
      if (r) this.focusRow(r.kind === "tab" ? { kind: "tab", sid: r.sid } : { kind: "head", gid: r.gid });
    };
    const head = row.kind === "head" ? this.prefs.collections.find((c) => c.id === row.gid) : undefined;
    switch (e.key) {
      case "ArrowUp":
      case "ArrowDown":
        if (e.shiftKey) return;
        to(vis[at + (e.key === "ArrowUp" ? -1 : 1)]);
        break;
      case "Enter":
        // 组头里的那几颗按钮（⌄ · 名字 · ⋯）上的 Enter 归按钮自己（收展 · 改名 · 开菜单）。
        if (row.kind === "head" && e.target !== this.groupEls.get(row.gid)?.head) return;
        if (row.kind === "tab") {
          this.host.pick(row.sid, "plain");
          this.host.switchTo(row.sid);
        } else this.toggleGroup(row.gid);
        break;
      case "ArrowLeft":
        if (row.kind === "head") {
          if (head && head.collapsed !== true) this.toggleGroup(row.gid);
        } else {
          const gid = this.store.tabs.get(row.sid)?.group ?? null;
          if (gid !== null) this.focusRow({ kind: "head", gid });
          else return;
        }
        break;
      case "ArrowRight":
        if (row.kind !== "head") return;
        if (head?.collapsed === true) this.toggleGroup(row.gid);
        break;
      case " ":
        if (row.kind !== "tab") return;
        this.host.toggleSelect(row.sid);
        break;
      default:
        return;
    }
    e.preventDefault();
    e.stopPropagation();
  }

  /** 量一遍栏里每一行（组头 · 标签页）在纵轴上占的那一段：拖拽判落点用（判定在 `tab-drop.ts::pickDropTarget`）。 */
  measureRows(): RowRect[] {
    const out: RowRect[] = [];
    for (const [gid, g] of this.groupEls) {
      const r = g.head.getBoundingClientRect();
      out.push({ kind: "head", id: gid, gid, top: r.top, height: r.height, collapsed: g.wrap.classList.contains("is-collapsed") });
    }
    for (const [sid, refs] of this.tabButtons) {
      // 「已隐藏」那一行里的不是落点（不能往里拖）。
      if (!this.listEl.contains(refs.root) || this.store.tabs.get(sid)?.hidden != null) continue;
      const r = refs.root.getBoundingClientRect();
      const gid = this.store.tabs.get(sid)?.group ?? null;
      out.push({ kind: "tab", id: sid, gid: gid !== null && this.groupEls.has(gid) ? gid : null, top: r.top, height: r.height });
    }
    return out;
  }

  /** 组 `gid` 的整块 · 组头 · 组员列表（拖拽画目标框 · 点亮那一块 · 拖整组时变暗用）；没有 ⇒ `undefined`。 */
  groupParts(gid: string): { wrap: HTMLElement; head: HTMLElement; list: HTMLElement } | undefined {
    return this.groupEls.get(gid);
  }

  /** 刚建的组：组头名字框立刻打开、全选（打字即改；Esc 留默认名）。 */
  renameGroupNow(gid: string): void {
    const g = this.groupEls.get(gid);
    if (g) this.beginGroupRename(gid, g.head.querySelector<HTMLElement>(".tab-group-name")!);
  }

  private onBarMouseDown(e: MouseEvent): void {
    const hit = this.hitOf(e);
    if (!hit) {
      // 按住组头（「⋯」除外）⇒ 候选拖整个组。
      const t = e.target as Element | null;
      const head = t && typeof t.closest === "function" ? t.closest<HTMLElement>(".tab-group-head") : null;
      const gid = head ? this.headGid.get(head) : undefined;
      if (e.button === 0 && head && gid !== undefined && !t!.closest(".tab-group-more")) this.host.beginGroupDrag(e, gid, head);
      return;
    }
    // 子动作按钮自己处理点击：吞掉 mousedown 避免在它们身上起 Tab 拖拽（也不触发下面的中键关）。
    if (hit.sub) {
      e.stopPropagation();
      return;
    }
    if (e.button === 0) {
      // 左键 mousedown：候选 Tab 撕离拖拽（越过阈值才真拖，否则仍是普通 click）。隐藏的会话不是标签页：不起拖（点照常切过去）。
      if (this.store.tabs.get(hit.sid)?.hidden == null) this.host.beginDrag(e, hit.sid, hit.root);
      return;
    }
    // 中键点击已结束的 Tab 也关闭（常见 UX）；说不清的只在右键菜单里关
    if (e.button === 1) {
      const t = this.store.tabs.get(hit.sid);
      if (t && closesWithoutMenu(t.state)) {
        e.preventDefault();
        this.host.closeTab(hit.sid);
      }
    }
  }

  /** 右键菜单「在新窗口打开」（双屏 / 并排）。菜单里放哪几项住 `tab-menu.ts`。 */
  private onBarContextMenu(e: MouseEvent): void {
    const hit = this.hitOf(e);
    if (!hit) return;
    e.preventDefault();
    this.host.openMenu(e, hit.sid);
  }

  /**
   * 局部更新策略（避免每次 onLine 都 replaceChildren）：
   *   1. 删除：tabButtons 缓存里有但 orderedIds 已没的 sid → 摘 DOM + 清缓存
   *   2. 创建：orderedIds 里有但缓存没的 sid → createTabButton 一次（含所有 5 个子
   *      元素 + 事件 listener），visibility 全交 CSS 控制
   *   3. 更新：updateTabButton 同步 active / ended / has-unread class · data-light / data-reconnectable 钩子 + label/badge 文本
   *   4. 排序：iterate orderedIds + insertBefore，确保 DOM 顺序 = orderedIds 顺序
   *
   * CSS 配合（styles.css）：
   *   .tab.ended .live-dot { display: none }
   *   .tab:not(.ended) .tab-close { display: none }
   *   .tab .tab-badge { display: none }
   *   .tab.has-unread:not(.active) .tab-badge { display: inline-block }
   */
  refresh(): void {
    // 1. 删（画在栏上的 ＝ 占标签页的 ＋「已隐藏」那一行里的；隐藏了又结束的不画）
    const hiddenIds = this.store.hiddenRow();
    const wanted = new Set([...this.store.barIds(), ...hiddenIds]);
    for (const sid of Array.from(this.tabButtons.keys())) {
      if (!wanted.has(sid)) {
        const refs = this.tabButtons.get(sid)!;
        refs.root.remove();
        this.tabButtons.delete(sid);
      }
    }

    // 2 + 3 + 4. 创建 / 更新 / 排序：照 `barRows` 一趟摆 —— 组（组头 ＋ 组员）与散的混排在同一个顺序里，
    // 组在它第一个组员那一格（`tab-drop.ts`）。每容器一个游标：列表一个、当前这个组一个。
    // 组表里没了的组 ⇒ 摘掉它的容器（在栏里的最后一个离开 · 解散）。
    for (const [id, g] of this.groupEls) {
      if (!this.prefs.collections.some((x) => x.id === id)) {
        g.wrap.remove();
        this.groupEls.delete(id);
      }
    }
    const place = (host: HTMLElement, el: HTMLElement, prev: ChildNode | null): void => {
      const next = prev ? prev.nextSibling : host.firstChild;
      if (el !== next || el.parentElement !== host) host.insertBefore(el, next);
    };
    let cursor: ChildNode | null = null;
    let inGroup: ChildNode | null = null;
    const cols = new Map(this.prefs.collections.map((c) => [c.id, c]));
    for (const row of this.rows()) {
      if (row.kind === "head") {
        this.groupElFor(cols.get(row.gid)!);
        const wrap = this.groupEls.get(row.gid)!.wrap;
        place(this.listEl, wrap, cursor);
        cursor = wrap;
        inGroup = null;
        continue;
      }
      const sid = row.sid;
      const refs = this.buttonFor(sid);
      if (!refs) continue;
      if (row.gid === null) {
        place(this.listEl, refs.root, cursor);
        cursor = refs.root;
      } else {
        place(this.groupEls.get(row.gid)!.list, refs.root, inGroup);
        inGroup = refs.root;
      }
    }
    // 栏底：占位标签页那几行之后是「已隐藏」那一行（栏的最后）。
    const hiddenList = this.paintHiddenRow(hiddenIds);
    const tail: HTMLElement[] = [this.slotTail, ...(hiddenList && this.hiddenEls ? [this.hiddenEls.wrap] : [])];
    if (tail.some((el, i) => this.listEl.children[this.listEl.children.length - tail.length + i] !== el)) this.listEl.append(...tail);
    if (hiddenList) {
      let at: ChildNode | null = null;
      for (const sid of hiddenIds) {
        const refs = this.buttonFor(sid);
        if (!refs) continue;
        place(hiddenList, refs.root, at);
        at = refs.root;
      }
    }
    this.updateNeedsStrip();
    this.repaintNeedsPanel();
    this.updateMachineDown();
    // 起步那一批（重放）画完之后，再新长出来的行才淡入。
    if (!this.store.inBatch && this.tabButtons.size > 0) this.painted = true;

    this.store.notify();
  }

  /** 这个会话那一颗按钮：没有就建（建出来之后再长出来的淡入），照此刻的 tab 更新；tab 没了 ⇒ `null`。 */
  private buttonFor(sid: string): TabButtonRefs | null {
    const tab = this.store.tabs.get(sid);
    if (!tab) return null;
    let refs = this.tabButtons.get(sid);
    if (!refs) {
      refs = this.createTabButton(sid);
      this.tabButtons.set(sid, refs);
      // 条上已经有东西之后新出现的一行淡入（起步那一批不算）；减少动效时时长为 0。
      if (this.painted) {
        const el = refs.root;
        el.classList.add("tab-enter");
        el.addEventListener("animationend", () => el.classList.remove("tab-enter"), { once: true });
      }
    }
    this.updateTabButton(refs, sid, tab);
    return refs;
  }

  /**
   * 「需手动 N」：N ≥ 1 才出（浮层开着时照留：最后一项刚解决 ⇒ 去掉数字，关了才消失）。
   * 有哪台回了话、清单却无法读取 ⇒ 不带数字、虚线框（数说不准）。
   */
  private updateNeedsStrip(): void {
    const n = this.needsCountNow();
    const open = popoverOpenOn(this.needsEl);
    const unreadable = this.manual.book?.anyUnreadable() ?? false;
    this.needsEl.style.display = n === 0 && !open && !unreadable ? "none" : "";
    if (this.needsEl.classList.contains("is-unknown") !== unreadable) this.needsEl.classList.toggle("is-unknown", unreadable);
    if (n === 0 && !open && !unreadable) return;
    const chord = dispatcher.effectiveChord("needs.next");
    const pretty = chord ? KeybindingDispatcher.prettyChord(chord) : "";
    if (this.needsKbd.dataset.chord !== pretty) {
      this.needsKbd.dataset.chord = pretty;
      this.needsKbd.replaceChildren(...(pretty ? [kbd(pretty)] : []));
    }
    const shown = unreadable ? "" : String(n);
    if (this.needsCount.dataset.n !== shown) {
      this.needsCount.dataset.n = shown;
      this.needsCount.replaceChildren(...[unreadable ? null : countBadge(n, "warn")].filter((b): b is HTMLSpanElement => b !== null));
    }
    this.needsEl.setAttribute("aria-label", copyText("tabBar.needs.aria", { n }));
  }

  /** 机器离线条：看不见的那台、还有状态不明的会话 ⇒ 一条（`{machine} 离线 · N 会话状态不明 / 采样 3m 前［重新连接］`）。 */
  private updateMachineDown(): void {
    const now = Date.now();
    const rows: { origin: string; n: number; since: number }[] = [];
    for (const [origin, since] of this.downSince) {
      let n = 0;
      for (const t of this.store.tabs.values()) if (t.origin === origin && t.state.liveness === "unseen") n++;
      if (n > 0) rows.push({ origin, n, since });
    }
    const drawn = rows.map((r) => `${r.origin}\u0000${r.n}\u0000${sinceText(r.since, now)}`).join("\u0001");
    if (this.downEl.dataset.drawn === drawn) return;
    this.downEl.dataset.drawn = drawn;
    this.downEl.replaceChildren(
      ...rows.map((r) => {
        const row = document.createElement("div");
        row.className = "tab-machine-down-row";
        row.appendChild(statusDot("unknown", copyText("sessionState.unseen.name"), "compact"));
        const body = document.createElement("span");
        body.className = "tab-machine-down-body";
        const l1 = document.createElement("span");
        l1.textContent = copyText("tabBar.machineDown.body", { machine: r.origin, n: r.n });
        const l2 = document.createElement("span");
        l2.className = "tab-machine-down-seen";
        l2.textContent = copyText("tabBar.machineDown.seen", { ago: sinceText(r.since, now) ?? fmtDur(0) });
        body.append(l1, l2);
        const re = document.createElement("button");
        re.type = "button";
        re.className = "tab-machine-down-act";
        re.dataset.reconnect = r.origin;
        re.textContent = copyText("tabBar.machineDown.reconnect");
        row.append(body, re);
        return row;
      }),
    );
  }

  /**
   * `refreshTabBar` 的**帧末合批**入口。
   *
   * 排一次位（`tabBarRefreshScheduled`）+ 无 rAF 时 `setTimeout` 兜底，
   * 范式与 `tab-stream-view.ts` 的 `scheduleIdleMaterialize` 一致（拆之前两者同住 `tabs.ts`）。
   * ⚠ 只给 live 路那一处用，别把用户动作触发的调用点也改过来（理由写在调用处）。
   */
  private tabBarRefreshScheduled = false;

  scheduleRefresh(): void {
    if (this.tabBarRefreshScheduled) return;
    this.tabBarRefreshScheduled = true;
    const run = (): void => {
      this.tabBarRefreshScheduled = false;
      // 经宿主刷 —— `TabManager.refreshTabBar`（拖拽守卫在那里）；判据会把它换成计数替身。
      this.host.refreshTabBar();
    };
    if (typeof requestAnimationFrame === "function") {
      // 调度：合批 —— 后台标签页的未读徽标排到帧末合批，排一次位
      requestAnimationFrame(run);
    } else {
      // 调度：合批 —— 上面那一处在没有 rAF 时的兜底，0ms 一次
      window.setTimeout(run, 0);
    }
  }

  /**
   * 拿到某个组在栏里的容器（没有就建）。组头：⌄（收起 / 展开）· 名字（点一下改名）· 组员数 · 收着时汇总（等你 · 在跑）·
   * 悬停 / 焦点时右侧「⋯」（组的菜单；组头右键同一份）。点 ⌄ 或组头空白 ＝ 收起 / 展开。
   * **解散只去掉分组，一个 tab 都不动**（在「⋯」菜单里，撤得回）。
   */
  private groupElFor(col: TabCollection): HTMLElement {
    let g = this.groupEls.get(col.id);
    if (!g) {
      const gid = col.id;
      const name = document.createElement("button");
      name.type = "button";
      name.className = "tab-group-name";
      name.addEventListener("click", (e) => {
        e.stopPropagation();
        this.beginGroupRename(gid, name);
      });
      const more = document.createElement("button");
      more.type = "button";
      more.className = "tab-group-more";
      more.setAttribute("aria-label", copyText("tabBar.group.more"));
      more.appendChild(icon("more", "compact"));
      more.addEventListener("click", (e) => {
        e.stopPropagation();
        this.openGroupMenu(gid, { el: more, align: "end" });
      });
      g = groupShell(name, [more]);
      const head = g.head;
      this.headGid.set(head, gid);
      // 拖整组刚松手的那次 click：不收展、不改名（捕获阶段先吞掉）。
      head.addEventListener(
        "click",
        (e) => {
          if (this.host.takeSuppressedHeadClick(gid)) e.stopPropagation();
        },
        true,
      );
      // 点 ⌄ 或组头空白（名字与「⋯」自己吞了 click）⇒ 收起 / 展开。
      head.addEventListener("click", () => this.toggleGroup(gid));
      head.addEventListener("contextmenu", (e) => {
        e.preventDefault();
        e.stopPropagation();
        this.openGroupMenu(gid, { x: e.clientX, y: e.clientY });
      });
      this.listEl.appendChild(g.wrap);
      this.groupEls.set(gid, g);
    }
    const members = [...this.store.tabs.values()].filter((t) => t.group === col.id);
    paintHead(g, col.name, members, col.collapsed === true, this.store.activeId);
    return g.list;
  }

  /**
   * 栏底「已隐藏 N」那一行：组头与收着的分组同一个组件（`groupShell` · `paintHead`：⌄ · 名字 · 收着时汇总需手动 / 在跑），
   * 展开列活着的隐藏会话（行与标签页同形，缩进一格）。没有活的隐藏会话 ⇒ 不画。点组头收 / 展（记在 `tabBar.hiddenOpen`）；
   * 窄栏只剩「隐藏」图标 ＋ 计数，点开是同一列的菜单。不能往里拖、不能分组、不能固定：这里没有组的菜单。
   */
  private hiddenEls: GroupEls | null = null;

  /** 「已隐藏」那一行照这几个画（空 ⇒ 摘掉）；回组员列表（行往里放）。 */
  private paintHiddenRow(ids: readonly string[]): HTMLElement | null {
    if (ids.length === 0) {
      this.hiddenEls?.wrap.remove();
      return null;
    }
    let g = this.hiddenEls;
    if (!g) {
      const name = document.createElement("span");
      const glyph = document.createElement("span");
      glyph.className = "tab-hidden-icon";
      glyph.appendChild(icon("hidden", "compact"));
      g = groupShell(name, []);
      g.head.insertBefore(glyph, g.name);
      g.wrap.classList.add("tab-hidden");
      g.head.dataset.role = "hidden-row";
      attachTooltip(g.head, () => copyText("tabBar.hidden.tip"));
      g.head.addEventListener("click", () => {
        if (folded()) this.openHiddenMenu();
        else this.toggleHiddenRow();
      });
      this.hiddenEls = g;
    }
    const members = ids.map((sid) => this.store.tabs.get(sid)).filter((t): t is Tab => t !== undefined);
    paintHead(g, copyText("tabBar.hidden.row", { n: members.length }), members, !this.hiddenOpen(), this.store.activeId);
    return g.list;
  }

  /** 收 / 展「已隐藏」那一行（落盘 `tabBar.hiddenOpen`）。 */
  private toggleHiddenRow(): void {
    void this.prefs.setHiddenOpen(!this.hiddenOpen());
    this.host.refreshTabBar();
  }

  /** 窄栏：点「已隐藏」那一颗 ⇒ 同一列开成菜单（每行：状态点 · 名字 · 状态一句；点一行切过去）。 */
  private openHiddenMenu(): void {
    const g = this.hiddenEls;
    if (!g) return;
    const now = Date.now();
    const items: MenuItem[] = this.store.hiddenRow().map((sid) => {
      const t = this.store.tabs.get(sid)!;
      return {
        id: sid,
        label: fullTitle(t),
        avatar: statusDot(dotOf(t), stateWord(t), "compact"),
        detail: stateLine(t, now).text,
        detailTone: needsOf(t) ? "warn" : undefined,
        onClick: () => this.host.switchTo(sid),
      };
    });
    openMenu({ el: g.head, align: "start" }, items, { label: copyText("tabBar.hidden.row", { n: items.length }) });
  }

  /** 收起 / 展开一个组（落盘）。 */
  private toggleGroup(gid: string): void {
    const col = this.prefs.collections.find((c) => c.id === gid);
    if (!col) return;
    // 收起时焦点若在组员上 ⇒ 落到组头（组员藏起来了）。
    const was = this.focusedRow();
    void this.prefs.setCollapsed([gid], col.collapsed !== true);
    this.host.refreshTabBar();
    if (was !== null && (was.kind === "head" ? was.gid === gid : this.store.tabs.get(was.sid)?.group === gid)) {
      if (was.kind === "head" || !this.focusRow(was)) this.focusRow({ kind: "head", gid });
    }
  }

  /** 组的菜单开在组头上（键盘：焦点在组头上按 Shift+F10 / 菜单键）。 */
  openGroupMenuAt(gid: string): void {
    const head = this.groupEls.get(gid)?.head;
    if (head) this.openGroupMenu(gid, { el: head, align: "end" });
  }

  /** 组的菜单（「⋯」/ 组头右键）：改名 · 收起或展开 · 全部收起 · 全部展开 · 关闭组里已结束的（n）· 解散分组。 */
  private openGroupMenu(gid: string, anchor: MenuAnchor): void {
    const col = this.prefs.collections.find((c) => c.id === gid);
    if (!col) return;
    const all = this.prefs.collections.map((c) => c.id);
    const ended = [...this.store.tabs.values()].filter((t) => t.group === gid && closesWithoutMenu(t.state)).length;
    const nameEl = this.groupEls.get(gid)?.name;
    const items: MenuItem[] = [
      { label: copyText("tabMenu.group.rename"), onClick: () => nameEl && this.beginGroupRename(gid, nameEl) },
      { label: col.collapsed ? copyText("tabMenu.group.expand") : copyText("tabMenu.group.collapse"), onClick: () => this.toggleGroup(gid) },
      {
        label: copyText("tabMenu.group.collapseAll"),
        onClick: () => {
          void this.prefs.setCollapsed(all, true);
          this.host.refreshTabBar();
        },
      },
      {
        label: copyText("tabMenu.group.expandAll"),
        onClick: () => {
          void this.prefs.setCollapsed(all, false);
          this.host.refreshTabBar();
        },
      },
      { divider: true, label: "" },
      { label: copyText("sessionState.closeEnded.inGroup", { n: ended }), enabled: ended > 0, onClick: () => this.host.closeEndedIn(gid) },
      { divider: true, label: "" },
      {
        label: copyText("tabMenu.group.dissolve"),
        onClick: () => {
          // 撤得回 ⇒ 不确认：直接解散 ＋ 8 秒撤销（组员留在原位）。
          const before = this.prefs.snapshot();
          void this.prefs.dissolveGroup(gid);
          this.host.refreshTabBar();
          this.prefs.offerUndo(copyText("tabBar.group.dissolved", { name: col.name }), before);
        },
      },
    ];
    openMenu(anchor, items, { label: col.name });
  }

  /**
   * 组头就地改名 —— 名字那一格换成 `<input>`：Enter 提交 / Esc 取消 / blur 提交。
   *
   * - 名字按钮只藏不摘（`hidden`），输入框插在它后面 ⇒ 整刷那条「名字没变就不写」照旧写在按钮上，不碰输入框。
   * - Esc 走 overlay 栈（INVARIANTS「别手搓 window 级 Esc 监听」）：改名时 Esc 只取消改名，不连带关别的弹层；
   *   输入框自己也认 Esc（`dispatcher` 没起的窗口 / 测试里照样能取消）—— 两路都进同一个只认一次的 `finish`。
   * - 提交值 `trim` 后为空 ⇒ 当取消；与现名相同 ⇒ 不写（`renameCollection` 本身也把空名当不改）。
   */
  private beginGroupRename(id: string, name: HTMLElement): void {
    if (name.hidden) return; // 已经在改
    const cur = this.prefs.collections.find((x) => x.id === id);
    const input = document.createElement("input");
    input.type = "text";
    input.className = rs.input;
    input.value = cur?.name ?? "";
    let done = false;
    const overlay: OverlayHandle = {
      handleEsc: () => {
        finish(false, true);
        return true;
      },
    };
    const finish = (commit: boolean, viaKey = false): void => {
      if (done) return;
      done = true;
      dispatcher.popOverlay(overlay);
      const next = input.value.trim();
      // 按 Enter / Esc 收的框：焦点回到组头（键盘接着走）；失焦收的不抢焦点。
      const refocus = viaKey && document.activeElement === input;
      input.remove();
      name.hidden = false;
      if (refocus) this.groupEls.get(id)?.head.focus();
      if (!commit || !next) return;
      const now = this.prefs.collections.find((x) => x.id === id);
      if (!now || now.name === next) return;
      void this.prefs.renameGroup(id, next);
      this.host.refreshTabBar();
    };
    input.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" && !ev.isComposing) {
        ev.preventDefault();
        finish(true, true);
      } else if (ev.key === "Escape") {
        ev.preventDefault();
        finish(false, true);
      }
    });
    input.addEventListener("blur", () => finish(true));
    name.hidden = true;
    name.after(input);
    dispatcher.pushOverlay(overlay);
    input.focus();
    input.select();
  }

  /** 账号快照换了 ⇒ 所有 tab 的账号徽章就地重刷。 */
  refreshAccountBadges(): void {
    for (const [sid, refs] of this.tabButtons) {
      const tab = this.store.tabs.get(sid);
      if (tab) this.updateAccountBadge(refs, sid, tab);
    }
  }

  /**
   * 账号头像：只在这个会话用的号与那台的默认号不同时出（全都出 ⇒ 每行一块、信息为零）；
   * 默认号不知道 ⇒ 不出（说不出「不同」）。悬停卡里永远写账号。头像悬停说「账号 {name}」。
   */
  private updateAccountBadge(refs: TabButtonRefs, sid: string, tab: Tab): void {
    const hide = (): void => {
      if (refs.acctDrawn === "") return;
      refs.acctDrawn = "";
      refs.acctBadge.textContent = "";
      refs.acctBadge.className = "tab-acct-badge";
      refs.acctBadge.style.display = "none";
    };
    const account = this.accountOf(sid, tab);
    if (account === null) return hide();
    const current = this.store.currentByOrigin.get(tab.origin) ?? null;
    if (!detectAccountMismatch(account.name, current)) return hide();
    const drawn = `${account.name}\u0000${account.ghost ? 1 : 0}\u0000${accountColorOf(account.name, tab.origin) ?? ""}`;
    if (refs.acctDrawn === drawn) return;
    refs.acctDrawn = drawn;
    refs.acctBadge.textContent = "";
    refs.acctBadge.className = "tab-acct-badge";
    refs.acctBadge.appendChild(accountAvatarEl(account.name, { size: 14, ghost: account.ghost, origin: tab.origin }));
    refs.acctBadge.title = copyText("tabBar.hover.account", { name: account.name });
    refs.acctBadge.style.display = "";
  }

  /** 这个会话用的号（`ghost` ＝ 上次用的、不是此刻探到的）；不知道 ⇒ `null`。走中转的会话照那台 `rotation-view` 的在用号（轮换换过号也对）。 */
  private accountOf(sid: string, tab: Tab): { name: string; ghost: boolean } | null {
    if (!shouldShowAccountBadge(tab.origin, this.store.accountReadyOrigins)) return null;
    const cur = appStore.sessionRotation.get().get(sid)?.read?.current;
    if (cur) return { name: cur, ghost: false };
    const b = sessionBadge(sid, tab.origin, this.store.sessionAccountsByS, this.store.accountEmailByName, this.store.accountLastByS);
    if (!b || !b.account) return null;
    return { name: b.account, ghost: b.source === "last" };
  }

  private createTabButton(sid: string): TabButtonRefs {
    // 按钮本身**一个监听器都不挂** —— 手势全在 `barEl` 上委托（见构造体）；悬停卡经 kit 挂在根上。
    const root = document.createElement("button");
    root.className = "tab";
    this.sidOf.set(root, sid);
    const span = (cls: string): HTMLSpanElement => {
      const e = document.createElement("span");
      e.className = cls;
      return e;
    };

    const dot = statusDot("running", copyText("sessionFace.dot.running"), "compact");
    dot.classList.add("tab-dot");
    dot.removeAttribute("title"); // 悬停卡说状态；点上不挂原生提示
    const abbr = span("tab-abbr");
    const machine = tag("");
    machine.classList.add("tab-machine");
    machine.style.display = "none";
    const titleEl = span("tab-title");
    const proj = span("tab-proj");
    const bg = span("tab-gear");
    bg.appendChild(icon("background", "compact"));
    bg.title = copyText("tabBarView.tab.bgHint");
    bg.style.display = "none";
    const label = span("tab-label");
    titleEl.append(proj, bg, label);

    const trail = span("tab-trail");
    const needs = span("tab-needs-word");
    // 额度：被卡住的会话标题后红字 `✕ 5h`（与「等批准」同位）。默认藏着，updateTabButton 按会话的轮换格填。
    const badge = span("tab-badge");
    const acctBadge = span("tab-acct-badge");
    acctBadge.style.display = "none";
    // 图钉：纯展示、不可点（固定是右键菜单那一项的事）。可见性交给 `.pinned` 类。
    const pinBadge = span("tab-pin");
    pinBadge.appendChild(icon("pin", "compact"));
    pinBadge.title = copyText("tabBarView.tab.pinHint");
    const holdBadge = document.createElement("span");
    holdBadge.className = qs.tabHold;
    holdBadge.style.display = "none";
    holdBadge.textContent = copyText("rot.tab.hold");
    trail.append(needs, holdBadge, badge, acctBadge, pinBadge);

    // 行尾动作：悬停 / 焦点时盖在行尾（带同色底与渐隐），标题不挪位。
    const acts = span("tab-acts");
    // 行尾动作的悬停提示委托在列表上（`actHint`）；这里只给读屏名。
    const act = (cls: string, name: Parameters<typeof icon>[0], label: string): HTMLSpanElement => {
      const b = span(cls);
      b.setAttribute("role", "button");
      b.setAttribute("aria-label", label);
      b.appendChild(icon(name, "compact"));
      return b;
    };
    const cwdBtn = act("tab-cwd", "folder", copyText("tabBarView.tab.cwdHint"));
    acts.appendChild(cwdBtn);
    // ↗ 切到终端：只 Windows 渲（`terminal-front.ts`）。
    if (terminalFrontAvailable()) acts.appendChild(act("tab-focus", "front", copyText("tabBarView.tab.terminalHint")));
    acts.appendChild(act("tab-more", "more", copyText("tabBarView.tab.moreHint")));
    acts.appendChild(act("tab-close", "close", copyText("tabBarView.tab.closeHint")));

    root.append(dot, abbr, machine, titleEl, trail, acts);

    return { root, dot, abbr, machine, proj, bg, label, needs, badge, acctBadge, holdBadge, cwdBtn, pinBadge, drawn: null, acctDrawn: "" };
  }

  /**
   * 悬停卡：标题全名 · 机器 · 目录 · 状态句（点 ＋ 一句，在等你时琥珀）· peek 一句 · 账号 · 分叉自 · 写入进程 · `5 切到这里`。
   * 卡里没有能点的东西；peek 拿不到就不出那一行。tab 已经没了 ⇒ 不出。
   */
  hoverCard(sid: string): HTMLElement | null {
    const tab = this.store.tabs.get(sid);
    if (!tab) return null;
    const now = Date.now();
    const card = document.createElement("div");
    card.className = "tab-hover";
    const line = (cls: string, text: string): HTMLDivElement => {
      const d = document.createElement("div");
      d.className = cls;
      d.textContent = text;
      return d;
    };
    // 窄栏看不见组名 ⇒ 组员的悬停卡首行带上（I8：收起后看不见的信息进提示）。
    const col = folded() && tab.group !== null ? this.prefs.collections.find((c) => c.id === tab.group) : undefined;
    card.appendChild(line("tab-hover-title", col ? copyText("tabBar.group.memberHint", { group: col.name, title: fullTitle(tab) }) : fullTitle(tab)));
    const where = line("tab-hover-row", machineOf(tab));
    card.appendChild(where);
    if (tab.projectDir) card.appendChild(line("tab-hover-row tab-hover-mono", tab.projectDir));
    const st = stateLine(tab, now);
    const state = document.createElement("div");
    state.className = st.needs ? "tab-hover-state tab-hover-need" : "tab-hover-state";
    const d = dotOf(tab);
    state.append(statusDot(d, stateWord(tab), "compact"), document.createTextNode(st.text));
    card.appendChild(state);
    const peek = peekLine(tab);
    if (peek) card.appendChild(line("tab-hover-peek", peek));
    // 隐藏的：后端那一句（为什么不占标签页）。
    if (tab.hidden) card.appendChild(line("tab-hover-row", tab.hidden.said));
    const acct = this.accountOf(sid, tab);
    if (acct) card.appendChild(line("tab-hover-row", copyText("tabBar.hover.account", { name: acct.name })));
    // 轮换换过号：从哪个号换过来（开会话的号）· 停发那一句。
    const rot = appStore.sessionRotation.get().get(sid)?.read;
    if (rot && rot.current !== rot.start) card.appendChild(line("tab-hover-row", copyText("rot.hover.from", { account: accountLabel(rot.start) })));
    const held = holdOf(rot);
    if (held !== null) card.appendChild(line("tab-hover-row", held));
    if (tab.forkedFromSessionId) card.appendChild(line("tab-hover-row", copyText("tabBar.hover.forked", { id: tab.forkedFromSessionId.slice(0, 8) })));
    if (isLive(tab.state) && tab.writers.length > 1) card.appendChild(line("tab-hover-row", copyText("tabBarView.tab.writers", { n: tab.writers.length })));
    const pos = this.visibleOrder().indexOf(sid);
    if (pos >= 0 && pos < 9) card.appendChild(line("tab-hover-foot", copyText("tabBar.hover.key", { key: String(pos + 1) })));
    return card;
  }

  private updateTabButton(refs: TabButtonRefs, sid: string, tab: Tab): void {
    // 先把**要画成什么样**整个算出来，与上次画出去的比 —— 一样就一个 DOM 都不写。
    const active = sid === this.store.activeId;
    // 多选里的样子（`.selected`）与「当前 tab」（`.active`）是两件事，两个类各画各的。
    const selected = this.host.isSelected(sid);
    // 两轴怎么画（类）只从 `stateView` 取：已结束（只能 resume）· 状态不明（那台看不见）· Claude 已退出（死了、容器还在）。
    const view = stateView(tab.state);
    const ended = view.ended;
    const unseen = view.unseen;
    const pinned = tab.pinned;
    const hasCwd = !!tab.projectDir;
    const remote = isRemoteOrigin(tab.origin);
    const lightClass = isLive(tab.state) ? activityFace(tab.activity?.tone ?? null).light : "";
    const reconnectable = view.reconnectable;
    const dot = dotOf(tab);
    const n = needsOf(tab);
    const needsText = n ? n.text : "";
    const parts = titleParts(tab);
    const unread = tab.unread > 0 && !active;
    // 未读数只在有未读、又不在等你时出（等你 ＞ 未读数）。
    const unreadText = unread && !n ? (tab.unread > 99 ? "99+" : String(tab.unread)) : "";
    // 空闲、跑完你还没看 ⇒ 标题 600 字重（点开即恢复）。
    const unseenDone = unread && dot === "idle";
    const titleText = `${parts.forked ? "↳ " : ""}${parts.title}`;
    const abbr = abbrOf(tab);

    const flags = [active, selected, ended, unseen, pinned, hasCwd, remote, reconnectable, unreadText !== "", unseenDone, n !== null, parts.bg]
      .map((b) => (b ? "1" : "0"))
      .join("");
    const held = holdOf(appStore.sessionRotation.get().get(sid)?.read) !== null;
    const drawn = [flags, lightClass, dot, titleText, parts.proj ?? "", unreadText, needsText, held ? "1" : "", abbr, remote ? tab.origin : ""].join("\u0000");
    if (refs.drawn !== drawn) {
      refs.drawn = drawn;
      refs.root.classList.toggle("active", active);
      refs.root.classList.toggle("selected", selected);
      refs.root.classList.toggle("ended", ended);
      refs.root.classList.toggle("unseen", unseen);
      refs.root.classList.toggle("pinned", pinned);
      refs.root.classList.toggle("has-cwd", hasCwd);
      refs.root.classList.toggle("remote", remote);
      // 状态钩子（没有样式、给判据与读屏读）：灯那一档 · 可重连。
      if (lightClass) refs.root.dataset.light = lightClass;
      else delete refs.root.dataset.light;
      refs.root.toggleAttribute("data-reconnectable", reconnectable);
      refs.root.classList.toggle("has-unread", unreadText !== "");
      refs.root.classList.toggle("unseen-done", unseenDone);
      refs.root.classList.toggle("waiting-you", n !== null);
      // 只写真变了的那几格（一个状态变了 ⇒ 这颗按钮上恰好：类 · 点的样子 · 点的读屏名）。
      const dotName = stateWord(tab);
      if (refs.dot.dataset.state !== dot || refs.dot.getAttribute("aria-label") !== dotName) {
        refs.dot.dataset.state = dot;
        refs.dot.setAttribute("aria-label", dotName);
      }
      if (refs.label.textContent !== titleText) refs.label.textContent = titleText;
      setText(refs.proj, parts.proj ?? "");
      setShown(refs.proj, parts.proj !== null);
      setShown(refs.bg, parts.bg);
      setText(refs.abbr, abbr);
      setShown(refs.machine, remote);
      if (remote) setText(refs.machine, tab.origin);
      setText(refs.needs, needsText);
      setShown(refs.needs, n !== null);
      setShown(refs.holdBadge, held);
      setText(refs.badge, unreadText);
    }
    this.updateAccountBadge(refs, sid, tab); // 账号头像随 tab 更新一并刷新
  }
}

/** 字变了才写（整刷里一格没变就一个 DOM 都不写）。 */
function setText(el: HTMLElement, text: string): void {
  if (el.textContent !== text) el.textContent = text;
}

/** 显 / 藏：变了才写。 */
function setShown(el: HTMLElement, on: boolean): void {
  const want = on ? "" : "none";
  if (el.style.display !== want) el.style.display = want;
}

/** 行尾那几颗动作的悬停提示（按类认是哪一颗）。 */
function actHint(el: HTMLElement): string | null {
  if (el.classList.contains("tab-cwd")) return withKey(copyText("tabBarView.tab.cwdHint"), "tab.open-cwd");
  if (el.classList.contains("tab-focus")) return withKey(copyText("tabBarView.tab.terminalHint"), "terminal.bring-front");
  if (el.classList.contains("tab-close")) return withKey(copyText("tabBarView.tab.closeHint"), "tab.close-archived");
  if (el.classList.contains("tab-more")) return copyText("tabBarView.tab.moreHint");
  return null;
}

/** 悬停提示「名字 · 当前键位」（键位按 `config.json.keybindings` 现拼；没绑键只写名字）。 */
function withKey(name: string, action: Parameters<typeof dispatcher.effectiveChord>[0]): string {
  const chord = dispatcher.effectiveChord(action);
  return chord ? `${name} · ${KeybindingDispatcher.prettyChord(chord)}` : name;
}

/**
 * 组头那个组件的壳（分组与栏底「已隐藏」那一行共用）：⌄ · 名字 · 计数 · 收着时的汇总 · 窄栏那一颗点 · 空白 ·（各自的尾巴）＋ 组员列表。
 * 名字那一格由调用方建好交进来（分组是点一下改名的按钮，「已隐藏」是一段字）。
 */
function groupShell(name: HTMLElement, tail: HTMLElement[]): GroupEls {
  const wrap = document.createElement("div");
  wrap.className = "tab-group";
  const head = document.createElement("div");
  head.className = "tab-group-head";
  // 键盘走行时组头也是一行（↑↓ 能停在它上面）；不进 Tab 顺序。
  head.tabIndex = -1;
  const caret = document.createElement("button");
  caret.type = "button";
  caret.className = "tab-group-caret";
  caret.appendChild(foldCaret());
  name.classList.add("tab-group-name");
  const count = document.createElement("span");
  count.className = "tab-group-count";
  const sum = document.createElement("span");
  sum.className = "tab-group-sum";
  const mini = document.createElement("span");
  mini.className = "tab-group-mini";
  const sp = document.createElement("span");
  sp.className = "tab-group-sp";
  head.append(caret, name, count, sum, mini, sp, ...tail);
  const list = document.createElement("div");
  list.className = "tab-group-list";
  wrap.append(head, list);
  return { wrap, head, list, caret, name, count, sum, mini, drawn: "" };
}

/** 组头那几格照此刻的组员现画：名字 · 组员数 · 收着 · 汇总（等你 · 在跑 · 后台任务）· 当前标签页在收着的里面。没变就不写。 */
function paintHead(g: GroupEls, label: string, members: readonly Tab[], collapsed: boolean, activeId: string | null): void {
  const sum = collapsed ? groupSummary(members) : { needs: 0, running: 0, background: 0, needsWord: "", runningWord: "", backgroundWord: "" };
  const current = collapsed && members.some((t) => t.sessionId === activeId);
  const drawn = `${label}\u0000${members.length}\u0000${collapsed ? 1 : 0}\u0000${sum.needs}\u0000${sum.running}\u0000${sum.background}\u0000${sum.needsWord}\u0000${sum.runningWord}\u0000${sum.backgroundWord}\u0000${current ? 1 : 0}`;
  if (g.drawn === drawn) return;
  g.drawn = drawn;
  if (g.name.textContent !== label) g.name.textContent = label;
  g.name.title = label;
  g.count.textContent = String(members.length);
  g.caret.setAttribute("aria-expanded", String(!collapsed));
  g.caret.setAttribute("aria-label", collapsed ? copyText("tabMenu.group.expand") : copyText("tabMenu.group.collapse"));
  g.wrap.classList.toggle("is-collapsed", collapsed);
  g.head.classList.toggle("is-current", current);
  g.wrap.classList.toggle("has-current", current);
  const chips: HTMLElement[] = [];
  const word = (dot: "needs-you" | "running" | "background"): string => (dot === "needs-you" ? sum.needsWord : dot === "running" ? sum.runningWord : sum.backgroundWord);
  const chip = (dot: "needs-you" | "running" | "background", text: string): HTMLElement => {
    const c = document.createElement("span");
    c.className = "tab-group-chip";
    c.dataset.dot = dot;
    c.append(statusDot(dot, word(dot), "compact"), document.createTextNode(text));
    return c;
  };
  if (sum.needs > 0) chips.push(chip("needs-you", copyText("tabBar.group.sumNeeds", { n: sum.needs })));
  if (sum.running > 0) chips.push(chip("running", copyText("tabBar.group.sumRunning", { n: sum.running })));
  if (sum.background > 0) chips.push(chip("background", copyText("tabBar.group.sumBackground", { n: sum.background })));
  g.sum.replaceChildren(...chips);
  // 窄栏只放得下一个点：需手动 ＞ 运行中 ＞ 后台任务运行中。
  const urgent = sum.needs > 0 ? "needs-you" : sum.running > 0 ? "running" : sum.background > 0 ? "background" : null;
  g.mini.replaceChildren(...(urgent ? [statusDot(urgent, word(urgent), "compact")] : []));
}
