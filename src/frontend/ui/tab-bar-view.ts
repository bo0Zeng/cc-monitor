/**
 * 〔拆 `tabs.ts` ④〕**标签页栏视图**：栏顶一排（全局入口 · 刷新）· 「需要你 N」· 机器离线条 · 列表（分组 ＋ 每个会话一行）。
 *
 * 一行：状态点（V10）· 窄窗两字母 · 机器徽标（远端）· 项目名 ＋ 标题（后台多一个齿轮）· 行尾（等批准 / 未读数 · 额度 `✕` ·
 * 与默认不同的账号头像 · 图钉）· 悬停时盖在行尾的动作（目录 · ↗ · 更多 / 关闭）。悬停卡（kit 悬停提示的卡式）锚在行右侧。
 * 一句话怎么写全从 `session-face.ts` 取（状态点 · 状态句 · peek · 需要你）；这里只排。
 *
 * 只画、只把用户手势转交出去：点按钮切 tab、按下起拖、右键开菜单、子动作按钮 —— 做事的都经
 * `TabBarViewHost` 交给宿主（路由 / 拖拽 / 菜单 / 会话动作），本文件不 import 它们。
 * 读的状态：`TabStore`（tab 集合 · 顺序 · 当前 tab · 账号快照）与 `TabBarPrefs`（集合）。
 */
import { sessionBadge, shouldShowAccountBadge, detectAccountMismatch } from "./accounts";
import { accountAvatarEl } from "./account-color";
import type { TabCollection } from "./tab-collections";
import { activityLightClass } from "./session-status";
import { terminalFrontAvailable } from "./terminal-front";
import { isRemoteOrigin } from "./ipc/origin";
import { closesWithoutMenu, hasTerminal, isLive, stateView } from "./tab-session-state";
import type { Tab } from "./tab-model";
import type { TabStore } from "./tab-store";
import type { TabBarPrefs } from "./tab-bar-prefs";
import { dispatcher, KeybindingDispatcher, type OverlayHandle } from "./keybindings/registry";
import rs from "./tab-group-rename.module.css";
import qs from "./tab-quota.module.css";
import { appStore } from "./app-store";
import { tabBlockedOf } from "./acct-view";
import { copyText } from "./copy-table";
import { undoToast } from "./kit/toast";
import { icon } from "./kit/icon";
import { statusDot } from "./kit/status-dot";
import { countBadge, tag, kbd } from "./kit/badge";
import { attachTooltip, delegateTooltip, TOOLTIP_DELAY_MS } from "./kit/tooltip";
import { closeMenu, menuAnchoredOn, openMenu, type MenuItem } from "./kit/menu";
import { abbrOf, dotLabel, dotOf, fullTitle, machineOf, needsOf, needsOrder, needsWord, nextNeeds, peekLine, stateLine, titleParts, sinceText } from "./session-face";

/** TabButton 的 DOM 引用：refreshTabBar 局部更新依赖这些 ref 避免重新创建 button */
export interface TabButtonRefs {
  root: HTMLButtonElement;
  /** 状态点（kit V10）。 */
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
  /** 行尾「等批准 / 等回答 / 需要你」（琥珀字，换掉未读数）。 */
  needs: HTMLSpanElement;
  badge: HTMLSpanElement;
  /** 额度：被卡住的会话标题后那一格红字 `✕ 5h`（能发时藏着）。 */
  quotaBadge: HTMLSpanElement;
  /** A3：账号徽章（该会话属于哪个账号；本地会话不显示，未知显 —）。 */
  acctBadge: HTMLSpanElement;
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
  /** 右键：开这个 tab 的菜单。 */
  openMenu(e: MouseEvent, sid: string): void;
  /** 栏顶「刷新」：有打开 tab 的每台对齐 ＋ 补读一次；做完才 resolve。 */
  rereadAll(): Promise<void>;
  /** 行尾「更多」：开这个 tab 的菜单（锚在那颗按钮上）。 */
  openMenuAt(anchor: HTMLElement, sid: string): void;
  /** 离线条的［重新连接］：那台断着在退避里等 ⇒ 立刻重拨。 */
  reconnect(origin: string): void;
}

/** 「需要你」悬停菜单的宽：标题放得下约 28 个汉字（≈ 420px），至少 360、至多 480 与视口减 32 的小者；可以盖过标签页栏伸进消息流。 */
const NEEDS_MENU = { min: 360, ideal: 420, max: 480, viewportGutter: 32 } as const;

/** 栏收成 44px 时（`tab-bar-fold.ts`）悬停卡宽 260。 */
const folded = (): boolean => document.body.dataset.tabBar === "folded";

export class TabBarView {
  /** sessionId → button DOM refs，避免 refreshTabBar 每次重建整个 bar */
  readonly tabButtons = new Map<string, TabButtonRefs>();
  /** 每个集合在主栏里的容器（组头 + 成员列表）。 */
  private readonly groupEls = new Map<string, { wrap: HTMLElement; head: HTMLElement; list: HTMLElement }>();
  // 🔴 **`ensureArchiveUi()` 整个删掉。**
  //
  // 抬头逐字「**已定**：删归档抽屉 · 固定灰 tab」，三条独立理由：
  //   ① 它永久吃 450px 屏宽（`.tab-archive` 是 `#app` 的 grid item 却没认领格子）
  //   ② 它是个撕窗口陷阱（tear-off 判定线对抽屉没有意义）
  //   ③ **它的存在理由本来就自相矛盾** —— 原 `belongsInArchive` 的注释自己写着：
  //      active tab 会「在你正看着它的时候」掉进折叠的抽屉里 ⇒ 已经为 active 开了例外。
  //      把例外推广到全部，抽屉就没了。
  //
  // ⚠ **删的是抽屉，不是状态。** 已结束（`isResumeOnly(tab.state)`）照旧存在，那种 tab
  //   **留在原位变淡**（`.tab.ended`，原名 `.tab.archived`，`§A.3` 逐字「不用新写」）。
  //   用户 2026-09-19 逐字：「没有归档这个东西，不要归档，就是灰 tab。」

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
  /** 「需要你 N」：有人在等你才出。 */
  private readonly needsEl: HTMLButtonElement;
  private readonly needsCount: HTMLSpanElement;
  /** 「需要你」那一条右端的键帽（按当前键位现拼，出现时才拼）。 */
  private readonly needsKbd: HTMLSpanElement;
  /** 机器离线条（一台一条）。 */
  private readonly downEl: HTMLDivElement;
  /** 列表：分组 ＋ 每个会话一行（滚动的是它）。 */
  readonly listEl: HTMLDivElement;
  /** 机器 → 它看不见是从何时起（`markOriginDown`）；连上了摘掉。 */
  private readonly downSince = new Map<string, number>();

  constructor(
    private readonly store: TabStore,
    private readonly prefs: TabBarPrefs,
    barEl: HTMLElement,
    private readonly host: TabBarViewHost,
  ) {
    // **事件委托**：整条栏只在 `barEl` 上挂三个监听器，不再每个 tab 挂 10 个
    // （原先 📂 / ↗ / × 各 click ＋ mousedown，根上 click ＋ 两个 mousedown ＋ contextmenu）。
    //
    // 原先三颗子按钮上的 `stopPropagation` 有两层意思，这里都保住：
    // ① 子按钮上按下**不起拖、不触发中键关** ⇒ mousedown 先看目标在不在子按钮里；
    // ② 子按钮的 click / mousedown **不往上冒到 `document`** ⇒ 这里照样 `stopPropagation()`
    //   （`barEl` 与按钮之间只隔组容器，组容器上没有 click / mousedown 监听）。
    barEl.addEventListener("click", (e) => this.onBarClick(e));
    barEl.addEventListener("mousedown", (e) => this.onBarMouseDown(e));
    barEl.addEventListener("contextmenu", (e) => this.onBarContextMenu(e));
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
    // 悬停 500ms ⇒ kit 菜单列出每个在等你的会话与那一句，点一行跳过去；没到点就移开 ⇒ 不开。
    let needsHover: ReturnType<typeof setTimeout> | null = null;
    this.needsEl.addEventListener("mouseenter", () => {
      if (needsHover !== null) clearTimeout(needsHover);
      needsHover = setTimeout(() => {
        needsHover = null;
        if (!menuAnchoredOn(this.needsEl)) this.openNeedsMenu();
      }, TOOLTIP_DELAY_MS);
    });
    this.needsEl.addEventListener("mouseleave", () => {
      if (needsHover !== null) clearTimeout(needsHover);
      needsHover = null;
    });

    this.downEl = document.createElement("div");
    this.downEl.className = "tab-machine-down";

    this.listEl = document.createElement("div");
    this.listEl.className = "tab-list";
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

  /** 跳到下一个需要你的会话（`Ctrl+J` · 点「需要你」那一条）。没有 ⇒ 什么都不做。 */
  nextNeedsSid(): string | null {
    const order = needsOrder(this.visibleOrder().map((sid) => this.store.tabs.get(sid)).filter((t): t is Tab => t !== undefined));
    return nextNeeds(order, this.store.activeId);
  }

  /** 此刻需要你的会话数（窗口标题用）。 */
  needsCountNow(): number {
    let n = 0;
    for (const t of this.store.tabs.values()) if (needsOf(t)) n++;
    return n;
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
      if (menuAnchoredOn(this.needsEl)) closeMenu();
      const next = this.nextNeedsSid();
      if (next !== null) this.host.switchTo(next);
      return;
    }
    if (e.target instanceof Element && typeof e.target.closest === "function") {
      const re = e.target.closest<HTMLElement>("[data-reconnect]");
      if (re && this.downEl.contains(re)) {
        this.host.reconnect(re.dataset.reconnect ?? "");
        return;
      }
    }
    // 栏顶那一排、需要你、离线条之外的才算「条上」：点在列表外不清多选。
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
    this.host.switchTo(sid);
  }

  /** 条上看得到的顺序（组在前、组内按条上位置、散 tab 在后；整刷就按它摆）。 */
  visibleOrder(): string[] {
    return this.store.visibleOrder(this.prefs.collections.map((c) => c.id));
  }

  private onBarMouseDown(e: MouseEvent): void {
    const hit = this.hitOf(e);
    if (!hit) return;
    // 子动作按钮自己处理点击：吞掉 mousedown 避免在它们身上起 Tab 拖拽（也不触发下面的中键关）。
    if (hit.sub) {
      e.stopPropagation();
      return;
    }
    if (e.button === 0) {
      // 左键 mousedown：候选 Tab 撕离拖拽（越过阈值才真拖，否则仍是普通 click）。
      // 原先这里有一条「归档区里的 tab 不参与拖拽」的例外 ——
      // 抽屉没了，那条例外自动不需要（`§A.3` 逐字「净收益」）。
      this.host.beginDrag(e, hit.sid, hit.root);
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

  /** issue #10：右键菜单「在新窗口打开」（双屏 / 并排）。菜单里放哪几项住 `tab-menu.ts`。 */
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
   *   3. 更新：updateTabButton 同步 active / ended / reconnectable / has-unread class + label/badge 文本
   *   4. 排序：iterate orderedIds + insertBefore，确保 DOM 顺序 = orderedIds 顺序
   *
   * CSS 配合（styles.css）：
   *   .tab.ended .live-dot { display: none }
   *   .tab:not(.ended) .tab-close { display: none }
   *   .tab .tab-badge { display: none }
   *   .tab.has-unread:not(.active) .tab-badge { display: inline-block }
   */
  refresh(): void {
    // 1. 删
    const wanted = new Set(this.store.orderedIds);
    for (const sid of Array.from(this.tabButtons.keys())) {
      if (!wanted.has(sid)) {
        const refs = this.tabButtons.get(sid)!;
        refs.root.remove();
        this.tabButtons.delete(sid);
      }
    }

    // 2 + 3 + 4. 创建 / 更新 / 排序
    // 抽屉没了 ⇒ 只剩主栏 ＋ 按集合分的若干组
    // ⇒ 推广成「**每容器一个游标**」。
    // 组容器按集合顺序先摆好（空集合也留着 —— 用户刚建的集合不该看不见）。
    // 「空」今天只剩一种来路：重启后组员还没到（意图在 `TabBarPrefs.savedGroupOf`）。
    //   在栏里的最后一个离开（× · 拖出 · 移出 · 挪组）⇒ 组已从组表里摘掉，画不出来。
    for (const [id, g] of this.groupEls) {
      if (!this.prefs.collections.some((x) => x.id === id)) {
        g.wrap.remove();
        this.groupEls.delete(id);
      }
    }
    for (const col of this.prefs.collections) this.groupElFor(col);
    const cursors = new Map<HTMLElement, ChildNode | null>();
    // ★ **未归组的排在所有组之后**〔D 阶段补审〕。
    //
    // `barEl` 的游标若从 `firstChild` 起，散 tab 会插到**组容器之前** ——
    // 而 `P7a3-Y2` 逐字写的是「未归组的照常**在后面**」。
    // 实现与自己的 DoD 措辞不符，是那种「读起来都对、跑起来是另一回事」的差错。
    // ⇒ 把 `barEl` 的起点定在最后一个组容器上（没有组则是栏顶那颗「重新读取」）。
    // 「最后一个组容器」不再把 `barEl.children` 物化成数组去找。
    // 组容器只在建的那一刻 `appendChild` 到 `barEl` 末尾、之后从不挪（挪的只有 tab 按钮），
    // 删的时候同时出 `groupEls` ⇒ **`groupEls` 的插入序就是组容器在 DOM 里的顺序**，最后一个就是它。
    // 没有组时从栏顶那颗「重新读取」之后起，它恒在第一个。
    let lastGroup: HTMLElement | null = null;
    for (const g of this.groupEls.values()) lastGroup = g.wrap;
    cursors.set(this.listEl, lastGroup);
    for (const sid of this.store.orderedIds) {
      const tab = this.store.tabs.get(sid);
      if (!tab) continue;
      let refs = this.tabButtons.get(sid);
      if (!refs) {
        refs = this.createTabButton(sid);
        this.tabButtons.set(sid, refs);
      }
      this.updateTabButton(refs, sid, tab);
      // 分流从三路（抽屉 / 组 / 主栏）降到**两路**（组 / 主栏）。
      // 「归档优先于集合」那条判定整条消失 ⇒ **灰 tab 也能在组里**（`§A.3` 逐字）。
      // 在哪个组读 tab 自己的 `group`（组表只有 `{id, name}`，不再扫成员名单）。
      const col =
        tab.group === null ? undefined : this.prefs.collections.find((c) => c.id === tab.group);
      const host = col ? this.groupElFor(col) : this.listEl;
      // 排序：希望此 button 出现在**同容器内**前一个之后。
      const prev = cursors.get(host) ?? null;
      const targetNext: ChildNode | null = prev ? prev.nextSibling : host.firstChild;
      if (refs.root !== targetNext || refs.root.parentElement !== host) {
        host.insertBefore(refs.root, targetNext);
      }
      cursors.set(host, refs.root);
    }
    this.updateNeedsStrip();
    this.updateMachineDown();

    this.store.notify();
  }

  /** 「需要你 N」：N ≥ 1 才出。 */
  private updateNeedsStrip(): void {
    const n = this.needsCountNow();
    this.needsEl.style.display = n === 0 ? "none" : "";
    if (n === 0) return;
    const chord = dispatcher.effectiveChord("needs.next");
    const pretty = chord ? KeybindingDispatcher.prettyChord(chord) : "";
    if (this.needsKbd.dataset.chord !== pretty) {
      this.needsKbd.dataset.chord = pretty;
      this.needsKbd.replaceChildren(...(pretty ? [kbd(pretty)] : []));
    }
    if (this.needsCount.dataset.n !== String(n)) {
      this.needsCount.dataset.n = String(n);
      this.needsCount.replaceChildren(...[countBadge(n, "warn")].filter((b): b is HTMLSpanElement => b !== null));
    }
    this.needsEl.setAttribute("aria-label", copyText("tabBar.needs.aria", { n }));
  }

  /** 「需要你」菜单：每个在等你的会话（等得最久的在前）· 状态 · 等的那一句；点一行切过去。 */
  private openNeedsMenu(): void {
    const tabs = needsOrder(this.visibleOrder().map((sid) => this.store.tabs.get(sid)).filter((t): t is Tab => t !== undefined));
    if (tabs.length === 0) return;
    const now = Date.now();
    const items: MenuItem[] = tabs.map((sid) => {
      const t = this.store.tabs.get(sid)!;
      return {
        id: sid,
        label: fullTitle(t),
        avatar: statusDot("needs-you", copyText("sessionFace.dot.needs"), "compact"),
        detail: stateLine(t, now).text,
        detailTone: "warn",
        body: needsOf(t)?.what || undefined,
        onClick: () => this.host.switchTo(sid),
      };
    });
    const room = window.innerWidth - NEEDS_MENU.viewportGutter;
    const width = Math.max(Math.min(NEEDS_MENU.min, room), Math.min(NEEDS_MENU.ideal, NEEDS_MENU.max, room));
    openMenu({ el: this.needsEl }, items, { label: copyText("tabBar.needs.label"), width });
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
        l2.textContent = copyText("tabBar.machineDown.seen", { ago: sinceText(r.since, now) ?? "0s" });
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
      requestAnimationFrame(run);
    } else {
      window.setTimeout(run, 0);
    }
  }

  /**
   * P7a-3：拿到某集合在主栏里的容器（没有就建）。
   *
   * 组头点一下改名、右侧 `×` 解散。**解散只去掉分组，一个 tab 都不动** ——
   * 集合是个视图，不是容器。
   */
  private groupElFor(col: TabCollection): HTMLElement {
    let g = this.groupEls.get(col.id);
    if (!g) {
      const wrap = document.createElement("div");
      wrap.className = "tab-group";
      const head = document.createElement("div");
      head.className = "tab-group-head";
      const name = document.createElement("button");
      name.type = "button";
      name.className = "tab-group-name";
      name.addEventListener("click", () => this.beginGroupRename(col.id, name));
      const del = document.createElement("button");
      del.type = "button";
      del.className = "tab-group-del";
      del.textContent = copyText("tabBarView.group.dissolve");
      del.title = copyText("tabBarView.group.dissolveHint");
      del.addEventListener("click", () => {
        // 撤得回 ⇒ 不确认：直接解散 ＋ 8 秒撤销。
        const members = [...this.store.tabs.values()].filter((t) => t.group === col.id).map((t) => t.sessionId);
        const before = { ...col };
        void this.prefs.dissolveGroup(col.id);
        this.host.refreshTabBar();
        undoToast(copyText("tabBar.group.dissolved", { name: col.name }), () => {
          void this.prefs.restoreGroup(before, members);
          this.host.refreshTabBar();
        }, () => {});
      });
      head.append(name, del);
      const list = document.createElement("div");
      list.className = "tab-group-list";
      wrap.append(head, list);
      this.listEl.appendChild(wrap);
      g = { wrap, head, list };
      this.groupEls.set(col.id, g);
    }
    // 名字没变就不写 —— 这里每次整刷都走一遍，无条件写就是每个组每次一条 DOM 写。
    const nameEl = g.head.firstElementChild as HTMLElement;
    if (nameEl.textContent !== col.name) nameEl.textContent = col.name;
    return g.list;
  }

  /**
   * P-extra：组头**就地**改名 —— 名字那一格换成 `<input>`：Enter 提交 / Esc 取消 / blur 提交。
   * 原先是 `window.prompt`（原生阻塞弹窗）。
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
        finish(false);
        return true;
      },
    };
    const finish = (commit: boolean): void => {
      if (done) return;
      done = true;
      dispatcher.popOverlay(overlay);
      const next = input.value.trim();
      input.remove();
      name.hidden = false;
      if (!commit || !next) return;
      const now = this.prefs.collections.find((x) => x.id === id);
      if (!now || now.name === next) return;
      void this.prefs.renameGroup(id, next);
      this.host.refreshTabBar();
    };
    input.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" && !ev.isComposing) {
        ev.preventDefault();
        finish(true);
      } else if (ev.key === "Escape") {
        ev.preventDefault();
        finish(false);
      }
    });
    input.addEventListener("blur", () => finish(true));
    name.hidden = true;
    name.after(input);
    dispatcher.pushOverlay(overlay);
    input.focus();
    input.select();
  }

  /** A3：账号快照换了 ⇒ 所有 tab 的账号徽章就地重刷（原是 `setSessionAccounts` 末尾那个循环，逐字）。 */
  refreshAccountBadges(): void {
    for (const [sid, refs] of this.tabButtons) {
      const tab = this.store.tabs.get(sid);
      if (tab) this.updateAccountBadge(refs, sid, tab);
    }
  }

  /**
   * 账号头像：**只在这个会话用的号与那台的默认号不同时出**（全都出 ⇒ 每行一块、信息为零，规范 V2）；
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
    const drawn = `${account.name}\u0000${account.ghost ? 1 : 0}`;
    if (refs.acctDrawn === drawn) return;
    refs.acctDrawn = drawn;
    refs.acctBadge.textContent = "";
    refs.acctBadge.className = "tab-acct-badge";
    refs.acctBadge.appendChild(accountAvatarEl(account.name, { size: 14, ghost: account.ghost }));
    refs.acctBadge.title = copyText("tabBar.hover.account", { name: account.name });
    refs.acctBadge.style.display = "";
  }

  /** 这个会话用的号（`ghost` ＝ 上次用的、不是此刻探到的）；不知道 ⇒ `null`。 */
  private accountOf(sid: string, tab: Tab): { name: string; ghost: boolean } | null {
    if (!shouldShowAccountBadge(tab.origin, this.store.accountReadyOrigins)) return null;
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
    dot.removeAttribute("title"); // 悬停卡说状态；点上不再挂一层原生提示
    const abbr = span("tab-abbr");
    const machine = tag("");
    machine.classList.add("tab-machine");
    machine.style.display = "none";
    const titleEl = span("tab-title");
    const proj = span("tab-proj");
    const bg = span("tab-gear");
    bg.appendChild(icon("settings", "compact"));
    bg.title = copyText("tabBarView.tab.bgHint");
    bg.style.display = "none";
    const label = span("tab-label");
    titleEl.append(proj, bg, label);

    const trail = span("tab-trail");
    const needs = span("tab-needs-word");
    // 额度：被卡住的会话标题后红字 `✕ 5h`（与「等批准」同位）。默认藏着，updateTabButton 按会话的轮换格填。
    const quotaBadge = document.createElement("span");
    quotaBadge.className = qs.tabQuotaBlocked;
    quotaBadge.style.display = "none";
    const badge = span("tab-badge");
    const acctBadge = span("tab-acct-badge");
    acctBadge.style.display = "none";
    // 图钉：纯展示、不可点（固定是右键菜单那一项的事）。可见性交给 `.pinned` 类。
    const pinBadge = span("tab-pin");
    pinBadge.appendChild(icon("pin", "compact"));
    pinBadge.title = copyText("tabBarView.tab.pinHint");
    trail.append(needs, quotaBadge, badge, acctBadge, pinBadge);

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

    return { root, dot, abbr, machine, proj, bg, label, needs, badge, quotaBadge, acctBadge, cwdBtn, pinBadge, drawn: null, acctDrawn: "" };
  }

  /**
   * 悬停卡（C12）：标题全名 · 机器 · 目录 · 状态句（点 ＋ 一句，在等你时琥珀）· peek 一句 · 账号 · 分叉自 · 写入进程 · `5 切到这里`。
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
    card.appendChild(line("tab-hover-title", fullTitle(tab)));
    const where = line("tab-hover-row", machineOf(tab));
    card.appendChild(where);
    if (tab.projectDir) card.appendChild(line("tab-hover-row tab-hover-mono", tab.projectDir));
    const st = stateLine(tab, now);
    const state = document.createElement("div");
    state.className = st.needs ? "tab-hover-state tab-hover-need" : "tab-hover-state";
    const d = dotOf(tab);
    state.append(statusDot(d, dotLabel(d), "compact"), document.createTextNode(st.text));
    card.appendChild(state);
    const peek = peekLine(tab);
    if (peek) card.appendChild(line("tab-hover-peek", peek));
    const acct = this.accountOf(sid, tab);
    if (acct) card.appendChild(line("tab-hover-row", copyText("tabBar.hover.account", { name: acct.name })));
    if (tab.forkedFromSessionId) card.appendChild(line("tab-hover-row", copyText("tabBar.hover.forked", { id: tab.forkedFromSessionId.slice(0, 8) })));
    if (isLive(tab.state) && tab.writers.length > 1) card.appendChild(line("tab-hover-row", copyText("tabBarView.tab.writers", { n: tab.writers.length })));
    const blocked = tabBlockedOf(appStore.sessionRotation.get().get(sid));
    if (blocked) card.appendChild(line("tab-hover-row tab-hover-blocked", blocked.hover));
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
    const actStatus = tab.activity?.status ?? null;
    const lightClass = isLive(tab.state) ? activityLightClass(actStatus) : "";
    const reconnectable = view.reconnectable;
    const dot = dotOf(tab);
    const n = needsOf(tab);
    const needsText = n ? needsWord(n.kind) : "";
    const parts = titleParts(tab);
    const unread = tab.unread > 0 && !active;
    // 未读数只在有未读、又不在等你时出（等你 ＞ 未读数）。
    const unreadText = unread && !n ? (tab.unread > 99 ? "99+" : String(tab.unread)) : "";
    // 空闲、跑完你还没看 ⇒ 标题 600 字重（点开即恢复）。
    const unseenDone = unread && dot === "idle";
    const blocked = tabBlockedOf(appStore.sessionRotation.get().get(sid));
    const titleText = `${parts.forked ? "↳ " : ""}${parts.title}`;
    const abbr = abbrOf(tab);

    const flags = [active, selected, ended, unseen, pinned, hasCwd, remote, lightClass === "act-idle", lightClass === "act-waiting", reconnectable, unreadText !== "", unseenDone, n !== null, parts.bg]
      .map((b) => (b ? "1" : "0"))
      .join("");
    const drawn = [flags, dot, titleText, parts.proj ?? "", unreadText, needsText, blocked?.text ?? "", abbr, remote ? tab.origin : ""].join("\u0000");
    if (refs.drawn !== drawn) {
      refs.drawn = drawn;
      refs.root.classList.toggle("active", active);
      refs.root.classList.toggle("selected", selected);
      refs.root.classList.toggle("ended", ended);
      refs.root.classList.toggle("unseen", unseen);
      refs.root.classList.toggle("pinned", pinned);
      refs.root.classList.toggle("has-cwd", hasCwd);
      refs.root.classList.toggle("remote", remote);
      refs.root.classList.toggle("act-idle", lightClass === "act-idle");
      refs.root.classList.toggle("act-waiting", lightClass === "act-waiting");
      refs.root.classList.toggle("reconnectable", reconnectable);
      refs.root.classList.toggle("has-unread", unreadText !== "");
      refs.root.classList.toggle("unseen-done", unseenDone);
      refs.root.classList.toggle("waiting-you", n !== null);
      // 只写真变了的那几格（一个状态变了 ⇒ 这颗按钮上恰好：类 · 点的样子 · 点的读屏名）。
      if (refs.dot.dataset.state !== dot) {
        refs.dot.dataset.state = dot;
        refs.dot.setAttribute("aria-label", dotLabel(dot));
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
      setText(refs.badge, unreadText);
      setShown(refs.quotaBadge, blocked !== null);
      setText(refs.quotaBadge, blocked?.text ?? "");
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
