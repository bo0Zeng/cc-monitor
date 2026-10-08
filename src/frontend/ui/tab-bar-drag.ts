/**
 * tab 栏上的拖拽状态机：栏内拖动排序 · 压住停留成组 / 拖进拖出组 ·
 * 拖出右缘松手撕成独立窗口。
 *
 * 「落在哪」的算术全在 `tab-drop.ts`（纯函数，判据直接打它）；这里只管量尺寸、挂 / 摘 document 监听、
 * ghost 与落点标记、停留计时器，以及松手那一拍把「顺序 ＋ 集合归属」一次落实（顺序落盘与集合落盘走
 * `tab-bar-prefs.ts`）。还有一件与 tab 栏视图的约定：**拖拽进行中不重排 tab 栏**（`deferRefresh`），
 * 被挡下的刷新收尾时补一次。
 */
import {
  groupMoveForDrop,
  defaultGroupName,
  moveTab,
  pickDropTarget,
  tabUnderY,
  DWELL_MS,
  DWELL_MOVE_PX,
  type DropTarget,
  type TabRect,
} from "./tab-drop";
import { newCollectionId } from "./tab-collections";
import type { TabStore } from "./tab-store";
import { sayCollectionRefusal, type TabBarPrefs } from "./tab-bar-prefs";
import { copyText } from "./copy-table";

/** 拖拽要宿主做的两件事。 */
export interface TabBarDragHost {
  refreshTabBar(): void;
  /** 拖出右缘松手 ⇒ 在落点屏幕坐标开独立窗口（与右键「在新窗口打开」同一个动作）。 */
  openInNewWindow(sid: string, screenX?: number, screenY?: number): Promise<void>;
}

export class TabBarDrag {
  /**
   * Tab 撕离（tear-off）拖拽状态机。同一时刻只允许一个拖拽，整段存这里。
   * - mousedown（左键，非子动作按钮）记录起点 → 候选拖拽（dragging=false）
   * - document mousemove 越过 6px 阈值 → dragging=true，建 ghost、源 Tab 变暗
   * - 指针拖离竖栏右缘（clientX > barRight + 16）→ armed=true（松手即弹窗）
   * - document mouseup：armed → openInNewWindow(落点)；否则取消。两种情况都抑制后续 click
   * null = 当前无拖拽。
   */
  private drag: {
    sid: string;
    /**
     * 松手时的落点。**三种语义**（`§D.3`）——在这之前这里是
     * `dropBefore?: string | null`（只有「插到谁之前」与「末尾」两种）。只在**未 armed** 时有意义。
     */
    dropTarget: DropTarget;
    /** 指针此刻压着谁（停留计时的对象）。`null` = 没压在任何 tab 上。 */
    dwellSid: string | null;
    /** 停留已经攒满的那个 sid。计时器到点才写，抖动 / 换目标即清回 `null`。 */
    dwellArmed: string | null;
    /** 本轮停留的锚点。指针离它超过 `DWELL_MOVE_PX` 就重新计时。 */
    dwellX: number;
    dwellY: number;
    /** 停留计时器句柄。`teardownDrag` 必须清 —— 否则它会在拖拽结束后才到点。 */
    dwellTimer: number | null;
    startX: number;
    startY: number;
    barRight: number;
    root: HTMLElement;
    dragging: boolean;
    armed: boolean;
    ghost: HTMLElement | null;
    /**
     * 栏里每个 tab 的矩形，**起拖后第一次要用时量一次**，之后 mousemove 只做算术。
     * `null` = 该量了。每次 mousemove 都量的话，前面紧挨着一次写（`ghost.style.left/top`），每次都是一次强制同步布局。
     *
     * 什么时候会过期、过期了怎么办：
     * - 整刷挪节点 —— 拖拽期间整刷挂起（`deferRefresh`）⇒ 不会发生；
     * - tab 栏滚动（按着拖的时候转滚轮）/ 窗口尺寸变 ⇒ `onInvalidate` 置回 `null`，下一次 mousemove 重量。
     * ⚠ 被标成 `drop-onto` 的那个 tab 有 `transform: scale(1.03)`，会改它自己的 `getBoundingClientRect`；
     *   缓存量的是没放大那一刻的矩形 ⇒ 停留判定不会被自己放大后的矩形带偏。
     */
    rects: TabRect[] | null;
    onInvalidate: () => void;
    onMove: (e: MouseEvent) => void;
    onUp: (e: MouseEvent) => void;
  } | null = null;
  /**
   * 此刻打着落点标记的是谁（`drop-before` / `drop-onto` 各一个）。
   * 换标记只动旧的与新的那两个，不遍历全部按钮。
   */
  private marked: { before: string | null; onto: string | null } = { before: null, onto: null };
  /**
   * ★ 6d：拖拽进行中有人要求刷 tab 栏 —— 记一笔，`teardownDrag` 收尾时补一次。
   * 只是一个「有没有」，不记是谁要求的：`refreshTabBar` 本来就是整栏重刷，补一次就够。
   */
  private tabBarDirtyDuringDrag = false;
  /**
   * 拖拽撕离阈值：指针移动超过此像素才判定为"拖"，否则视为普通点击。
   */
  private static readonly DRAG_THRESHOLD_PX = 6;
  /**
   * 拖拽结束后需抑制掉紧随 mouseup 的那次 click 的 sid（避免拖完又误切 Tab）。
   * null = 不抑制。click handler 命中后清零（一次性）。
   */
  private suppressClickSid: string | null = null;

  constructor(
    private readonly store: TabStore,
    private readonly prefs: TabBarPrefs,
    private readonly barEl: HTMLElement,
    /** tab 栏视图那张「sid → 按钮」表（同一个 Map 实例；这里只读 `root`）。 */
    private readonly buttons: ReadonlyMap<string, { root: HTMLElement }>,
    private readonly host: TabBarDragHost,
  ) {}

  /**
   * ★ 6d：tab 栏视图每次要整刷之前先问这一句 —— **拖拽进行中（真起拖了）就别刷**，
   * 记一笔脏，`teardownDrag` 收尾时补一次。返回 `true` = 这次挡下了。
   * ⚠ 守的是 `dragging` 而不是「有没有 `drag`」：按下还没动那一段照常刷新（与点击没有区别）。
   */
  deferRefresh(): boolean {
    if (this.drag?.dragging) {
      this.tabBarDirtyDuringDrag = true;
      return true;
    }
    return false;
  }

  /** 拖拽刚结束的那次 click 不切 Tab（drag-then-release ≠ 选中）。一次性消费：命中返回 `true` 并清掉。 */
  takeSuppressedClick(sid: string): boolean {
    if (this.suppressClickSid === sid) {
      this.suppressClickSid = null;
      return true;
    }
    return false;
  }

  /**
   * Tab 撕离拖拽起点（左键 mousedown）。只是"候选"：记录起点 + 挂 document 级
   * mousemove/mouseup，等指针越过阈值才真正进入拖拽。子动作按钮（📂/↗/×）的
   * mousedown 已 stopPropagation，不会走到这里。
   */
  begin(e: MouseEvent, sid: string, root: HTMLElement): void {
    // 已有拖拽在进行（理论上不会，因 mouseup 会清）—— 防御性忽略。
    if (this.drag) return;
    // 新一轮交互开始：清掉可能残留的抑制标记，避免陈旧 flag 误吞下次 click。
    this.suppressClickSid = null;

    const barRight = this.barEl.getBoundingClientRect().right;
    const onMove = (ev: MouseEvent): void => this.onDragMove(ev);
    const onUp = (ev: MouseEvent): void => this.onDragUp(ev);
    const onInvalidate = (): void => {
      if (this.drag) this.drag.rects = null;
    };
    this.drag = {
      sid,
      startX: e.clientX,
      startY: e.clientY,
      barRight,
      root,
      dragging: false,
      armed: false,
      dropTarget: { kind: "end" },
      dwellSid: null,
      dwellArmed: null,
      dwellX: e.clientX,
      dwellY: e.clientY,
      dwellTimer: null,
      ghost: null,
      rects: null,
      onInvalidate,
      onMove,
      onUp,
    };
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
    // 缓存的矩形只在这两件事上过期（见 `rects` 头注）。捕获阶段挂在 `barEl` 上：
    // `scroll` 不冒泡，捕获才收得到栏里任何一层滚动。
    this.barEl.addEventListener("scroll", onInvalidate, true);
    window.addEventListener("resize", onInvalidate);
  }

  /** document mousemove：阈值判定 → 起拖（建 ghost / 变暗），随后跟随 + arm 检测。 */
  private onDragMove(e: MouseEvent): void {
    const d = this.drag;
    if (!d) return;

    // 容错：主键已松开（mouseup 在窗口外丢失，比如拖到别的 app 上释放）→ 收尾取消，
    // 不弹窗（落点不可信），避免 ghost 残留 + 拖拽状态卡死。下次按下会重新开始。
    if ((e.buttons & 1) === 0) {
      const wasDragging = d.dragging;
      const sid = d.sid;
      this.teardownDrag();
      if (wasDragging) this.suppressClickSid = sid;
      return;
    }

    if (!d.dragging) {
      const dx = e.clientX - d.startX;
      const dy = e.clientY - d.startY;
      if (Math.hypot(dx, dy) <= TabBarDrag.DRAG_THRESHOLD_PX) return;
      // 越过阈值 → 正式起拖：阻止文本选区、建 ghost、源 Tab 变暗。
      e.preventDefault();
      d.dragging = true;
      d.root.classList.add("dragging");
      const ghost = document.createElement("div");
      ghost.className = "tab-drag-ghost";
      ghost.textContent = this.store.tabs.get(d.sid)?.title ?? "";
      document.body.appendChild(ghost);
      d.ghost = ghost;
    }

    // 跟随光标（偏右下避免压在指针正下方）。
    if (d.ghost) {
      d.ghost.style.left = `${e.clientX + 8}px`;
      d.ghost.style.top = `${e.clientY + 8}px`;
    }

    // 栏内重排走 `clientY`，撕窗口（arm）只看 `clientX` —— 两者不争同一根轴。
    // 这里先更新停留状态，再算落点 —— 落点的 `onto` 那一支要读停留的结论。
    this.updateDwell(e.clientX, e.clientY);
    d.dropTarget = this.computeDropTarget(e.clientY);
    // ★ **落点要看得见**〔D 阶段补审〕：只搬不指示的话，「拖动排序」是一次盲操作 ——
    // 用户松手前不知道会落在哪，只能松开看结果、错了再拖一次。
    // armed（拖出右缘）时不指示：那一路根本不重排，指一条不会发生的落点是在骗人。
    this.markDropTarget(e.clientX > d.barRight + 16 ? null : d.dropTarget);

    // arm：指针拖离竖栏右缘一段距离 = 松手即弹独立窗口。
    const armed = e.clientX > d.barRight + 16;
    if (armed !== d.armed) {
      d.armed = armed;
      if (d.ghost) {
        d.ghost.classList.toggle("armed", armed);
        d.ghost.textContent = armed
          ? copyText("tabBarDrag.onDragMove.detachHint")
          : (this.store.tabs.get(d.sid)?.title ?? "");
      }
    }
  }

  /**
   * 量一遍栏里每个 tab 在纵轴上占的那一段。用 `barEl.contains(...)`：组里的 tab 照常参与（拖得进也拖得出组），栏外的东西不参与。
   * 返回纯数据 —— 判定在 `pickDropTarget`（纯函数）；「一次拖拽只量一次」住 `dragRects`（缓存 ＋ 滚动 / 改尺寸作废）。
   */
  tabRects(): TabRect[] {
    const out: TabRect[] = [];
    for (const [sid, refs] of this.buttons) {
      if (!this.barEl.contains(refs.root)) continue;
      const r = refs.root.getBoundingClientRect();
      out.push({ sid, top: r.top, height: r.height });
    }
    return out;
  }

  /** 拖拽期间用的矩形：缓存里有就用缓存，没有（刚起拖 / 刚过期）才量一次。 */
  private dragRects(): TabRect[] {
    const d = this.drag;
    if (!d) return this.tabRects();
    if (d.rects === null) d.rects = this.tabRects();
    return d.rects;
  }

  /** 现在的落点。三种语义的判定住 `pickDropTarget`，这里只负责喂它读数。 */
  private computeDropTarget(clientY: number): DropTarget {
    const d = this.drag;
    if (!d) return { kind: "end" };
    return pickDropTarget(
      this.dragRects(),
      clientY,
      d.sid,
      d.dwellArmed,
    );
  }

  /**
   * 停留（dwell）判定：压住 ≥ 250ms 且抖动 < 4px ⇒ 切进 `onto` 态。
   * 用计时器，不在 `mousemove` 里数时间：指针停住之后 `mousemove` 就不来了。
   *
   * 三种情况清零重来：
   * ① 换了压着的 tab；② 没压在任何 tab 上；③ 还在同一个上但离锚点超过 `DWELL_MOVE_PX`。
   */
  private updateDwell(clientX: number, clientY: number): void {
    const d = this.drag;
    if (!d) return;
    const hovered = tabUnderY(this.dragRects(), clientY, d.sid);
    const moved = Math.hypot(clientX - d.dwellX, clientY - d.dwellY);
    if (hovered === d.dwellSid && moved < DWELL_MOVE_PX) return; // 还在攒，别打断计时
    d.dwellSid = hovered;
    d.dwellArmed = null;
    d.dwellX = clientX;
    d.dwellY = clientY;
    if (d.dwellTimer !== null) window.clearTimeout(d.dwellTimer);
    d.dwellTimer = null;
    if (hovered === null) return;
    // 调度：一次性 —— 拖动时压住一个标签页满 250ms 才切成「与它成组」；换目标 / 收尾时清
    d.dwellTimer = window.setTimeout(() => {
      const cur = this.drag;
      // 计时器到点时拖拽可能已经结束 / 已经换了目标 —— 两者都不许再改状态。
      if (!cur || cur.dwellTimer === null || cur.dwellSid !== hovered) return;
      cur.dwellTimer = null;
      cur.dwellArmed = hovered;
      // **可逆可见**（`§D.4` 理由 3）：攒满的那一刻就给反馈，用户看到了再松手。
      // 指针停着不动 ⇒ 不会再有 `mousemove` 来重算落点，所以这里自己算一次。
      cur.dropTarget = { kind: "onto", sid: hovered };
      if (!cur.armed) this.markDropTarget(cur.dropTarget);
    }, DWELL_MS);
  }

  /**
   * 给落点那个 tab 打标（`null` = 不指示，armed 那一路用）：`before` 顶部一条线 · `onto` 整块描边 ＋ 轻微放大；`end` 不标（末尾没有可描的对象）。
   */
  private markDropTarget(target: DropTarget | null): void {
    const beforeSid = target?.kind === "before" ? target.sid : null;
    const ontoSid = target?.kind === "onto" ? target.sid : null;
    // 只动旧的与新的那两个。
    const flip = (sid: string | null, cls: string, on: boolean): void => {
      if (sid !== null) this.buttons.get(sid)?.root.classList.toggle(cls, on);
    };
    if (this.marked.before !== beforeSid) {
      flip(this.marked.before, "drop-before", false);
      flip(beforeSid, "drop-before", true);
      this.marked.before = beforeSid;
    }
    if (this.marked.onto !== ontoSid) {
      flip(this.marked.onto, "drop-onto", false);
      flip(ontoSid, "drop-onto", true);
      this.marked.onto = ontoSid;
    }
  }

  /**
   * 把一次落点的**全部后果**落实：顺序 ＋ 集合归属，一拍做完。
   *
   * 🔴 **两件事不许分两拍** —— 顺序没变（拖回原位）但归属变了（从组里拖出来）是
   *   真实情形；反过来也是。谁先 `return`，另一半就静默丢了。
   *   在这之前这里叫 `applyReorder`，只管顺序、`if (没变化) return` 直接结束。
   */
  applyDrop(sid: string, target: DropTarget): void {
    // 只拖被按下的那一个（bg tab 与普通 tab 拖法相同）。
    // ① 集合归属跟着落点宿主走（「拖出组」与「拖进组」是同一条规则的两侧）。
    // 组员关系是 tab 自己的属性 ⇒ 只改被拖那个 tab（现建组时连落点那个）的 `group`，
    //   先改内存（下面统一重画一次），再由落盘偏好把那几条补丁一次写掉；归属没变（`stay`）⇒ 零写。
    if (this.prefs.collectionsLoaded) {
      const move = groupMoveForDrop((s) => this.store.tabs.get(s)?.group ?? null, sid, target);
      if (move.kind === "found") {
        const name = defaultGroupName(
          this.store.tabs.get(sid)?.projectDir ?? null,
          this.store.tabs.get(move.with)?.projectDir ?? null,
          this.prefs.collections.map((c) => c.name),
        );
        // 组数到上界、没建出来 ⇒ 说出来；顺序那一半照常做。
        const why = this.prefs.foundGroup([move.with, sid], name, newCollectionId());
        if (why) sayCollectionRefusal(why);
      } else if (move.kind === "join") {
        void this.prefs.joinGroup(sid, move.gid);
      } else if (move.kind === "leave") {
        void this.prefs.leaveGroup(sid);
      }
    }
    // ② 顺序。`onto` 的落位 = 插到目标**之前**（组里成员的相对次序由 `orderedIds` 定，
    //    见 `refreshTabBar`）；`end` 是末尾。
    const beforeSid = target.kind === "end" ? null : target.sid;
    const next = moveTab(this.store.orderedIds, sid, beforeSid);
    if (next.length !== this.store.orderedIds.length) {
      this.host.refreshTabBar(); // 防御：被拖的 sid 已不在顺序里（拖拽中 tab 没了）就只重画（①可能已经改了归属）
      return;
    }
    if (next.every((x, i) => x === this.store.orderedIds[i])) {
      this.host.refreshTabBar();
      return;
    }
    this.store.orderedIds = next;
    this.host.refreshTabBar();
    // 🔴 **拖动的结果要落盘** —— 「今天拖了白拖」。
    //   在这之前 `orderedIds` 的 8 个写入点零持久化，而集合（`tabCollections`）是落盘的
    //   ⇒ 同一个栏里两种寿命：**你建的分组活过重启，你拖的顺序活不过**。
    //   `tab-collections.ts` 立集合落盘的理由是「用户手写的真相，不是能重算的缓存」，
    //   而拖动排序**完全符合那条判据** ⇒ 不给它同样的待遇，那条理由就是选择性适用的。
    // ⚠ 形状照分组那几个动作（`tab-bar-prefs.ts`）：**先改内存再落盘**（上面两行已做完），
    //   落盘失败只记日志 —— 顺序丢一次远好过拖动卡一下。
    void this.prefs.persistOrder();
  }

  /** 收尾：拆 document listener、清 ghost / 源 Tab 变暗、清空拖拽状态。 */
  private teardownDrag(): void {
    const d = this.drag;
    if (!d) return;
    document.removeEventListener("mousemove", d.onMove);
    document.removeEventListener("mouseup", d.onUp);
    this.barEl.removeEventListener("scroll", d.onInvalidate, true);
    window.removeEventListener("resize", d.onInvalidate);
    // 停留计时器必须在这里清。不清的话它会在拖拽结束之后才到点，
    // 往一个已经收尾的状态上写 `onto` —— 而那时 `this.drag` 已是 null，
    // 回调里的守卫会吞掉它，但计时器本身是条悬空引线。
    if (d.dwellTimer !== null) window.clearTimeout(d.dwellTimer);
    d.dwellTimer = null;
    this.markDropTarget(null); // 拖拽结束必须清掉落点标记，否则它会挂在那儿
    d.ghost?.remove();
    d.root.classList.remove("dragging");
    this.drag = null;
    // ★ 6d：拖拽期间被守卫挡下的那些刷新，在这里**补一次**。
    // 少了这一句，守卫就从「推迟」变成「静默丢弃」：会话在拖拽那一秒里跑完了，
    // 它的 tab 会一直留在主栏假装还活着，直到下一件无关的事碰巧再刷一次栏。
    // ⚠ 必须在 `this.drag = null` **之后** —— 否则自己被自己的守卫挡回去。
    if (this.tabBarDirtyDuringDrag) {
      this.tabBarDirtyDuringDrag = false;
      this.host.refreshTabBar();
    }
  }

  /** document mouseup：收尾；armed 则在落点弹独立窗口。 */
  private onDragUp(e: MouseEvent): void {
    const d = this.drag;
    if (!d) return;
    // 落点要在 `teardownDrag` 之前取出来 —— 它会把整个 `drag` 清成 null。
    const { dragging, armed, sid, dropTarget } = d;
    this.teardownDrag();

    if (!dragging) return; // 没越阈值 = 纯点击，交给 click handler 正常切 Tab。

    // 起过拖（无论 armed 与否）都抑制紧随的 click —— 拖完不该顺带切 Tab。
    this.suppressClickSid = sid;
    if (armed) {
      // 撕窗口这一路顺序不动：用户撕出一个窗口时没要求改原栏的序。
      void this.host.openInNewWindow(sid, e.screenX, e.screenY);
      return;
    }
    this.applyDrop(sid, dropTarget);
  }
}
