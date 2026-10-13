/**
 * **每个 tab 的第一趟会话事实 ＋ 轮次什么时候问**（切到的先拉，没有哪一步等全部 tab）。
 *
 * 第一趟是冷读：`history-facts` 不带上一份成品、`history-turns` 从 0 —— 后端要把那份记录从头扫一遍。
 * F5 之后 N 个 tab 一起要，就是 N 份同时扫、当前 tab 那一份夹在中间。这里排成：
 *
 * - **前台**（当前 tab）当场要，记成「前台那一趟」；它落地（两问都回来，成没成都算）之前，后台一份都不发。
 * - **后台**排队；前台那一趟落地后，空闲时一个一个要，同时至多一份在途。
 * - 切进来停住的（`TabStreamView.activate` 那一下停够了）当场按前台要，从队里摘掉。
 * - 要过第一趟之后的再问（facts 带上一份成品、turns 从还没收尾那一轮）不是冷读，随时照旧要（[`want`]）。
 *
 * 只排「什么时候问」：问什么、回来怎么落都是 `FactsSource` / `TurnFold` 的事；本类不判会话事实。
 */
import type { Tab } from "./tab-model";
import type { TabStore } from "./tab-store";

/**
 * 一个空闲回调的位子（同时至多挂一个），挂在它的主人身上：主人拆掉（[`stop`]）⇒ 撤掉挂着的那一个；
 * 停了以后再来排一概不排、不碰 `window`（主人拆掉之后才回来的读，回来时会再排下一趟）。
 */
export class IdleSlot {
  /** 撤掉挂着的那一个（排的那一刻连同撤法一起记下：排的是哪一种就用哪一种撤）。 */
  private cancel: (() => void) | null = null;
  private stoppedFlag = false;

  get stopped(): boolean {
    return this.stoppedFlag;
  }

  /** 排一个（`fallbackMs`：没有 requestIdleCallback 的引擎上隔多久）。已停 ⇒ 不排。 */
  schedule(run: (deadline?: IdleDeadline) => void, fallbackMs: number): void {
    if (this.stoppedFlag) return;
    const go = (deadline?: IdleDeadline): void => {
      this.cancel = null;
      if (!this.stoppedFlag) run(deadline);
    };
    if (typeof window.requestIdleCallback === "function") {
      // 调度：自链 —— 主人的空闲队列（后台 tab 的第一趟读 · 后台 tab 的空闲物化）：处理一个再排自己，队空或主人拆掉即停
      const id = window.requestIdleCallback(go, { timeout: 2000 });
      const off = window.cancelIdleCallback;
      this.cancel = () => off?.call(window, id);
    } else {
      // 调度：自链 —— 上面那条在没有 rIC 的引擎（WebKitGTK）上的兜底，同一条链
      const id = window.setTimeout(() => go(), fallbackMs);
      this.cancel = () => window.clearTimeout(id);
    }
  }

  /** 主人拆掉：撤掉挂着的那一个，之后不再排。 */
  stop(): void {
    this.stoppedFlag = true;
    const c = this.cancel;
    this.cancel = null;
    c?.();
  }
}

export class ColdReads {
  /** 第一趟已经发过的 tab。 */
  private readonly warmed = new WeakSet<Tab>();
  /** 排着的后台 tab（sid，先来先要）。 */
  private queue: string[] = [];
  /** 前台那一趟（落地之前后台不发）。 */
  private front: Promise<unknown> | null = null;
  /** 在途的那一份后台（同时至多一份）。 */
  private running: Promise<unknown> | null = null;
  /** 已经排了一个空闲回调。 */
  private scheduled = false;
  /** 空闲回调挂在哪（主人拆掉时 [`stop`] 撤掉）。 */
  private readonly slot = new IdleSlot();

  constructor(private readonly store: TabStore) {}

  /** 这个 tab 的第一趟发过了。 */
  isWarm(tab: Tab): boolean {
    return this.warmed.has(tab);
  }

  /**
   * 这个 tab 的会话事实要再问一次（又长了 · 状态变了 · 那台说需手动变了 · 上限表换了 ……）：
   * 发过第一趟的照旧续读；没发过的 —— 当前 tab ⇒ 当场按前台要；后台 ⇒ 只记一笔、排队，轮到再要。
   */
  want(tab: Tab): void {
    tab.facts.markStale();
    if (this.warmed.has(tab)) {
      void tab.facts.refresh();
      return;
    }
    if (this.store.activeId === tab.sessionId) this.toFront(tab);
    else this.later(tab);
  }

  /** 又来了一行（非批期）：发过第一趟的照旧续读两样；没发过的同 [`want`]（当前 ⇒ 当场要；后台 ⇒ 排着）。 */
  grew(tab: Tab): void {
    if (!this.warmed.has(tab)) {
      this.want(tab);
      return;
    }
    void tab.facts.refresh();
    void tab.turnFold.refresh();
  }

  /**
   * 当前 tab（批结束 · 切进来停住了）：没发过第一趟 ⇒ 当场要、记成前台那一趟、从队里摘掉；
   * 发过 ⇒ 照旧续读（有新写的才要）。
   */
  toFront(tab: Tab): void {
    if (this.warmed.has(tab)) {
      if (tab.facts.needsFetch) void tab.facts.refresh();
      return;
    }
    this.queue = this.queue.filter((sid) => sid !== tab.sessionId);
    const p = this.warm(tab);
    if (!p) return;
    this.front = p;
    void p.finally(() => {
      if (this.front === p) this.front = null;
      this.pump();
    });
  }

  /** 后台 tab：没发过第一趟 ⇒ 排队（已排着的不重排）；发过 ⇒ 照旧续读。 */
  later(tab: Tab): void {
    if (this.warmed.has(tab)) {
      if (tab.facts.needsFetch) void tab.facts.refresh();
      void tab.turnFold.refresh();
      return;
    }
    if (!this.queue.includes(tab.sessionId)) this.queue.push(tab.sessionId);
    this.pump();
  }

  /** 主人（流视图）拆掉：撤掉挂着的空闲回调、清队；之后回来的读不再排下一趟，也不再发新的。 */
  stop(): void {
    this.slot.stop();
    this.queue = [];
    this.front = null;
    this.running = null;
    this.scheduled = false;
  }

  /** 关掉的 tab 出队。 */
  drop(sid: string): void {
    this.queue = this.queue.filter((s) => s !== sid);
  }

  /** 发第一趟（facts ＋ turns），两问都回来（成没成都算）才落地。路径还没到（tab 先于首条行建出来）⇒ 不算发过，下次再说。 */
  private warm(tab: Tab): Promise<unknown> | null {
    if (!tab.parentPath || this.slot.stopped) return null;
    this.warmed.add(tab);
    return Promise.allSettled([tab.facts.refresh(), tab.turnFold.refresh()]);
  }

  /** 前台那一趟落地了、没有在途的后台 ⇒ 空闲时要队首那一个。 */
  private pump(): void {
    if (this.slot.stopped || this.front || this.running || this.scheduled || this.queue.length === 0) return;
    this.scheduled = true;
    const run = (): void => {
      this.scheduled = false;
      if (this.front || this.running) return; // 等的时候来了前台那一趟：它落地后再排
      for (;;) {
        const sid = this.queue.shift();
        if (sid === undefined) return;
        const tab = this.store.tabs.get(sid);
        if (!tab || this.warmed.has(tab)) continue;
        const p = this.warm(tab);
        if (!p) continue; // 还没有路径：首条行到了会再来（`want`）
        this.running = p;
        void p.finally(() => {
          if (this.running === p) this.running = null;
          this.pump();
        });
        return;
      }
    };
    // 后台 tab 的第一趟会话事实与轮次：空闲时要一个，回来了再排下一个，队空或主人拆掉即停
    this.slot.schedule(run, 200);
  }
}
