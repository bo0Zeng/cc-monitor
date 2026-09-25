/**
 * 〔U2 · 拆 `tabs.ts` ④〕**tab 栏视图**：每个 tab 一颗按钮（状态灯 · 标题 · 账号徽章 · 未读数 · 📌 ·
 * 📂 / ↗ / × 三个子动作）、按集合分的组容器、整刷（删 / 建 / 更新 / 排序）与帧末合批。
 *
 * 只画、只把用户手势转交出去：点按钮切 tab、按下起拖、右键开菜单、子动作按钮 —— 做事的都经
 * `TabBarViewHost` 交给宿主（路由 / 拖拽 / 菜单 / 会话动作），本文件不 import 它们。
 * 读的状态：`TabStore`（tab 集合 · 顺序 · 当前 tab · 账号快照）与 `TabBarPrefs`（集合）。
 *
 * 字段与方法逐字从 `tabs.ts` 搬来（`refreshTabBar` 的主体成了 `refresh`，拖拽守卫留在 `TabManager` 那一层；
 * `scheduleTabBarRefresh` 成了 `scheduleRefresh`），唯一的改写：按钮上那几处转交换成 `this.host.…`。
 */
import { sessionBadge, shouldShowAccountBadge, detectAccountMismatch } from "./accounts";
import { accountAvatarEl } from "./account-color";
import {
  collectionOf,
  deleteCollection,
  renameCollection,
  type TabCollection,
} from "./tab-collections";
import { activityLightClass } from "./session-status";
import { terminalFrontAvailable } from "./terminal-front";
import { isRemoteOrigin } from "./ipc/origin";
import { hasTerminal, isLive, isResumeOnly, stateView } from "./tab-session-state";
import type { Tab } from "./tab-model";
import type { TabStore } from "./tab-store";
import type { TabBarPrefs } from "./tab-bar-prefs";

/** TabButton 的 DOM 引用：refreshTabBar 局部更新依赖这些 ref 避免重新创建 button */
export interface TabButtonRefs {
  root: HTMLButtonElement;
  label: HTMLSpanElement;
  badge: HTMLSpanElement;
  /** A3：账号徽章（该会话属于哪个账号；本地会话不显示，未知显 —）。 */
  acctBadge: HTMLSpanElement;
  cwdBtn: HTMLSpanElement;
  /**
   * 〔步 17·B〕📌 角标。**纯展示，不可点** —— 与 `.tab-badge`（未读数）同族。
   * 固定是右键菜单那一项的事；这里再放一个能点的东西就是同一个动作两个入口。
   */
  pinBadge: HTMLSpanElement;
  /**
   * 〔UP1 · `设计/30 §3` P7〕这颗按钮上次**画出去的样子**（class 开关 ＋ title ＋ 标题 ＋ 未读数拼成一串）。
   * 一样 ⇒ `updateTabButton` 一个 DOM 都不写。按**输出**比而不是按输入比：输入散在 store 的好几张表里，
   * 漏一个就是静默不刷；输出只有这几项。`null` = 还没画过。
   */
  drawn: string | null;
  /**
   * 〔UP1 · P2〕账号徽章上次画出去的样子：`""` = 藏着（`createTabButton` 建出来就是藏着的，所以初值是 `""`
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
  /** 左键按下：候选拖拽。 */
  beginDrag(e: MouseEvent, sid: string, root: HTMLElement): void;
  /** 拖完那一下的 click 不切 tab（一次性消费）。 */
  takeSuppressedClick(sid: string): boolean;
  /** 右键：开这个 tab 的菜单。 */
  openMenu(e: MouseEvent, sid: string): void;
}

export class TabBarView {
  /** sessionId → button DOM refs，避免 refreshTabBar 每次重建整个 bar */
  readonly tabButtons = new Map<string, TabButtonRefs>();
  /**
   * P7a-1（#61）：**归档区**。`#61` 正文自陈「状态机已经有了，缺的是那个「口」」——
   * 那个口就在 [`refreshTabBar`]，它是全仓**唯一**把 tab 按钮塞进 `barEl` 的地方。
   *
   * 三个元素由本类自己建（不改构造签名：那有两个生产调用点 + 一批夹具），
   * 挂在 `barEl` 之后，作为它的兄弟。
   */
  /** 每个集合在主栏里的容器（组头 + 成员列表）。 */
  private readonly groupEls = new Map<string, { wrap: HTMLElement; head: HTMLElement; list: HTMLElement }>();
  /** P7a-1：归档区 UI 建一次。默认折叠（缺省 `"1"`，与 agents/tasks 面板同形态）。 */
  // 🔴 〔步 17·A · 2026-09-19〕**`ensureArchiveUi()` 整个删掉。**
  //
  // `设计/30 §A` 抬头逐字「**已定**：删归档抽屉 · 固定灰 tab」，三条独立理由：
  //   ① 它永久吃 450px 屏宽（`.tab-archive` 是 `#app` 的 grid item 却没认领格子）
  //   ② 它是个撕窗口陷阱（tear-off 判定线对抽屉没有意义）
  //   ③ **它的存在理由本来就自相矛盾** —— 原 `belongsInArchive` 的注释自己写着：
  //      active tab 会「在你正看着它的时候」掉进折叠的抽屉里 ⇒ 已经为 active 开了例外。
  //      把例外推广到全部，抽屉就没了。
  //
  // ⚠ **删的是抽屉，不是状态。** 已结束（〔U4〕`isResumeOnly(tab.state)`）照旧存在，那种 tab
  //   **留在原位变淡**（`.tab.ended`，〔U4〕原名 `.tab.archived`，`§A.3` 逐字「不用新写」）。
  //   用户 2026-09-19 逐字：「没有归档这个东西，不要归档，就是灰 tab。」

  /**
   * 〔UP1 · `设计/30 §3` P8〕按钮根 → sid。事件委托靠它从 `closest(".tab")` 找回 sid（不往 DOM 上加属性）。
   * `WeakMap`：按钮被摘掉之后这一条跟着它一起被回收，不用另外清。
   */
  private readonly sidOf = new WeakMap<Element, string>();

  constructor(
    private readonly store: TabStore,
    private readonly prefs: TabBarPrefs,
    private readonly barEl: HTMLElement,
    private readonly host: TabBarViewHost,
  ) {
    // 〔UP1 · `设计/30 §3` P8〕**事件委托**：整条栏只在 `barEl` 上挂三个监听器，不再每个 tab 挂 10 个
    // （原先 📂 / ↗ / × 各 click ＋ mousedown，根上 click ＋ 两个 mousedown ＋ contextmenu）。
    //
    // 原先三颗子按钮上的 `stopPropagation` 有两层意思，这里都保住：
    // ① 子按钮上按下**不起拖、不触发中键关** ⇒ mousedown 先看目标在不在子按钮里；
    // ② 子按钮的 click / mousedown **不往上冒到 `document`** ⇒ 这里照样 `stopPropagation()`
    //   （`barEl` 与按钮之间只隔组容器，组容器上没有 click / mousedown 监听）。
    barEl.addEventListener("click", (e) => this.onBarClick(e));
    barEl.addEventListener("mousedown", (e) => this.onBarMouseDown(e));
    barEl.addEventListener("contextmenu", (e) => this.onBarContextMenu(e));
  }

  /** 这个事件落在哪颗 tab 按钮上、是不是落在它的某颗子按钮（📂 / ↗ / ×）上。不是 tab 按钮 ⇒ `null`。 */
  private hitOf(e: Event): { sid: string; root: HTMLElement; sub: Element | null } | null {
    const t = e.target as Element | null;
    if (!t || typeof t.closest !== "function") return null;
    const root = t.closest(".tab");
    if (!root) return null;
    const sid = this.sidOf.get(root);
    if (sid === undefined) return null;
    const sub = t.closest(".tab-cwd, .tab-focus, .tab-close");
    return { sid, root: root as HTMLElement, sub: sub && root.contains(sub) ? sub : null };
  }

  private onBarClick(e: MouseEvent): void {
    const hit = this.hitOf(e);
    if (!hit) return;
    const { sid, sub } = hit;
    if (sub) {
      e.stopPropagation();
      if (sub.classList.contains("tab-cwd")) {
        // 📂 打开工作目录（cwd）—— 系统默认文件管理器
        void this.host.openTabCwd(sid);
      } else if (sub.classList.contains("tab-focus")) {
        // ↗ 拉对应终端窗口。〔第二波 T4 · LF1〕非 Windows 不渲（`terminal-front.ts`）——不渲就点不到。
        const t = this.store.tabs.get(sid);
        if (!t || !hasTerminal(t.state)) return; // 〔U4〕活着，或可重连（ssh 窗还在）才拉
        // Feature ②：远端 Tab → 后端唯一分派点（先启动令牌、后 ccm-rbind 标题退路）；
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
    this.host.switchTo(sid);
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
      // 〔步 17·A〕原先这里有一条「归档区里的 tab 不参与拖拽」的例外 ——
      // 抽屉没了，那条例外自动不需要（`§A.3` 逐字「净收益」）。
      this.host.beginDrag(e, hit.sid, hit.root);
      return;
    }
    // 中键点击已结束的 Tab 也关闭（常见 UX）
    if (e.button === 1) {
      const t = this.store.tabs.get(hit.sid);
      if (t && isResumeOnly(t.state)) {
        e.preventDefault();
        this.host.closeTab(hit.sid);
      }
    }
  }

  /** issue #10：右键菜单「在新窗口打开」（双屏 / 并排）。〔U2〕菜单里放哪几项住 `tab-menu.ts`。 */
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
    // 〔步 17·A〕抽屉没了 ⇒ 只剩主栏 ＋ 按集合分的若干组
    // ⇒ 推广成「**每容器一个游标**」。
    // 组容器按集合顺序先摆好（空集合也留着 —— 用户刚建的集合不该看不见）。
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
    // ⇒ 把 `barEl` 的起点定在最后一个组容器上（没有组则回到 `firstChild` 语义）。
    // 〔UP1 · `设计/30 §3` P6〕「最后一个组容器」不再把 `barEl.children` 物化成数组去找。
    // 组容器只在建的那一刻 `appendChild` 到 `barEl` 末尾、之后从不挪（挪的只有 tab 按钮），
    // 删的时候同时出 `groupEls` ⇒ **`groupEls` 的插入序就是组容器在 DOM 里的顺序**，最后一个就是它。
    let lastGroup: HTMLElement | null = null;
    for (const g of this.groupEls.values()) lastGroup = g.wrap;
    if (lastGroup) cursors.set(this.barEl, lastGroup);
    for (const sid of this.store.orderedIds) {
      const tab = this.store.tabs.get(sid);
      if (!tab) continue;
      let refs = this.tabButtons.get(sid);
      if (!refs) {
        refs = this.createTabButton(sid);
        this.tabButtons.set(sid, refs);
      }
      this.updateTabButton(refs, sid, tab);
      // 〔步 17·A〕分流从三路（抽屉 / 组 / 主栏）降到**两路**（组 / 主栏）。
      // 「归档优先于集合」那条判定整条消失 ⇒ **灰 tab 也能在组里**（`§A.3` 逐字）。
      const col = collectionOf(this.prefs.collections, sid);
      const host = col ? this.groupElFor(col) : this.barEl;
      // 排序：希望此 button 出现在**同容器内**前一个之后。
      const prev = cursors.get(host) ?? null;
      const targetNext: ChildNode | null = prev ? prev.nextSibling : host.firstChild;
      if (refs.root !== targetNext || refs.root.parentElement !== host) {
        host.insertBefore(refs.root, targetNext);
      }
      cursors.set(host, refs.root);
    }

    this.store.notify();
  }

  /**
   * `refreshTabBar` 的**帧末合批**入口〔audit-0805 F15〕。
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
      // 〔U2〕经宿主刷 —— `TabManager.refreshTabBar`（拖拽守卫在那里）；判据会把它换成计数替身。
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
      name.addEventListener("click", () => {
        const cur = this.prefs.collections.find((x) => x.id === col.id);
        const next = window.prompt("集合名:", cur?.name ?? "");
        if (next === null) return;
        void this.prefs.commitCollections(renameCollection(this.prefs.collections, col.id, next));
      });
      const del = document.createElement("button");
      del.type = "button";
      del.className = "tab-group-del";
      del.textContent = "×";
      del.title = "解散这个集合（只去掉分组，会话一个都不会关）";
      del.addEventListener("click", () => {
        void this.prefs.commitCollections(deleteCollection(this.prefs.collections, col.id));
      });
      head.append(name, del);
      const list = document.createElement("div");
      list.className = "tab-group-list";
      wrap.append(head, list);
      this.barEl.appendChild(wrap);
      g = { wrap, head, list };
      this.groupEls.set(col.id, g);
    }
    // 〔UP1〕名字没变就不写 —— 这里每次整刷都走一遍，无条件写就是每个组每次一条 DOM 写。
    const nameEl = g.head.firstElementChild as HTMLElement;
    if (nameEl.textContent !== col.name) nameEl.textContent = col.name;
    return g.list;
  }

  /** A3：账号快照换了 ⇒ 所有 tab 的账号徽章就地重刷（原是 `setSessionAccounts` 末尾那个循环，逐字）。 */
  refreshAccountBadges(): void {
    for (const [sid, refs] of this.tabButtons) {
      const tab = this.store.tabs.get(sid);
      if (tab) this.updateAccountBadge(refs, sid, tab);
    }
  }

  /** 单个 tab 的账号徽章就地重刷（in-flight 状态变化时用；tab 已没了就静默跳过）。 */
  refreshAccountBadgeFor(sid: string): void {
    const refs = this.tabButtons.get(sid);
    const tab = this.store.tabs.get(sid);
    if (refs && tab) this.updateAccountBadge(refs, sid, tab);
  }

  /**
   * F09（R7 语义反转）：账号徽章从"仅不一致时才显示的警示信号"改为"账号已知即恒显示的身份
   * 标识"——门（`shouldShowAccountBadge`）通过 + 账号已知（源③已知除外）就显示，不再要求
   * `detectAccountMismatch` 为真。旧版"不一致才显示"这条信息没有消失，只是从"触发显示的唯一
   * 条件"降级为"视觉区分的一个维度"：一致态也显示头像（用户能一眼看出这个会话归属哪个账号），
   * 不一致态仍用 tooltip 追加"与当前账号不一致"提示，且沿用既有 live 实心/last 幽灵区分
   * （不新增视觉语言）。⇄ 一键对齐按钮随对齐全套一并删除（见 features/F09-ui-convergence.md
   * §1"不做什么"——批量/一键对齐是组合层便利,不做等价替代,用户改走 flyout 逐会话操作）。
   */
  private updateAccountBadge(refs: TabButtonRefs, sid: string, tab: Tab): void {
    // 〔UP1 · `设计/30 §3` P2〕先算出**要画成什么样**，与上次画出去的一样就一个 DOM 都不写。
    // 原先每次整刷都 `textContent=""` ＋ 新建一个头像 span（3 次内联样式写）＋ 写 title ＋ 写 display，
    // 20 个 tab 就是每次整刷 20 个新 span、20 个旧 span 变垃圾。
    const hide = (): void => {
      if (refs.acctDrawn === "") return;
      refs.acctDrawn = "";
      refs.acctBadge.textContent = "";
      refs.acctBadge.className = "tab-acct-badge";
      refs.acctBadge.style.display = "none";
    };
    if (!shouldShowAccountBadge(tab.origin, this.store.accountReadyOrigins)) return hide();
    const b = sessionBadge(
      sid,
      tab.origin,
      this.store.sessionAccountsByS,
      this.store.accountEmailByName,
      this.store.accountLastByS,
    );
    if (!b || !b.account) return hide(); // 未知账号（源③）→ 退 hover；顺带把 b.account 窄化为 string
    const ghost = b.source === "last";
    const current = isRemoteOrigin(tab.origin)
      ? this.store.currentByOrigin.get(tab.origin) ?? null
      : null;
    const mismatch = detectAccountMismatch(b.account, current);
    // 头像是 `(账号名, 幽灵态)` 的纯函数（`account-color.ts::accountAvatarEl`）；提示文字是
    // `(b.tooltip, 不一致时的当前账号)` 的纯函数（下面那一行）⇒ 这几样就是全部输出。
    const drawn = `${b.account}\u0000${ghost ? 1 : 0}\u0000${b.tooltip}\u0000${mismatch ? current : ""}`;
    if (refs.acctDrawn === drawn) return;
    refs.acctDrawn = drawn;
    refs.acctBadge.textContent = "";
    refs.acctBadge.className = "tab-acct-badge";
    refs.acctBadge.appendChild(accountAvatarEl(b.account, { size: 14, ghost }));
    refs.acctBadge.title = mismatch ? `${b.tooltip} · 与当前账号「${current}」不一致` : b.tooltip;
    refs.acctBadge.style.display = "";
  }

  private createTabButton(sid: string): TabButtonRefs {
    // 〔UP1 · P8〕按钮本身**一个监听器都不挂** —— 手势全在 `barEl` 上委托（见构造体）。
    const root = document.createElement("button");
    root.className = "tab";
    this.sidOf.set(root, sid);

    const dot = document.createElement("span");
    dot.className = "live-dot";
    root.appendChild(dot);

    const label = document.createElement("span");
    label.className = "tab-title";
    root.appendChild(label);

    // A3：账号徽章（该会话属于哪个账号）。默认隐藏，updateTabButton 按 sessionBadge 填。
    const acctBadge = document.createElement("span");
    acctBadge.className = "tab-acct-badge";
    acctBadge.style.display = "none";
    root.appendChild(acctBadge);

    const badge = document.createElement("span");
    badge.className = "tab-badge";
    root.appendChild(badge);

    // 〔步 17·B〕📌 固定角标。默认不显（CSS `.tab:not(.pinned) .tab-pin { display:none }`），
    // `updateTabButton` 只翻 `.pinned` 这一个类 —— 与其它 5 个子元素同一套「一次性 append、
    // 可见性交给 class」的形状（见 `.tab .tab-badge` 那条注释）。
    const pinBadge = document.createElement("span");
    pinBadge.className = "tab-pin";
    pinBadge.textContent = "📌";
    pinBadge.title = "已固定：关掉 app 再打开它还在";
    root.appendChild(pinBadge);

    // 📂 打开工作目录（cwd）—— 系统默认文件管理器
    const cwdBtn = document.createElement("span");
    cwdBtn.className = "tab-cwd";
    cwdBtn.textContent = "📂";
    cwdBtn.title = "打开工作目录 (E)";
    root.appendChild(cwdBtn);

    // ↗ 拉对应终端窗口（v1.7 用 sid_hwnd_cache）。〔第二波 T4 · LF1〕非 Windows 不渲（`terminal-front.ts`）。
    if (terminalFrontAvailable()) {
      const focusBtn = document.createElement("span");
      focusBtn.className = "tab-focus";
      focusBtn.textContent = "↗";
      focusBtn.title = "调出对应终端 (`)";
      root.appendChild(focusBtn);
    }

    const closeBtn = document.createElement("span");
    closeBtn.className = "tab-close";
    closeBtn.textContent = "×";
    closeBtn.title = "关闭 Tab";
    root.appendChild(closeBtn);

    return { root, label, badge, acctBadge, cwdBtn, pinBadge, drawn: null, acctDrawn: "" };
  }

  private updateTabButton(refs: TabButtonRefs, sid: string, tab: Tab): void {
    // 〔UP1 · `设计/30 §3` P7〕先把**要画成什么样**整个算出来，与上次画出去的比 —— 一样就一个 DOM 都不写。
    //
    // ⚠ 下面那几个 `classList.toggle(x, 布尔)` 状态没变时本来就不写（DOM 规范：`force` 与现状一致
    //   直接返回，不跑 update steps、不排 mutation record）；**真在每次整刷里写 DOM 的是 `title`**
    //   （属性赋值不管值变没变都写）。这一段早退省下的是那次写 ＋ 这一堆字符串拼接。
    const active = sid === this.store.activeId;
    // 〔U4〕两个轴怎么画（类 · 提示句）只从 `stateView` 取：已结束（只能 resume）· 可重连（死了、容器还在）。
    const view = stateView(tab.state);
    const ended = view.ended;
    // 〔步 17·B〕固定：**只多一个 📌 角标，位置一个字不动**（`§B.3b`：没有「固定区」，
    // pin 管的是「别丢」不是「排前面」；位置由 `§C` 的顺序落盘管，两者不抢）。
    const pinned = tab.pinned;
    const hasCwd = !!tab.cwd;
    // FIX 5 / Feature ②（issue #15）：远端 Tab 的 cwd 是 Pi 上的路径，
    // 本地不存在，故 .remote 类只隐藏「打开工作目录」📂（CSS）。「调出终端」↗ 现在保留
    // 给远端 —— 点击走 bringRemoteTerminalToFront（后端按 ccm-rbind 拉本地 ssh 窗口）。
    const remote = isRemoteOrigin(tab.origin);
    // Batch7-F24：bg 任务 tab——缩进 + ⌞ 前缀由 CSS 承担
    const bg = tab.kind !== null && tab.kind !== "interactive";
    // issue #23 红绿灯：busy=绿（.live-dot 默认色）/ idle·shell=红 / waiting=黄。
    // activity 为 null（旧版 CC / 远端 v1）不加类 → 维持现状绿点。
    // F91：语义抽到 session-status.ts 供 tab-bar 与 mission-control grid 共用（逐字节等价）。
    const actStatus = tab.activity?.status ?? null;
    const lightClass = activityLightClass(actStatus);
    // audit-fixes F03.2：可重连（claude 退但 tmux 会话仍在）。灯不被 `.ended` 隐藏，
    // `.reconnectable` 把 .live-dot 覆写成暗色、压过红绿黄。
    const reconnectable = view.reconnectable;
    const titleParts: string[] = [];
    // 〔U4〕第一行说状态（活着不说）。
    if (view.tooltip !== null) titleParts.push(view.tooltip);
    // 「等待操作」只对活着的会话说：可重连的会话 claude 已经没了，留着的活动信号是陈旧的
    // （改两轴之前灯被 CSS 盖住了，tooltip 却还挂着这一句）。
    if (isLive(tab.state) && actStatus === "waiting" && tab.activity?.waitingFor) {
      titleParts.push(`等待操作：${tab.activity.waitingFor}`);
    }
    // issue #63①：fork 会话在 tooltip 里标出血缘(徽标 `↳` 在标题上、来源 sid 在此)。
    if (tab.forkedFromSessionId) {
      titleParts.push(`↳ 从 ${tab.forkedFromSessionId.slice(0, 8)} fork 而来`);
    }
    const title = titleParts.join("\n");
    const unread = tab.unread > 0 && !active;
    // 未读数只在有未读时写（没未读时徽标由 CSS 藏，文字留着上一次的，与原来逐字相同）。
    const unreadText = unread ? (tab.unread > 99 ? "99+" : String(tab.unread)) : "";

    const flags = [
      active,
      ended,
      pinned,
      hasCwd,
      remote,
      bg,
      lightClass === "act-idle",
      lightClass === "act-waiting",
      reconnectable,
      unread,
    ]
      .map((b) => (b ? "1" : "0"))
      .join("");
    const drawn = `${flags}\u0000${title}\u0000${tab.title}\u0000${unreadText}`;
    if (refs.drawn !== drawn) {
      refs.drawn = drawn;
      refs.root.classList.toggle("active", active);
      refs.root.classList.toggle("ended", ended);
      refs.root.classList.toggle("pinned", pinned);
      refs.root.classList.toggle("has-cwd", hasCwd);
      refs.root.classList.toggle("remote", remote);
      refs.root.classList.toggle("tab-bg", bg);
      refs.root.classList.toggle("act-idle", lightClass === "act-idle");
      refs.root.classList.toggle("act-waiting", lightClass === "act-waiting");
      refs.root.classList.toggle("reconnectable", reconnectable);
      if (refs.root.title !== title) refs.root.title = title;
      refs.root.classList.toggle("has-unread", unread);
      if (refs.label.textContent !== tab.title) {
        refs.label.textContent = tab.title;
      }
      if (unread && refs.badge.textContent !== unreadText) {
        refs.badge.textContent = unreadText;
      }
    }
    this.updateAccountBadge(refs, sid, tab); // A3：账号徽章随 tab 更新一并刷新
  }
}
